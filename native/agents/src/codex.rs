// Adapted from T3 Code (https://github.com/pingdotgg/t3code), MIT, © 2026 T3 Tools Inc.
// See THIRD_PARTY_NOTICES.md.

//! Codex, driven through `codex app-server`: JSON-RPC (without the
//! `jsonrpc` field) as one JSON object per line on stdio. Method names and
//! shapes follow T3 Code's effect-codex-app-server package
//! (https://github.com/pingdotgg/t3code), MIT, © 2026 T3 Tools Inc.
//!
//! One app-server serves every Codex chat. A chat is a Codex thread
//! (`thread/start`, or `thread/resume` after a restart); each message is a
//! `turn/start` carrying the model and effort; `turn/interrupt` stops it.
//! Codex asks before commands and file changes with server requests, which
//! become approvals. The FeedHQ connector goes in each thread's `config` as
//! `mcp_servers.feedhq` with the relay's URL (T3 Code passes its own MCP
//! server the same way).
//!
//! Only the person's own, unmodified `codex` runs, signed in as they left
//! it. `initialize` names Billable itself (`clientInfo.name = "billable"`,
//! the app's version), never a first-party client, and no credential file
//! or API key is touched.

use std::{
    collections::HashMap,
    io::Write as _,
    path::Path,
    process::{Child, ChildStdin, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    time::Duration,
};

use serde_json::{json, Value};

use crate::{
    driver::DriverEvent,
    process::{agent_command, kill, last_error_line, read_lines, run_with_timeout, tail_stderr},
    protocol::{Decision, Model, TurnStatus, Usage},
    tools::output_text,
};

pub const AGENT_ID: &str = "codex";
pub const AGENT_NAME: &str = "Codex";

pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

// ---------- Discovery ----------

/// `codex --version` → "0.172.0" (from "codex-cli 0.172.0").
pub fn parse_version(out: &str) -> Option<String> {
    out.split_whitespace().last().map(|v| v.trim_start_matches('v')).filter(|v| v.chars().next().is_some_and(|c| c.is_ascii_digit())).map(str::to_string)
}

pub fn version(bin: &Path) -> Option<String> {
    let out = run_with_timeout(agent_command(bin, &std::env::temp_dir()).arg("--version"), Duration::from_secs(10))?;
    parse_version(&out.stdout)
}

/// Used until an app-server the person started lists its models.
pub fn fallback_models() -> Vec<Model> {
    let efforts: Vec<String> = ["low", "medium", "high", "xhigh"].iter().map(|s| s.to_string()).collect();
    [("gpt-6.1-sol", "GPT-6.1 Sol"), ("gpt-6-astra", "GPT-6 Astra"), ("gpt-6-luna", "GPT-6 Luna")]
        .iter()
        .map(|(id, name)| Model { id: id.to_string(), name: name.to_string(), efforts: efforts.clone(), default_effort: Some("medium".into()) })
        .collect()
}

/// `model/list`'s `data` → models (hidden ones left out) and the default.
pub fn models_from_list(result: &Value) -> Option<(Vec<Model>, String)> {
    let data = result.get("data")?.as_array()?;
    let mut models = Vec::new();
    let mut default = None;
    for m in data {
        if m.get("hidden").and_then(Value::as_bool).unwrap_or(false) {
            continue;
        }
        let Some(id) = m.get("model").or_else(|| m.get("id")).and_then(Value::as_str) else { continue };
        let efforts: Vec<String> = m
            .get("supportedReasoningEfforts")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|e| e.get("reasoningEffort").or(Some(e)).and_then(Value::as_str)).map(str::to_string).collect())
            .unwrap_or_default();
        let default_effort = m.get("defaultReasoningEffort").and_then(Value::as_str).map(str::to_string).filter(|e| efforts.contains(e)).or_else(|| efforts.first().cloned());
        if m.get("isDefault").and_then(Value::as_bool).unwrap_or(false) {
            default = Some(id.to_string());
        }
        models.push(Model { id: id.to_string(), name: m.get("displayName").and_then(Value::as_str).unwrap_or(id).to_string(), efforts, default_effort });
    }
    if models.is_empty() {
        return None;
    }
    let default = default.unwrap_or_else(|| models[0].id.clone());
    Some((models, default))
}

