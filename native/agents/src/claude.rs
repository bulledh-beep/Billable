// Adapted from T3 Code (https://github.com/pingdotgg/t3code), MIT, © 2026 T3 Tools Inc.
// See THIRD_PARTY_NOTICES.md.

//! Claude Code, driven as `claude -p` with stream-json in and out.
//!
//! One long-lived process per chat. Turns go in as `user` messages on stdin;
//! the CLI asks before a step with a `can_use_tool` control request, and is
//! stopped with an `interrupt` control request. A chat that comes
//! back later resumes its session with `--resume <session id>`.
//!
//! Only the person's own, unmodified `claude` runs, signed in as they left
//! it: Billable never reads its credentials, never sets a token or API key
//! for it, and passes its environment through (PATH aside). The models come
//! from the `initialize` answer of a session the person started.
//!
//! The FeedHQ connector goes in with `--mcp-config` as an `http` server
//! named `feedhq` pointing at the local relay; the user's own settings are
//! read as usual and never written.

use std::{
    collections::HashMap,
    io::Write as _,
    path::Path,
    process::{Child, ChildStdin, Stdio},
    sync::{Arc, Mutex},
    time::Duration,
};

use serde_json::{json, Value};

use crate::{
    driver::DriverEvent,
    process::{agent_command, kill, last_error_line, read_lines, run_with_timeout, tail_stderr},
    protocol::{Decision, Model, TurnStatus, Usage},
    tools::output_text,
};

pub const AGENT_ID: &str = "claude";
pub const AGENT_NAME: &str = "Claude Code";

/// The id of the `initialize` each session starts with.
const INIT_REQUEST: &str = "init";

/// Said to every session, after Claude Code's own system prompt.
pub const SYSTEM_NOTE: &str = "You are running inside FeedHQ's Agents chat in the Billable app on the user's Mac. \
Tools named mcp__feedhq__* act on the user's FeedHQ workspace (content, posts, tasks, calendar); use them for FeedHQ work.";

// ---------- Models ----------

/// What ships if the CLI can't be asked (aliases Claude Code expands).
pub fn fallback_models() -> Vec<Model> {
    let efforts: Vec<String> = ["low", "medium", "high", "xhigh", "max"].iter().map(|s| s.to_string()).collect();
    [("opus", "Opus 5.5"), ("fable", "Fable 5.2"), ("sonnet", "Sonnet 5.5"), ("haiku", "Haiku 5.5")]
        .iter()
        .map(|(id, name)| Model { id: id.to_string(), name: name.to_string(), efforts: efforts.clone(), default_effort: Some("medium".into()) })
        .collect()
}

/// The aliased models from the CLI's `initialize` answer (`models`), and the
/// one "default" resolves to.
pub fn models_from_initialize(response: &Value) -> Option<(Vec<Model>, String)> {
    let list = response.get("models")?.as_array()?;
    let default_resolved = list.iter().find(|m| m.get("value").and_then(Value::as_str) == Some("default")).and_then(|m| m.get("resolvedModel")).and_then(Value::as_str);
    let mut models = Vec::new();
    let mut default = None;
    for m in list {
        let Some(id) = m.get("value").and_then(Value::as_str) else { continue };
        // Aliases only ("opus", "sonnet"…): they follow the newest release.
        if id == "default" || id.starts_with("claude-") || id.contains('[') {
            continue;
        }
        let name = m.get("displayName").and_then(Value::as_str).unwrap_or(id).to_string();
        let efforts: Vec<String> = if m.get("supportsEffort").and_then(Value::as_bool).unwrap_or(false) {
            m.get("supportedEffortLevels").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default()
        } else {
            Vec::new()
        };
        let default_effort = if efforts.iter().any(|e| e == "medium") { Some("medium".to_string()) } else { efforts.first().cloned() };
        if default.is_none() && default_resolved.is_some() && m.get("resolvedModel").and_then(Value::as_str) == default_resolved {
            default = Some(id.to_string());
        }
        models.push(Model { id: id.to_string(), name, efforts, default_effort });
    }
    if models.is_empty() {
        return None;
    }
    let default = default.unwrap_or_else(|| models[0].id.clone());
    Some((models, default))
}

