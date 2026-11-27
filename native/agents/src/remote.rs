//! The remote link (Stage 2): the website and phone reach this Mac's engine
//! through FeedHQ's relay. Billable long-polls its inbox for requests and
//! posts replies and events to its outbox, with its phone-sync token
//! (`Authorization: Bearer bhq_…`).
//!
//! - `GET /api/agents/remote/inbox?after=<cursor>&wait=25` → `{ ok, requests, cursor }`.
//!   Delivery is at least once, so requests are de-duplicated by id. Each
//!   poll marks the Mac online, so polling never pauses while connected.
//! - `POST /api/agents/remote/outbox` `{ messages }`: at most 200 per call,
//!   sent every ~250 ms while there's something to send; a body stays under
//!   1 MB and each message under 256 KB.

use std::{
    collections::{HashSet, VecDeque},
    sync::{
        atomic::{AtomicBool, AtomicU8, Ordering},
        mpsc::{Receiver, RecvTimeoutError},
        Arc,
    },
    time::{Duration, Instant},
};

use serde_json::{json, Value};

pub const BATCH_EVERY: Duration = Duration::from_millis(250);
pub const MAX_PER_CALL: usize = 200;
const MAX_MESSAGE_BYTES: usize = 256 * 1024;
const MAX_BODY_BYTES: usize = 900 * 1024;
const MAX_QUEUED: usize = 5000;
const POLL_WAIT_S: u64 = 25;

/// What the link needs from the engine.
pub trait RemoteEngine: Send + Sync + 'static {
    /// A request from the website or phone.
    fn submit_remote(&self, request_json: String);
    /// Replies to remote requests, and every event.
    fn attach_remote(&self) -> Receiver<String>;
    fn detach_remote(&self);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemoteState {
    Running,
    /// FeedHQ refused the token (401): phone sync was disconnected.
    Refused,
    Stopped,
}

pub struct RemoteLink {
    stop: Arc<AtomicBool>,
    state: Arc<AtomicU8>,
}

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(timeout))
        .user_agent(format!("Billable/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

fn backoff(failures: u32) -> Duration {
    Duration::from_millis((1000u64 << failures.min(6)).min(60_000))
}

/// Sleep in short steps so a stop is noticed quickly.
fn nap(d: Duration, stop: &AtomicBool) {
    let until = Instant::now() + d;
    while Instant::now() < until && !stop.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(50).min(until.saturating_duration_since(Instant::now())));
    }
}

/// Bounded memory of request ids already handed to the engine.
struct Seen {
    order: VecDeque<String>,
    set: HashSet<String>,
}

impl Seen {
    fn new() -> Seen {
        Seen { order: VecDeque::new(), set: HashSet::new() }
    }

    /// True the first time an id is seen.
    fn first(&mut self, id: &str) -> bool {
        if self.set.contains(id) {
            return false;
        }
        self.set.insert(id.to_string());
        self.order.push_back(id.to_string());
        if self.order.len() > 2000 {
            if let Some(old) = self.order.pop_front() {
                self.set.remove(&old);
            }
        }
        true
    }
}

/// A message too big for the outbox: a reply becomes a short failure, an
/// event is dropped.
fn fit(message: String) -> Option<String> {
    if message.len() <= MAX_MESSAGE_BYTES {
        return Some(message);
    }
    let v: Value = serde_json::from_str(&message).ok()?;
    if v.get("type").and_then(Value::as_str) == Some("reply") {
        let id = v.get("id").cloned().unwrap_or(Value::Null);
        return Some(json!({"v":1,"type":"reply","id":id,"ok":false,"error":{"code":"failed","message":"That's too big to open away from the Mac. Open it in Billable."}}).to_string());
    }
    None
}

/// Split queued messages into one call's worth: at most 200, under 1 MB.
pub fn take_batch(queue: &mut VecDeque<String>) -> Vec<String> {
    let mut out = Vec::new();
    let mut bytes = 16;
    while let Some(next) = queue.front() {
        if out.len() >= MAX_PER_CALL || (!out.is_empty() && bytes + next.len() + 1 > MAX_BODY_BYTES) {
            break;
        }
        bytes += next.len() + 1;
        out.extend(queue.pop_front());
    }
    out
}