/// `codex login status`: exit 0 when signed in ("Logged in using …"), an
/// error saying "Not logged in" when not. `None` when it can't tell (an
/// older CLI without the command).
pub fn parse_login_status(status: Option<i32>, output: &str) -> Option<bool> {
    let text = output.to_ascii_lowercase();
    match status {
        Some(0) if text.contains("logged in") => Some(true),
        Some(_) if text.contains("not logged in") => Some(false),
        _ => None,
    }
}

/// Asks the CLI's own status command; never reads its files.
pub fn signed_in(bin: &Path) -> Option<bool> {
    let out = run_with_timeout(agent_command(bin, &std::env::temp_dir()).args(["login", "status"]), Duration::from_secs(10))?;
    parse_login_status(out.status, &format!("{}\n{}", out.stdout, out.stderr))
}

// ---------- JSON-RPC over stdio ----------

#[derive(Clone, Debug, PartialEq)]
pub enum Incoming {
    Notification { method: String, params: Value },
    Request { id: Value, method: String, params: Value },
    Exited { error: Option<String> },
}

pub type IncomingSink = Arc<dyn Fn(Incoming) + Send + Sync>;

type Pending = Arc<Mutex<HashMap<String, mpsc::Sender<Result<Value, String>>>>>;

pub struct Peer {
    stdin: Mutex<ChildStdin>,
    pending: Pending,
    next: AtomicU64,
}

fn id_key(id: &Value) -> String {
    id.to_string()
}

/// Route one line from app-server: answers go to whoever asked; requests and
/// notifications to `sink`.
fn route(line: &str, pending: &Pending, sink: &IncomingSink) {
    let Ok(v) = serde_json::from_str::<Value>(line) else { return };
    let method = v.get("method").and_then(Value::as_str).map(str::to_string);
    let params = v.get("params").cloned().unwrap_or(Value::Null);
    match (v.get("id"), method) {
        (Some(id), Some(method)) => sink(Incoming::Request { id: id.clone(), method, params }),
        (None, Some(method)) => sink(Incoming::Notification { method, params }),
        (Some(id), None) => {
            let waiter = pending.lock().ok().and_then(|mut p| p.remove(&id_key(id)));
            if let Some(w) = waiter {
                let result = match v.get("error") {
                    Some(e) => Err(e.get("message").and_then(Value::as_str).unwrap_or("Codex refused the request.").to_string()),
                    None => Ok(v.get("result").cloned().unwrap_or(Value::Null)),
                };
                let _ = w.send(result);
            }
        }
        (None, None) => {}
    }
}

impl Peer {
    fn write(&self, msg: &Value) -> anyhow::Result<()> {
        let mut s = self.stdin.lock().map_err(|_| anyhow::anyhow!("stdin poisoned"))?;
        s.write_all(msg.to_string().as_bytes())?;
        s.write_all(b"\n")?;
        s.flush()?;
        Ok(())
    }

    /// Send a request and wait for its answer.
    pub fn request(&self, method: &str, params: Value, timeout: Duration) -> Result<Value, String> {
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = mpsc::channel();
        if let Ok(mut p) = self.pending.lock() {
            p.insert(id_key(&json!(id)), tx);
        }
        if let Err(e) = self.write(&json!({"id": id, "method": method, "params": params})) {
            return Err(e.to_string());
        }
        let answer = rx.recv_timeout(timeout);
        if let Ok(mut p) = self.pending.lock() {
            p.remove(&id_key(&json!(id)));
        }
        answer.unwrap_or_else(|_| Err(format!("Codex didn't answer {method} in time.")))
    }

    /// Send a request without waiting for the answer.
    pub fn send(&self, method: &str, params: Value) -> anyhow::Result<()> {
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        self.write(&json!({"id": id, "method": method, "params": params}))
    }

    pub fn notify(&self, method: &str, params: Option<Value>) -> anyhow::Result<()> {
        match params {
            Some(p) => self.write(&json!({"method": method, "params": p})),
            None => self.write(&json!({"method": method})),
        }
    }

    pub fn respond(&self, id: &Value, result: Value) -> anyhow::Result<()> {
        self.write(&json!({"id": id, "result": result}))
    }

