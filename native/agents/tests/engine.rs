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
    cfg.probe_models = false;
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

    // A Claude chat: streaming, usage, and an automatic title
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
    h.wait_event(mark, |e| e["type"] == "thread.updated" && e["thread"]["title"] == "Pottery Launch Plan");
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

    // A Codex chat's title falls back to the first words (its exec fails)
    let cx = h.ok("thread.create", json!({"agent": "codex", "model": "gpt-6.1-sol"}))["thread"]["id"].as_str().unwrap().to_string();
    let mark = h.events.len();
    h.ok("turn.send", json!({"threadId": cx, "text": "Draft seven posts about the spring sale please"}));
    assert_eq!(h.finished(mark, &cx)["status"], "done");
    let cx2 = cx.clone();
    h.wait_event(mark, |e| e["type"] == "thread.updated" && e["thread"]["id"] == json!(cx2) && e["thread"]["title"] == "Draft seven posts about the spring");

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
    assert_eq!(threads[0]["title"], "Pottery Launch Plan");
    assert_eq!(h.items(&tid).len(), count);
    h.engine.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}
