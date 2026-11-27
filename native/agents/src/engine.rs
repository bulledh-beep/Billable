//! The engine: one thread that owns the chats, the agent processes and the
//! FeedHQ connection, and answers the protocol (PROTOCOL.md).
//!
//! Requests come in with [`Engine::handle`] (the web view inside Billable)
//! or [`Engine::handle_remote`] (the FeedHQ relay). Replies go back to where
//! the request came from; events go to both. Nothing here blocks the
//! caller: anything slow (finding the CLIs, titles, sign-in) runs on a
//! helper thread and reports back to the engine thread.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use serde_json::{json, Value};

use crate::{
    claude, codex,
    discovery::{self, Binaries},
    driver::DriverEvent,
    handoff,
    oauth::{self, FeedhqAuth},
    protocol::*,
    relay::Relay,
    remote::RemoteEngine,
    secrets::SecretStore,
    store::Store,
    tools,
    util::{cut, first_words, new_id, now_iso, one_line},
};

/// Claude Code processes with nothing to do are stopped after this; the
/// next message resumes their session.
const IDLE_LIMIT: Duration = Duration::from_secs(15 * 60);
/// After Stop, how long an agent gets to wind down before it's killed.
const STOP_GRACE: Duration = Duration::from_secs(10);
/// A sign-in started with feedhq.connect must finish within this.
const SIGN_IN_LIMIT: Duration = Duration::from_secs(15 * 60);

pub struct EngineConfig {
    /// `<user data>/agents.db` (never billable.db).
    pub db_path: PathBuf,
    /// Where agents work (their current directory).
    pub work_dir: PathBuf,
    /// FeedHQ's origin, e.g. `https://feedhq.app`.
    pub feedhq_origin: String,
    pub secrets: Arc<dyn SecretStore>,
    /// The Mac's name for `hello` (`None`: asked of the system).
    pub device_name: Option<String>,
    pub engine_version: String,
    /// Fixed CLI paths (tests); otherwise they're searched for.
    pub claude_bin: Option<PathBuf>,
    pub codex_bin: Option<PathBuf>,
    /// Ask the login shell for its PATH when looking for the CLIs.
    pub login_shell: bool,
    /// Ask each CLI for its model list (off in tests).
    pub probe_models: bool,
    /// Ask FeedHQ for its OAuth issuer, which may differ from the
    /// configured address (e.g. a vercel.app alias of feedhq.app).
    pub discover_issuer: bool,
}