    pub fn respond_error(&self, id: &Value, message: &str) -> anyhow::Result<()> {
        self.write(&json!({"id": id, "error": {"code": -32601, "message": message}}))
    }
}

/// A running app-server.
pub struct Server {
    child: Child,
    pub peer: Arc<Peer>,
}

impl Server {
    pub fn start(bin: &Path, cwd: &Path, sink: IncomingSink) -> anyhow::Result<Server> {
        let mut child = agent_command(bin, cwd).arg("app-server").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
        let stdin = child.stdin.take().ok_or_else(|| anyhow::anyhow!("no stdin"))?;
        let stdout = child.stdout.take().ok_or_else(|| anyhow::anyhow!("no stdout"))?;
        let stderr = tail_stderr(child.stderr.take().ok_or_else(|| anyhow::anyhow!("no stderr"))?);
        let pending: Pending = Arc::default();
        let peer = Arc::new(Peer { stdin: Mutex::new(stdin), pending: pending.clone(), next: AtomicU64::new(1) });
        let (line_sink, end_sink, end_pending) = (sink.clone(), sink, pending.clone());
        read_lines(
            stdout,
            move |line| route(&line, &pending, &line_sink),
            move || {
                // Whoever is still waiting hears now, not at their timeout.
                if let Ok(mut p) = end_pending.lock() {
                    for (_, w) in p.drain() {
                        let _ = w.send(Err("Codex stopped.".into()));
                    }
                }
                std::thread::sleep(Duration::from_millis(100));
                let tail = stderr.lock().map(|t| t.clone()).unwrap_or_default();
                end_sink(Incoming::Exited { error: last_error_line(&tail) });
            },
        );
        let init = peer.request(
            "initialize",
            json!({"clientInfo": {"name": "billable", "title": "Billable", "version": env!("CARGO_PKG_VERSION")}, "capabilities": {"experimentalApi": true}}),
            Duration::from_secs(30),
        );
        let mut server = Server { child, peer };
        if let Err(e) = init {
            server.stop();
            anyhow::bail!(e);
        }
        server.peer.notify("initialized", None)?;
        Ok(server)
    }

    pub fn alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    pub fn stop(&mut self) {
        kill(&mut self.child);
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop();
    }
}

// ---------- Threads and turns ----------

/// Said to every Codex thread.
pub const DEVELOPER_NOTE: &str = crate::claude::SYSTEM_NOTE;

/// `thread/start` (or, with `resume`, `thread/resume`) params. `notes`
/// (the seat's guidance and project memory) follow the usual note; a chat
/// with `orchestrate_url` gets the `billable` server; a `read_only` lookup
/// runs in Codex's read-only sandbox and never asks.
pub fn thread_params(model: &str, cwd: &Path, mcp_url: Option<&str>, orchestrate_url: Option<&str>, notes: &str, read_only: bool, resume: Option<&str>) -> Value {
    let mut config = serde_json::Map::new();
    if let Some(url) = mcp_url {
        config.insert("mcp_servers.feedhq".into(), json!({"url": url, "tool_timeout_sec": 300}));
    }
    let developer = if notes.is_empty() { DEVELOPER_NOTE.to_string() } else { format!("{DEVELOPER_NOTE}\n\n{notes}") };
    if let Some(url) = orchestrate_url {
        // wait_for_agents and ask_person can take up to 30 minutes.
        config.insert(format!("mcp_servers.{}", crate::orchestrator::SERVER_NAME), json!({"url": url, "tool_timeout_sec": 1900}));
    }
    let mut p = json!({
        "model": model,
        "cwd": cwd.to_string_lossy(),
        "approvalPolicy": if read_only { "never" } else { "untrusted" },
        "sandbox": if read_only { "read-only" } else { "workspace-write" },
        "developerInstructions": developer,
        "config": Value::Object(config),
    });
    if let Some(id) = resume {
        p["threadId"] = json!(id);
    }
    p
}

pub fn turn_params(codex_thread: &str, text: &str, model: &str, effort: Option<&str>, images: &[std::path::PathBuf]) -> Value {
    let mut input = vec![json!({"type": "text", "text": text})];
    input.extend(images.iter().map(|p| json!({"type": "localImage", "path": p.to_string_lossy()})));
    let mut p = json!({"threadId": codex_thread, "input": input, "model": model, "summary": "auto"});
    if let Some(e) = effort {
        p["effort"] = json!(e);
    }
    p
}