pub fn batch_body(batch: &[String]) -> String {
    format!("{{\"messages\":[{}]}}", batch.join(","))
}

impl RemoteLink {
    /// Start polling and posting; runs until [`RemoteLink::stop`] or a 401.
    pub fn start(engine: Arc<dyn RemoteEngine>, origin: &str, token: &str) -> RemoteLink {
        let stop = Arc::new(AtomicBool::new(false));
        let state = Arc::new(AtomicU8::new(0));
        let origin = origin.trim_end_matches('/').to_string();
        let outgoing = engine.attach_remote();
        {
            let (engine, origin, token, stop, state) = (engine.clone(), origin.clone(), token.to_string(), stop.clone(), state.clone());
            std::thread::spawn(move || poll_inbox(&*engine, &origin, &token, &stop, &state));
        }
        {
            let (origin, token, stop2, state) = (origin, token.to_string(), stop.clone(), state.clone());
            std::thread::spawn(move || {
                post_outbox(outgoing, &origin, &token, &stop2, &state);
                engine.detach_remote();
            });
        }
        RemoteLink { stop, state }
    }

    pub fn state(&self) -> RemoteState {
        match self.state.load(Ordering::SeqCst) {
            0 if !self.stop.load(Ordering::SeqCst) => RemoteState::Running,
            1 => RemoteState::Refused,
            _ => RemoteState::Stopped,
        }
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

impl Drop for RemoteLink {
    fn drop(&mut self) {
        self.stop();
    }
}

fn refused(state: &AtomicU8, stop: &AtomicBool) {
    state.store(1, Ordering::SeqCst);
    stop.store(true, Ordering::SeqCst);
}

fn poll_inbox(engine: &dyn RemoteEngine, origin: &str, token: &str, stop: &AtomicBool, state: &AtomicU8) {
    let http = agent(Duration::from_secs(POLL_WAIT_S + 15));
    let mut cursor: Option<String> = None;
    let mut seen = Seen::new();
    let mut failures = 0u32;
    while !stop.load(Ordering::SeqCst) {
        let mut url = format!("{origin}/api/agents/remote/inbox?wait={POLL_WAIT_S}");
        if let Some(c) = &cursor {
            url.push_str(&format!("&after={}", crate::util::url_encode(c)));
        }
        let res = http.get(&url).header("Authorization", format!("Bearer {token}")).header("Accept", "application/json").call();
        let outcome = res.map_err(|e| e.to_string()).map(|mut r| {
            let status = r.status().as_u16();
            let body = r.body_mut().read_to_string().unwrap_or_default();
            (status, body)
        });
        match outcome {
            Ok((200, body)) => {
                failures = 0;
                let v: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
                for req in v.get("requests").and_then(Value::as_array).cloned().unwrap_or_default() {
                    let id = req.get("id").and_then(Value::as_str).unwrap_or("").to_string();
                    if id.is_empty() || seen.first(&id) {
                        engine.submit_remote(req.to_string());
                    }
                }
                match v.get("cursor") {
                    Some(Value::String(c)) => cursor = Some(c.clone()),
                    Some(Value::Number(n)) => cursor = Some(n.to_string()),
                    _ => {}
                }
            }
            Ok((401, _)) => return refused(state, stop),
            Ok((429, _)) | Ok(_) | Err(_) => {
                failures += 1;
                nap(backoff(failures), stop);
            }
        }
    }
}

fn post_outbox(outgoing: Receiver<String>, origin: &str, token: &str, stop: &AtomicBool, state: &AtomicU8) {
    let http = agent(Duration::from_secs(20));
    let mut queue: VecDeque<String> = VecDeque::new();
    let mut failures = 0u32;
    let mut first_at: Option<Instant> = None;
    loop {
        if stop.load(Ordering::SeqCst) {
            return;
        }
        match outgoing.recv_timeout(Duration::from_millis(50)) {
            Ok(m) => {
                if let Some(m) = fit(m) {
                    queue.push_back(m);
                    first_at.get_or_insert_with(Instant::now);
                }
                while queue.len() > MAX_QUEUED {
                    queue.pop_front();
                }
                // Drain whatever else is ready without waiting.
                while let Ok(m) = outgoing.try_recv() {
                    queue.extend(fit(m));
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) if queue.is_empty() => return,
            Err(RecvTimeoutError::Disconnected) => std::thread::sleep(Duration::from_millis(50)),
        }
        let due = first_at.is_some_and(|t| t.elapsed() >= BATCH_EVERY) || queue.len() >= MAX_PER_CALL;
        if !due || queue.is_empty() {
            continue;
        }
        let batch = take_batch(&mut queue);
        let res = http
            .post(format!("{origin}/api/agents/remote/outbox"))
            .header("Authorization", format!("Bearer {token}"))
            .header("Content-Type", "application/json")
            .send(batch_body(&batch).as_bytes());
        let (status, retry_after) = match res {
            Ok(r) => (r.status().as_u16(), r.headers().get("retry-after").and_then(|v| v.to_str().ok()).and_then(|v| v.trim().parse::<u64>().ok())),
            Err(_) => (0, None),
        };
        match status {
            200..=299 => {
                failures = 0;
                first_at = if queue.is_empty() { None } else { Some(Instant::now() - BATCH_EVERY) };
            }
            401 => return refused(state, stop),
            // FeedHQ won't take these; sending them again won't help.
            400 | 413 => first_at = if queue.is_empty() { None } else { Some(Instant::now()) },
            _ => {
                failures += 1;
                for m in batch.into_iter().rev() {
                    queue.push_front(m);
                }
                let wait = retry_after.map(|s| Duration::from_secs(s.clamp(1, 300))).unwrap_or_else(|| backoff(failures));
                nap(wait, stop);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::{read_request, write_response};
    use std::{
        io::BufReader,
        net::TcpListener,
        sync::{mpsc, Mutex},
    };

    struct FakeEngine {
        got: Mutex<Vec<String>>,
        tx: Mutex<Option<mpsc::Sender<String>>>,
        detached: AtomicBool,
    }

    impl RemoteEngine for FakeEngine {
        fn submit_remote(&self, request_json: String) {
            let id = serde_json::from_str::<Value>(&request_json).unwrap()["id"].as_str().unwrap().to_string();
            self.got.lock().unwrap().push(id.clone());
            // Answer each request, plus a burst of events for the first one.
            let tx = self.tx.lock().unwrap().clone().unwrap();
            let _ = tx.send(json!({"v":1,"type":"reply","id":id,"ok":true,"result":{}}).to_string());
            if id == "r1" {
                for i in 0..450 {
                    let _ = tx.send(json!({"v":1,"type":"event","event":{"type":"item.delta","threadId":"t","itemId":"i","delta":i.to_string()}}).to_string());
                }
            }
        }
        fn attach_remote(&self) -> Receiver<String> {
            let (tx, rx) = mpsc::channel();
            *self.tx.lock().unwrap() = Some(tx);
            rx
        }
        fn detach_remote(&self) {
            self.detached.store(true, Ordering::SeqCst);
        }
    }

    /// A fake relay: the inbox hands out r1, then r1 again (at least once)
    /// with r2, then 401; the outbox records batch sizes.
    type Log<T> = Arc<Mutex<Vec<T>>>;

    fn fake() -> (String, Log<String>, Log<usize>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", l.local_addr().unwrap());
        let polls: Log<String> = Arc::default();
        let batches: Log<usize> = Arc::default();
        let (p, b) = (polls.clone(), batches.clone());
        std::thread::spawn(move || {
            let inbox_calls = Arc::new(Mutex::new(0));
            for s in l.incoming() {
                let s = s.unwrap();
                let (p, b, inbox_calls) = (p.clone(), b.clone(), inbox_calls.clone());
                // One thread per connection: a held inbox poll mustn't block the outbox.
                std::thread::spawn(move || {
                let mut r = BufReader::new(s.try_clone().unwrap());
                let mut w = s;
                let Ok(Some(req)) = read_request(&mut r) else { return };
                assert_eq!(req.header("authorization"), Some("Bearer bhq_test"));
                if req.path.starts_with("/api/agents/remote/inbox") {
                    let n = {
                        let mut c = inbox_calls.lock().unwrap();
                        *c += 1;
                        *c - 1
                    };
                    p.lock().unwrap().push(req.path.clone());
                    let body = match n {
                        0 => json!({"ok":true,"requests":[{"v":1,"type":"request","id":"r1","method":"hello","params":{}}],"cursor":"5"}),
                        1 => json!({"ok":true,"requests":[{"v":1,"type":"request","id":"r1","method":"hello","params":{}},{"v":1,"type":"request","id":"r2","method":"threads.list","params":{}}],"cursor":"7"}),
                        _ => {
                            // Give the outbox time to flush before refusing.
                            std::thread::sleep(Duration::from_millis(1500));
                            write_response(&mut w, 401, &[], br#"{"ok":false}"#, false).unwrap();
                            return;
                        }
                    };
                    write_response(&mut w, 200, &[("Content-Type", "application/json")], body.to_string().as_bytes(), false).unwrap();
                } else {
                    let v: Value = serde_json::from_slice(&req.body).unwrap();
                    b.lock().unwrap().push(v["messages"].as_array().unwrap().len());
                    write_response(&mut w, 200, &[], br#"{"ok":true}"#, false).unwrap();
                }
                });
            }
        });
        (origin, polls, batches)
    }

    #[test]
    fn polls_dedupes_batches_and_stops_on_401() {
        let (origin, polls, batches) = fake();
        let engine = Arc::new(FakeEngine { got: Mutex::default(), tx: Mutex::default(), detached: AtomicBool::new(false) });
        let link = RemoteLink::start(engine.clone(), &origin, "bhq_test");
        let started = Instant::now();
        while link.state() == RemoteState::Running && started.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(link.state(), RemoteState::Refused);
        assert_eq!(*engine.got.lock().unwrap(), vec!["r1".to_string(), "r2".to_string()]);
        let polls = polls.lock().unwrap();
        assert_eq!(polls[0], "/api/agents/remote/inbox?wait=25");
        assert_eq!(polls[1], "/api/agents/remote/inbox?wait=25&after=5");
        assert_eq!(polls[2], "/api/agents/remote/inbox?wait=25&after=7");
        let batches = batches.lock().unwrap();
        assert_eq!(batches.iter().sum::<usize>(), 452, "{batches:?}");
        assert!(batches.iter().all(|n| *n <= MAX_PER_CALL));
        let started = Instant::now();
        while !engine.detached.load(Ordering::SeqCst) && started.elapsed() < Duration::from_secs(3) {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(engine.detached.load(Ordering::SeqCst));
    }

    #[test]
    fn batches_respect_limits() {
        let mut q: VecDeque<String> = (0..450).map(|i| format!("{{\"n\":{i}}}")).collect();
        assert_eq!(take_batch(&mut q).len(), 200);
        assert_eq!(take_batch(&mut q).len(), 200);
        assert_eq!(take_batch(&mut q).len(), 50);
        let mut q: VecDeque<String> = (0..10).map(|_| "x".repeat(200 * 1024)).collect();
        assert_eq!(take_batch(&mut q).len(), 4);
        assert_eq!(batch_body(&["{}".into(), "{\"a\":1}".into()]), r#"{"messages":[{},{"a":1}]}"#);
        let big_reply = json!({"v":1,"type":"reply","id":"r9","ok":true,"result":{"x":"y".repeat(300 * 1024)}}).to_string();
        let fitted: Value = serde_json::from_str(&fit(big_reply).unwrap()).unwrap();
        assert_eq!((fitted["id"].as_str(), fitted["ok"].as_bool()), (Some("r9"), Some(false)));
        assert!(fit(json!({"v":1,"type":"event","event":{"type":"x","d":"y".repeat(300 * 1024)}}).to_string()).is_none());
        assert_eq!(backoff(1), Duration::from_secs(2));
        assert_eq!(backoff(20), Duration::from_secs(60));
    }
}