impl EngineConfig {
    pub fn new(db_path: PathBuf, work_dir: PathBuf, feedhq_origin: &str, secrets: Arc<dyn SecretStore>) -> EngineConfig {
        EngineConfig {
            db_path,
            work_dir,
            feedhq_origin: feedhq_origin.trim_end_matches('/').to_string(),
            secrets,
            device_name: None,
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            claude_bin: None,
            codex_bin: None,
            login_shell: true,
            probe_models: true,
            discover_issuer: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Source {
    Local,
    Remote,
}

type ReplyTo = (Source, String);

enum Cmd {
    Request(Source, String),
    OAuthRedirect(String),
    Claude { thread: String, gen: u64, ev: DriverEvent },
    Codex { gen: u64, inc: codex::Incoming },
    CodexServer { gen: u64, result: Result<codex::Server, String> },
    CodexBound { thread: String, gen: u64, codex_thread: String },
    CodexTurn { thread: String, turn: String, gen: u64, result: Result<String, String> },
    Agents { agents: Vec<Agent>, bins: Binaries, last: bool, reply: Option<ReplyTo> },
    Titled { thread: String, title: String, reply: Option<ReplyTo> },
    FeedhqChanged,
    SignInFailed(String),
    Shutdown(Sender<()>),
}

/// A handle to the engine. Cheap to clone; every clone talks to the same
/// engine thread.
#[derive(Clone)]
pub struct Engine {
    tx: Sender<Cmd>,
    remote: Arc<Mutex<Option<Sender<String>>>>,
}

impl Engine {
    /// Start the engine. Its replies and events for the web view arrive on
    /// the receiver as JSON strings.
    pub fn new(cfg: EngineConfig) -> anyhow::Result<(Engine, Receiver<String>)> {
        let store = Store::open(&cfg.db_path)?;
        std::fs::create_dir_all(&cfg.work_dir)?;
        let (tx, rx) = mpsc::channel();
        let (out_tx, out_rx) = mpsc::channel();
        let remote: Arc<Mutex<Option<Sender<String>>>> = Arc::default();
        let (state_tx, state_remote) = (tx.clone(), remote.clone());
        std::thread::Builder::new().name("agents-engine".into()).spawn(move || {
            // Requests wait in the channel while FeedHQ's issuer is looked up.
            let issuer = issuer_for(&cfg, &store);
            let auth = Arc::new(FeedhqAuth::new(&issuer, cfg.secrets.clone()));
            {
                let tx = state_tx.clone();
                auth.set_on_change(Arc::new(move |_| {
                    let _ = tx.send(Cmd::FeedhqChanged);
                }));
            }
            let relay = Relay::start(auth.clone()).map_err(|e| eprintln!("[agents] the FeedHQ relay didn't start: {e}")).ok();
            let mut state = State::new(cfg, store, state_tx, out_tx, state_remote, auth, relay);
            state.run(rx)
        })?;
        Ok((Engine { tx, remote }, out_rx))
    }

    /// A request from the web view (a JSON envelope).
    pub fn handle(&self, request_json: &str) {
        let _ = self.tx.send(Cmd::Request(Source::Local, request_json.to_string()));
    }

    /// A request that came through the FeedHQ relay.
    pub fn handle_remote(&self, request_json: &str) {
        let _ = self.tx.send(Cmd::Request(Source::Remote, request_json.to_string()));
    }

    /// A `billable://oauth?…` link. Returns false for any other link.
    pub fn handle_oauth_redirect(&self, url: &str) -> bool {
        if oauth::parse_redirect(url).is_none() {
            return false;
        }
        let _ = self.tx.send(Cmd::OAuthRedirect(url.to_string()));
        true
    }

    /// Stop every agent process and the relay (at quit). Waits up to 3 s.
    pub fn shutdown(&self) {
        let (tx, rx) = mpsc::channel();
        if self.tx.send(Cmd::Shutdown(tx)).is_ok() {
            let _ = rx.recv_timeout(Duration::from_secs(3));
        }
    }
}

impl RemoteEngine for Engine {
    fn submit_remote(&self, request_json: String) {
        let _ = self.tx.send(Cmd::Request(Source::Remote, request_json));
    }

    fn attach_remote(&self) -> Receiver<String> {
        let (tx, rx) = mpsc::channel();
        if let Ok(mut r) = self.remote.lock() {
            *r = Some(tx);
        }
        rx
    }

    fn detach_remote(&self) {
        if let Ok(mut r) = self.remote.lock() {
            *r = None;
        }
    }
}

// ---------- State ----------

struct Turn {
    id: String,
    agent: String,
    model: String,
    effort: Option<String>,
    /// Driver key → item id.
    keys: HashMap<String, String>,
    /// Text and reasoning items still streaming, by item id.
    live: HashMap<String, Item>,
    /// Tool items whose approval was denied.
    denied: Vec<String>,
    outputs: usize,
    usage: Option<Usage>,
    error: Option<String>,
    stop_at: Option<Instant>,
    /// Started by resuming the agent's own session: if that fails before
    /// anything comes back, try again fresh with a handoff.
    resumed: bool,
    user_text: String,
    /// Codex's id for this turn, once it's known.
    codex_turn: Option<String>,
}

struct OpenApproval {
    thread: String,
    native: String,
    agent: String,
    item_id: Option<String>,
}

struct ClaudeRun {
    session: claude::Session,
    gen: u64,
    idle_since: Option<Instant>,
}

struct CodexRun {
    server: codex::Server,
    gen: u64,
    /// Our thread → Codex thread, for threads loaded in this server.
    loaded: HashMap<String, String>,
}

struct State {
    cfg: EngineConfig,
    store: Store,
    tx: Sender<Cmd>,
    local: Sender<String>,
    remote: Arc<Mutex<Option<Sender<String>>>>,
    auth: Arc<FeedhqAuth>,
    relay: Option<Relay>,
    device: Option<String>,
    agents: Vec<Agent>,
    agents_ready: bool,
    bins: Binaries,
    waiting_hello: Vec<ReplyTo>,
    refreshing: bool,
    turns: HashMap<String, Turn>,
    approvals: HashMap<String, OpenApproval>,
    claude: HashMap<String, ClaudeRun>,
    codex: Option<CodexRun>,
    codex_starting: bool,
    /// Threads waiting for the Codex server: (thread, prompt).
    codex_queue: Vec<(String, String)>,
    next_gen: u64,
    sign_ins: HashMap<String, (String, Instant)>,
    stopping: bool,
}

fn device_name() -> Option<String> {
    let out = crate::process::run_with_timeout(std::process::Command::new("/usr/sbin/scutil").args(["--get", "ComputerName"]), Duration::from_secs(2))
        .filter(|o| o.status == Some(0))
        .map(|o| o.stdout.trim().to_string())
        .filter(|n| !n.is_empty());
    out.or_else(|| {
        crate::process::run_with_timeout(&mut std::process::Command::new("hostname"), Duration::from_secs(2))
            .map(|o| o.stdout.trim().trim_end_matches(".local").to_string())
            .filter(|n| !n.is_empty())
    })
    .map(|n| cut(&n, 60))
}

/// The issuer FeedHQ names at the configured address, remembered so an
/// offline start still finds the same Keychain item.
fn issuer_for(cfg: &EngineConfig, store: &Store) -> String {
    if !cfg.discover_issuer {
        return cfg.feedhq_origin.clone();
    }
    let key = format!("issuer {}", cfg.feedhq_origin);
    match oauth::discover_issuer(&cfg.feedhq_origin) {
        Some(issuer) => {
            let _ = store.set_meta(&key, &issuer);
            issuer
        }
        None => store.meta(&key).ok().flatten().unwrap_or_else(|| cfg.feedhq_origin.clone()),
    }
}

/// A title as the model wrote it → a short plain one.
pub fn clean_title(raw: &str) -> Option<String> {
    let line = raw.lines().map(str::trim).find(|l| !l.is_empty())?;
    let line = line.trim_start_matches("Title:").trim_start_matches("title:");
    let t = line.trim().trim_matches(|c: char| c == '"' || c == '\'' || c == '*' || c == '`' || c == '#' || c == '“' || c == '”').trim();
    let t = t.trim_end_matches(['.', '!', ':', ';']).trim();
    (!t.is_empty()).then(|| cut(t, 60))
}

pub fn title_prompt(first_message: &str) -> String {
    format!(
        "Write a 2 to 5 word title for a chat that starts with the message below. Reply with only the title, no quotes.\n\nMessage: {}",
        cut(first_message, 2000)
    )
}

fn params<T: serde::de::DeserializeOwned>(v: &Value) -> Result<T, ProtoError> {
    serde_json::from_value(if v.is_null() { json!({}) } else { v.clone() }).map_err(|e| ProtoError::bad_request(format!("Those details aren't right ({e}).")))
}

fn store_err(e: anyhow::Error) -> ProtoError {
    eprintln!("[agents] store: {e:#}");
    ProtoError::failed("Billable couldn't save that.")
}

impl State {
    fn new(
        cfg: EngineConfig,
        store: Store,
        tx: Sender<Cmd>,
        local: Sender<String>,
        remote: Arc<Mutex<Option<Sender<String>>>>,
        auth: Arc<FeedhqAuth>,
        relay: Option<Relay>,
    ) -> State {
        State {
            device: cfg.device_name.clone(),
            cfg,
            store,
            tx,
            local,
            remote,
            auth,
            relay,
            agents: Vec::new(),
            agents_ready: false,
            bins: Binaries::default(),
            waiting_hello: Vec::new(),
            refreshing: false,
            turns: HashMap::new(),
            approvals: HashMap::new(),
            claude: HashMap::new(),
            codex: None,
            codex_starting: false,
            codex_queue: Vec::new(),
            next_gen: 0,
            sign_ins: HashMap::new(),
            stopping: false,
        }
    }

    fn run(&mut self, rx: Receiver<Cmd>) {
        self.start_up();
        loop {
            match rx.recv_timeout(Duration::from_millis(500)) {
                Ok(Cmd::Shutdown(ack)) => {
                    self.shut_down();
                    let _ = ack.send(());
                    return;
                }
                Ok(cmd) => self.command(cmd),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    self.shut_down();
                    return;
                }
            }
            self.tick();
        }
    }

    fn start_up(&mut self) {
        let now = now_iso();
        match self.store.recover_after_restart(&now) {
            Ok(ids) => {
                for id in ids {
                    let _ = self.add_item(&id, None, ItemKind::Error, Some("Billable was closed while this was running.".into()), |_| {});
                }
            }
            Err(e) => eprintln!("[agents] recovery: {e:#}"),
        }
        if self.device.is_none() {
            self.device = device_name();
        }
        // Last time's agents answer hello at once; a fresh look follows.
        if let Some(cached) = self.store.meta("agents").ok().flatten().and_then(|s| serde_json::from_str::<Vec<Agent>>(&s).ok()) {
            if !cached.is_empty() {
                self.agents = cached;
            }
        }
        self.refresh_agents(None);
    }

    fn shut_down(&mut self) {
        self.stopping = true;
        for (_, mut run) in self.claude.drain() {
            run.session.stop();
        }
        if let Some(mut c) = self.codex.take() {
            c.server.stop();
        }
        if let Some(r) = self.relay.take() {
            r.stop();
        }
        if let Ok(mut r) = self.remote.lock() {
            *r = None;
        }
    }

    // ---------- Output ----------

    fn send(&self, to: Source, json: String) {
        match to {
            Source::Local => {
                let _ = self.local.send(json);
            }
            Source::Remote => {
                if let Some(r) = self.remote.lock().ok().and_then(|r| r.clone()) {
                    let _ = r.send(json);
                }
            }
        }
    }

    fn reply(&self, to: &ReplyTo, result: Result<Value, ProtoError>) {
        let r = match result {
            Ok(v) => Reply::ok(&to.1, v),
            Err(e) => Reply::err(&to.1, e),
        };
        self.send(to.0, serde_json::to_string(&r).unwrap_or_default());
    }

    fn emit(&self, event: Event) {
        let json = serde_json::to_string(&EventEnvelope::new(event)).unwrap_or_default();
        self.send(Source::Remote, json.clone());
        self.send(Source::Local, json);
    }

    // ---------- Commands ----------

    fn command(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::Request(src, json) => self.request(src, &json),
            Cmd::OAuthRedirect(url) => self.oauth_redirect(&url),
            Cmd::Claude { thread, gen, ev } => {
                if self.claude.get(&thread).is_some_and(|r| r.gen == gen) {
                    if let DriverEvent::Exited { .. } = ev {
                        self.claude.remove(&thread);
                    }
                    self.driver_event(&thread, ev);
                }
            }
            Cmd::Codex { gen, inc } => self.codex_incoming(gen, inc),
            Cmd::CodexServer { gen, result } => self.codex_server(gen, result),
            Cmd::CodexBound { thread, gen, codex_thread } => {
                if let Some(c) = self.codex.as_mut().filter(|c| c.gen == gen) {
                    c.loaded.insert(thread.clone(), codex_thread.clone());
                    let _ = self.store.set_session(&thread, codex::AGENT_ID, &codex_thread, &now_iso());
                }
            }
            Cmd::CodexTurn { thread, turn, gen, result } => self.codex_turn_started(&thread, &turn, gen, result),
            Cmd::Agents { agents, bins, last, reply } => self.agents_found(agents, bins, last, reply),
            Cmd::Titled { thread, title, reply } => self.titled(&thread, title, reply),
            Cmd::FeedhqChanged => self.feedhq_changed(),
            Cmd::SignInFailed(message) => {
                eprintln!("[agents] FeedHQ sign-in failed: {message}");
                self.emit(Event::FeedhqStatus { connected: self.auth.connected(), account: self.auth.account() });
            }
            Cmd::Shutdown(_) => {}
        }
    }

    fn tick(&mut self) {
        let now = Instant::now();
        // Agents that didn't stop when asked
        let overdue: Vec<String> = self.turns.iter().filter(|(_, t)| t.stop_at.is_some_and(|at| now >= at)).map(|(id, _)| id.clone()).collect();
        for thread in overdue {
            if let Some(mut run) = self.claude.remove(&thread) {
                run.session.stop();
            }
            self.finish_turn(&thread, TurnStatus::Stopped, None, None);
        }
        // Idle Claude Code processes
        let idle: Vec<String> = self.claude.iter().filter(|(_, r)| r.idle_since.is_some_and(|t| now.duration_since(t) > IDLE_LIMIT)).map(|(id, _)| id.clone()).collect();
        for thread in idle {
            if let Some(mut run) = self.claude.remove(&thread) {
                run.session.stop();
            }
        }
        self.sign_ins.retain(|_, (_, at)| now.duration_since(*at) < SIGN_IN_LIMIT);
    }

    fn request(&mut self, src: Source, json: &str) {
        let Ok(raw) = serde_json::from_str::<Value>(json) else { return };
        let Some(id) = raw.get("id").and_then(Value::as_str).map(str::to_string) else { return };
        let to = (src, id);
        let req: Request = match serde_json::from_value(raw) {
            Ok(r) => r,
            Err(_) => return self.reply(&to, Err(ProtoError::bad_request("That request isn't one Billable understands."))),
        };
        let p = &req.params;
        let result = match req.method.as_str() {
            "hello" => {
                if !self.agents_ready && self.agents.is_empty() {
                    self.waiting_hello.push(to);
                    return;
                }
                Ok(self.hello())
            }
            "threads.list" => params::<ThreadsListParams>(p).and_then(|p| self.store.threads(p.include_archived).map_err(store_err)).map(|threads| json!({ "threads": threads })),
            "thread.get" => params::<ThreadIdParams>(p).and_then(|p| self.thread_get(&p.thread_id)),
            "thread.create" => params::<ThreadCreateParams>(p).and_then(|p| self.thread_create(p)),
            "thread.update" => params::<ThreadUpdateParams>(p).and_then(|p| self.thread_update(p)),
            "thread.delete" => params::<ThreadIdParams>(p).and_then(|p| self.thread_delete(&p.thread_id)),
            "thread.retitle" => match params::<ThreadIdParams>(p).and_then(|p| self.thread(&p.thread_id)) {
                Ok(t) => return self.start_title(&t, Some(to)),
                Err(e) => Err(e),
            },
            "turn.send" => match params::<TurnSendParams>(p).and_then(|p| self.turn_send(p, &to)) {
                // turn_send replies itself, before the turn starts.
                Ok(()) => return,
                Err(e) => Err(e),
            },
            "turn.stop" => params::<ThreadIdParams>(p).and_then(|p| self.turn_stop(&p.thread_id)),
            "approval.answer" => params::<ApprovalAnswerParams>(p).and_then(|p| self.approval_answer(p)),
            "agents.refresh" => {
                self.refresh_agents(Some(to));
                return;
            }
            "feedhq.connect" => Ok(self.feedhq_connect()),
            "feedhq.disconnect" => {
                let auth = self.auth.clone();
                std::thread::spawn(move || auth.disconnect());
                Ok(json!({}))
            }
            _ => Err(ProtoError::bad_request("Billable doesn't know that request.")),
        };
        self.reply(&to, result);
    }

    fn hello(&self) -> Value {
        json!({
            "engine": "mac",
            "engineVersion": self.cfg.engine_version,
            "protocol": PROTOCOL_VERSION,
            "agents": self.agents,
            "feedhq": { "connected": self.auth.connected(), "account": self.auth.account() },
            "device": self.device,
        })
    }

    // ---------- Agents ----------

    fn refresh_agents(&mut self, reply: Option<ReplyTo>) {
        if self.refreshing && reply.is_none() {
            return;
        }
        self.refreshing = true;
        let opts = discovery::Options { claude_bin: self.cfg.claude_bin.clone(), codex_bin: self.cfg.codex_bin.clone(), login_shell: self.cfg.login_shell, cwd: self.cfg.work_dir.clone() };
        let (tx, cached, probe, quick_first) = (self.tx.clone(), self.agents.clone(), self.cfg.probe_models, self.agents.is_empty());
        std::thread::spawn(move || {
            if quick_first && probe {
                let (agents, bins) = discovery::detect(&opts, false, &cached);
                let _ = tx.send(Cmd::Agents { agents: agents.clone(), bins, last: false, reply: None });
            }
            let (agents, bins) = discovery::detect(&opts, probe, &cached);
            let _ = tx.send(Cmd::Agents { agents, bins, last: true, reply });
        });
    }

    fn agents_found(&mut self, agents: Vec<Agent>, bins: Binaries, last: bool, reply: Option<ReplyTo>) {
        // A hello may have been answered from last time's list, so any
        // change is announced, the first detection included.
        let changed = agents != self.agents;
        self.agents = agents;
        self.bins = bins;
        self.agents_ready = true;
        if last {
            self.refreshing = false;
            if let Ok(s) = serde_json::to_string(&self.agents) {
                let _ = self.store.set_meta("agents", &s);
            }
        }
        for to in std::mem::take(&mut self.waiting_hello) {
            self.reply(&to, Ok(self.hello()));
        }
        if changed {
            self.emit(Event::AgentsChanged { agents: self.agents.clone() });
        }
        if let Some(to) = reply {
            self.reply(&to, Ok(json!({ "agents": self.agents })));
        }
    }

    fn agent(&self, id: &str) -> Option<&Agent> {
        self.agents.iter().find(|a| a.id == id)
    }

    /// "Claude Code, Opus 5.5" for the handoff.
    fn speaker(&self, agent: &str, model: &str) -> String {
        match self.agent(agent) {
            Some(a) => format!("{}, {}", a.name, a.models.iter().find(|m| m.id == model).map(|m| m.name.as_str()).unwrap_or(model)),
            None => format!("{agent}, {model}"),
        }
    }

    /// The effort to use with a model: the asked one if the model takes it,
    /// else the model's default.
    fn effort_for(&self, agent: &str, model: &str, asked: Option<String>) -> Option<String> {
        let m = self.agent(agent).and_then(|a| a.models.iter().find(|m| m.id == model));
        match m {
            Some(m) if m.efforts.is_empty() => None,
            Some(m) => asked.filter(|e| m.efforts.contains(e)).or_else(|| m.default_effort.clone()),
            None => asked,
        }
    }

    // ---------- Threads ----------

    fn thread(&self, id: &str) -> Result<Thread, ProtoError> {
        self.store.thread(id).map_err(store_err)?.ok_or_else(ProtoError::not_found)
    }

    fn save_thread(&self, t: &Thread) {
        if let Err(e) = self.store.save_thread(t) {
            eprintln!("[agents] saving a chat: {e:#}");
        }
        self.emit(Event::ThreadUpdated { thread: t.clone() });
    }

    fn thread_get(&self, id: &str) -> Result<Value, ProtoError> {
        let thread = self.thread(id)?;
        let mut items = self.store.items(id).map_err(store_err)?;
        if let Some(turn) = self.turns.get(id) {
            for item in items.iter_mut() {
                if let Some(live) = turn.live.get(&item.id) {
                    *item = live.clone();
                }
            }
        }
        let approvals = self.store.open_approvals(id).map_err(store_err)?;
        Ok(json!({ "thread": thread, "items": items, "approvals": approvals }))
    }

    fn check_agent(&self, agent: &str) -> Result<(), ProtoError> {
        if agent != claude::AGENT_ID && agent != codex::AGENT_ID {
            return Err(ProtoError::bad_request("This Mac runs Claude Code and Codex only."));
        }
        if !self.agents_ready && self.agents.is_empty() {
            return Err(ProtoError::failed("Billable is still looking for your agents. Try again in a moment."));
        }
        let a = self.agent(agent).ok_or_else(|| ProtoError::new(ErrorCode::AgentMissing, "That agent isn't installed on this Mac."))?;
        if !a.installed {
            return Err(ProtoError::new(ErrorCode::AgentMissing, format!("{} isn't installed on this Mac.", a.name)));
        }
        if !a.signed_in {
            return Err(ProtoError::new(ErrorCode::AgentSignedOut, format!("{} isn't signed in on this Mac.", a.name)));
        }
        Ok(())
    }

    fn thread_create(&mut self, p: ThreadCreateParams) -> Result<Value, ProtoError> {
        self.check_agent(&p.agent)?;
        let now = now_iso();
        let t = Thread {
            id: new_id("t"),
            engine: "mac".into(),
            title: String::new(),
            effort: self.effort_for(&p.agent, &p.model, p.effort),
            agent: p.agent,
            model: p.model,
            status: ThreadStatus::Idle,
            pinned: false,
            settled_at: None,
            archived_at: None,
            created_at: now.clone(),
            updated_at: now,
            preview: String::new(),
        };
        self.store.insert_thread(&t).map_err(store_err)?;
        self.emit(Event::ThreadUpdated { thread: t.clone() });
        Ok(json!({ "thread": t }))
    }

    fn thread_update(&mut self, p: ThreadUpdateParams) -> Result<Value, ProtoError> {
        let mut t = self.thread(&p.thread_id)?;
        let now = now_iso();
        if let Some(title) = p.title {
            t.title = one_line(&title, 120);
        }
        if let Some(pinned) = p.pinned {
            t.pinned = pinned;
        }
        if let Some(settled) = p.settled {
            t.settled_at = if settled { t.settled_at.or(Some(now.clone())) } else { None };
        }
        if let Some(archived) = p.archived {
            t.archived_at = if archived { t.archived_at.or(Some(now)) } else { None };
        }
        self.save_thread(&t);
        Ok(json!({ "thread": t }))
    }

    fn thread_delete(&mut self, id: &str) -> Result<Value, ProtoError> {
        self.thread(id)?;
        self.turns.remove(id);
        self.approvals.retain(|_, a| a.thread != id);
        if let Some(mut run) = self.claude.remove(id) {
            run.session.stop();
        }
        if let Some(c) = self.codex.as_mut() {
            c.loaded.remove(id);
        }
        self.store.delete_thread(id).map_err(store_err)?;
        self.emit(Event::ThreadDeleted { thread_id: id.to_string() });
        Ok(json!({}))
    }

    // ---------- Items ----------

    /// Make an item, save it and announce it; `fill` sets its contents.
    fn add_item(&mut self, thread: &str, turn: Option<&str>, kind: ItemKind, text: Option<String>, fill: impl FnOnce(&mut Item)) -> Item {
        let mut item = Item {
            id: new_id("i"),
            thread_id: thread.to_string(),
            turn_id: turn.map(str::to_string),
            kind,
            created_at: now_iso(),
            text,
            agent: None,
            model: None,
            tool: None,
            divider: None,
        };
        fill(&mut item);
        if let Err(e) = self.store.insert_item(&item) {
            eprintln!("[agents] saving an item: {e:#}");
        }
        self.emit(Event::ItemAdded { item: item.clone() });
        item
    }

    fn saved_item(&self, thread: &str, id: &str) -> Option<Item> {
        self.store.items(thread).ok()?.into_iter().find(|i| i.id == id)
    }

    fn update_item(&self, item: &Item) {
        if let Err(e) = self.store.save_item(item) {
            eprintln!("[agents] saving an item: {e:#}");
        }
        self.emit(Event::ItemUpdated { item: item.clone() });
    }

    // ---------- Turns ----------

    fn turn_send(&mut self, p: TurnSendParams, to: &ReplyTo) -> Result<(), ProtoError> {
        let mut t = self.thread(&p.thread_id)?;
        if self.turns.contains_key(&t.id) || matches!(t.status, ThreadStatus::Running | ThreadStatus::Waiting) {
            return Err(ProtoError::new(ErrorCode::Busy, "This chat is still working. Stop it or wait for it to finish."));
        }
        let text = p.text.trim().to_string();
        if text.is_empty() {
            return Err(ProtoError::bad_request("Write a message first."));
        }
        let agent = p.agent.unwrap_or_else(|| t.agent.clone());
        self.check_agent(&agent)?;
        let model = p.model.filter(|m| !m.is_empty()).unwrap_or_else(|| if agent == t.agent { t.model.clone() } else { self.agent(&agent).map(|a| a.default_model.clone()).unwrap_or_default() });
        let asked = match p.effort {
            Some(e) => e,
            None => t.effort.clone(),
        };
        let effort = self.effort_for(&agent, &model, asked);
        let switched_agent = agent != t.agent;
        let had_items = self.store.item_count(&t.id).map_err(store_err)? > 0;
        let turn_id = new_id("u");

        if had_items && (switched_agent || model != t.model) {
            let divider = Divider { from: AgentModel { agent: t.agent.clone(), model: t.model.clone() }, to: AgentModel { agent: agent.clone(), model: model.clone() } };
            self.add_item(&t.id, Some(&turn_id), ItemKind::Divider, None, |i| i.divider = Some(divider));
        }
        if switched_agent {
            // The new agent starts fresh with a handoff; its old session (if
            // any) doesn't know what happened since.
            let _ = self.store.clear_session(&t.id, &agent);
            if let Some(mut run) = self.claude.remove(&t.id) {
                run.session.stop();
            }
            if let Some(c) = self.codex.as_mut() {
                c.loaded.remove(&t.id);
            }
        }
        let history = if had_items { self.store.items(&t.id).map_err(store_err)? } else { Vec::new() };
        let user = self.add_item(&t.id, Some(&turn_id), ItemKind::User, Some(text.clone()), |_| {});

        t.agent = agent.clone();
        t.model = model.clone();
        t.effort = effort.clone();
        t.status = ThreadStatus::Running;
        t.updated_at = now_iso();
        t.preview = one_line(&text, 140);
        self.save_thread(&t);

        let session = self.store.session(&t.id, &agent).ok().flatten();
        let has_conversation = history.iter().any(|i| matches!(i.kind, ItemKind::User | ItemKind::Assistant));
        let prompt = if session.is_none() && has_conversation { handoff::build(&history, &|a, m| self.speaker(a, m), &text) } else { text.clone() };
        self.turns.insert(
            t.id.clone(),
            Turn {
                id: turn_id.clone(),
                agent: agent.clone(),
                model,
                effort,
                keys: HashMap::new(),
                live: HashMap::new(),
                denied: Vec::new(),
                outputs: 0,
                usage: None,
                error: None,
                stop_at: None,
                resumed: session.is_some(),
                user_text: text,
                codex_turn: None,
            },
        );
        self.reply(to, Ok(json!({ "turnId": turn_id, "userItem": user })));
        self.emit(Event::TurnStarted { thread_id: t.id.clone(), turn_id });
        self.start_agent(&t.id, &prompt, session);
        Ok(())
    }

    fn mcp_url(&self) -> Option<String> {
        if self.auth.connected() {
            self.relay.as_ref().map(|r| r.url().to_string())
        } else {
            None
        }
    }

    fn start_agent(&mut self, thread: &str, prompt: &str, session: Option<String>) {
        let Some(turn) = self.turns.get(thread) else { return };
        let (agent, model, effort) = (turn.agent.clone(), turn.model.clone(), turn.effort.clone());
        if agent == claude::AGENT_ID {
            self.start_claude(thread, prompt, model, effort, session);
        } else {
            self.start_codex(thread, prompt);
        }
    }

    fn start_claude(&mut self, thread: &str, prompt: &str, model: String, effort: Option<String>, session: Option<String>) {
        let Some(bin) = self.bins.claude.clone() else {
            return self.finish_turn(thread, TurnStatus::Failed, Some("Claude Code isn't installed on this Mac.".into()), None);
        };
        let launch = claude::Launch { model, effort, resume: session, mcp_url: self.mcp_url() };
        let reuse = self.claude.get_mut(thread).is_some_and(|r| {
            let same = r.session.launch.model == launch.model && r.session.launch.effort == launch.effort && r.session.launch.mcp_url == launch.mcp_url;
            same && r.session.alive()
        });
        if !reuse {
            if let Some(mut old) = self.claude.remove(thread) {
                old.session.stop();
            }
            self.next_gen += 1;
            let gen = self.next_gen;
            let (tx, id) = (self.tx.clone(), thread.to_string());
            let sink: claude::Sink = Arc::new(move |ev| {
                let _ = tx.send(Cmd::Claude { thread: id.clone(), gen, ev });
            });
            match claude::Session::start(&bin, &self.cfg.work_dir, launch, sink) {
                Ok(session) => {
                    self.claude.insert(thread.to_string(), ClaudeRun { session, gen, idle_since: None });
                }
                Err(e) => return self.finish_turn(thread, TurnStatus::Failed, Some(format!("Claude Code didn't start ({e}).")), None),
            }
        }
        let sent = self.claude.get_mut(thread).map(|r| {
            r.idle_since = None;
            r.session.send(prompt)
        });
        if let Some(Err(e)) = sent {
            self.finish_turn(thread, TurnStatus::Failed, Some(format!("Couldn't reach Claude Code ({e}).")), None);
        }
    }

    fn start_codex(&mut self, thread: &str, prompt: &str) {
        let Some(bin) = self.bins.codex.clone() else {
            return self.finish_turn(thread, TurnStatus::Failed, Some("Codex isn't installed on this Mac.".into()), None);
        };
        let alive = self.codex.as_mut().is_some_and(|c| c.server.alive());
        if !alive {
            self.codex = None;
            self.codex_queue.push((thread.to_string(), prompt.to_string()));
            if !self.codex_starting {
                self.codex_starting = true;
                self.next_gen += 1;
                let gen = self.next_gen;
                let (tx, cwd) = (self.tx.clone(), self.cfg.work_dir.clone());
                std::thread::spawn(move || {
                    let sink_tx = tx.clone();
                    let sink: codex::IncomingSink = Arc::new(move |inc| {
                        let _ = sink_tx.send(Cmd::Codex { gen, inc });
                    });
                    let result = codex::Server::start(&bin, &cwd, sink).map_err(|e| e.to_string());
                    let _ = tx.send(Cmd::CodexServer { gen, result });
                });
            }
            return;
        }
        let Some(c) = self.codex.as_ref() else { return };
        let Some(turn) = self.turns.get(thread) else { return };
        let peer = c.server.peer.clone();
        let gen = c.gen;
        let loaded = c.loaded.get(thread).cloned();
        let resume = if loaded.is_none() { self.store.session(thread, codex::AGENT_ID).ok().flatten() } else { None };
        let (model, effort, turn_id) = (turn.model.clone(), turn.effort.clone(), turn.id.clone());
        let (tx, cwd, mcp, id, prompt) = (self.tx.clone(), self.cfg.work_dir.clone(), self.mcp_url(), thread.to_string(), prompt.to_string());
        std::thread::spawn(move || {
            let result = (|| {
                let codex_thread = match loaded {
                    Some(ct) => ct,
                    None => {
                        let started = match &resume {
                            Some(ct) => peer.request("thread/resume", codex::thread_params(&model, &cwd, mcp.as_deref(), Some(ct)), codex::REQUEST_TIMEOUT),
                            None => Err(String::new()),
                        };
                        let started = match started {
                            Ok(v) => v,
                            // Not resumable (or nothing to resume): a new thread
                            Err(_) => peer.request("thread/start", codex::thread_params(&model, &cwd, mcp.as_deref(), None), codex::REQUEST_TIMEOUT)?,
                        };
                        let ct = started.pointer("/thread/id").and_then(Value::as_str).ok_or("Codex didn't start a thread.")?.to_string();
                        let _ = tx.send(Cmd::CodexBound { thread: id.clone(), gen, codex_thread: ct.clone() });
                        ct
                    }
                };
                let started = peer.request("turn/start", codex::turn_params(&codex_thread, &prompt, &model, effort.as_deref()), codex::REQUEST_TIMEOUT)?;
                Ok::<String, String>(started.pointer("/turn/id").and_then(Value::as_str).unwrap_or("").to_string())
            })();
            let _ = tx.send(Cmd::CodexTurn { thread: id, turn: turn_id, gen, result });
        });
    }

    fn codex_server(&mut self, gen: u64, result: Result<codex::Server, String>) {
        self.codex_starting = false;
        let queued = std::mem::take(&mut self.codex_queue);
        match result {
            Ok(server) => {
                self.codex = Some(CodexRun { server, gen, loaded: HashMap::new() });
                for (thread, prompt) in queued {
                    self.start_codex(&thread, &prompt);
                }
            }
            Err(e) => {
                for (thread, _) in queued {
                    self.finish_turn(&thread, TurnStatus::Failed, Some(format!("Codex didn't start ({e}).")), None);
                }
            }
        }
    }

    fn codex_turn_started(&mut self, thread: &str, turn: &str, gen: u64, result: Result<String, String>) {
        if self.codex.as_ref().map(|c| c.gen) != Some(gen) {
            return;
        }
        let Some(t) = self.turns.get_mut(thread).filter(|t| t.id == turn) else { return };
        match result {
            Ok(codex_turn) => {
                t.codex_turn = Some(codex_turn.clone());
                if t.stop_at.is_some() {
                    self.interrupt_codex(thread, &codex_turn);
                }
            }
            Err(e) => self.finish_turn(thread, TurnStatus::Failed, Some(if e.is_empty() { "Codex couldn't start the turn.".into() } else { e }), None),
        }
    }

    fn interrupt_codex(&self, thread: &str, codex_turn: &str) {
        if let Some(c) = self.codex.as_ref() {
            if let Some(ct) = c.loaded.get(thread) {
                let _ = c.server.peer.send("turn/interrupt", json!({"threadId": ct, "turnId": codex_turn}));
            }
        }
    }

    fn codex_incoming(&mut self, gen: u64, inc: codex::Incoming) {
        if self.codex.as_ref().map(|c| c.gen) != Some(gen) {
            return;
        }
        let thread_of = |ct: &str, c: &CodexRun| c.loaded.iter().find(|(_, v)| v.as_str() == ct).map(|(k, _)| k.clone());
        match inc {
            codex::Incoming::Notification { method, params } => {
                if method == "turn/started" {
                    let ct = params.get("threadId").and_then(Value::as_str).unwrap_or("");
                    let id = params.pointer("/turn/id").and_then(Value::as_str).map(str::to_string);
                    if let Some(thread) = self.codex.as_ref().and_then(|c| thread_of(ct, c)) {
                        if let Some(t) = self.turns.get_mut(&thread) {
                            if t.codex_turn.is_none() {
                                t.codex_turn = id;
                            }
                        }
                    }
                    return;
                }
                let Some((ct, ev)) = codex::map_notification(&method, &params) else { return };
                if let Some(thread) = self.codex.as_ref().and_then(|c| thread_of(&ct, c)) {
                    self.driver_event(&thread, ev);
                }
            }
            codex::Incoming::Request { id, method, params } => {
                let mapped = codex::map_request(&id, &method, &params).and_then(|(ct, ev)| self.codex.as_ref().and_then(|c| thread_of(&ct, c)).map(|t| (t, ev)));
                match mapped {
                    Some((thread, ev)) if self.turns.contains_key(&thread) => self.driver_event(&thread, ev),
                    _ => {
                        if let Some(c) = self.codex.as_ref() {
                            let _ = c.server.peer.respond_error(&id, "Billable doesn't handle this request.");
                        }
                    }
                }
            }
            codex::Incoming::Exited { error } => {
                let c = self.codex.take();
                let threads: Vec<String> = self.turns.iter().filter(|(_, t)| t.agent == codex::AGENT_ID).map(|(id, _)| id.clone()).collect();
                drop(c);
                for thread in threads {
                    self.driver_event(&thread, DriverEvent::Exited { error: error.clone() });
                }
            }
        }
    }

    // ---------- Driver events ----------

    fn item_for(&mut self, thread: &str, key: &str, kind: ItemKind) -> (String, bool) {
        if let Some(id) = self.turns.get(thread).and_then(|t| t.keys.get(key)).cloned() {
            return (id, false);
        }
        let Some(turn) = self.turns.get(thread) else { return (String::new(), false) };
        let (turn_id, agent, model) = (turn.id.clone(), turn.agent.clone(), turn.model.clone());
        let text = matches!(kind, ItemKind::Assistant | ItemKind::Reasoning).then(String::new);
        let item = self.add_item(thread, Some(&turn_id), kind, text, |i| {
            i.agent = Some(agent);
            i.model = Some(model);
        });
        if let Some(t) = self.turns.get_mut(thread) {
            t.keys.insert(key.to_string(), item.id.clone());
            t.outputs += 1;
            if matches!(kind, ItemKind::Assistant | ItemKind::Reasoning) {
                t.live.insert(item.id.clone(), item.clone());
            }
        }
        (item.id, true)
    }

    fn text_delta(&mut self, thread: &str, key: &str, kind: ItemKind, text: String) {
        let (id, _) = self.item_for(thread, key, kind);
        let Some(t) = self.turns.get_mut(thread) else { return };
        if let Some(item) = t.live.get_mut(&id) {
            item.text.get_or_insert_with(String::new).push_str(&text);
            self.emit(Event::ItemDelta { thread_id: thread.to_string(), item_id: id, delta: text });
        }
    }

    fn text_done(&mut self, thread: &str, key: &str, kind: ItemKind, text: String) {
        if text.trim().is_empty() && !self.turns.get(thread).is_some_and(|t| t.keys.contains_key(key)) {
            return;
        }
        let (id, _) = self.item_for(thread, key, kind);
        let Some(t) = self.turns.get_mut(thread) else { return };
        let Some(mut item) = t.live.remove(&id).or_else(|| self.saved_item(thread, &id)) else { return };
        let changed = item.text.as_deref() != Some(text.as_str());
        item.text = Some(text);
        if changed {
            self.update_item(&item);
        } else if let Err(e) = self.store.save_item(&item) {
            eprintln!("[agents] saving an item: {e:#}");
        }
        if kind == ItemKind::Assistant {
            if let Ok(Some(mut th)) = self.store.thread(thread) {
                th.preview = one_line(item.text.as_deref().unwrap_or(""), 140);
                th.updated_at = now_iso();
                self.save_thread(&th);
            }
        }
    }

    fn driver_event(&mut self, thread: &str, ev: DriverEvent) {
        match ev {
            DriverEvent::Session(id) => {
                if let Some(agent) = self.turns.get(thread).map(|t| t.agent.clone()) {
                    let _ = self.store.set_session(thread, &agent, &id, &now_iso());
                }
            }
            _ if !self.turns.contains_key(thread) => {}
            DriverEvent::TextDelta { key, text } => self.text_delta(thread, &key, ItemKind::Assistant, text),
            DriverEvent::ReasoningDelta { key, text } => self.text_delta(thread, &key, ItemKind::Reasoning, text),
            DriverEvent::TextDone { key, text } => self.text_done(thread, &key, ItemKind::Assistant, text),
            DriverEvent::ReasoningDone { key, text } => self.text_done(thread, &key, ItemKind::Reasoning, text),
            DriverEvent::ToolStarted { key, name, input } => {
                let (id, created) = self.item_for(thread, &key, ItemKind::Tool);
                let (title, detail) = tools::describe(&name, &input);
                let mut item = match self.saved_item(thread, &id) {
                    Some(i) => i,
                    None => return,
                };
                let keep_old = !created && input.as_object().is_some_and(|o| o.is_empty());
                if keep_old && item.tool.is_some() {
                    return;
                }
                let status = item.tool.as_ref().map(|t| t.status).unwrap_or(ToolStatus::Running);
                let output = item.tool.as_ref().and_then(|t| t.output.clone());
                item.tool = Some(ToolInfo { feedhq: tools::is_feedhq(&name), name, title, detail, status, output });
                self.update_item(&item);
            }
            DriverEvent::ToolFinished { key, output, failed, declined } => {
                let Some(id) = self.turns.get(thread).and_then(|t| t.keys.get(&key)).cloned() else { return };
                let denied = declined || self.turns.get(thread).is_some_and(|t| t.denied.contains(&id));
                let Some(mut item) = self.saved_item(thread, &id) else { return };
                if let Some(tool) = item.tool.as_mut() {
                    tool.status = if denied {
                        ToolStatus::Denied
                    } else if failed {
                        ToolStatus::Failed
                    } else {
                        ToolStatus::Done
                    };
                    tool.output = output.map(|o| cut(&o, 4000));
                }
                self.update_item(&item);
            }
            DriverEvent::Approval { native, tool_key, name, input, options } => self.approval_requested(thread, native, tool_key, &name, &input, options),
            DriverEvent::ApprovalWithdrawn { native } => {
                if let Some(id) = self.approvals.iter().find(|(_, a)| a.thread == thread && a.native == native).map(|(id, _)| id.clone()) {
                    self.resolve_approval(&id, None);
                }
            }
            DriverEvent::Usage(u) => {
                if let Some(t) = self.turns.get_mut(thread) {
                    t.usage = Some(u);
                }
            }
            DriverEvent::Error(m) => {
                if let Some(t) = self.turns.get_mut(thread) {
                    t.error = Some(m);
                }
            }
            DriverEvent::TurnDone { status, error, usage } => self.finish_turn(thread, status, error, usage),
            DriverEvent::Exited { error } => {
                let agent = self.turns.get(thread).map(|t| t.agent.clone()).unwrap_or_default();
                let name = self.agent(&agent).map(|a| a.name.clone()).unwrap_or_else(|| "The agent".into());
                let message = match error {
                    Some(e) => format!("{name} stopped: {e}"),
                    None => format!("{name} stopped unexpectedly."),
                };
                self.finish_turn(thread, TurnStatus::Failed, Some(message), None);
            }
        }
    }

    // ---------- Approvals ----------

    fn approval_requested(&mut self, thread: &str, native: String, tool_key: Option<String>, name: &str, input: &Value, options: Vec<Decision>) {
        let Some(turn) = self.turns.get(thread) else { return };
        let agent = turn.agent.clone();
        let item_id = tool_key.as_ref().and_then(|k| turn.keys.get(k)).cloned();
        let (title, mut detail) = tools::approval(name, input);
        if detail.trim().is_empty() || detail.trim() == "{}" {
            if let Some(d) = item_id.as_ref().and_then(|id| self.saved_item(thread, id)).and_then(|i| i.tool.and_then(|t| t.detail)) {
                detail = cut(&d, 2000);
            }
        }
        let approval = Approval { id: new_id("a"), thread_id: thread.to_string(), item_id: item_id.clone(), title, detail, options };
        if let Err(e) = self.store.insert_approval(&approval, &now_iso()) {
            eprintln!("[agents] saving an approval: {e:#}");
        }
        self.approvals.insert(approval.id.clone(), OpenApproval { thread: thread.to_string(), native, agent, item_id });
        self.set_status(thread);
        self.emit(Event::ApprovalRequested { approval });
    }

    /// Running, or waiting while an approval is open.
    fn set_status(&self, thread: &str) {
        let Ok(Some(mut t)) = self.store.thread(thread) else { return };
        let status = if !self.turns.contains_key(thread) {
            return;
        } else if self.approvals.values().any(|a| a.thread == thread) {
            ThreadStatus::Waiting
        } else {
            ThreadStatus::Running
        };
        if t.status != status {
            t.status = status;
            self.save_thread(&t);
        }
    }

    /// Close an approval; `None`: withdrawn or expired (shown as denied).
    fn resolve_approval(&mut self, id: &str, decision: Option<Decision>) -> Option<OpenApproval> {
        let a = self.approvals.remove(id)?;
        let _ = self.store.resolve_approval(id, decision, &now_iso());
        if decision.is_none_or(|d| d == Decision::Deny) {
            if let (Some(item), Some(t)) = (a.item_id.clone(), self.turns.get_mut(&a.thread)) {
                t.denied.push(item);
            }
        }
        self.emit(Event::ApprovalResolved { thread_id: a.thread.clone(), approval_id: id.to_string(), decision: decision.unwrap_or(Decision::Deny) });
        self.set_status(&a.thread);
        Some(a)
    }

    fn approval_answer(&mut self, p: ApprovalAnswerParams) -> Result<Value, ProtoError> {
        let belongs = self.approvals.get(&p.approval_id).is_some_and(|a| a.thread == p.thread_id);
        if !belongs {
            return Err(ProtoError::new(ErrorCode::NotFound, "That question was already answered."));
        }
        let a = self.resolve_approval(&p.approval_id, Some(p.decision)).ok_or_else(ProtoError::not_found)?;
        self.answer_agent(&a, p.decision);
        Ok(json!({}))
    }

    fn answer_agent(&mut self, a: &OpenApproval, decision: Decision) {
        if a.agent == claude::AGENT_ID {
            if let Some(run) = self.claude.get_mut(&a.thread) {
                let _ = run.session.answer(&a.native, decision);
            }
        } else if let (Some(c), Some((kind, id))) = (self.codex.as_ref(), codex::parse_native(&a.native)) {
            let _ = c.server.peer.respond(&id, codex::answer_result(kind, decision));
        }
    }

    // ---------- Stop and finish ----------

    fn turn_stop(&mut self, thread: &str) -> Result<Value, ProtoError> {
        self.thread(thread)?;
        let Some(turn) = self.turns.get_mut(thread) else { return Ok(json!({})) };
        if turn.stop_at.is_some() {
            return Ok(json!({}));
        }
        turn.stop_at = Some(Instant::now() + STOP_GRACE);
        let (agent, codex_turn) = (turn.agent.clone(), turn.codex_turn.clone());
        // Questions still open are answered "no".
        let open: Vec<String> = self.approvals.iter().filter(|(_, a)| a.thread == thread).map(|(id, _)| id.clone()).collect();
        for id in open {
            if let Some(a) = self.resolve_approval(&id, Some(Decision::Deny)) {
                self.answer_agent(&a, Decision::Deny);
            }
        }
        if agent == claude::AGENT_ID {
            match self.claude.get_mut(thread) {
                Some(run) => {
                    let _ = run.session.interrupt();
                }
                None => self.finish_turn(thread, TurnStatus::Stopped, None, None),
            }
        } else if let Some(ct) = codex_turn {
            self.interrupt_codex(thread, &ct);
        } else if self.codex.is_none() && !self.codex_starting {
            self.finish_turn(thread, TurnStatus::Stopped, None, None);
        }
        Ok(json!({}))
    }

    fn finish_turn(&mut self, thread: &str, status: TurnStatus, error: Option<String>, usage: Option<Usage>) {
        let Some(turn) = self.turns.get(thread) else { return };
        let stopped = turn.stop_at.is_some();
        // A resumed session that failed straight away (e.g. its transcript is
        // gone): start over with a handoff, once.
        if status == TurnStatus::Failed && turn.resumed && turn.outputs == 0 && !stopped {
            let (agent, text) = (turn.agent.clone(), turn.user_text.clone());
            let _ = self.store.clear_session(thread, &agent);
            if let Some(mut run) = self.claude.remove(thread) {
                run.session.stop();
            }
            if let Some(c) = self.codex.as_mut() {
                c.loaded.remove(thread);
            }
            let mut history = self.store.items(thread).unwrap_or_default();
            // The message being retried goes after the handoff, not in it.
            if let Some(pos) = history.iter().rposition(|i| i.kind == ItemKind::User) {
                history.truncate(pos);
            }
            let prompt = handoff::build(&history, &|a, m| self.speaker(a, m), &text);
            if let Some(t) = self.turns.get_mut(thread) {
                t.resumed = false;
            }
            return self.start_agent(thread, &prompt, None);
        }
        let Some(mut turn) = self.turns.remove(thread) else { return };
        let status = if stopped { TurnStatus::Stopped } else { status };
        // Whatever was still streaming is saved as it stands.
        for (_, item) in turn.live.drain() {
            self.update_item(&item);
        }
        if let Ok(items) = self.store.items(thread) {
            for mut item in items.into_iter().filter(|i| i.turn_id.as_deref() == Some(turn.id.as_str())) {
                if let Some(tool) = item.tool.as_mut().filter(|t| t.status == ToolStatus::Running) {
                    tool.status = if status == TurnStatus::Done { ToolStatus::Done } else { ToolStatus::Failed };
                    self.update_item(&item);
                }
            }
        }
        let open: Vec<String> = self.approvals.iter().filter(|(_, a)| a.thread == thread).map(|(id, _)| id.clone()).collect();
        for id in open {
            self.resolve_approval(&id, None);
        }
        let error = if status == TurnStatus::Stopped { None } else { error.or(turn.error.take()) };
        if status == TurnStatus::Failed {
            let message = error.clone().unwrap_or_else(|| "Something went wrong.".into());
            self.add_item(thread, Some(&turn.id), ItemKind::Error, Some(message), |_| {});
        }
        if let Some(run) = self.claude.get_mut(thread) {
            run.idle_since = Some(Instant::now());
        }
        let Ok(Some(mut t)) = self.store.thread(thread) else { return };
        t.status = if status == TurnStatus::Failed { ThreadStatus::Failed } else { ThreadStatus::Idle };
        t.updated_at = now_iso();
        let needs_title = t.title.is_empty() && status == TurnStatus::Done;
        self.save_thread(&t);
        self.emit(Event::TurnFinished { thread_id: thread.to_string(), turn_id: turn.id.clone(), status, error, usage: usage.or(turn.usage) });
        if needs_title {
            self.start_title(&t, None);
        }
    }

    // ---------- Titles ----------

    fn start_title(&mut self, t: &Thread, reply: Option<ReplyTo>) {
        let first = self.store.items(&t.id).ok().and_then(|items| items.into_iter().find(|i| i.kind == ItemKind::User)).and_then(|i| i.text).unwrap_or_default();
        if first.trim().is_empty() {
            if let Some(to) = reply {
                self.reply(&to, Ok(json!({ "thread": t })));
            }
            return;
        }
        let (tx, id, cwd) = (self.tx.clone(), t.id.clone(), self.cfg.work_dir.clone());
        let codex_model = self.agent(codex::AGENT_ID).and_then(|a| codex::title_model(&a.models));
        let job: Box<dyn FnOnce() -> Option<String> + Send> = match (t.agent.as_str(), self.bins.claude.clone(), self.bins.codex.clone()) {
            ("claude", Some(bin), _) => Box::new(move || claude::title(&bin, &cwd, &title_prompt(&first))),
            ("codex", _, Some(bin)) => Box::new(move || codex::title(&bin, &cwd, codex_model.as_deref(), &title_prompt(&first))),
            _ => Box::new(|| None),
        };
        let fallback = first_words(&self.store.items(&t.id).ok().and_then(|items| items.into_iter().find(|i| i.kind == ItemKind::User)).and_then(|i| i.text).unwrap_or_default(), 6);
        std::thread::spawn(move || {
            let title = job().and_then(|raw| clean_title(&raw)).unwrap_or_else(|| cut(&fallback, 60));
            let _ = tx.send(Cmd::Titled { thread: id, title, reply });
        });
    }

    fn titled(&mut self, thread: &str, title: String, reply: Option<ReplyTo>) {
        let Ok(Some(mut t)) = self.store.thread(thread) else {
            if let Some(to) = reply {
                self.reply(&to, Err(ProtoError::not_found()));
            }
            return;
        };
        // An automatic title never replaces one the person set meanwhile.
        if reply.is_some() || t.title.is_empty() {
            t.title = title;
            self.save_thread(&t);
        }
        if let Some(to) = reply {
            self.reply(&to, Ok(json!({ "thread": t })));
        }
    }

    // ---------- FeedHQ ----------

    fn feedhq_connect(&mut self) -> Value {
        let pkce = oauth::pkce();
        let state = oauth::new_state();
        let url = oauth::authorize_url(self.auth.origin(), &pkce.challenge, &state);
        self.sign_ins.insert(state, (pkce.verifier, Instant::now()));
        json!({ "authorizeUrl": url })
    }

    fn oauth_redirect(&mut self, url: &str) {
        let Some(r) = oauth::parse_redirect(url) else { return };
        let Some((verifier, _)) = r.state.as_ref().and_then(|s| self.sign_ins.remove(s)) else {
            eprintln!("[agents] a FeedHQ sign-in came back that Billable didn't start");
            return;
        };
        if let Some(iss) = &r.iss {
            if crate::util::origin_of(iss).as_deref() != Some(self.auth.origin()) {
                return self.command(Cmd::SignInFailed(format!("the sign-in came from {iss}")));
            }
        }
        let Some(code) = r.code else {
            let why = r.error_description.or(r.error).unwrap_or_else(|| "no code".into());
            return self.command(Cmd::SignInFailed(why));
        };
        let (auth, tx) = (self.auth.clone(), self.tx.clone());
        std::thread::spawn(move || {
            if let Err(e) = auth.exchange_code(&code, &verifier) {
                let _ = tx.send(Cmd::SignInFailed(e.to_string()));
            }
        });
    }

    fn feedhq_changed(&mut self) {
        // Sessions pick up (or drop) the connector on their next turn.
        let idle: Vec<String> = self.claude.keys().filter(|t| !self.turns.contains_key(*t)).cloned().collect();
        for t in idle {
            if let Some(mut run) = self.claude.remove(&t) {
                run.session.stop();
            }
        }
        if let Some(c) = self.codex.as_mut() {
            let busy: Vec<String> = self.turns.keys().cloned().collect();
            c.loaded.retain(|t, _| busy.contains(t));
        }
        self.emit(Event::FeedhqStatus { connected: self.auth.connected(), account: self.auth.account() });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_are_cleaned() {
        assert_eq!(clean_title("\"Instagram Pottery Plan.\"\n").as_deref(), Some("Instagram Pottery Plan"));
        assert_eq!(clean_title("Title: **Weekly posts**").as_deref(), Some("Weekly posts"));
        assert_eq!(clean_title("  \n "), None);
        assert!(title_prompt("hi").ends_with("Message: hi"));
    }
}