/// What an approval's `native` id says: which kind and the request id.
fn native_id(kind: &str, id: &Value) -> String {
    format!("{kind}|{id}")
}

/// `(kind, request id)` from a native approval id.
pub fn parse_native(native: &str) -> Option<(&str, Value)> {
    let (kind, id) = native.split_once('|')?;
    Some((kind, serde_json::from_str(id).ok()?))
}

/// The answer for an approval of `kind`.
pub fn answer_result(kind: &str, decision: Decision) -> Value {
    match kind {
        "elicit" => match decision {
            Decision::Deny => json!({"action": "decline"}),
            _ => json!({"action": "accept", "content": {}}),
        },
        _ => json!({"decision": match decision {
            Decision::Allow => "accept",
            Decision::AllowAlways => "acceptForSession",
            Decision::Deny => "decline",
        }}),
    }
}

fn s(v: &Value, k: &str) -> String {
    v.get(k).and_then(Value::as_str).unwrap_or("").to_string()
}

fn tool_of(item: &Value) -> Option<(String, Value)> {
    match item.get("type").and_then(Value::as_str)? {
        "commandExecution" => Some(("shell".into(), json!({"command": s(item, "command"), "cwd": s(item, "cwd")}))),
        "fileChange" => Some(("apply_patch".into(), json!({"changes": item.get("changes").cloned().unwrap_or(json!([]))}))),
        "mcpToolCall" => Some((format!("mcp__{}__{}", s(item, "server"), s(item, "tool")), item.get("arguments").cloned().unwrap_or(json!({})))),
        "dynamicToolCall" => Some((s(item, "tool"), item.get("arguments").cloned().unwrap_or(json!({})))),
        "webSearch" => Some(("web_search".into(), json!({"query": s(item, "query")}))),
        _ => None,
    }
}

fn finished(item: &Value) -> Option<DriverEvent> {
    let key = s(item, "id");
    let status = s(item, "status");
    let declined = status == "declined";
    match item.get("type").and_then(Value::as_str)? {
        "agentMessage" => Some(DriverEvent::TextDone { key, text: s(item, "text") }),
        "reasoning" => {
            let join = |k: &str| item.get(k).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("\n\n")).unwrap_or_default();
            let text = Some(join("summary")).filter(|t| !t.trim().is_empty()).unwrap_or_else(|| join("content"));
            (!text.trim().is_empty()).then_some(DriverEvent::ReasoningDone { key, text })
        }
        "commandExecution" => {
            let exit = item.get("exitCode").and_then(Value::as_i64);
            Some(DriverEvent::ToolFinished {
                key,
                output: item.get("aggregatedOutput").and_then(output_text),
                failed: status == "failed" || exit.is_some_and(|c| c != 0),
                declined,
            })
        }
        "fileChange" => {
            let paths: Vec<String> = item.get("changes").and_then(Value::as_array).map(|c| c.iter().map(|ch| s(ch, "path")).collect()).unwrap_or_default();
            Some(DriverEvent::ToolFinished { key, output: Some(paths.join("\n")).filter(|p| !p.is_empty()), failed: status == "failed", declined })
        }
        "mcpToolCall" | "dynamicToolCall" => {
            let error = item.pointer("/error/message").and_then(Value::as_str).map(str::to_string);
            let output = error.clone().or_else(|| item.pointer("/result/content").and_then(output_text)).or_else(|| item.get("contentItems").and_then(output_text));
            Some(DriverEvent::ToolFinished { key, output, failed: status == "failed" || error.is_some(), declined })
        }
        "webSearch" => Some(DriverEvent::ToolFinished { key, output: None, failed: false, declined: false }),
        _ => None,
    }
}

