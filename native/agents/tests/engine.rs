//! The engine end to end, against fake Claude Code and Codex CLIs
//! (tests/fakes/*.py) and a fake FeedHQ token endpoint.

use std::{
    io::BufReader,
    net::TcpListener,
    path::{Path, PathBuf},
    sync::{mpsc::Receiver, Arc},
    time::{Duration, Instant},
};

use billable_agents::{
    http::{read_request, write_response},
    remote::RemoteEngine,
    Engine, EngineConfig, MemorySecrets, SecretStore,
};
use serde_json::{json, Value};

const WAIT: Duration = Duration::from_secs(20);

struct Harness {
    engine: Engine,
    rx: Receiver<String>,
    /// Every event seen so far, in order.
    events: Vec<Value>,
    next: u32,
    dir: PathBuf,
}

fn python_available() -> bool {
    std::process::Command::new("python3").arg("--version").output().is_ok_and(|o| o.status.success())
}

fn install_fake(dir: &Path, name: &str) -> PathBuf {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fakes").join(format!("{name}.py"));
    let dst = dir.join(name);
    std::fs::copy(&src, &dst).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dst, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    dst
}

/// A fake FeedHQ that hands out tokens.
fn fake_feedhq() -> String {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", l.local_addr().unwrap());
    let issuer = origin.clone();
    std::thread::spawn(move || {
        let mut n = 0;
        for s in l.incoming() {
            let Ok(s) = s else { continue };
            let mut r = BufReader::new(s.try_clone().unwrap());
            let mut w = s;
            while let Ok(Some(req)) = read_request(&mut r) {
                n += 1;
                let body = if req.path == "/api/oauth/token" {
                    format!(r#"{{"access_token":"fhq_at_{n}","expires_in":3600,"refresh_token":"fhq_rt_{n}"}}"#)
                } else if req.path == "/.well-known/oauth-authorization-server" {
                    json!({ "issuer": issuer }).to_string()
                } else {
                    "{}".to_string()
                };
                let _ = write_response(&mut w, 200, &[("Content-Type", "application/json")], body.as_bytes(), true);
            }
        }
    });
    origin
}

fn start(dir: &Path, origin: &str, secrets: Arc<dyn SecretStore>) -> (Engine, Receiver<String>) {
    let mut cfg = EngineConfig::new(dir.join("agents.db"), dir.join("work"), origin, secrets);
    cfg.claude_bin = Some(dir.join("claude"));
    cfg.codex_bin = Some(dir.join("codex"));
    cfg.login_shell = false;
    cfg.device_name = Some("Test Mac".into());
    Engine::new(cfg).unwrap()
}

impl Harness {
    fn new(dir: PathBuf, origin: &str, secrets: Arc<dyn SecretStore>) -> Harness {
        let (engine, rx) = start(&dir, origin, secrets);
        Harness { engine, rx, events: Vec::new(), next: 0, dir }
    }

    /// Send a request and wait for its reply, keeping events on the way.
    fn call(&mut self, method: &str, params: Value) -> Value {
        self.next += 1;
        let id = format!("r{}", self.next);
        self.engine.handle(&json!({"v":1,"type":"request","id":id,"method":method,"params":params}).to_string());
        let until = Instant::now() + WAIT;
        loop {
            let msg = self.rx.recv_timeout(until.saturating_duration_since(Instant::now())).unwrap_or_else(|_| panic!("no reply to {method}"));
            let v: Value = serde_json::from_str(&msg).unwrap();
            match v["type"].as_str() {
                Some("reply") if v["id"] == json!(id) => return v,
                Some("event") => self.events.push(v["event"].clone()),
                _ => {}
            }
        }
    }

    fn ok(&mut self, method: &str, params: Value) -> Value {
        let r = self.call(method, params);
        assert_eq!(r["ok"], json!(true), "{method}: {r}");
        r["result"].clone()
    }

    /// Wait for an event matching `pred` (seen already, after `from`, or new).
    fn wait_event(&mut self, from: usize, pred: impl Fn(&Value) -> bool) -> Value {
        if let Some(e) = self.events[from.min(self.events.len())..].iter().find(|e| pred(e)) {
            return e.clone();
        }
        let until = Instant::now() + WAIT;
        loop {
            let msg = self.rx.recv_timeout(until.saturating_duration_since(Instant::now())).unwrap_or_else(|_| panic!("event never came; saw {:#?}", &self.events[from.min(self.events.len())..]));
            let v: Value = serde_json::from_str(&msg).unwrap();
            if v["type"] == "event" {
                let e = v["event"].clone();
                self.events.push(e.clone());
                if pred(&e) {
                    return e;
                }
            }
        }
    }

    fn finished(&mut self, from: usize, thread: &str) -> Value {
        let thread = thread.to_string();
        self.wait_event(from, |e| e["type"] == "turn.finished" && e["threadId"] == json!(thread))
    }

    fn items(&mut self, thread: &str) -> Vec<Value> {
        self.ok("thread.get", json!({"threadId": thread}))["items"].as_array().unwrap().clone()
    }

    fn log(&self, name: &str) -> Vec<Value> {
        std::fs::read_to_string(self.dir.join(format!("{name}.log"))).unwrap_or_default().lines().map(|l| serde_json::from_str(l).unwrap()).collect()
    }
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("billable-agents-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn chats_end_to_end_with_fake_agents() {
    if !python_available() {
        eprintln!("python3 isn't available; skipping");
        return;
    }
    let dir = temp_dir("e2e");
    install_fake(&dir, "claude");
    install_fake(&dir, "codex");
    let origin = fake_feedhq();
    let secrets: Arc<dyn SecretStore> = Arc::new(MemorySecrets::default());
    let mut h = Harness::new(dir.clone(), &origin, secrets.clone());

    // hello waits for the agents to be found
    let hello = h.ok("hello", json!({"protocol": 1}));
    assert_eq!(hello["engine"], "mac");
    assert_eq!(hello["device"], "Test Mac");
    assert_eq!(hello["feedhq"]["connected"], false);
    let agents = hello["agents"].as_array().unwrap();
    assert_eq!(agents.iter().map(|a| a["id"].as_str().unwrap()).collect::<Vec<_>>(), vec!["claude", "codex"]);
    assert!(agents.iter().all(|a| a["installed"] == true && a["signedIn"] == true), "{agents:?}");
    assert_eq!(agents[0]["version"], "2.1.332");

    // Bad requests
    let r = h.call("nope", json!({}));
    assert_eq!((r["ok"].clone(), r["error"]["code"].clone()), (json!(false), json!("bad_request")));
    let r = h.call("turn.send", json!({"threadId": "t_missing", "text": "hi"}));
    assert_eq!(r["error"]["code"], "not_found");

    // A Claude chat: streaming, usage, a title from its first words, and the
    // models Claude Code listed when the session came up
    let thread = h.ok("thread.create", json!({"agent": "claude", "model": "opus"}))["thread"].clone();
    let tid = thread["id"].as_str().unwrap().to_string();
    let mark = h.events.len();
    let sent = h.ok("turn.send", json!({"threadId": tid, "text": "Plan my pottery launch"}));
    assert_eq!(sent["userItem"]["kind"], "user");
    let done = h.finished(mark, &tid);
    assert_eq!(done["status"], "done", "{done}");
    assert_eq!(done["usage"]["inputTokens"], 15);
    assert_eq!(done["usage"]["costUsd"], 0.0123);
    let deltas: String = h.events[mark..].iter().filter(|e| e["type"] == "item.delta").map(|e| e["delta"].as_str().unwrap().to_string()).collect();
    assert_eq!(deltas, "Hello there");
    let items = h.items(&tid);
    assert_eq!(items.iter().map(|i| i["kind"].as_str().unwrap()).collect::<Vec<_>>(), vec!["user", "assistant"]);
    assert_eq!(items[1]["text"], "Hello there");
    assert_eq!(items[1]["agent"], "claude");
    assert_eq!(h.ok("thread.get", json!({"threadId": tid}))["thread"]["title"], "Plan my pottery launch");
    let models = h.wait_event(0, |e| e["type"] == "agents.changed" && e["agents"][0]["models"].as_array().is_some_and(|m| m.len() == 1))["agents"][0]["models"].clone();
    assert_eq!(models[0]["efforts"], json!(["low", "medium", "high"]));
    assert_eq!(h.call("thread.retitle", json!({"threadId": tid}))["error"], json!({"code": "bad_request", "message": "Rename it instead."}));
    // Nothing but the user's own session ran: no title or probe calls.
    assert!(h.log("claude").iter().all(|l| l["kind"] != "title"));
    let launches: Vec<Value> = h.log("claude").into_iter().filter(|l| l["kind"] == "launch").collect();
    assert_eq!(launches.len(), 1);
    let args: Vec<String> = serde_json::from_value(launches[0]["value"].clone()).unwrap();
    assert!(args.windows(2).any(|w| w == ["--model", "opus"]), "{args:?}");
    assert!(!args.contains(&"--mcp-config".to_string()), "FeedHQ isn't connected yet");

    // An approval: the chat waits, then the tool runs
    let mark = h.events.len();
    h.ok("turn.send", json!({"threadId": tid, "text": "please use a tool"}));
    let asked = h.wait_event(mark, |e| e["type"] == "approval.requested")["approval"].clone();
    assert_eq!(asked["title"], "Run a command");
    assert_eq!(h.ok("thread.get", json!({"threadId": tid}))["thread"]["status"], "waiting");
    let r = h.call("approval.answer", json!({"threadId": tid, "approvalId": "a_wrong", "decision": "allow"}));
    assert_eq!(r["error"]["code"], "not_found");
    h.ok("approval.answer", json!({"threadId": tid, "approvalId": asked["id"], "decision": "allow"}));
    assert_eq!(h.finished(mark, &tid)["status"], "done");
    let tool = h.items(&tid).into_iter().find(|i| i["kind"] == "tool").unwrap();
    assert_eq!(tool["tool"]["status"], "done", "{tool}");
    assert_eq!(tool["tool"]["output"], "a.txt");
    assert_eq!(tool["tool"]["feedhq"], false);
    assert!(h.log("claude").iter().any(|l| l["value"]["response"]["response"]["behavior"] == "allow"));
    // One process served both turns.
    assert_eq!(h.log("claude").iter().filter(|l| l["kind"] == "launch").count(), 1);

    // A second message while one is running is refused; Stop ends the turn
    let mark = h.events.len();
    h.ok("turn.send", json!({"threadId": tid, "text": "wait forever"}));
    h.wait_event(mark, |e| e["type"] == "item.delta");
    assert_eq!(h.call("turn.send", json!({"threadId": tid, "text": "again"}))["error"]["code"], "busy");
    h.ok("turn.stop", json!({"threadId": tid}));
    assert_eq!(h.finished(mark, &tid)["status"], "stopped");
    assert_eq!(h.ok("thread.get", json!({"threadId": tid}))["thread"]["status"], "idle");

    // Switching to Codex: a divider, a handoff, a denied command
    let mark = h.events.len();
    h.ok("turn.send", json!({"threadId": tid, "text": "Now a tool, Codex", "agent": "codex", "model": "gpt-6.1-sol"}));
    let asked = h.wait_event(mark, |e| e["type"] == "approval.requested")["approval"].clone();
    h.ok("approval.answer", json!({"threadId": tid, "approvalId": asked["id"], "decision": "deny"}));
    let done = h.finished(mark, &tid);
    assert_eq!(done["status"], "done", "{done}");
    assert_eq!(done["usage"]["outputTokens"], 4);
    let items = h.items(&tid);
    let divider = items.iter().find(|i| i["kind"] == "divider").unwrap();
    assert_eq!(divider["divider"]["from"]["agent"], "claude");
    assert_eq!(divider["divider"]["to"]["agent"], "codex");
    let denied = items.iter().rev().find(|i| i["kind"] == "tool").unwrap();
    assert_eq!(denied["tool"]["status"], "denied", "{denied}");
    assert_eq!(items.last().unwrap()["text"], "Codex took over");
    let codex_in = h.log("codex");
    let turn = codex_in.iter().find(|l| l["value"]["method"] == "turn/start").unwrap();
    let prompt = turn["value"]["params"]["input"][0]["text"].as_str().unwrap();
    assert!(prompt.contains("<feedhq-handoff>") && prompt.contains("Plan my pottery launch") && prompt.ends_with("Now a tool, Codex"), "{prompt}");
    assert!(codex_in.iter().any(|l| l["value"]["id"] == 900 && l["value"]["result"]["decision"] == "decline"));
    let thread_now = h.ok("thread.get", json!({"threadId": tid}))["thread"].clone();
    assert_eq!((thread_now["agent"].as_str(), thread_now["model"].as_str()), (Some("codex"), Some("gpt-6.1-sol")));

    // A Codex chat's title is its first words too
    let cx = h.ok("thread.create", json!({"agent": "codex", "model": "gpt-6.1-sol"}))["thread"]["id"].as_str().unwrap().to_string();
    let mark = h.events.len();
    h.ok("turn.send", json!({"threadId": cx, "text": "Draft seven posts about the spring sale please"}));
    assert_eq!(h.finished(mark, &cx)["status"], "done");
    assert_eq!(h.ok("thread.get", json!({"threadId": cx}))["thread"]["title"], "Draft seven posts about the spring");
    assert!(!h.log("codex").iter().any(|l| l["kind"] == "exec"), "no codex exec");
    let init = h.log("codex").into_iter().find(|l| l["value"]["method"] == "initialize").unwrap();
    assert_eq!(init["value"]["params"]["clientInfo"]["name"], "billable");

    // FeedHQ sign-in: the authorize URL, the redirect, then agents get the relay
    let url = h.ok("feedhq.connect", json!({}))["authorizeUrl"].as_str().unwrap().to_string();
    assert!(url.starts_with(&format!("{origin}/")), "{url}");
    assert!(url.contains("client_id=billable-mac") && url.contains("code_challenge_method=S256"), "{url}");
    let state = url.split(['?', '&']).find_map(|kv| kv.strip_prefix("state=")).unwrap().to_string();
    assert!(!h.engine.handle_oauth_redirect("billable://connect?code=x"));
    let mark = h.events.len();
    assert!(h.engine.handle_oauth_redirect(&format!("billable://oauth?code=fhq_code_1&state={state}&iss={origin}")));
    h.wait_event(mark, |e| e["type"] == "feedhq.status" && e["connected"] == true);
    assert_eq!(h.ok("hello", json!({}))["feedhq"]["connected"], true);
    assert!(secrets.get(&format!("refresh {origin}")).is_some());
    let mark = h.events.len();
    h.ok("turn.send", json!({"threadId": tid, "text": "Back to Claude", "agent": "claude", "model": "opus"}));
    assert_eq!(h.finished(mark, &tid)["status"], "done");
    let launches: Vec<Vec<String>> = h.log("claude").into_iter().filter(|l| l["kind"] == "launch").map(|l| serde_json::from_value(l["value"].clone()).unwrap()).collect();
    let last = launches.last().unwrap();
    let mcp = last.windows(2).find(|w| w[0] == "--mcp-config").map(|w| w[1].clone()).expect("the relay is attached");
    let mcp: Value = serde_json::from_str(&mcp).unwrap();
    let relay = mcp["mcpServers"]["feedhq"]["url"].as_str().unwrap();
    assert!(relay.starts_with("http://127.0.0.1:") && relay.contains("/mcp/"), "{relay}");
    assert!(!last.iter().any(|a| a.contains("fhq_")), "no token on the command line");
    // Switching agents started Claude fresh, with a handoff, not a resume.
    assert!(!last.contains(&"--resume".to_string()), "{last:?}");

    // Remote requests are answered remotely; events reach both sides
    let remote = h.engine.attach_remote();
    h.engine.handle_remote(&json!({"v":1,"type":"request","id":"remote-1","method":"threads.list","params":{}}).to_string());
    let until = Instant::now() + WAIT;
    let reply = loop {
        let v: Value = serde_json::from_str(&remote.recv_timeout(until.saturating_duration_since(Instant::now())).unwrap()).unwrap();
        if v["type"] == "reply" {
            break v;
        }
    };
    assert_eq!(reply["id"], "remote-1");
    assert_eq!(reply["result"]["threads"].as_array().unwrap().len(), 2);
    h.ok("thread.update", json!({"threadId": cx, "pinned": true}));
    let seen_remotely = std::iter::from_fn(|| remote.recv_timeout(Duration::from_secs(2)).ok()).any(|m| m.contains("\"pinned\":true"));
    assert!(seen_remotely);
    h.engine.detach_remote();

    // Delete, then restart: what's saved is still there
    h.ok("thread.delete", json!({"threadId": cx}));
    assert_eq!(h.call("thread.get", json!({"threadId": cx}))["error"]["code"], "not_found");
    let count = h.items(&tid).len();
    h.engine.shutdown();
    drop(h);
    let mut h = Harness::new(dir.clone(), &origin, secrets);
    let threads = h.ok("threads.list", json!({}))["threads"].as_array().unwrap().clone();
    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0]["title"], "Plan my pottery launch");
    assert_eq!(h.items(&tid).len(), count);
    h.engine.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

fn setup(name: &str) -> Option<Harness> {
    if !python_available() {
        eprintln!("python3 isn't available; skipping");
        return None;
    }
    let dir = temp_dir(name);
    install_fake(&dir, "claude");
    install_fake(&dir, "codex");
    let origin = fake_feedhq();
    let mut h = Harness::new(dir, &origin, Arc::new(MemorySecrets::default()));
    h.ok("hello", json!({}));
    Some(h)
}

fn id_of(v: &Value) -> String {
    v["thread"]["id"].as_str().unwrap().to_string()
}

#[test]
fn folders_and_bulk_delete() {
    let Some(mut h) = setup("folders") else { return };
    let hello = h.ok("hello", json!({}));
    assert_eq!(hello["features"], json!(["folders", "orchestrator", "bulk-delete"]));
    let site = h.dir.join("my site");
    std::fs::create_dir_all(&site).unwrap();
    let site_s = site.to_string_lossy().to_string();

    // The folder must be one on this Mac
    for bad in [h.dir.join("missing").to_string_lossy().to_string(), "relative/path".to_string()] {
        let r = h.call("thread.create", json!({"agent": "claude", "model": "opus", "cwd": bad}));
        assert_eq!(r["error"], json!({"code": "bad_request", "message": "That folder isn't on this Mac."}), "{r}");
    }
    assert_eq!(h.call("thread.create", json!({"agent": "claude", "model": "opus", "role": "worker"}))["error"]["code"], "bad_request");
    // No folder picker in tests, and never from the relay
    assert_eq!(h.call("system.pickFolder", json!({}))["error"]["code"], "bad_request");

    // Claude Code and Codex both work in the chat's folder
    let plain = h.ok("thread.create", json!({"agent": "claude", "model": "opus"}));
    assert_eq!((plain["thread"]["cwd"].clone(), plain["thread"]["role"].clone(), plain["thread"]["parentId"].clone()), (Value::Null, Value::Null, Value::Null));
    let made = h.ok("thread.create", json!({"agent": "claude", "model": "opus", "cwd": site_s}));
    assert_eq!(made["thread"]["cwd"], json!(site_s));
    let cl = id_of(&made);
    let mark = h.events.len();
    h.ok("turn.send", json!({"threadId": cl, "text": "hello"}));
    assert_eq!(h.finished(mark, &cl)["status"], "done");
    let cwds: Vec<String> = h.log("claude").into_iter().filter(|l| l["kind"] == "cwd").map(|l| l["value"].as_str().unwrap().to_string()).collect();
    assert_eq!(cwds, vec![std::fs::canonicalize(&site).unwrap().to_string_lossy().to_string()]);
    let cx = id_of(&h.ok("thread.create", json!({"agent": "codex", "model": "gpt-6.1-sol", "cwd": site_s})));
    let mark = h.events.len();
    h.ok("turn.send", json!({"threadId": cx, "text": "hello"}));
    assert_eq!(h.finished(mark, &cx)["status"], "done");
    let start = h.log("codex").into_iter().find(|l| l["value"]["method"] == "thread/start").unwrap();
    assert_eq!(start["value"]["params"]["cwd"], json!(site_s));
    assert!(start["value"]["params"]["config"].get("mcp_servers.billable").is_none(), "not an orchestrator");

    // Many at once: unknown ids are skipped, one event each
    let remote = h.engine.attach_remote();
    h.engine.handle_remote(&json!({"v":1,"type":"request","id":"pick","method":"system.pickFolder","params":{}}).to_string());
    let until = Instant::now() + WAIT;
    let reply = loop {
        let v: Value = serde_json::from_str(&remote.recv_timeout(until.saturating_duration_since(Instant::now())).unwrap()).unwrap();
        if v["type"] == "reply" {
            break v;
        }
    };
    assert_eq!(reply["error"], json!({"code": "bad_request", "message": "Pick the folder on your Mac."}));
    h.engine.detach_remote();
    let too_many: Vec<String> = (0..201).map(|n| format!("t_{n}")).collect();
    assert_eq!(h.call("threads.delete", json!({"threadIds": too_many}))["error"]["code"], "bad_request");
    let pid = id_of(&plain);
    let mark = h.events.len();
    let r = h.ok("threads.delete", json!({"threadIds": [pid, "t_unknown", cx, pid]}));
    assert_eq!(r["deleted"], json!([pid, cx]));
    let gone: Vec<Value> = h.events[mark..].iter().filter(|e| e["type"] == "thread.deleted").map(|e| e["threadId"].clone()).collect();
    assert_eq!(gone, vec![json!(pid), json!(cx)]);
    let left = h.ok("threads.list", json!({}))["threads"].as_array().unwrap().clone();
    assert_eq!(left.iter().map(|t| t["id"].as_str().unwrap()).collect::<Vec<_>>(), vec![cl.as_str()]);
    h.engine.shutdown();
    let _ = std::fs::remove_dir_all(&h.dir);
}

/// One MCP call over HTTP to the relay (what an agent would do).
fn mcp_call(url: &str, body: &Value) -> Value {
    use std::io::{Read, Write};
    let rest = url.strip_prefix("http://").unwrap();
    let (host, path) = rest.split_once('/').unwrap();
    let body = body.to_string();
    let mut s = std::net::TcpStream::connect(host).unwrap();
    write!(s, "POST /{path} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    serde_json::from_str(out.split_once("\r\n\r\n").unwrap().1).unwrap()
}

#[test]
fn orchestrator_runs_workers() {
    let Some(mut h) = setup("orchestrator") else { return };
    let site = h.dir.join("site");
    std::fs::create_dir_all(&site).unwrap();
    let site_s = site.to_string_lossy().to_string();
    let made = h.ok("thread.create", json!({"agent": "claude", "model": "opus", "cwd": site_s, "role": "orchestrator"}));
    assert_eq!(made["thread"]["role"], "orchestrator");
    let boss = id_of(&made);

    // The person's message: the lead starts 5 helpers (4 run at once), waits, sums up
    let mark = h.events.len();
    h.ok("turn.send", json!({"threadId": boss, "text": "Please orchestrate the work"}));
    let done = h.finished(mark, &boss);
    assert_eq!(done["status"], "done", "{done}");

    let rpcs: Vec<Value> = h.log("claude").into_iter().filter(|l| l["kind"] == "rpc").map(|l| l["value"].clone()).collect();
    let tools_list = rpcs.iter().find(|r| r["method"] == "tools/list").unwrap();
    assert_eq!(tools_list["answer"]["result"]["tools"].as_array().unwrap().len(), 5);
    let result_of = |name: &str| -> Value {
        let r = rpcs.iter().find(|r| r["method"] == "tools/call" && r["params"]["name"] == name).unwrap_or_else(|| panic!("no {name}"));
        assert_eq!(r["answer"]["result"]["isError"], false, "{r}");
        serde_json::from_str(r["answer"]["result"]["content"][0]["text"].as_str().unwrap()).unwrap()
    };
    let started = result_of("start_agents");
    let ids: Vec<String> = started.as_array().unwrap().iter().map(|w| w["id"].as_str().unwrap().to_string()).collect();
    assert_eq!(ids.len(), 5);
    assert_eq!(started[0]["effort"], "low");
    assert_eq!(started[0]["model"], "opus");
    let listed = result_of("list_agents");
    let statuses: Vec<&str> = listed.as_array().unwrap().iter().map(|w| w["status"].as_str().unwrap()).collect();
    assert_eq!(statuses.iter().filter(|s| **s == "running").count(), 4, "{listed}");
    assert_eq!(statuses.iter().filter(|s| **s == "queued").count(), 1, "{listed}");
    let waited = result_of("wait_for_agents");
    for (n, w) in waited.as_array().unwrap().iter().enumerate() {
        assert_eq!(w["status"], "done", "{waited}");
        assert_eq!(w["answer"], json!(format!("Finished slow task {}", n + 1)));
        assert_eq!(w["title"], json!(format!("Slow {}", n + 1)));
    }

    // Workers: ordinary threads with role, parent and the lead's folder
    let threads = h.ok("threads.list", json!({}))["threads"].as_array().unwrap().clone();
    let workers: Vec<&Value> = threads.iter().filter(|t| t["parentId"] == json!(boss)).collect();
    assert_eq!(workers.len(), 5);
    assert!(workers.iter().all(|w| w["role"] == "worker" && w["cwd"] == json!(site_s) && w["status"] == "idle"), "{workers:?}");
    // Never more than 4 running at once
    let mut running = 0usize;
    let mut most = 0usize;
    for e in &h.events[mark..] {
        let worker = ids.iter().any(|i| e["threadId"] == json!(i));
        match e["type"].as_str() {
            Some("turn.started") if worker => {
                running += 1;
                most = most.max(running);
            }
            Some("turn.finished") if worker => running -= 1,
            _ => {}
        }
    }
    assert_eq!(most, 4);
    // The lead's tool rows name the workers
    let items = h.items(&boss);
    let start_row = items.iter().find(|i| i["tool"]["name"] == "mcp__billable__start_agents").unwrap();
    assert_eq!(start_row["tool"]["title"], "Started 5 agents");
    assert_eq!(start_row["tool"]["threads"], json!(ids));
    assert_eq!(items.iter().find(|i| i["tool"]["name"] == "mcp__billable__wait_for_agents").unwrap()["tool"]["threads"], json!(ids));
    assert!(items.last().unwrap()["text"].as_str().unwrap().starts_with("Summary: Slow 1 done"));
    // Only the lead got the billable server; helpers ran in the folder
    let launches: Vec<Vec<String>> = h.log("claude").into_iter().filter(|l| l["kind"] == "launch").map(|l| serde_json::from_value(l["value"].clone()).unwrap()).collect();
    assert_eq!(launches.len(), 6);
    let with_billable: Vec<&Vec<String>> = launches.iter().filter(|a| a.iter().any(|x| x.contains("/orchestrate/"))).collect();
    assert_eq!(with_billable.len(), 1);
    assert!(with_billable[0].windows(2).any(|w| w == ["--allowedTools", "mcp__billable"]));
    let canon = std::fs::canonicalize(&site).unwrap().to_string_lossy().to_string();
    assert!(h.log("claude").iter().filter(|l| l["kind"] == "cwd").all(|l| l["value"] == json!(canon)));

    // Nothing starts outside a turn the person sent
    let cfg: Value = serde_json::from_str(&with_billable[0][with_billable[0].iter().position(|x| x == "--mcp-config").unwrap() + 1]).unwrap();
    let url = cfg["mcpServers"]["billable"]["url"].as_str().unwrap().to_string();
    let r = mcp_call(&url, &json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"start_agents","arguments":{"agents":[{"title":"x","task":"y"}]}}}));
    assert_eq!(r["result"]["isError"], true, "{r}");
    let r = mcp_call(&url, &json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"list_agents","arguments":{}}}));
    assert_eq!(serde_json::from_str::<Value>(r["result"]["content"][0]["text"].as_str().unwrap()).unwrap().as_array().unwrap().len(), 5);
    // A thread that isn't an orchestrator gets nothing
    let other = url.rsplit_once('/').unwrap().0.to_string() + "/" + &ids[0];
    assert_eq!(mcp_call(&other, &json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"list_agents","arguments":{}}}))["result"]["isError"], true);

    // Stop on the lead stops its helpers and answers its wait
    let lead2 = id_of(&h.ok("thread.create", json!({"agent": "claude", "model": "opus", "role": "orchestrator"})));
    let mark = h.events.len();
    h.ok("turn.send", json!({"threadId": lead2, "text": "orchestrate and hang"}));
    // Both helpers are running before Stop
    let helpers_started = |h: &Harness| h.events[mark..].iter().filter(|e| e["type"] == "turn.started" && e["threadId"] != json!(lead2)).count();
    while helpers_started(&h) < 2 {
        let at = h.events.len();
        h.wait_event(at, |_| true);
    }
    h.ok("turn.stop", json!({"threadId": lead2}));
    assert_eq!(h.finished(mark, &lead2)["status"], "stopped");
    let hung: Vec<String> = h.ok("threads.list", json!({}))["threads"].as_array().unwrap().iter().filter(|t| t["parentId"] == json!(lead2)).map(|t| t["id"].as_str().unwrap().to_string()).collect();
    assert_eq!(hung.len(), 2);
    for w in &hung {
        assert_eq!(h.finished(mark, w)["status"], "stopped");
    }

    // Deleting a lead deletes its helpers
    let mark = h.events.len();
    let r = h.ok("threads.delete", json!({"threadIds": [boss]}));
    assert_eq!(r["deleted"].as_array().unwrap().len(), 6);
    assert_eq!(h.events[mark..].iter().filter(|e| e["type"] == "thread.deleted").count(), 6);
    h.ok("thread.delete", json!({"threadId": lead2}));
    assert!(h.ok("threads.list", json!({}))["threads"].as_array().unwrap().is_empty());
    h.engine.shutdown();
    let _ = std::fs::remove_dir_all(&h.dir);
}