/// `claude --version` → "2.1.332".
pub fn parse_version(out: &str) -> Option<String> {
    out.split_whitespace().next().filter(|v| v.chars().next().is_some_and(|c| c.is_ascii_digit())).map(str::to_string)
}

/// `claude auth status` (JSON): `loggedIn`. `None` when it can't tell.
pub fn parse_auth_status(out: &str) -> Option<bool> {
    let v: Value = serde_json::from_str(out.trim()).ok()?;
    v.get("loggedIn").and_then(Value::as_bool)
}

pub fn version(bin: &Path) -> Option<String> {
    let out = run_with_timeout(agent_command(bin, &std::env::temp_dir()).arg("--version"), Duration::from_secs(10))?;
    parse_version(&out.stdout)
}

pub fn signed_in(bin: &Path) -> Option<bool> {
    let out = run_with_timeout(agent_command(bin, &std::env::temp_dir()).args(["auth", "status", "--json"]), Duration::from_secs(10))?;
    parse_auth_status(&out.stdout)
}

// ---------- Launch ----------

#[derive(Clone, Debug, PartialEq)]
pub struct Launch {
    pub model: String,
    pub effort: Option<String>,
    pub resume: Option<String>,
    /// The FeedHQ relay's URL, when FeedHQ is connected.
    pub mcp_url: Option<String>,
    /// Orchestrators only: the relay's `billable` server for this thread.
    pub orchestrate_url: Option<String>,
}

/// The `--mcp-config` JSON: `feedhq` (the relay) and, for an orchestrator,
/// `billable`. `None` when there's neither.
pub fn mcp_config(l: &Launch) -> Option<String> {
    let mut servers = serde_json::Map::new();
    if let Some(url) = &l.mcp_url {
        servers.insert("feedhq".into(), json!({"type": "http", "url": url}));
    }
    if let Some(url) = &l.orchestrate_url {
        servers.insert(crate::orchestrator::SERVER_NAME.into(), json!({"type": "http", "url": url}));
    }
    (!servers.is_empty()).then(|| json!({ "mcpServers": servers }).to_string())
}