/// A notification → (Codex thread id, event), or the turn it started.
pub fn map_notification(method: &str, p: &Value) -> Option<(String, DriverEvent)> {
    let thread = s(p, "threadId");
    let ev = match method {
        "item/started" => {
            let item = p.get("item")?;
            let (name, input) = tool_of(item)?;
            DriverEvent::ToolStarted { key: s(item, "id"), name, input }
        }
        "item/completed" => finished(p.get("item")?)?,
        "item/agentMessage/delta" => DriverEvent::TextDelta { key: s(p, "itemId"), text: s(p, "delta") },
        "item/reasoning/summaryTextDelta" => DriverEvent::ReasoningDelta { key: s(p, "itemId"), text: s(p, "delta") },
        "thread/tokenUsage/updated" => {
            let last = p.pointer("/tokenUsage/last")?;
            let n = |k: &str| last.get(k).and_then(Value::as_u64).unwrap_or(0);
            DriverEvent::Usage(Usage { input_tokens: n("inputTokens"), output_tokens: n("outputTokens"), cost_usd: None })
        }
        "error" => {
            if p.get("willRetry").and_then(Value::as_bool).unwrap_or(false) {
                return None;
            }
            DriverEvent::Error(p.pointer("/error/message").and_then(Value::as_str).unwrap_or("Codex hit an error.").to_string())
        }
        "turn/completed" => {
            let turn = p.get("turn")?;
            let status = match turn.get("status").and_then(Value::as_str) {
                Some("completed") => TurnStatus::Done,
                Some("interrupted") => TurnStatus::Stopped,
                _ => TurnStatus::Failed,
            };
            let error = turn.pointer("/error/message").and_then(Value::as_str).map(str::to_string);
            DriverEvent::TurnDone { status, error: if status == TurnStatus::Failed { error.or(Some("Codex couldn't finish.".into())) } else { error }, usage: None }
        }
        _ => return None,
    };
    if thread.is_empty() {
        return None;
    }
    Some((thread, ev))
}

/// A server request → (Codex thread id, approval), or `None` when it's one
/// Billable answers with an error.
pub fn map_request(id: &Value, method: &str, p: &Value) -> Option<(String, DriverEvent)> {
    let thread = s(p, "threadId");
    let all = vec![Decision::Allow, Decision::AllowAlways, Decision::Deny];
    let ev = match method {
        "item/commandExecution/requestApproval" => DriverEvent::Approval {
            native: native_id("cmd", id),
            tool_key: Some(s(p, "itemId")).filter(|k| !k.is_empty()),
            name: "shell".into(),
            input: json!({"command": s(p, "command"), "cwd": s(p, "cwd"), "reason": s(p, "reason")}),
            options: all,
        },
        "item/fileChange/requestApproval" => DriverEvent::Approval {
            native: native_id("patch", id),
            tool_key: Some(s(p, "itemId")).filter(|k| !k.is_empty()),
            name: "apply_patch".into(),
            input: json!({"reason": s(p, "reason"), "path": s(p, "grantRoot")}),
            options: all,
        },
        "mcpServer/elicitation/request" => DriverEvent::Approval {
            native: native_id("elicit", id),
            tool_key: None,
            name: format!("mcp__{}__request", s(p, "serverName")),
            input: json!({"message": s(p, "message")}),
            options: vec![Decision::Allow, Decision::Deny],
        },
        _ => return None,
    };
    (!thread.is_empty()).then_some((thread, ev))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_notifications() {
        let n = |m: &str, p: Value| map_notification(m, &p);
        assert_eq!(
            n("item/started", json!({"threadId":"c1","turnId":"u","item":{"type":"commandExecution","id":"call_1","command":"ls -la","cwd":"/w","commandActions":[],"status":"inProgress"}})),
            Some(("c1".into(), DriverEvent::ToolStarted { key: "call_1".into(), name: "shell".into(), input: json!({"command":"ls -la","cwd":"/w"}) }))
        );
        assert_eq!(n("item/started", json!({"threadId":"c1","item":{"type":"agentMessage","id":"m","text":""}})), None);
        assert_eq!(
            n("item/agentMessage/delta", json!({"threadId":"c1","turnId":"u","itemId":"m","delta":"Hi"})),
            Some(("c1".into(), DriverEvent::TextDelta { key: "m".into(), text: "Hi".into() }))
        );
        assert_eq!(
            n("item/completed", json!({"threadId":"c1","item":{"type":"commandExecution","id":"call_1","command":"ls","cwd":"/","commandActions":[],"status":"completed","exitCode":2,"aggregatedOutput":"nope"}})),
            Some(("c1".into(), DriverEvent::ToolFinished { key: "call_1".into(), output: Some("nope".into()), failed: true, declined: false }))
        );
        assert_eq!(
            n("item/started", json!({"threadId":"c1","item":{"type":"mcpToolCall","id":"x","server":"feedhq","tool":"createTask","arguments":{"title":"A"},"status":"inProgress"}})),
            Some(("c1".into(), DriverEvent::ToolStarted { key: "x".into(), name: "mcp__feedhq__createTask".into(), input: json!({"title":"A"}) }))
        );
        assert_eq!(
            n("item/completed", json!({"threadId":"c1","item":{"type":"mcpToolCall","id":"x","server":"feedhq","tool":"createTask","arguments":{},"status":"completed","result":{"content":[{"type":"text","text":"made"}]}}})),
            Some(("c1".into(), DriverEvent::ToolFinished { key: "x".into(), output: Some("made".into()), failed: false, declined: false }))
        );
        assert_eq!(
            n("item/completed", json!({"threadId":"c1","item":{"type":"reasoning","id":"r","summary":["**Plan**","then"],"content":[]}})),
            Some(("c1".into(), DriverEvent::ReasoningDone { key: "r".into(), text: "**Plan**\n\nthen".into() }))
        );
        assert_eq!(
            n("thread/tokenUsage/updated", json!({"threadId":"c1","turnId":"u","tokenUsage":{"last":{"inputTokens":10,"outputTokens":3,"cachedInputTokens":0,"reasoningOutputTokens":0,"totalTokens":13},"total":{}}})),
            Some(("c1".into(), DriverEvent::Usage(Usage { input_tokens: 10, output_tokens: 3, cost_usd: None })))
        );
        assert_eq!(
            n("turn/completed", json!({"threadId":"c1","turn":{"id":"u","items":[],"status":"interrupted"}})),
            Some(("c1".into(), DriverEvent::TurnDone { status: TurnStatus::Stopped, error: None, usage: None }))
        );
        assert!(matches!(
            n("turn/completed", json!({"threadId":"c1","turn":{"id":"u","items":[],"status":"failed","error":{"message":"usage limit"}}})),
            Some((_, DriverEvent::TurnDone { status: TurnStatus::Failed, error: Some(e), .. })) if e == "usage limit"
        ));
        assert_eq!(n("error", json!({"threadId":"c1","turnId":"u","willRetry":true,"error":{"message":"x"}})), None);
        assert_eq!(n("thread/started", json!({"thread":{}})), None);
    }

    #[test]
    fn approvals_round_trip() {
        let (thread, ev) = map_request(&json!(7), "item/commandExecution/requestApproval", &json!({"threadId":"c1","turnId":"u","itemId":"call_1","command":"rm x","startedAtMs":1})).unwrap();
        assert_eq!(thread, "c1");
        let DriverEvent::Approval { native, tool_key, name, .. } = ev else { panic!() };
        assert_eq!((tool_key.as_deref(), name.as_str()), (Some("call_1"), "shell"));
        let (kind, id) = parse_native(&native).unwrap();
        assert_eq!((kind, id), ("cmd", json!(7)));
        assert_eq!(answer_result("cmd", Decision::AllowAlways), json!({"decision":"acceptForSession"}));
        assert_eq!(answer_result("patch", Decision::Deny), json!({"decision":"decline"}));
        assert_eq!(answer_result("elicit", Decision::Allow), json!({"action":"accept","content":{}}));
        let (_, ev) = map_request(&json!("s-1"), "mcpServer/elicitation/request", &json!({"threadId":"c1","serverName":"feedhq","message":"Allow?","mode":"form","requestedSchema":{}})).unwrap();
        let DriverEvent::Approval { native, name, options, .. } = ev else { panic!() };
        assert_eq!(parse_native(&native).unwrap(), ("elicit", json!("s-1")));
        assert_eq!(name, "mcp__feedhq__request");
        assert_eq!(options, vec![Decision::Allow, Decision::Deny]);
        assert!(map_request(&json!(1), "item/tool/requestUserInput", &json!({"threadId":"c1"})).is_none());
    }

    #[test]
    fn params_and_discovery() {
        let p = thread_params("gpt-6.1-sol", Path::new("/w"), Some("http://127.0.0.1:5/mcp/s"), None, "", false, Some("c1"));
        assert_eq!(p["config"]["mcp_servers.feedhq"]["url"], json!("http://127.0.0.1:5/mcp/s"));
        assert_eq!(p["threadId"], json!("c1"));
        assert_eq!(p["approvalPolicy"], json!("untrusted"));
        assert!(p["config"].get("mcp_servers.billable").is_none());
        assert_eq!(p["developerInstructions"], json!(DEVELOPER_NOTE));
        let p = thread_params("m", Path::new("/w"), None, None, crate::orchestrator::LOOKUP, true, None);
        assert_eq!(p["config"], json!({}));
        assert!(p.get("threadId").is_none());
        assert_eq!((p["sandbox"].clone(), p["approvalPolicy"].clone()), (json!("read-only"), json!("never")));
        let notes = crate::orchestrator::notes(Some(crate::orchestrator::Seat::Coordinator), None);
        let p = thread_params("m", Path::new("/site"), None, Some("http://h/mcp/s/orchestrate/t_1"), &notes, false, None);
        assert_eq!(p["config"]["mcp_servers.billable"]["url"], json!("http://h/mcp/s/orchestrate/t_1"));
        assert_eq!(p["cwd"], json!("/site"));
        assert!(p["developerInstructions"].as_str().unwrap().contains("wait_for_agents"));
        assert_eq!(turn_params("c1", "hi", "m", Some("high"), &[]), json!({"threadId":"c1","input":[{"type":"text","text":"hi"}],"model":"m","summary":"auto","effort":"high"}));
        let with = turn_params("c1", "hi", "m", None, &[std::path::PathBuf::from("/a/shot.png")]);
        assert_eq!(with["input"][1], json!({"type": "localImage", "path": "/a/shot.png"}));
        assert_eq!(parse_version("codex-cli 0.172.0\n").as_deref(), Some("0.172.0"));
        let list = json!({"data":[
            {"id":"a","model":"gpt-6.1-sol","displayName":"GPT-6.1 Sol","hidden":false,"isDefault":true,"defaultReasoningEffort":"medium","supportedReasoningEfforts":[{"reasoningEffort":"low","description":""},{"reasoningEffort":"medium","description":""}],"description":""},
            {"id":"b","model":"secret","displayName":"S","hidden":true,"isDefault":false,"defaultReasoningEffort":"low","supportedReasoningEfforts":[],"description":""},
            {"id":"c","model":"gpt-6-mini","displayName":"Mini","hidden":false,"isDefault":false,"defaultReasoningEffort":"low","supportedReasoningEfforts":[{"reasoningEffort":"low","description":""}],"description":""}
        ]});
        let (models, default) = models_from_list(&list).unwrap();
        assert_eq!(models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), vec!["gpt-6.1-sol", "gpt-6-mini"]);
        assert_eq!(default, "gpt-6.1-sol");
        assert_eq!(models[0].efforts, vec!["low", "medium"]);
        assert_eq!(parse_login_status(Some(0), "Logged in using ChatGPT\n"), Some(true));
        assert_eq!(parse_login_status(Some(1), "Not logged in\n"), Some(false));
        assert_eq!(parse_login_status(Some(2), "error: unrecognized subcommand 'status'"), None);
    }

    #[test]
    fn routes_lines() {
        let pending: Pending = Arc::default();
        let (tx, rx) = mpsc::channel();
        pending.lock().unwrap().insert(id_key(&json!(3)), tx);
        let got: Arc<Mutex<Vec<Incoming>>> = Arc::default();
        let g = got.clone();
        let sink: IncomingSink = Arc::new(move |i| g.lock().unwrap().push(i));
        route(r#"{"id":3,"result":{"ok":1}}"#, &pending, &sink);
        assert_eq!(rx.recv().unwrap(), Ok(json!({"ok":1})));
        route(r#"{"id":9,"method":"item/fileChange/requestApproval","params":{"threadId":"c"}}"#, &pending, &sink);
        route(r#"{"method":"turn/started","params":{"threadId":"c"}}"#, &pending, &sink);
        let got = got.lock().unwrap();
        assert!(matches!(&got[0], Incoming::Request { method, .. } if method == "item/fileChange/requestApproval"));
        assert!(matches!(&got[1], Incoming::Notification { method, .. } if method == "turn/started"));
    }
}