pub fn launch_args(l: &Launch) -> Vec<String> {
    let mut a: Vec<String> = [
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--include-partial-messages",
        // Approvals come to us as can_use_tool control requests.
        "--permission-prompt-tool",
        "stdio",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    a.push("--model".into());
    a.push(l.model.clone());
    if let Some(e) = &l.effort {
        a.push("--effort".into());
        a.push(e.clone());
    }
    if let Some(s) = &l.resume {
        a.push("--resume".into());
        a.push(s.clone());
    }
    if let Some(cfg) = mcp_config(l) {
        a.push("--mcp-config".into());
        a.push(cfg);
    }
    a.push("--append-system-prompt".into());
    if l.orchestrate_url.is_some() {
        // The engine's own tools need no approval; what workers do still does.
        a.push(format!("{SYSTEM_NOTE}\n\n{}", crate::orchestrator::GUIDANCE));
        a.push("--allowedTools".into());
        a.push(format!("mcp__{}", crate::orchestrator::SERVER_NAME));
    } else {
        a.push(SYSTEM_NOTE.into());
    }
    a
}

// ---------- The stream ----------

/// A `can_use_tool` request kept until it's answered.
#[derive(Clone, Debug)]
struct PendingTool {
    tool_name: String,
    input: Value,
    suggestions: Option<Value>,
}

/// Turns stream-json lines into driver events (and, for control requests
/// we don't handle, the reply to write back).
#[derive(Default)]
pub struct Mapper {
    /// message id → (text blocks seen, thinking blocks seen) in the stream
    stream_counts: HashMap<String, (usize, usize)>,
    /// message id → the same, counted from whole `assistant` messages
    final_counts: HashMap<String, (usize, usize)>,
    /// content block index → (key, is_thinking) for the message streaming now
    blocks: HashMap<u64, (String, bool)>,
    current_msg: String,
    pending: Arc<Mutex<HashMap<String, PendingTool>>>,
}

fn key(msg: &str, thinking: bool, n: usize) -> String {
    format!("{msg}:{}{n}", if thinking { "r" } else { "t" })
}

pub fn usage_of(result: &Value) -> Option<Usage> {
    let u = result.get("usage")?;
    let n = |k: &str| u.get(k).and_then(Value::as_u64).unwrap_or(0);
    Some(Usage {
        input_tokens: n("input_tokens") + n("cache_creation_input_tokens") + n("cache_read_input_tokens"),
        output_tokens: n("output_tokens"),
        cost_usd: result.get("total_cost_usd").and_then(Value::as_f64),
    })
}

impl Mapper {
    pub fn new() -> Mapper {
        Mapper::default()
    }

    fn shared(pending: Arc<Mutex<HashMap<String, PendingTool>>>) -> Mapper {
        Mapper { pending, ..Mapper::default() }
    }

    /// Events for one line, and lines to write back to the CLI.
    pub fn line(&mut self, line: &str) -> (Vec<DriverEvent>, Vec<String>) {
        let Ok(v) = serde_json::from_str::<Value>(line) else { return (vec![], vec![]) };
        let mut out = Vec::new();
        let mut replies = Vec::new();
        let main = v.get("parent_tool_use_id").is_none_or(Value::is_null);
        match v.get("type").and_then(Value::as_str).unwrap_or("") {
            "system" if v.get("subtype").and_then(Value::as_str) == Some("init") => {
                if let Some(s) = v.get("session_id").and_then(Value::as_str) {
                    out.push(DriverEvent::Session(s.to_string()));
                }
            }
            "stream_event" if main => self.stream_event(v.get("event").unwrap_or(&Value::Null), &mut out),
            "assistant" if main => {
                let msg = v.pointer("/message/id").and_then(Value::as_str).unwrap_or("").to_string();
                let blocks = v.pointer("/message/content").and_then(Value::as_array).cloned().unwrap_or_default();
                for b in blocks {
                    match b.get("type").and_then(Value::as_str) {
                        Some("text") => {
                            let c = self.final_counts.entry(msg.clone()).or_default();
                            let k = key(&msg, false, c.0);
                            c.0 += 1;
                            out.push(DriverEvent::TextDone { key: k, text: b.get("text").and_then(Value::as_str).unwrap_or("").to_string() });
                        }
                        Some("thinking") => {
                            let c = self.final_counts.entry(msg.clone()).or_default();
                            let k = key(&msg, true, c.1);
                            c.1 += 1;
                            out.push(DriverEvent::ReasoningDone { key: k, text: b.get("thinking").and_then(Value::as_str).unwrap_or("").to_string() });
                        }
                        Some("tool_use") => out.push(DriverEvent::ToolStarted {
                            key: b.get("id").and_then(Value::as_str).unwrap_or("").to_string(),
                            name: b.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
                            input: b.get("input").cloned().unwrap_or(Value::Null),
                        }),
                        _ => {}
                    }
                }
            }
            "user" if main => {
                if let Some(blocks) = v.pointer("/message/content").and_then(Value::as_array) {
                    for b in blocks.iter().filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_result")) {
                        out.push(DriverEvent::ToolFinished {
                            key: b.get("tool_use_id").and_then(Value::as_str).unwrap_or("").to_string(),
                            output: output_text(b.get("content").unwrap_or(&Value::Null)),
                            failed: b.get("is_error").and_then(Value::as_bool).unwrap_or(false),
                            declined: false,
                        });
                    }
                }
            }
            "result" => {
                let ok = v.get("subtype").and_then(Value::as_str) == Some("success") && !v.get("is_error").and_then(Value::as_bool).unwrap_or(false);
                let error = if ok {
                    None
                } else {
                    let errors = v.get("errors").and_then(Value::as_array).map(|e| e.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" ")).filter(|e| !e.is_empty());
                    Some(v.get("result").and_then(Value::as_str).map(str::to_string).filter(|r| !r.is_empty()).or(errors).unwrap_or_else(|| "Claude Code stopped with an error.".into()))
                };
                out.push(DriverEvent::TurnDone { status: if ok { TurnStatus::Done } else { TurnStatus::Failed }, error, usage: usage_of(&v) });
            }
            "control_request" => {
                let id = v.get("request_id").and_then(Value::as_str).unwrap_or("").to_string();
                let req = v.get("request").cloned().unwrap_or(Value::Null);
                if req.get("subtype").and_then(Value::as_str) == Some("can_use_tool") {
                    let name = req.get("tool_name").and_then(Value::as_str).unwrap_or("").to_string();
                    let input = req.get("input").cloned().unwrap_or(json!({}));
                    if let Ok(mut p) = self.pending.lock() {
                        p.insert(id.clone(), PendingTool { tool_name: name.clone(), input: input.clone(), suggestions: req.get("permission_suggestions").cloned() });
                    }
                    out.push(DriverEvent::Approval {
                        native: id,
                        tool_key: req.get("tool_use_id").and_then(Value::as_str).map(str::to_string),
                        name,
                        input,
                        options: vec![Decision::Allow, Decision::AllowAlways, Decision::Deny],
                    });
                } else {
                    replies.push(json!({"type":"control_response","response":{"subtype":"error","request_id":id,"error":"Billable doesn't handle this request."}}).to_string());
                }
            }
            // Our `initialize`, sent as the session starts: its model list.
            "control_response" if v.pointer("/response/request_id").and_then(Value::as_str) == Some(INIT_REQUEST) => {
                if let Some((models, default)) = v.pointer("/response/response").and_then(models_from_initialize) {
                    out.push(DriverEvent::Models(models, default));
                }
            }
            "control_cancel_request" => {
                if let Some(id) = v.get("request_id").and_then(Value::as_str) {
                    if let Ok(mut p) = self.pending.lock() {
                        p.remove(id);
                    }
                    out.push(DriverEvent::ApprovalWithdrawn { native: id.to_string() });
                }
            }
            _ => {}
        }
        (out, replies)
    }

    fn stream_event(&mut self, e: &Value, out: &mut Vec<DriverEvent>) {
        match e.get("type").and_then(Value::as_str).unwrap_or("") {
            "message_start" => {
                self.current_msg = e.pointer("/message/id").and_then(Value::as_str).unwrap_or("").to_string();
                self.blocks.clear();
            }
            "content_block_start" => {
                let index = e.get("index").and_then(Value::as_u64).unwrap_or(0);
                let block = e.get("content_block").cloned().unwrap_or(Value::Null);
                match block.get("type").and_then(Value::as_str) {
                    Some(t @ ("text" | "thinking")) => {
                        let thinking = t == "thinking";
                        let c = self.stream_counts.entry(self.current_msg.clone()).or_default();
                        let n = if thinking { &mut c.1 } else { &mut c.0 };
                        let k = key(&self.current_msg, thinking, *n);
                        *n += 1;
                        self.blocks.insert(index, (k, thinking));
                    }
                    Some("tool_use") => out.push(DriverEvent::ToolStarted {
                        key: block.get("id").and_then(Value::as_str).unwrap_or("").to_string(),
                        name: block.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
                        input: json!({}),
                    }),
                    _ => {}
                }
            }
            "content_block_delta" => {
                let index = e.get("index").and_then(Value::as_u64).unwrap_or(0);
                let Some((k, _)) = self.blocks.get(&index) else { return };
                let d = e.get("delta").cloned().unwrap_or(Value::Null);
                match d.get("type").and_then(Value::as_str) {
                    Some("text_delta") => {
                        if let Some(t) = d.get("text").and_then(Value::as_str).filter(|t| !t.is_empty()) {
                            out.push(DriverEvent::TextDelta { key: k.clone(), text: t.to_string() });
                        }
                    }
                    Some("thinking_delta") => {
                        if let Some(t) = d.get("thinking").and_then(Value::as_str).filter(|t| !t.is_empty()) {
                            out.push(DriverEvent::ReasoningDelta { key: k.clone(), text: t.to_string() });
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

/// The control response for an approval.
fn answer_line(request_id: &str, decision: Decision, pending: Option<&PendingTool>) -> String {
    let response = match decision {
        Decision::Deny => json!({"behavior": "deny", "message": "The user declined this step in FeedHQ."}),
        Decision::Allow | Decision::AllowAlways => {
            let mut r = json!({"behavior": "allow", "updatedInput": pending.map(|p| p.input.clone()).unwrap_or(json!({}))});
            if decision == Decision::AllowAlways {
                // For this session only: the user's settings files are never written.
                let mut updates: Vec<Value> = pending
                    .and_then(|p| p.suggestions.as_ref())
                    .and_then(Value::as_array)
                    .map(|s| {
                        s.iter()
                            .filter(|u| u.get("type").and_then(Value::as_str) != Some("setMode"))
                            .map(|u| {
                                let mut u = u.clone();
                                u["destination"] = json!("session");
                                u
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                if updates.is_empty() {
                    if let Some(p) = pending {
                        updates.push(json!({"type": "addRules", "rules": [{"toolName": p.tool_name}], "behavior": "allow", "destination": "session"}));
                    }
                }
                r["updatedPermissions"] = Value::Array(updates);
            }
            r
        }
    };
    json!({"type": "control_response", "response": {"subtype": "success", "request_id": request_id, "response": response}}).to_string()
}

pub fn user_message_line(text: &str) -> String {
    json!({"type": "user", "message": {"role": "user", "content": text}, "parent_tool_use_id": null, "session_id": ""}).to_string()
}

// ---------- The process ----------

pub type Sink = Arc<dyn Fn(DriverEvent) + Send + Sync>;

pub struct Session {
    child: Child,
    stdin: Arc<Mutex<ChildStdin>>,
    pending: Arc<Mutex<HashMap<String, PendingTool>>>,
    pub launch: Launch,
    next_request: u64,
}

fn write_line(stdin: &Mutex<ChildStdin>, line: &str) -> std::io::Result<()> {
    let mut s = stdin.lock().map_err(|_| std::io::Error::other("stdin poisoned"))?;
    s.write_all(line.as_bytes())?;
    s.write_all(b"\n")?;
    s.flush()
}

impl Session {
    pub fn start(bin: &Path, cwd: &Path, launch: Launch, sink: Sink) -> anyhow::Result<Session> {
        let mut cmd = agent_command(bin, cwd);
        // wait_for_agents can take up to 30 minutes; the person's own
        // setting, if they made one, stands.
        if launch.orchestrate_url.is_some() && std::env::var_os("MCP_TOOL_TIMEOUT").is_none() {
            cmd.env("MCP_TOOL_TIMEOUT", "1900000");
        }
        let mut child = cmd.args(launch_args(&launch)).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
        let stdin = Arc::new(Mutex::new(child.stdin.take().ok_or_else(|| anyhow::anyhow!("no stdin"))?));
        let stdout = child.stdout.take().ok_or_else(|| anyhow::anyhow!("no stdout"))?;
        let stderr = tail_stderr(child.stderr.take().ok_or_else(|| anyhow::anyhow!("no stderr"))?);
        let pending: Arc<Mutex<HashMap<String, PendingTool>>> = Arc::default();
        let mut mapper = Mapper::shared(pending.clone());
        let (reply_to, line_sink, end_sink) = (stdin.clone(), sink.clone(), sink);
        read_lines(
            stdout,
            move |line| {
                let (events, replies) = mapper.line(&line);
                for r in replies {
                    let _ = write_line(&reply_to, &r);
                }
                for e in events {
                    line_sink(e);
                }
            },
            move || {
                // Give stderr a moment to arrive before reading its tail.
                std::thread::sleep(Duration::from_millis(100));
                let tail = stderr.lock().map(|t| t.clone()).unwrap_or_default();
                end_sink(DriverEvent::Exited { error: last_error_line(&tail) });
            },
        );
        let session = Session { child, stdin, pending, launch, next_request: 0 };
        write_line(&session.stdin, &json!({"type":"control_request","request_id":INIT_REQUEST,"request":{"subtype":"initialize"}}).to_string())?;
        Ok(session)
    }

    pub fn send(&mut self, text: &str) -> anyhow::Result<()> {
        write_line(&self.stdin, &user_message_line(text))?;
        Ok(())
    }

    pub fn interrupt(&mut self) -> anyhow::Result<()> {
        self.next_request += 1;
        let line = json!({"type":"control_request","request_id":format!("stop{}", self.next_request),"request":{"subtype":"interrupt"}}).to_string();
        write_line(&self.stdin, &line)?;
        Ok(())
    }

    pub fn answer(&mut self, native: &str, decision: Decision) -> anyhow::Result<()> {
        let pending = self.pending.lock().ok().and_then(|mut p| p.remove(native));
        write_line(&self.stdin, &answer_line(native, decision, pending.as_ref()))?;
        Ok(())
    }

    pub fn alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    pub fn stop(&mut self) {
        kill(&mut self.child);
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(m: &mut Mapper, input: &str) -> Vec<DriverEvent> {
        input.lines().flat_map(|l| m.line(l).0).collect()
    }

    /// Recorded from Claude Code 2.1.332 (trimmed): a Bash call with an
    /// approval, then a text reply, streamed with partial messages.
    const TRANSCRIPT: &str = r#"{"type":"system","subtype":"init","cwd":"/w","session_id":"s-1","tools":["Bash"],"mcp_servers":[],"model":"claude-haiku-5-5","permissionMode":"default"}
{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_1","type":"message","role":"assistant","content":[]}},"session_id":"s-1","parent_tool_use_id":null}
{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}},"session_id":"s-1","parent_tool_use_id":null}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"Need to run it"}},"session_id":"s-1","parent_tool_use_id":null}
{"type":"stream_event","event":{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"toolu_1","name":"Bash","input":{}}},"session_id":"s-1","parent_tool_use_id":null}
{"type":"stream_event","event":{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"command\": \"echo hi"}},"session_id":"s-1","parent_tool_use_id":null}
{"type":"assistant","message":{"id":"msg_1","content":[{"type":"thinking","thinking":"Need to run it","signature":"x"}]},"parent_tool_use_id":null,"session_id":"s-1"}
{"type":"assistant","message":{"id":"msg_1","content":[{"type":"tool_use","id":"toolu_1","name":"Bash","input":{"command":"echo hi > out.txt","description":"Write hi to out.txt"}}]},"parent_tool_use_id":null,"session_id":"s-1"}
{"type":"control_request","request_id":"req-9","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"echo hi > out.txt"},"permission_suggestions":[{"type":"addRules","rules":[{"toolName":"Bash","ruleContent":"echo hi *"}],"behavior":"allow","destination":"localSettings"}],"tool_use_id":"toolu_1"}}
{"type":"user","message":{"role":"user","content":[{"tool_use_id":"toolu_1","type":"tool_result","content":"(Bash completed with no output)","is_error":false}]},"parent_tool_use_id":null,"session_id":"s-1"}
{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_2","content":[]}},"parent_tool_use_id":null}
{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}},"parent_tool_use_id":null}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Do"}},"parent_tool_use_id":null}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"ne."}},"parent_tool_use_id":null}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"sub"}},"parent_tool_use_id":"toolu_9"}
{"type":"assistant","message":{"id":"msg_2","content":[{"type":"text","text":"Done."}]},"parent_tool_use_id":null,"session_id":"s-1"}
{"type":"rate_limit_event","rate_limit_info":{"status":"allowed"}}
{"type":"result","subtype":"success","is_error":false,"result":"Done.","session_id":"s-1","total_cost_usd":0.0086,"usage":{"input_tokens":4,"cache_creation_input_tokens":41001,"cache_read_input_tokens":40871,"output_tokens":96}}"#;

    #[test]
    fn maps_a_recorded_turn() {
        let mut m = Mapper::new();
        let events = lines(&mut m, TRANSCRIPT);
        let expected = vec![
            DriverEvent::Session("s-1".into()),
            DriverEvent::ReasoningDelta { key: "msg_1:r0".into(), text: "Need to run it".into() },
            DriverEvent::ToolStarted { key: "toolu_1".into(), name: "Bash".into(), input: json!({}) },
            DriverEvent::ReasoningDone { key: "msg_1:r0".into(), text: "Need to run it".into() },
            DriverEvent::ToolStarted { key: "toolu_1".into(), name: "Bash".into(), input: json!({"command":"echo hi > out.txt","description":"Write hi to out.txt"}) },
            DriverEvent::Approval {
                native: "req-9".into(),
                tool_key: Some("toolu_1".into()),
                name: "Bash".into(),
                input: json!({"command":"echo hi > out.txt"}),
                options: vec![Decision::Allow, Decision::AllowAlways, Decision::Deny],
            },
            DriverEvent::ToolFinished { key: "toolu_1".into(), output: Some("(Bash completed with no output)".into()), failed: false, declined: false },
            DriverEvent::TextDelta { key: "msg_2:t0".into(), text: "Do".into() },
            DriverEvent::TextDelta { key: "msg_2:t0".into(), text: "ne.".into() },
            DriverEvent::TextDone { key: "msg_2:t0".into(), text: "Done.".into() },
            DriverEvent::TurnDone { status: TurnStatus::Done, error: None, usage: Some(Usage { input_tokens: 81876, output_tokens: 96, cost_usd: Some(0.0086) }) },
        ];
        assert_eq!(events, expected);
    }

    #[test]
    fn approval_answers() {
        let mut m = Mapper::new();
        let pending = m.pending.clone();
        lines(&mut m, TRANSCRIPT);
        let p = pending.lock().unwrap().get("req-9").cloned().unwrap();
        let allow: Value = serde_json::from_str(&answer_line("req-9", Decision::Allow, Some(&p))).unwrap();
        assert_eq!(allow, json!({"type":"control_response","response":{"subtype":"success","request_id":"req-9","response":{"behavior":"allow","updatedInput":{"command":"echo hi > out.txt"}}}}));
        let always: Value = serde_json::from_str(&answer_line("req-9", Decision::AllowAlways, Some(&p))).unwrap();
        assert_eq!(always.pointer("/response/response/updatedPermissions/0/destination"), Some(&json!("session")));
        assert_eq!(always.pointer("/response/response/updatedPermissions/0/rules/0/ruleContent"), Some(&json!("echo hi *")));
        let deny: Value = serde_json::from_str(&answer_line("req-9", Decision::Deny, Some(&p))).unwrap();
        assert_eq!(deny.pointer("/response/response/behavior"), Some(&json!("deny")));
        // No suggestions: allow the tool for the session
        let bare = PendingTool { tool_name: "Edit".into(), input: json!({}), suggestions: None };
        let always: Value = serde_json::from_str(&answer_line("r", Decision::AllowAlways, Some(&bare))).unwrap();
        assert_eq!(always.pointer("/response/response/updatedPermissions/0/rules/0/toolName"), Some(&json!("Edit")));
    }

    #[test]
    fn other_control_requests_get_an_error_back() {
        let mut m = Mapper::new();
        let (events, replies) = m.line(r#"{"type":"control_request","request_id":"h1","request":{"subtype":"hook_callback"}}"#);
        assert!(events.is_empty());
        let r: Value = serde_json::from_str(&replies[0]).unwrap();
        assert_eq!(r.pointer("/response/subtype"), Some(&json!("error")));
        assert_eq!(r.pointer("/response/request_id"), Some(&json!("h1")));
        let (events, _) = m.line(r#"{"type":"control_cancel_request","request_id":"req-1"}"#);
        assert_eq!(events, vec![DriverEvent::ApprovalWithdrawn { native: "req-1".into() }]);
    }

    #[test]
    fn failed_results() {
        let mut m = Mapper::new();
        let e = lines(&mut m, r#"{"type":"result","subtype":"error_during_execution","is_error":true,"errors":["boom"],"usage":{"input_tokens":1,"output_tokens":0}}"#);
        assert!(matches!(&e[0], DriverEvent::TurnDone { status: TurnStatus::Failed, error: Some(msg), .. } if msg == "boom"));
        let e = lines(&mut m, r#"{"type":"result","subtype":"success","is_error":true,"result":"API Error: 529 overloaded"}"#);
        assert!(matches!(&e[0], DriverEvent::TurnDone { status: TurnStatus::Failed, error: Some(msg), .. } if msg.contains("529")));
        assert!(lines(&mut m, "not json").is_empty());
    }

    #[test]
    fn launch_flags() {
        let a = launch_args(&Launch { model: "opus".into(), effort: Some("high".into()), resume: Some("s-1".into()), mcp_url: Some("http://127.0.0.1:9/mcp/x".into()), orchestrate_url: None });
        let joined = a.join(" ");
        for f in ["-p", "--input-format stream-json", "--output-format stream-json", "--verbose", "--include-partial-messages", "--permission-prompt-tool stdio", "--model opus", "--effort high", "--resume s-1"] {
            assert!(joined.contains(f), "{f} missing from {joined}");
        }
        let i = a.iter().position(|x| x == "--mcp-config").unwrap();
        let cfg: Value = serde_json::from_str(&a[i + 1]).unwrap();
        assert_eq!(cfg, json!({"mcpServers":{"feedhq":{"type":"http","url":"http://127.0.0.1:9/mcp/x"}}}));
        assert!(!a.iter().any(|x| x == "--allowedTools"));
        let a = launch_args(&Launch { model: "haiku".into(), effort: None, resume: None, mcp_url: None, orchestrate_url: None });
        assert!(!a.iter().any(|x| x == "--effort" || x == "--resume" || x == "--mcp-config"));
        // An orchestrator: both servers, the guidance, its own tools allowed
        let a = launch_args(&Launch { model: "opus".into(), effort: None, resume: None, mcp_url: Some("http://h/mcp/x".into()), orchestrate_url: Some("http://h/mcp/x/orchestrate/t_1".into()) });
        let i = a.iter().position(|x| x == "--mcp-config").unwrap();
        let cfg: Value = serde_json::from_str(&a[i + 1]).unwrap();
        assert_eq!(cfg["mcpServers"]["billable"], json!({"type":"http","url":"http://h/mcp/x/orchestrate/t_1"}));
        assert_eq!(cfg["mcpServers"]["feedhq"]["url"], json!("http://h/mcp/x"));
        let i = a.iter().position(|x| x == "--append-system-prompt").unwrap();
        assert!(a[i + 1].contains("start_agents") && a[i + 1].starts_with(SYSTEM_NOTE));
        assert!(a.windows(2).any(|w| w == ["--allowedTools", "mcp__billable"]));
    }

    #[test]
    fn models_versions_and_auth() {
        let init = json!({"models":[
            {"value":"default","resolvedModel":"claude-opus-5-5","displayName":"Default (recommended)","supportsEffort":true,"supportedEffortLevels":["low","medium","high"]},
            {"value":"sonnet","resolvedModel":"claude-sonnet-5-5","displayName":"Sonnet 5.5","supportsEffort":true,"supportedEffortLevels":["low","medium","high","xhigh","max"]},
            {"value":"opus","resolvedModel":"claude-opus-5-5","displayName":"Opus 5.5","supportsEffort":true,"supportedEffortLevels":["low","medium","high","xhigh","max"]},
            {"value":"claude-haiku-4-5-20251001","resolvedModel":"claude-haiku-4-5-20251001","displayName":"Haiku 4.5"},
            {"value":"haiku","resolvedModel":"claude-haiku-5-5","displayName":"Haiku 5.5"}
        ]});
        let (models, default) = models_from_initialize(&init).unwrap();
        assert_eq!(models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), vec!["sonnet", "opus", "haiku"]);
        assert_eq!(default, "opus");
        assert_eq!(models[0].default_effort.as_deref(), Some("medium"));
        assert!(models[2].efforts.is_empty() && models[2].default_effort.is_none());
        assert!(models_from_initialize(&json!({})).is_none());
        assert_eq!(parse_version("2.1.332 (Claude Code)\n").as_deref(), Some("2.1.332"));
        assert_eq!(parse_version("error"), None);
        assert_eq!(parse_auth_status("{\"loggedIn\": true, \"authMethod\":\"oauth_token\"}"), Some(true));
        assert_eq!(parse_auth_status("{\"loggedIn\": false}"), Some(false));
        assert_eq!(parse_auth_status("Not logged in"), None);
        assert_eq!(fallback_models()[0].id, "opus");
    }
}
