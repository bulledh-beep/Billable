//! The engine: one thread that owns the chats, the agent processes and the
//! FeedHQ connection, and answers the protocol (PROTOCOL.md).
//!
//! Requests come in with [`Engine::handle`] (the web view inside Billable)
//! or [`Engine::handle_remote`] (the FeedHQ relay). Replies go back to where
//! the request came from; events go to both. Nothing here blocks the
//! caller: anything slow (finding the CLIs, titles, sign-in) runs on a
//! helper thread and reports back to the engine thread.
//!
//! An orchestrator thread's session also gets the relay's `billable` MCP
//! server (orchestrator.rs). Its tool calls arrive as [`Cmd::Orchestrate`]
//! and start workers: ordinary threads with a parent, at most
//! [`orchestrator::MAX_RUNNING`] running at once per orchestrator, the rest
//! queued. A wait_for_agents call is parked and answered when its workers
//! finish, at its timeout, or when the orchestrator stops.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::{Path, PathBuf},
    sync::{
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use serde_json::{json, Value};

use crate::{
    attachments, claude, codex, commands, computer,
    discovery::{self, Binaries},
    driver::DriverEvent,
    guard, handoff,
    oauth::{self, FeedhqAuth},
    orchestrator::{self, Call, Seat},
    protocol::*,
    relay::{Orchestrate, Relay},
    remote::RemoteEngine,
    secrets::SecretStore,
    servers::{self, Places},
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

/// Answers a pick: what was chosen (empty when cancelled).
pub type FolderAnswer = Box<dyn FnOnce(Vec<PathBuf>) + Send>;
/// What the picker is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickKind {
    /// One folder (`system.pickFolder`).
    Folder,
    /// Files and folders to attach, several at once (`system.pickFiles`).
    Files,
}
/// Shows the Mac's picker. It's called on the engine thread and must return
/// at once, calling the answer later (from any thread).
pub type FolderPicker = Arc<dyn Fn(PickKind, FolderAnswer) + Send + Sync>;

/// The `features` this engine adds to protocol v1.
pub const FEATURES: &[&str] = &["folders", "orchestrator", "bulk-delete", "coordinator", "decisions", "memory", "lookups", "review", "tools", "computer", "attachments", "modes"];
/// At most this many chats per threads.delete.
const MAX_BULK_DELETE: usize = 200;

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
    /// Ask FeedHQ for its OAuth issuer, which may differ from the
    /// configured address (e.g. a vercel.app alias of feedhq.app).
    pub discover_issuer: bool,
    /// Billable's folder picker (`None`: system.pickFolder is refused).
    pub folder_picker: Option<FolderPicker>,
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
            discover_issuer: true,
            folder_picker: None,
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
    Agents { agents: Vec<Agent>, bins: Binaries, reply: Option<ReplyTo> },
    /// Models a Codex app-server the user started listed.
    CodexModels { models: Vec<Model>, default: String },
    FeedhqChanged,
    SignInFailed(String),
    /// A chat's call through the relay's `billable` server.
    Orchestrate { thread: String, call: Call, reply: Sender<Result<Value, String>> },
    FolderPicked { to: ReplyTo, path: Option<PathBuf> },
    /// A message's uploads came down (or one didn't).
    Downloaded { begun: Box<Begun>, failed: Option<String> },
    /// An answer worked out off the engine thread.
    Answer { to: ReplyTo, result: Result<Value, ProtoError> },
    /// A Tools change or check finished (servers.rs): the list is sent again.
    ToolsDone { to: ReplyTo, cwd: Option<PathBuf>, result: Result<Option<servers::Statuses>, String> },
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
            let hook_tx = state_tx.clone();
            let hook: Orchestrate = Arc::new(move |thread, call| {
                let (reply, answer) = mpsc::channel();
                hook_tx.send(Cmd::Orchestrate { thread: thread.to_string(), call, reply }).map_err(|_| "Billable's agents have stopped.".to_string())?;
                answer.recv_timeout(Duration::from_secs(orchestrator::MAX_WAIT_SECS + 60)).unwrap_or_else(|_| Err("Billable didn't answer in time.".into()))
            });
            let relay = Relay::start(auth.clone(), Some(hook)).map_err(|e| eprintln!("[agents] the FeedHQ relay didn't start: {e}")).ok();
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
    /// Codex's id for this turn, once it's known.
    codex_turn: Option<String>,
    /// A worker's orchestrator.
    parent: Option<String>,
    /// A lookup: read-only, its own slots, never asks the person.
    lookup: bool,
    /// The message's images, for the agent's own image inputs.
    images: Vec<PathBuf>,
}

/// An ask_person or propose_plan call, answered when the person picks on
/// its card, or at `until` (the card stays open).
struct Ask {
    thread: String,
    item: String,
    until: Instant,
    reply: Sender<Result<Value, String>>,
    /// A computer tool waiting on the person's OK: (tool, args, grant).
    computer: Option<(String, Value, String)>,
}

/// A wait_for_agents call, answered when its workers are done or at `until`.
struct Wait {
    orchestrator: String,
    ids: Vec<String>,
    until: Instant,
    reply: Sender<Result<Value, String>>,
}

/// A turn ready to start (turn.send, or an orchestrator's worker).
struct Begun {
    thread: String,
    turn_id: String,
    user: Item,
    prompt: String,
    session: Option<String>,
    /// Uploads to fetch before the agent starts (link → path).
    downloads: Vec<(String, PathBuf)>,
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
    /// Workers waiting for a free slot, oldest first: (orchestrator, worker, message).
    worker_queue: VecDeque<(String, String, String)>,
    /// How each worker's latest turn ended (this run).
    worker_ends: HashMap<String, TurnStatus>,
    waits: Vec<Wait>,
    asks: Vec<Ask>,
    /// Claude Code's MCP servers as they last came up: name → (status, error).
    mcp_status: servers::Statuses,
    /// Claude Code's slash commands, as its latest session listed them.
    claude_commands: Vec<String>,
    /// The chat using the screen (one at a time, until its turn ends).
    screen_holder: Option<String>,
    /// What the person allowed each chat on the screen: "screen", "app:<name>".
    grants: HashMap<String, HashSet<String>>,
    /// Open "Let this chat use …?" cards: item id → (thread, grant).
    computer_cards: HashMap<String, (String, String)>,
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

/// A chat's title: the first 6 words of its first message. No model is
/// asked, so naming a chat never uses the person's subscription.
pub fn title_from(first_message: &str) -> String {
    let words = first_words(first_message, 6);
    let t = words.trim().trim_end_matches(['.', ',', '!', '?', ':', ';']).trim();
    cut(t, 60)
}

/// The CLIs' own words when they're signed out (Claude Code: "Invalid API
/// key · Please run /login", "not logged in"; Codex: "Not logged in",
/// 401 Unauthorized).
pub fn looks_signed_out(error: &str) -> bool {
    let e = error.to_ascii_lowercase();
    ["/login", "not logged in", "please log in", "please login", "codex login", "unauthorized", "authentication_error", "oauth token has expired"].iter().any(|m| e.contains(m))
}

/// A chat's seat; `None` for a lookup, which gets no Billable tools.
fn seat_of(t: &Thread) -> Option<Seat> {
    match (t.role, t.worker_kind) {
        (Some(ThreadRole::Orchestrator), _) => Some(Seat::Coordinator),
        (Some(ThreadRole::Worker), Some(WorkerKind::Lookup)) => None,
        (Some(ThreadRole::Worker), _) => Some(Seat::Job),
        (None, _) => Some(Seat::Chat),
    }
}

fn kind_of(t: &Thread) -> &'static str {
    t.worker_kind.unwrap_or(WorkerKind::Job).as_str()
}

/// How many of a kind run at once per coordinator.
fn slots(lookup: bool) -> usize {
    if lookup {
        orchestrator::MAX_LOOKUPS_RUNNING
    } else {
        orchestrator::MAX_RUNNING
    }
}

/// A FeedHQ project id as the screen sends it (a uuid, or any short id).
fn clean_project(id: Option<&str>) -> Option<String> {
    id.map(str::trim).filter(|p| !p.is_empty() && p.len() <= 80 && p.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')).map(str::to_string)
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
            worker_queue: VecDeque::new(),
            worker_ends: HashMap::new(),
            waits: Vec::new(),
            asks: Vec::new(),
            mcp_status: servers::Statuses::new(),
            screen_holder: None,
            claude_commands: Vec::new(),
            grants: HashMap::new(),
            computer_cards: HashMap::new(),
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
        // Workers still waiting for a slot when Billable closed don't start
        // on their own now: nothing runs that the person didn't just ask for.
        if let Ok(threads) = self.store.threads(true) {
            for mut t in threads.into_iter().filter(|t| t.role == Some(ThreadRole::Worker) && t.preview == orchestrator::WAITING_PREVIEW) {
                if self.store.item_count(&t.id).unwrap_or(0) == 0 {
                    t.preview = "Didn't start: Billable was closed.".into();
                    let _ = self.store.save_thread(&t);
                }
            }
        }
        // A card still waiting for the person keeps its chat at Needs you.
        if let Ok(open) = self.store.open_decisions(None) {
            let mut seen: Vec<String> = Vec::new();
            for item in open {
                if !seen.contains(&item.thread_id) {
                    seen.push(item.thread_id.clone());
                    self.set_status(&item.thread_id);
                }
            }
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
        for w in self.waits.drain(..) {
            let _ = w.reply.send(Err("Billable is closing.".into()));
        }
        for a in self.asks.drain(..) {
            let _ = a.reply.send(Err("Billable is closing.".into()));
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
            Cmd::Agents { agents, bins, reply } => self.agents_found(agents, bins, reply),
            Cmd::CodexModels { models, default } => self.models_reported(codex::AGENT_ID, models, default),
            Cmd::FeedhqChanged => self.feedhq_changed(),
            Cmd::SignInFailed(message) => {
                eprintln!("[agents] FeedHQ sign-in failed: {message}");
                self.emit(Event::FeedhqStatus { connected: self.auth.connected(), account: self.auth.account() });
            }
            Cmd::Orchestrate { thread, call, reply } => self.orchestrate(&thread, call, reply),
            Cmd::FolderPicked { to, path } => self.reply(&to, Ok(json!({ "path": path.map(|p| p.to_string_lossy().to_string()) }))),
            Cmd::Downloaded { begun, failed } => match failed {
                None => self.launch(*begun),
                Some(e) => {
                    self.emit(Event::TurnStarted { thread_id: begun.thread.clone(), turn_id: begun.turn_id.clone() });
                    self.finish_turn(&begun.thread, TurnStatus::Failed, Some(e), None);
                }
            },
            Cmd::Answer { to, result } => self.reply(&to, result),
            Cmd::ToolsDone { to, cwd, result } => {
                let result = result.map_err(ProtoError::failed).map(|checked| {
                    if let Some(checked) = checked {
                        self.mcp_status.extend(checked);
                    }
                    self.tools_list(cwd.as_deref())
                });
                self.reply(&to, result);
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
        if !self.waits.is_empty() {
            self.check_waits();
        }
        if !self.asks.is_empty() {
            let (late, keep): (Vec<Ask>, Vec<Ask>) = std::mem::take(&mut self.asks).into_iter().partition(|a| now >= a.until);
            self.asks = keep;
            for a in late {
                let _ = a.reply.send(Ok(json!({"answered": false, "note": "No answer yet. The card stays open: finish what you can without it, say what you're waiting on, and end your turn. Their answer will arrive as a new message."})));
            }
        }
    }

    fn request(&mut self, src: Source, json: &str) {
        let Ok(raw) = serde_json::from_str::<Value>(json) else {
            return;
        };
        let Some(id) = raw.get("id").and_then(Value::as_str).map(str::to_string) else {
            return;
        };
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
            "threads.delete" => params::<ThreadsDeleteParams>(p).and_then(|p| self.threads_delete(p.thread_ids)),
            // Titles never cost a model call; the person renames instead.
            "thread.retitle" => Err(ProtoError::bad_request("Rename it instead.")),
            "turn.send" => match params::<TurnSendParams>(p).and_then(|p| self.turn_send(p, &to)) {
                // turn_send replies itself, before the turn starts.
                Ok(()) => return,
                Err(e) => Err(e),
            },
            "turn.stop" => params::<TurnStopParams>(p).and_then(|p| self.turn_stop_with(&p.thread_id, p.jobs.unwrap_or(true))),
            "approval.answer" => params::<ApprovalAnswerParams>(p).and_then(|p| self.approval_answer(p)),
            "decision.answer" => params::<DecisionAnswerParams>(p).and_then(|p| self.decision_answer(p)),
            "memory.get" => params::<MemoryGetParams>(p).and_then(|p| self.memory_get(&p.project_id)),
            "memory.set" => params::<MemorySetParams>(p).and_then(|p| self.memory_set(&p.project_id, &p.text)),
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
            "system.pickFolder" => match (src, self.cfg.folder_picker.clone()) {
                (Source::Remote, _) => Err(ProtoError::bad_request("Pick the folder on your Mac.")),
                (Source::Local, None) => Err(ProtoError::bad_request("Billable can't show a folder picker here.")),
                (Source::Local, Some(picker)) => {
                    // Answered when the person picks (or cancels); the engine
                    // carries on meanwhile.
                    let (tx, reply_to) = (self.tx.clone(), to.clone());
                    picker(PickKind::Folder, Box::new(move |paths| {
                        let _ = tx.send(Cmd::FolderPicked { to: reply_to, path: paths.into_iter().next() });
                    }));
                    return;
                }
            },
            "tools.list" => params::<ToolsParams>(p).map(|p| self.tools_list(self.tools_cwd(p.cwd.as_deref()).as_deref())),
            "tools.check" | "tools.add" | "tools.remove" | "tools.import" => match params::<ToolsParams>(p) {
                Ok(p) => {
                    self.tools_change(req.method.as_str(), p, to);
                    return;
                }
                Err(e) => Err(e),
            },
            "files.search" => {
                let root = p.get("cwd").and_then(Value::as_str).filter(|_| src == Source::Local).map(PathBuf::from).filter(|c| c.is_dir()).or_else(|| p.get("threadId").and_then(Value::as_str).and_then(|t| self.work_dir(t).ok()));
                let Some(root) = root else {
                    return self.reply(&to, Err(ProtoError::bad_request("This chat has no folder.")));
                };
                let query = p.get("query").and_then(Value::as_str).unwrap_or("").to_string();
                let tx = self.tx.clone();
                std::thread::spawn(move || {
                    let files = attachments::project_files(&root);
                    let hits = attachments::search(&root, &files, &query, 30);
                    let _ = tx.send(Cmd::Answer { to, result: Ok(json!({ "root": root.to_string_lossy(), "results": hits })) });
                });
                return;
            }
            "commands.list" => {
                let cwd = p.get("cwd").and_then(Value::as_str).filter(|_| src == Source::Local).map(PathBuf::from).filter(|c| c.is_dir()).or_else(|| p.get("threadId").and_then(Value::as_str).and_then(|t| self.work_dir(t).ok()));
                let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
                Ok(json!({ "commands": commands::list(&home, cwd.as_deref(), &self.claude_commands) }))
            }
            "system.pickFiles" => match (src, self.cfg.folder_picker.clone()) {
                (Source::Remote, _) => Err(ProtoError::bad_request("Pick files on your Mac.")),
                (Source::Local, None) => Err(ProtoError::bad_request("Billable can't show a file picker here.")),
                (Source::Local, Some(picker)) => {
                    let (tx, reply_to) = (self.tx.clone(), to.clone());
                    picker(PickKind::Files, Box::new(move |paths| {
                        let paths: Vec<String> = paths.iter().map(|p| p.to_string_lossy().to_string()).collect();
                        let _ = tx.send(Cmd::Answer { to: reply_to, result: Ok(json!({ "paths": paths })) });
                    }));
                    return;
                }
            },
            "computer.allow" => {
                match p.get("which").and_then(Value::as_str) {
                    Some("screen") => computer::request_screen(),
                    Some("control") => computer::request_control(),
                    _ => {}
                }
                Ok(json!({ "computer": computer::access().to_json() }))
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
            "features": FEATURES,
        })
    }

    // ---------- Agents ----------

    fn refresh_agents(&mut self, reply: Option<ReplyTo>) {
        if self.refreshing && reply.is_none() {
            return;
        }
        self.refreshing = true;
        let opts = discovery::Options { claude_bin: self.cfg.claude_bin.clone(), codex_bin: self.cfg.codex_bin.clone(), login_shell: self.cfg.login_shell };
        let (tx, cached) = (self.tx.clone(), self.agents.clone());
        std::thread::spawn(move || {
            let (agents, bins) = discovery::detect(&opts, &cached);
            let _ = tx.send(Cmd::Agents { agents, bins, reply });
        });
    }

    fn agents_found(&mut self, agents: Vec<Agent>, bins: Binaries, reply: Option<ReplyTo>) {
        // A hello may have been answered from last time's list, so any
        // change is announced, the first detection included.
        let changed = agents != self.agents;
        self.agents = agents;
        self.bins = bins;
        self.agents_ready = true;
        self.refreshing = false;
        self.save_agents();
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

    fn save_agents(&self) {
        if let Ok(s) = serde_json::to_string(&self.agents) {
            let _ = self.store.set_meta("agents", &s);
        }
    }

    /// An agent's models, as its CLI listed them in a session the user
    /// started (never asked for in the background).
    fn models_reported(&mut self, id: &str, models: Vec<Model>, default: String) {
        let Some(a) = self.agents.iter_mut().find(|a| a.id == id) else {
            return;
        };
        if a.models == models && a.default_model == default {
            return;
        }
        a.models = models;
        a.default_model = default;
        self.save_agents();
        self.emit(Event::AgentsChanged { agents: self.agents.clone() });
    }

    /// A turn failed with what reads like the CLI's own "sign in" error:
    /// show the agent as signed out until the next look says otherwise.
    fn note_sign_in_error(&mut self, id: &str, error: &str) {
        if !looks_signed_out(error) {
            return;
        }
        let Some(a) = self.agents.iter_mut().find(|a| a.id == id) else {
            return;
        };
        if !a.signed_in {
            return;
        }
        a.signed_in = false;
        self.save_agents();
        self.emit(Event::AgentsChanged { agents: self.agents.clone() });
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
        let cwd = match p.cwd.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
            Some(c) if Path::new(c).is_absolute() && Path::new(c).is_dir() => Some(c.to_string()),
            Some(_) => return Err(ProtoError::bad_request("That folder isn't on this Mac.")),
            None => None,
        };
        let role = match p.role.as_deref().filter(|r| !r.is_empty()) {
            None => None,
            Some("orchestrator") => Some(ThreadRole::Orchestrator),
            Some(_) => return Err(ProtoError::bad_request("A new chat can only be an orchestrator.")),
        };
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
            cwd,
            role,
            parent_id: None,
            project_id: clean_project(p.project_id.as_deref()),
            worker_kind: None,
            review_at: None,
            // A coordinator starts trusted (the person's pick, 2026-11-28).
            mode: Some(p.mode.unwrap_or(if role == Some(ThreadRole::Orchestrator) { Mode::Trusted } else { Mode::Ask })),
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
            // Marking a job done is approving its review.
            if settled {
                t.review_at = None;
            }
        }
        if let Some(project) = p.project_id {
            t.project_id = clean_project(project.as_deref());
            // Its jobs follow it.
            for mut w in self.store.workers(&t.id).map_err(store_err)? {
                w.project_id = t.project_id.clone();
                self.save_thread(&w);
            }
        }
        if let Some(archived) = p.archived {
            t.archived_at = if archived { t.archived_at.or(Some(now)) } else { None };
        }
        if let Some(mode) = p.mode {
            t.mode = Some(mode);
            // Its jobs follow it; what's waiting is looked at again.
            for mut w in self.store.workers(&t.id).map_err(store_err)? {
                w.mode = Some(mode);
                self.save_thread(&w);
            }
        }
        self.save_thread(&t);
        Ok(json!({ "thread": t }))
    }

    fn thread_delete(&mut self, id: &str) -> Result<Value, ProtoError> {
        self.delete_tree(id)?;
        self.check_waits();
        Ok(json!({}))
    }

    fn threads_delete(&mut self, ids: Vec<String>) -> Result<Value, ProtoError> {
        if ids.len() > MAX_BULK_DELETE {
            return Err(ProtoError::bad_request(format!("Delete at most {MAX_BULK_DELETE} chats at once.")));
        }
        let mut deleted: Vec<String> = Vec::new();
        for id in ids {
            // Unknown ids (or workers already gone with their orchestrator) are skipped.
            if deleted.contains(&id) || self.store.thread(&id).map_err(store_err)?.is_none() {
                continue;
            }
            deleted.extend(self.delete_tree(&id)?);
        }
        self.check_waits();
        Ok(json!({ "deleted": deleted }))
    }

    /// Delete a chat and, for an orchestrator, its workers. Returns the ids.
    fn delete_tree(&mut self, id: &str) -> Result<Vec<String>, ProtoError> {
        let t = self.thread(id)?;
        let mut ids: Vec<String> = Vec::new();
        // Its waiting workers mustn't take a freed slot on the way out.
        self.worker_queue.retain(|(o, _, _)| o != id);
        if t.role == Some(ThreadRole::Orchestrator) {
            ids.extend(self.store.workers(id).map_err(store_err)?.into_iter().map(|w| w.id));
        }
        ids.push(id.to_string());
        for one in &ids {
            self.delete_one(one)?;
        }
        Ok(ids)
    }

    fn delete_one(&mut self, id: &str) -> Result<(), ProtoError> {
        let mut freed = None;
        if let Some(turn) = self.turns.remove(id) {
            if let Some(ct) = turn.codex_turn {
                self.interrupt_codex(id, &ct);
            }
            freed = turn.parent;
        }
        self.approvals.retain(|_, a| a.thread != id);
        if let Some(mut run) = self.claude.remove(id) {
            run.session.stop();
        }
        if let Some(c) = self.codex.as_mut() {
            c.loaded.remove(id);
        }
        self.worker_queue.retain(|(o, w, _)| o != id && w != id);
        self.worker_ends.remove(id);
        let (gone, keep): (Vec<Wait>, Vec<Wait>) = std::mem::take(&mut self.waits).into_iter().partition(|w| w.orchestrator == id);
        self.waits = keep;
        for w in gone {
            let _ = w.reply.send(Err("This chat was deleted.".into()));
        }
        let (gone, keep): (Vec<Ask>, Vec<Ask>) = std::mem::take(&mut self.asks).into_iter().partition(|a| a.thread == id);
        self.asks = keep;
        for a in gone {
            let _ = a.reply.send(Err("This chat was deleted.".into()));
        }
        self.store.delete_thread(id).map_err(store_err)?;
        self.emit(Event::ThreadDeleted { thread_id: id.to_string() });
        // A running worker's slot goes to the next one waiting.
        if let Some(parent) = freed {
            self.pump_workers(&parent);
        }
        Ok(())
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
            decision: None,
            attachments: None,
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
        if self.worker_queue.iter().any(|(_, w, _)| *w == p.thread_id) {
            return Err(ProtoError::new(ErrorCode::Busy, "This helper is waiting to start. Stop it or wait for it to finish."));
        }
        let begun = self.begin_turn(p)?;
        self.reply(to, Ok(json!({ "turnId": begun.turn_id, "userItem": begun.user })));
        self.launch(begun);
        Ok(())
    }

    /// Check and save a new turn (its user item, the thread's new state);
    /// [`State::launch`] starts it.
    fn begin_turn(&mut self, p: TurnSendParams) -> Result<Begun, ProtoError> {
        let mut t = self.thread(&p.thread_id)?;
        // An open decision card alone doesn't block a message.
        if self.turns.contains_key(&t.id) || t.status == ThreadStatus::Running {
            return Err(ProtoError::new(ErrorCode::Busy, "This chat is still working. Stop it or wait for it to finish."));
        }
        let text = p.text.trim().to_string();
        if text.is_empty() && p.attachments.is_empty() {
            return Err(ProtoError::bad_request("Write a message first."));
        }
        let attached = attachments::prepare(&p.attachments, &self.attachments_dir(&t.id)).map_err(ProtoError::bad_request)?;
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
        let shown = (!attached.shown.is_empty()).then(|| attached.shown.clone());
        let user = self.add_item(&t.id, Some(&turn_id), ItemKind::User, Some(text.clone()), |i| i.attachments = shown);
        let text = attachments::prompt_with(&text, &attached.shown);

        t.agent = agent.clone();
        t.model = model.clone();
        t.effort = effort.clone();
        // A new message reopens a job that was waiting for review.
        t.review_at = None;
        t.status = ThreadStatus::Running;
        t.updated_at = now_iso();
        let said = p.text.trim();
        t.preview = one_line(if said.is_empty() { attached.shown.first().map(|a| a.name.as_str()).unwrap_or("") } else { said }, 140);
        if t.title.is_empty() {
            t.title = title_from(if said.is_empty() { &t.preview } else { said });
        }
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
                codex_turn: None,
                parent: t.parent_id.clone(),
                lookup: t.worker_kind == Some(WorkerKind::Lookup),
                images: attachments::images(&attached.shown),
            },
        );
        Ok(Begun { thread: t.id, turn_id, user, prompt, session, downloads: attached.downloads })
    }

    fn launch(&mut self, mut b: Begun) {
        // Uploads from the website or phone come down first.
        if !b.downloads.is_empty() {
            let downloads = std::mem::take(&mut b.downloads);
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                let failed = downloads.iter().find_map(|(url, dest)| attachments::download(url, dest).err());
                let _ = tx.send(Cmd::Downloaded { begun: Box::new(b), failed });
            });
            return;
        }
        self.emit(Event::TurnStarted { thread_id: b.thread.clone(), turn_id: b.turn_id });
        self.start_agent(&b.thread, &b.prompt, b.session);
    }

    /// Where a chat's attachments are saved: beside the agents' database.
    fn attachments_dir(&self, thread: &str) -> PathBuf {
        self.cfg.db_path.parent().unwrap_or(Path::new(".")).join("agents-attachments").join(thread)
    }

    fn mcp_url(&self) -> Option<String> {
        if self.auth.connected() {
            self.relay.as_ref().map(|r| r.url().to_string())
        } else {
            None
        }
    }

    /// Where a thread's agent works: its own folder, else the engine's.
    fn work_dir(&self, thread: &str) -> Result<PathBuf, String> {
        match self.store.thread(thread).ok().flatten().and_then(|t| t.cwd) {
            Some(c) if Path::new(&c).is_dir() => Ok(PathBuf::from(c)),
            Some(_) => Err("This chat's folder isn't on this Mac any more.".into()),
            None => Ok(self.cfg.work_dir.clone()),
        }
    }

    /// The `billable` server's URL: every chat but a lookup.
    fn orchestrate_url(&self, thread: &str) -> Option<String> {
        let t = self.store.thread(thread).ok().flatten()?;
        seat_of(&t)?;
        self.relay.as_ref().map(|r| r.orchestrate_url(thread))
    }

    /// What a chat's agent is told after the usual note, and whether it may
    /// only read.
    fn notes_for(&self, thread: &str) -> (String, bool) {
        let Some(t) = self.store.thread(thread).ok().flatten() else {
            return (String::new(), false);
        };
        let memory = t.project_id.as_deref().and_then(|p| self.store.memory(p).ok().flatten()).map(|(text, _)| text);
        let seat = seat_of(&t);
        (orchestrator::notes(seat, memory.as_deref()), seat.is_none())
    }

    fn start_agent(&mut self, thread: &str, prompt: &str, session: Option<String>) {
        let Some(turn) = self.turns.get(thread) else {
            return;
        };
        let (agent, model, effort) = (turn.agent.clone(), turn.model.clone(), turn.effort.clone());
        let cwd = match self.work_dir(thread) {
            Ok(c) => c,
            Err(e) => return self.finish_turn(thread, TurnStatus::Failed, Some(e), None),
        };
        if agent == claude::AGENT_ID {
            self.start_claude(thread, prompt, model, effort, session, &cwd);
        } else {
            self.start_codex(thread, prompt);
        }
    }

    fn start_claude(&mut self, thread: &str, prompt: &str, model: String, effort: Option<String>, session: Option<String>, cwd: &Path) {
        let Some(bin) = self.bins.claude.clone() else {
            return self.finish_turn(thread, TurnStatus::Failed, Some("Claude Code isn't installed on this Mac.".into()), None);
        };
        let (notes, read_only) = self.notes_for(thread);
        let coordinator = self.store.thread(thread).ok().flatten().is_some_and(|t| t.role == Some(ThreadRole::Orchestrator));
        let launch = claude::Launch { model, effort, resume: session, mcp_url: self.mcp_url(), orchestrate_url: self.orchestrate_url(thread), notes, read_only, coordinator };
        // A changed memory or seat starts a fresh process (the session resumes).
        let reuse = self.claude.get_mut(thread).is_some_and(|r| {
            let l = &r.session.launch;
            let same =
                l.model == launch.model && l.effort == launch.effort && l.mcp_url == launch.mcp_url && l.orchestrate_url == launch.orchestrate_url && l.notes == launch.notes && l.read_only == launch.read_only && l.coordinator == launch.coordinator;
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
            match claude::Session::start(&bin, cwd, launch, sink) {
                Ok(session) => {
                    self.claude.insert(thread.to_string(), ClaudeRun { session, gen, idle_since: None });
                }
                Err(e) => return self.finish_turn(thread, TurnStatus::Failed, Some(format!("Claude Code didn't start ({e}).")), None),
            }
        }
        let images = self.turns.get(thread).map(|t| t.images.clone()).unwrap_or_default();
        let sent = self.claude.get_mut(thread).map(|r| {
            r.idle_since = None;
            r.session.send(prompt, &images)
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
                    // The server the person's message started lists its models
                    // (a local call: nothing goes to a model).
                    if let Ok(server) = &result {
                        let (peer, tx) = (server.peer.clone(), tx.clone());
                        std::thread::spawn(move || {
                            if let Some((models, default)) = peer.request("model/list", json!({}), codex::REQUEST_TIMEOUT).ok().and_then(|r| codex::models_from_list(&r)) {
                                let _ = tx.send(Cmd::CodexModels { models, default });
                            }
                        });
                    }
                    let _ = tx.send(Cmd::CodexServer { gen, result });
                });
            }
            return;
        }
        let Some(c) = self.codex.as_ref() else { return };
        let Some(turn) = self.turns.get(thread) else {
            return;
        };
        let peer = c.server.peer.clone();
        let gen = c.gen;
        let loaded = c.loaded.get(thread).cloned();
        let resume = if loaded.is_none() { self.store.session(thread, codex::AGENT_ID).ok().flatten() } else { None };
        let (model, effort, turn_id, images) = (turn.model.clone(), turn.effort.clone(), turn.id.clone(), turn.images.clone());
        let cwd = match self.work_dir(thread) {
            Ok(c) => c,
            Err(e) => return self.finish_turn(thread, TurnStatus::Failed, Some(e), None),
        };
        let orchestrate = self.orchestrate_url(thread);
        let (notes, read_only) = self.notes_for(thread);
        let (tx, mcp, id, prompt) = (self.tx.clone(), self.mcp_url(), thread.to_string(), prompt.to_string());
        std::thread::spawn(move || {
            let result = (|| {
                let codex_thread = match loaded {
                    Some(ct) => ct,
                    None => {
                        let started = match &resume {
                            Some(ct) => peer.request("thread/resume", codex::thread_params(&model, &cwd, mcp.as_deref(), orchestrate.as_deref(), &notes, read_only, Some(ct)), codex::REQUEST_TIMEOUT),
                            None => Err(String::new()),
                        };
                        let started = match started {
                            Ok(v) => v,
                            // Not resumable (or nothing to resume): a new thread
                            Err(_) => peer.request("thread/start", codex::thread_params(&model, &cwd, mcp.as_deref(), orchestrate.as_deref(), &notes, read_only, None), codex::REQUEST_TIMEOUT)?,
                        };
                        let ct = started.pointer("/thread/id").and_then(Value::as_str).ok_or("Codex didn't start a thread.")?.to_string();
                        let _ = tx.send(Cmd::CodexBound { thread: id.clone(), gen, codex_thread: ct.clone() });
                        ct
                    }
                };
                let started = peer.request("turn/start", codex::turn_params(&codex_thread, &prompt, &model, effort.as_deref(), &images), codex::REQUEST_TIMEOUT)?;
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
        let Some(t) = self.turns.get_mut(thread).filter(|t| t.id == turn) else {
            return;
        };
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
                let Some((ct, ev)) = codex::map_notification(&method, &params) else {
                    return;
                };
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
        let Some(turn) = self.turns.get(thread) else {
            return (String::new(), false);
        };
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
        let Some(t) = self.turns.get_mut(thread) else {
            return;
        };
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
        let Some(t) = self.turns.get_mut(thread) else {
            return;
        };
        let Some(mut item) = t.live.remove(&id).or_else(|| self.saved_item(thread, &id)) else {
            return;
        };
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
            // Only Claude Code reports these (its initialize answer).
            DriverEvent::Models(models, default) => self.models_reported(claude::AGENT_ID, models, default),
            DriverEvent::Commands(list) => self.claude_commands = list,
            DriverEvent::McpStatus(list) => {
                for (name, status) in list {
                    let error = (status == "failed").then(|| "It didn't start.".to_string());
                    self.mcp_status.insert(name, (status, error));
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
                let mut threads = item.tool.as_ref().map(|t| t.threads.clone()).unwrap_or_default();
                if name.starts_with(orchestrator::TOOL_PREFIX) {
                    for id in orchestrator::threads_of(&input, None) {
                        if !threads.contains(&id) {
                            threads.push(id);
                        }
                    }
                }
                item.tool = Some(ToolInfo { feedhq: tools::is_feedhq(&name), name, title, detail, status, output, threads });
                self.update_item(&item);
            }
            DriverEvent::ToolFinished { key, output, failed, declined } => {
                let Some(id) = self.turns.get(thread).and_then(|t| t.keys.get(&key)).cloned() else {
                    return;
                };
                let denied = declined || self.turns.get(thread).is_some_and(|t| t.denied.contains(&id));
                let Some(mut item) = self.saved_item(thread, &id) else {
                    return;
                };
                if let Some(tool) = item.tool.as_mut() {
                    tool.status = if denied {
                        ToolStatus::Denied
                    } else if failed {
                        ToolStatus::Failed
                    } else {
                        ToolStatus::Done
                    };
                    if tool.name.starts_with(orchestrator::TOOL_PREFIX) {
                        for id in orchestrator::threads_of(&json!({}), output.as_deref()) {
                            if !tool.threads.contains(&id) {
                                tool.threads.push(id);
                            }
                        }
                    }
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
        let Some(turn) = self.turns.get(thread) else {
            return;
        };
        let agent = turn.agent.clone();
        // A lookup only reads: anything that would need the person is turned down.
        if turn.lookup {
            let item_id = tool_key.as_ref().and_then(|k| turn.keys.get(k)).cloned();
            if let (Some(item), Some(t)) = (item_id.clone(), self.turns.get_mut(thread)) {
                t.denied.push(item);
            }
            let a = OpenApproval { thread: thread.to_string(), native, agent, item_id };
            return self.answer_agent(&a, Decision::Deny);
        }
        let item_id = tool_key.as_ref().and_then(|k| turn.keys.get(k)).cloned();
        // The chat's mode may let it go ahead without the person.
        let mode = self.store.thread(thread).ok().flatten().and_then(|t| t.mode).unwrap_or_default();
        if mode != Mode::Ask {
            if let Ok(cwd) = self.work_dir(thread) {
                if guard::verdict(mode, name, input, &cwd) == guard::Verdict::Allow {
                    let a = OpenApproval { thread: thread.to_string(), native, agent, item_id };
                    return self.answer_agent(&a, Decision::Allow);
                }
            }
        }
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

    fn needs_person(&self, thread: &str) -> bool {
        self.approvals.values().any(|a| a.thread == thread) || self.store.open_decisions(Some(thread)).is_ok_and(|d| !d.is_empty())
    }

    /// Running; waiting while an approval or a decision card is open (a
    /// card keeps a chat waiting after its turn, too); else as it was.
    fn set_status(&self, thread: &str) {
        let Ok(Some(mut t)) = self.store.thread(thread) else {
            return;
        };
        let busy = self.turns.contains_key(thread);
        let status = if self.needs_person(thread) {
            ThreadStatus::Waiting
        } else if busy {
            ThreadStatus::Running
        } else if t.status == ThreadStatus::Waiting {
            ThreadStatus::Idle
        } else {
            return;
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
        self.turn_stop_with(thread, true)
    }

    /// `jobs`: a coordinator's jobs and lookups stop with it.
    fn turn_stop_with(&mut self, thread: &str, jobs: bool) -> Result<Value, ProtoError> {
        let t = self.thread(thread)?;
        if t.role == Some(ThreadRole::Orchestrator) {
            // Its workers stop with it (unless the person kept them), and
            // its waits are answered now.
            if jobs {
                self.stop_workers(thread, None);
            }
            self.answer_waits_of(thread);
        }
        // A question it's waiting on lets go (its card stays open).
        let (gone, keep): (Vec<Ask>, Vec<Ask>) = std::mem::take(&mut self.asks).into_iter().partition(|a| a.thread == thread);
        self.asks = keep;
        for a in gone {
            let _ = a.reply.send(Err("The person stopped this turn.".into()));
        }
        let Some(turn) = self.turns.get_mut(thread) else {
            return Ok(json!({}));
        };
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
        let Some(turn) = self.turns.get(thread) else {
            return;
        };
        let stopped = turn.stop_at.is_some();
        // A resumed session that failed straight away (e.g. its transcript is
        // gone) is forgotten, so the person's next message starts fresh with a
        // handoff. Nothing is sent again on their behalf.
        if status == TurnStatus::Failed && turn.resumed && turn.outputs == 0 && !stopped {
            let agent = turn.agent.clone();
            let _ = self.store.clear_session(thread, &agent);
            if let Some(mut run) = self.claude.remove(thread) {
                run.session.stop();
            }
            if let Some(c) = self.codex.as_mut() {
                c.loaded.remove(thread);
            }
        }
        let Some(mut turn) = self.turns.remove(thread) else {
            return;
        };
        if self.screen_holder.as_deref() == Some(thread) {
            self.screen_holder = None;
        }
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
            self.note_sign_in_error(&turn.agent, &message);
            self.add_item(thread, Some(&turn.id), ItemKind::Error, Some(message), |_| {});
        }
        if let Some(run) = self.claude.get_mut(thread) {
            run.idle_since = Some(Instant::now());
        }
        let Ok(Some(mut t)) = self.store.thread(thread) else {
            return;
        };
        let open_card = self.store.open_decisions(Some(thread)).is_ok_and(|d| !d.is_empty());
        t.status = if status == TurnStatus::Failed {
            ThreadStatus::Failed
        } else if open_card {
            ThreadStatus::Waiting
        } else {
            ThreadStatus::Idle
        };
        t.updated_at = now_iso();
        self.save_thread(&t);
        // A turn that ended can't hear an answer; the card stays open.
        let (gone, keep): (Vec<Ask>, Vec<Ask>) = std::mem::take(&mut self.asks).into_iter().partition(|a| a.thread == thread);
        self.asks = keep;
        for a in gone {
            let _ = a.reply.send(Err("The turn ended before the person answered.".into()));
        }
        self.emit(Event::TurnFinished { thread_id: thread.to_string(), turn_id: turn.id.clone(), status, error, usage: usage.or(turn.usage) });
        match &turn.parent {
            // A worker's slot is free for the next one waiting.
            Some(parent) => {
                self.worker_ends.insert(thread.to_string(), status);
                self.pump_workers(parent);
            }
            // An orchestrator whose turn ended can't hear answers any more.
            None if t.role == Some(ThreadRole::Orchestrator) => self.answer_waits_of(thread),
            None => {}
        }
        self.check_waits();
    }

    // ---------- Coordinator (PROTOCOL.md) ----------

    /// A call from a chat's session through the `billable` server. Most are
    /// answered at once; wait_for_agents, ask_person and propose_plan are
    /// parked until their answer comes.
    fn orchestrate(&mut self, thread: &str, call: Call, reply: Sender<Result<Value, String>>) {
        let Some((t, seat)) = self.store.thread(thread).ok().flatten().and_then(|t| seat_of(&t).map(|s| (t, s))) else {
            let _ = reply.send(Err("This chat has no Billable tools.".into()));
            return;
        };
        let (name, args) = match call {
            Call::List => {
                let models = if seat == Seat::Coordinator { self.describe_agents() } else { String::new() };
                let mut tools = orchestrator::tools(seat, &models, t.project_id.is_some());
                if let Value::Array(list) = &mut tools {
                    list.extend(computer::tools());
                }
                let _ = reply.send(Ok(tools));
                return;
            }
            Call::Tool { name, args } => (name, args),
        };
        if name.starts_with(computer::PREFIX) {
            return self.computer_call(&t, &name, args, reply);
        }
        let coordinator = |r: Result<Value, String>| {
            if seat == Seat::Coordinator {
                r
            } else {
                Err(format!("Only a coordinator can use {name}."))
            }
        };
        let result = match name.as_str() {
            "start_agents" => coordinator(Ok(Value::Null)).and_then(|_| {
                self.person_asked(&t)?;
                let jobs = self.plan_jobs(&t, args.get("agents"))?;
                self.start_jobs(&t, jobs)
            }),
            "propose_plan" => match coordinator(Ok(Value::Null)).and_then(|_| self.propose_plan(&t, &args)) {
                Ok(item) => return self.park_ask(thread, item, reply),
                Err(e) => Err(e),
            },
            "wait_for_agents" => match coordinator(Ok(Value::Null)).and_then(|_| self.begin_wait(&t, &args)) {
                Ok(wait) => {
                    self.waits.push(Wait { orchestrator: t.id.clone(), ids: wait.0, until: wait.1, reply });
                    self.check_waits();
                    return;
                }
                Err(e) => Err(e),
            },
            "message_agent" => coordinator(Ok(Value::Null)).and_then(|_| self.message_agent(&t, &args)),
            "list_agents" => coordinator(Ok(Value::Null)).and_then(|_| {
                self.store.workers(&t.id).map_err(|e| format!("{e:#}")).map(|ws| {
                    Value::Array(ws.iter().map(|w| json!({"id": w.id, "title": w.title, "kind": kind_of(w), "agent": w.agent, "model": w.model, "effort": w.effort, "status": self.worker_status(w)})).collect())
                })
            }),
            "stop_agents" => coordinator(Ok(Value::Null)).and_then(|_| {
                self.worker_ids(&t.id, args.get("ids")).map(|ids| {
                    self.stop_workers(&t.id, Some(&ids));
                    self.check_waits();
                    json!({ "ok": true })
                })
            }),
            "ask_person" => match self.ask_person(&t, &args) {
                Ok(item) => return self.park_ask(thread, item, reply),
                Err(e) => Err(e),
            },
            "ready_for_review" if seat == Seat::Job => self.ready_for_review(&t, &args),
            "remember" if t.project_id.is_some() => self.remember(&t, &args),
            other => Err(format!("Billable has no tool called {other} here.")),
        };
        let _ = reply.send(result);
    }

    /// The agents and models a worker can use, for start_agents' description.
    fn describe_agents(&self) -> String {
        let mut out = String::from("Agents and models on this Mac:");
        for a in self.agents.iter().filter(|a| a.installed && a.signed_in) {
            let models: Vec<String> = a
                .models
                .iter()
                .filter(|m| !orchestrator::refused_for_jobs(&m.id))
                .map(|m| if m.efforts.is_empty() { format!("{} ({})", m.id, m.name) } else { format!("{} ({}; effort {})", m.id, m.name, m.efforts.join(", ")) })
                .collect();
            out.push_str(&format!("\n- {} ({}): {}", a.id, a.name, models.join("; ")));
        }
        out
    }

    /// The coordinator is in a turn the person sent: jobs start only then
    /// (or when the person approves a plan on its card).
    fn person_asked(&self, o: &Thread) -> Result<(), String> {
        if self.turns.get(&o.id).is_some_and(|t| t.stop_at.is_none()) {
            Ok(())
        } else {
            Err("Jobs start only while you're answering the person's message.".into())
        }
    }

    /// Check a start_agents (or propose_plan) list, every job before any starts.
    fn plan_jobs(&self, o: &Thread, specs: Option<&Value>) -> Result<Vec<PlanJob>, String> {
        let specs = specs.and_then(Value::as_array).cloned().unwrap_or_default();
        if specs.is_empty() || specs.len() > orchestrator::MAX_PER_CALL {
            return Err(format!("Start 1 to {} at a time.", orchestrator::MAX_PER_CALL));
        }
        let existing = self.store.workers(&o.id).map_err(|e| format!("{e:#}"))?.len();
        if existing + specs.len() > orchestrator::MAX_WORKERS {
            return Err(format!("A chat can have at most {} jobs and lookups; this one has {existing}.", orchestrator::MAX_WORKERS));
        }
        let mut plans = Vec::new();
        for (n, spec) in specs.iter().enumerate() {
            let text = |k: &str| spec.get(k).and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty()).map(str::to_string);
            let task = text("task").ok_or_else(|| format!("Job {} has no task.", n + 1))?;
            let title = one_line(&text("title").unwrap_or_else(|| title_from(&task)), 120);
            let kind = match text("kind").as_deref() {
                None | Some("job") => WorkerKind::Job,
                Some("lookup") => WorkerKind::Lookup,
                Some(k) => return Err(format!("{k} isn't a kind; use job or lookup.")),
            };
            let agent = text("agent").unwrap_or_else(|| o.agent.clone());
            self.check_agent(&agent).map_err(|e| e.message)?;
            let a = self.agent(&agent).cloned().ok_or("That agent isn't on this Mac.")?;
            let allowed: Vec<&str> = a.models.iter().map(|x| x.id.as_str()).filter(|m| !orchestrator::refused_for_jobs(m)).collect();
            let model = match text("model") {
                Some(m) if orchestrator::refused_for_jobs(&m) => {
                    return Err(format!("Jobs never run on {m}: it uses too much of the person's plan. Choose one of: {}.", allowed.join(", ")));
                }
                Some(m) if !a.models.is_empty() && !a.models.iter().any(|x| x.id == m) => {
                    return Err(format!("{m} isn't one of {}'s models here. Choose one of: {}.", a.name, allowed.join(", ")));
                }
                Some(m) => m,
                // Left out: the cheapest that fits, then the agent's default
                // (never a refused one).
                None => {
                    let cheap = orchestrator::default_job_model(kind == WorkerKind::Lookup);
                    if a.models.iter().any(|x| x.id == cheap) {
                        cheap.to_string()
                    } else if !orchestrator::refused_for_jobs(&a.default_model) {
                        a.default_model.clone()
                    } else {
                        allowed.first().map(|m| m.to_string()).ok_or("This agent has no model a job can use.")?
                    }
                }
            };
            // Left out (or not one the model takes): the model's default.
            // A lookup left to itself runs at low effort.
            let effort = self.effort_for(&agent, &model, text("effort").or_else(|| (kind == WorkerKind::Lookup).then(|| "low".to_string())));
            let reason = text("reason").map(|r| one_line(&r, 140));
            plans.push(PlanJob { title, task, agent, model, effort, kind, reason });
        }
        Ok(plans)
    }

    /// Make the jobs' chats and queue them; they start as slots free up.
    fn start_jobs(&mut self, o: &Thread, jobs: Vec<PlanJob>) -> Result<Value, String> {
        let mut out = Vec::new();
        for job in jobs {
            let now = now_iso();
            let w = Thread {
                id: new_id("t"),
                engine: "mac".into(),
                title: job.title,
                agent: job.agent,
                model: job.model,
                effort: job.effort,
                status: ThreadStatus::Idle,
                pinned: false,
                settled_at: None,
                archived_at: None,
                created_at: now.clone(),
                updated_at: now,
                preview: orchestrator::WAITING_PREVIEW.into(),
                cwd: o.cwd.clone(),
                role: Some(ThreadRole::Worker),
                parent_id: Some(o.id.clone()),
                project_id: o.project_id.clone(),
                worker_kind: Some(job.kind),
                review_at: None,
                // A job may do what its coordinator may.
                mode: o.mode,
            };
            self.store.insert_thread(&w).map_err(|e| format!("{e:#}"))?;
            self.emit(Event::ThreadUpdated { thread: w.clone() });
            out.push(json!({"id": w.id, "title": w.title, "kind": job.kind.as_str(), "agent": w.agent, "model": w.model, "effort": w.effort}));
            self.worker_queue.push_back((o.id.clone(), w.id, job.task));
        }
        self.pump_workers(&o.id);
        Ok(Value::Array(out))
    }

    /// Hold a call open until the person answers its card.
    fn park_ask(&mut self, thread: &str, item: Item, reply: Sender<Result<Value, String>>) {
        self.asks.push(Ask { thread: thread.to_string(), item: item.id, until: Instant::now() + Duration::from_secs(orchestrator::ASK_WAIT_SECS), reply, computer: None });
        self.set_status(thread);
    }

    /// A decision card in the chat, on the turn that's running.
    fn add_card(&mut self, thread: &str, card: DecisionCard) -> Item {
        let turn = self.turns.get(thread).map(|t| t.id.clone());
        self.add_item(thread, turn.as_deref(), ItemKind::Decision, None, |i| i.decision = Some(card))
    }

    fn propose_plan(&mut self, o: &Thread, args: &Value) -> Result<Item, String> {
        self.person_asked(o)?;
        let jobs = self.plan_jobs(o, args.get("agents"))?;
        let n = jobs.len();
        let summary = args.get("summary").and_then(Value::as_str).map(|s| one_line(s, 200)).filter(|s| !s.is_empty());
        let start = if n == 1 { "Start it".to_string() } else { format!("Start {n}") };
        let card = DecisionCard {
            question: summary.unwrap_or_else(|| if n == 1 { "Start this job?".into() } else { format!("Start these {n} jobs?") }),
            detail: None,
            options: vec![DecisionOption { label: start, detail: None }, DecisionOption { label: "Not now".into(), detail: None }],
            recommended: Some(0),
            plan: Some(jobs),
            ..DecisionCard::default()
        };
        Ok(self.add_card(&o.id, card))
    }

    fn ask_person(&mut self, t: &Thread, args: &Value) -> Result<Item, String> {
        if !self.turns.contains_key(&t.id) {
            return Err("Ask while you're answering the person's message.".into());
        }
        let text = |v: &Value, k: &str, max: usize| v.get(k).and_then(Value::as_str).map(|s| one_line(s, max)).filter(|s| !s.is_empty());
        let question = text(args, "question", 200).ok_or("Write the question first.")?;
        let options: Vec<DecisionOption> = args
            .get("options")
            .and_then(Value::as_array)
            .map(|o| o.iter().filter_map(|o| Some(DecisionOption { label: text(o, "label", 60)?, detail: text(o, "detail", 200) })).collect())
            .unwrap_or_default();
        if !(2..=4).contains(&options.len()) {
            return Err("Give 2 to 4 options, each with a label.".into());
        }
        let recommended = args.get("recommended").and_then(Value::as_u64).map(|r| r as usize).filter(|r| *r < options.len());
        let card = DecisionCard { question, detail: text(args, "detail", 400), options, recommended, ..DecisionCard::default() };
        Ok(self.add_card(&t.id, card))
    }

    fn ready_for_review(&mut self, w: &Thread, args: &Value) -> Result<Value, String> {
        let summary = args.get("summary").and_then(Value::as_str).map(|s| one_line(s, 140)).filter(|s| !s.is_empty()).ok_or("Write a one-line summary.")?;
        let mut w = w.clone();
        w.review_at = Some(now_iso());
        w.preview = summary;
        self.save_thread(&w);
        Ok(json!({ "ok": true }))
    }

    fn remember(&mut self, t: &Thread, args: &Value) -> Result<Value, String> {
        let project = t.project_id.clone().ok_or("This chat isn't in a project.")?;
        let note = args.get("note").and_then(Value::as_str).map(|s| one_line(s, 300)).filter(|s| !s.is_empty()).ok_or("Write the note first.")?;
        let note = note.trim_start_matches(['-', '*', ' ']).to_string();
        let current = self.store.memory(&project).map_err(|e| format!("{e:#}"))?.map(|(t, _)| t).unwrap_or_default();
        if current.lines().any(|l| l.trim_start_matches(['-', '*', ' ']).eq_ignore_ascii_case(&note)) {
            return Ok(json!({ "memory": current }));
        }
        let text = if current.trim().is_empty() { format!("- {note}") } else { format!("{}\n- {note}", current.trim_end()) };
        if text.chars().count() > orchestrator::MEMORY_CHARS {
            return Err("The project's memory is full. Ask the person to tidy it in the project's settings.".into());
        }
        self.memory_set(&project, &text).map_err(|e| e.message)?;
        Ok(json!({ "memory": text }))
    }

    // ---------- Computer (PROTOCOL.md) ----------

    /// A computer tool: one chat at a time, and only what the person
    /// allowed this chat (a card asks the first time).
    fn computer_call(&mut self, t: &Thread, name: &str, args: Value, reply: Sender<Result<Value, String>>) {
        let refuse = |m: &str| {
            let _ = reply.send(Err(m.to_string()));
        };
        if !self.turns.contains_key(&t.id) {
            return refuse("Use the screen while you're answering the person's message.");
        }
        if let Some(holder) = self.screen_holder.as_deref().filter(|h| *h != t.id && self.turns.contains_key(*h)) {
            let other = self.store.thread(holder).ok().flatten().map(|o| o.title).filter(|s| !s.is_empty()).unwrap_or_else(|| "Another chat".into());
            return refuse(&format!("{other} is using the screen right now. Try again when it's finished."));
        }
        let access = computer::access();
        if !access.supported {
            return refuse("Computer use needs a Mac.");
        }
        let (grant, label) = match computer::needs(name, &args) {
            computer::Needs::Nothing => (None, String::new()),
            computer::Needs::Screen => {
                if !access.screen {
                    computer::request_screen();
                    return refuse("Billable needs Screen Recording to see the screen. The person can turn it on in Agents → Tools. Tell them, then stop.");
                }
                (Some("screen".to_string()), "see your screen".to_string())
            }
            computer::Needs::App(app) => (Some(format!("app:{}", app.to_lowercase())), format!("use {app}")),
            computer::Needs::FrontApp => {
                if !access.control {
                    computer::request_control();
                    return refuse("Billable needs Accessibility to click and type. The person can turn it on in Agents → Tools. Tell them, then stop.");
                }
                match computer::front_app() {
                    Some(app) if app.eq_ignore_ascii_case("billable") => {
                        return refuse("Billable is in front. Open the app you need with computer_open_app first; Billable itself is off limits.");
                    }
                    Some(app) => (Some(format!("app:{}", app.to_lowercase())), format!("use {app}")),
                    None => return refuse("Couldn't tell which app is in front. Take a screenshot and try again."),
                }
            }
        };
        let granted = |s: &Self, g: &str| s.grants.get(&t.id).is_some_and(|set| set.contains(g));
        // Acting in an app means seeing it too.
        let grant = grant.filter(|g| !granted(self, g));
        let Some(grant) = grant else {
            return self.computer_run(&t.id, name.to_string(), args, reply);
        };
        let card = DecisionCard {
            question: format!("Let this chat {label}?"),
            detail: None,
            options: vec![DecisionOption { label: "Allow".into(), detail: None }, DecisionOption { label: "Don't allow".into(), detail: None }],
            recommended: None,
            ..DecisionCard::default()
        };
        let item = self.add_card(&t.id, card);
        self.computer_cards.insert(item.id.clone(), (t.id.clone(), grant.clone()));
        self.asks.push(Ask { thread: t.id.clone(), item: item.id, until: Instant::now() + Duration::from_secs(orchestrator::ASK_WAIT_SECS), reply, computer: Some((name.to_string(), args, grant)) });
        self.set_status(&t.id);
    }

    /// Do it, off the engine thread (a screenshot takes a moment).
    fn computer_run(&mut self, thread: &str, tool: String, args: Value, reply: Sender<Result<Value, String>>) {
        if !self.turns.contains_key(thread) {
            let _ = reply.send(Err("The turn ended before the person answered.".into()));
            return;
        }
        self.screen_holder = Some(thread.to_string());
        if tool == "computer_screenshot" || tool == "computer_cursor_position" {
            self.grants.entry(thread.to_string()).or_default().insert("screen".into());
        }
        std::thread::spawn(move || {
            let result = computer::perform(&tool, &args).map(|content| json!({ orchestrator::CONTENT: content }));
            let _ = reply.send(result);
        });
    }

    // ---------- Tools (PROTOCOL.md) ----------

    fn places(&self) -> Places {
        Places::from_env(self.bins.claude.clone(), self.bins.codex.clone())
    }

    /// The folder a Tools request is about: the one asked, if it's here.
    fn tools_cwd(&self, cwd: Option<&str>) -> Option<PathBuf> {
        cwd.map(str::trim).filter(|c| !c.is_empty()).map(PathBuf::from).filter(|p| p.is_dir())
    }

    fn tools_list(&self, cwd: Option<&Path>) -> Value {
        json!({
            "servers": servers::list(&self.places(), cwd, &self.mcp_status),
            "computer": computer::access().to_json(),
        })
    }

    /// Checks and changes run the agents' own commands, off the engine thread.
    fn tools_change(&mut self, method: &str, p: ToolsParams, to: ReplyTo) {
        let places = self.places();
        let cwd = self.tools_cwd(p.cwd.as_deref());
        let run_in = cwd.clone().unwrap_or_else(|| self.cfg.work_dir.clone());
        let method = method.to_string();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = match method.as_str() {
                "tools.check" => servers::check_claude(&places, &run_in).map(Some),
                "tools.remove" => servers::remove(&places, p.id.as_deref().unwrap_or(""), &run_in).map(|_| None),
                "tools.import" => {
                    let node = std::env::split_paths(&crate::process::child_path(None)).any(|d| d.join("node").is_file());
                    servers::import(&places, p.id.as_deref().unwrap_or(""), &p.agents, &run_in, node).map(|_| None)
                }
                _ => servers::spec_from_input(p.command.as_deref(), p.url.as_deref(), &p.env).and_then(|spec| {
                    if p.agents.is_empty() {
                        return Err("Pick Claude Code, Codex or both.".into());
                    }
                    let scope = if p.scope.as_deref() == Some("local") && cwd.is_some() { "local" } else { "user" };
                    p.agents.iter().try_for_each(|a| servers::add(&places, a, p.name.as_deref().unwrap_or(""), &spec, scope, &run_in)).map(|_| None)
                }),
            };
            let _ = tx.send(Cmd::ToolsDone { to, cwd, result });
        });
    }

    // ---------- Decisions and memory (PROTOCOL.md) ----------

    fn decision_answer(&mut self, p: DecisionAnswerParams) -> Result<Value, ProtoError> {
        let mut item = self.saved_item(&p.thread_id, &p.item_id).filter(|i| i.kind == ItemKind::Decision).ok_or_else(|| ProtoError::new(ErrorCode::NotFound, "That card is gone."))?;
        let Some(card) = item.decision.as_mut() else {
            return Err(ProtoError::not_found());
        };
        if card.answered_at.is_some() {
            return Err(ProtoError::new(ErrorCode::NotFound, "That was already answered."));
        }
        if p.choice >= card.options.len() {
            return Err(ProtoError::bad_request("That isn't one of the options."));
        }
        let plan = card.plan.clone();
        let starting = plan.is_some() && p.choice == 0;
        let kept: Option<Vec<usize>> = plan.as_ref().map(|jobs| match &p.kept {
            Some(k) => {
                let mut k: Vec<usize> = k.iter().copied().filter(|i| *i < jobs.len()).collect();
                k.sort_unstable();
                k.dedup();
                k
            }
            None => (0..jobs.len()).collect(),
        });
        if starting && kept.as_ref().is_some_and(Vec::is_empty) {
            return Err(ProtoError::bad_request("Keep at least one job, or choose Not now."));
        }
        card.chosen = Some(p.choice);
        card.answer = p.text.as_deref().map(|t| one_line(t, 500)).filter(|t| !t.is_empty());
        card.kept = if starting { kept.clone() } else { None };
        card.answered_at = Some(now_iso());
        let card = card.clone();
        self.update_item(&item);

        // A plan the person approved starts now, whether or not the
        // coordinator is still waiting: the person asked for it.
        let mut started = Value::Array(Vec::new());
        if let (true, Some(jobs), Some(kept)) = (starting, plan, kept) {
            let o = self.thread(&p.thread_id)?;
            let chosen: Vec<PlanJob> = jobs.into_iter().enumerate().filter(|(i, _)| kept.contains(i)).map(|(_, j)| j).collect();
            started = self.start_jobs(&o, chosen).map_err(ProtoError::failed)?;
        }
        let label = card.options[p.choice].label.clone();
        let answer = if card.plan.is_some() {
            json!({"approved": starting, "started": started, "note": card.answer})
        } else {
            json!({"choice": p.choice, "label": label, "note": card.answer})
        };
        if let Some((thread, grant)) = self.computer_cards.remove(&item.id) {
            if p.choice == 0 {
                self.grants.entry(thread).or_default().insert(grant);
            }
        }
        match self.asks.iter().position(|a| a.item == item.id) {
            Some(pos) => {
                let ask = self.asks.remove(pos);
                match ask.computer {
                    Some((tool, args, _)) if p.choice == 0 => self.computer_run(&ask.thread, tool, args, ask.reply),
                    Some(_) => {
                        let _ = ask.reply.send(Err("The person said no. Don't try it again unless they ask.".into()));
                    }
                    None => {
                        let _ = ask.reply.send(Ok(answer));
                    }
                }
            }
            // The turn that asked is over: the answer is the person's next
            // message (a plan they approved has already started).
            None if card.plan.is_none() && !self.turns.contains_key(&p.thread_id) => {
                let mut text = format!("My answer to \u{201c}{}\u{201d}: {label}.", card.question);
                if let Some(note) = &card.answer {
                    text.push(' ');
                    text.push_str(note);
                }
                if let Ok(begun) = self.begin_turn(TurnSendParams { thread_id: p.thread_id.clone(), text, agent: None, model: None, effort: None, attachments: Vec::new() }) {
                    self.launch(begun);
                }
            }
            None => {}
        }
        self.set_status(&p.thread_id);
        Ok(json!({ "item": self.saved_item(&p.thread_id, &item.id) }))
    }

    fn memory_get(&self, project: &str) -> Result<Value, ProtoError> {
        let project = clean_project(Some(project)).ok_or_else(|| ProtoError::bad_request("That isn't a project."))?;
        let (text, updated_at) = self.store.memory(&project).map_err(store_err)?.map(|(t, at)| (t, Some(at))).unwrap_or_default();
        Ok(json!({ "projectId": project, "text": text, "updatedAt": updated_at }))
    }

    fn memory_set(&mut self, project: &str, text: &str) -> Result<Value, ProtoError> {
        let project = clean_project(Some(project)).ok_or_else(|| ProtoError::bad_request("That isn't a project."))?;
        let text = text.trim().replace("\r\n", "\n");
        if text.chars().count() > orchestrator::MEMORY_CHARS {
            return Err(ProtoError::bad_request(format!("Keep the memory under {} characters.", orchestrator::MEMORY_CHARS)));
        }
        let at = now_iso();
        self.store.set_memory(&project, &text, &at).map_err(store_err)?;
        let updated_at = (!text.is_empty()).then_some(at);
        self.emit(Event::MemoryUpdated { project_id: project.clone(), text: text.clone(), updated_at: updated_at.clone() });
        Ok(json!({ "projectId": project, "text": text, "updatedAt": updated_at }))
    }

    // ---------- Jobs ----------

    /// The orchestrator's workers named in `ids` (all of them when absent).
    fn worker_ids(&self, orchestrator: &str, ids: Option<&Value>) -> Result<Vec<String>, String> {
        let all: Vec<String> = self.store.workers(orchestrator).map_err(|e| format!("{e:#}"))?.into_iter().map(|w| w.id).collect();
        let Some(asked) = ids.and_then(Value::as_array) else {
            return Ok(all);
        };
        let mut out = Vec::new();
        for id in asked {
            let id = id.as_str().unwrap_or("");
            if !all.iter().any(|a| a == id) {
                return Err(format!("This chat has no job {id}."));
            }
            if !out.iter().any(|o: &String| o == id) {
                out.push(id.to_string());
            }
        }
        Ok(out)
    }

    fn begin_wait(&self, o: &Thread, args: &Value) -> Result<(Vec<String>, Instant), String> {
        let ids = self.worker_ids(&o.id, args.get("ids"))?;
        let secs = args.get("timeoutSeconds").and_then(Value::as_f64).map(|s| s.clamp(1.0, orchestrator::MAX_WAIT_SECS as f64)).unwrap_or(orchestrator::DEFAULT_WAIT_SECS as f64);
        Ok((ids, Instant::now() + Duration::from_secs_f64(secs)))
    }

    fn message_agent(&mut self, o: &Thread, args: &Value) -> Result<Value, String> {
        self.person_asked(o)?;
        let id = args.get("id").and_then(Value::as_str).unwrap_or("").to_string();
        let text = args.get("text").and_then(Value::as_str).map(str::trim).unwrap_or("").to_string();
        self.worker_ids(&o.id, Some(&json!([id])))?;
        if text.is_empty() {
            return Err("Write a message first.".into());
        }
        if self.worker_busy(&id) {
            return Err("busy: That job is still working. Wait for it first.".into());
        }
        let lookup = self.store.thread(&id).ok().flatten().is_some_and(|w| w.worker_kind == Some(WorkerKind::Lookup));
        self.worker_queue.push_back((o.id.clone(), id.clone(), text));
        if self.running_workers(&o.id, lookup) >= slots(lookup) {
            if let Ok(Some(mut w)) = self.store.thread(&id) {
                w.preview = orchestrator::WAITING_PREVIEW.into();
                self.save_thread(&w);
            }
        }
        self.pump_workers(&o.id);
        match self.worker_ends.get(&id) {
            Some(TurnStatus::Failed) if !self.worker_busy(&id) => Err("That job couldn't start.".into()),
            _ => Ok(json!({ "ok": true })),
        }
    }

    /// Running jobs (or lookups) of a coordinator.
    fn running_workers(&self, orchestrator: &str, lookup: bool) -> usize {
        self.turns.values().filter(|t| t.parent.as_deref() == Some(orchestrator) && t.lookup == lookup).count()
    }

    fn worker_busy(&self, id: &str) -> bool {
        self.turns.contains_key(id) || self.worker_queue.iter().any(|(_, w, _)| w == id)
    }

    /// Start waiting jobs and lookups, oldest first, while their kind has a
    /// free slot.
    fn pump_workers(&mut self, orchestrator: &str) {
        let mut pos = 0;
        while pos < self.worker_queue.len() {
            let (o, worker) = {
                let (o, w, _) = &self.worker_queue[pos];
                (o.clone(), w.clone())
            };
            if o != orchestrator {
                pos += 1;
                continue;
            }
            let lookup = self.store.thread(&worker).ok().flatten().is_some_and(|w| w.worker_kind == Some(WorkerKind::Lookup));
            if self.running_workers(orchestrator, lookup) >= slots(lookup) {
                pos += 1;
                continue;
            }
            let Some((_, worker, text)) = self.worker_queue.remove(pos) else {
                return;
            };
            self.worker_ends.remove(&worker);
            match self.begin_turn(TurnSendParams { thread_id: worker.clone(), text, agent: None, model: None, effort: None, attachments: Vec::new() }) {
                Ok(begun) => self.launch(begun),
                Err(e) => self.worker_failed(&worker, &e.message),
            }
        }
    }

    fn worker_failed(&mut self, worker: &str, message: &str) {
        self.worker_ends.insert(worker.to_string(), TurnStatus::Failed);
        self.add_item(worker, None, ItemKind::Error, Some(message.to_string()), |_| {});
        if let Ok(Some(mut w)) = self.store.thread(worker) {
            w.status = ThreadStatus::Failed;
            w.preview = one_line(message, 140);
            w.updated_at = now_iso();
            self.save_thread(&w);
        }
    }

    /// Stop a coordinator's jobs (`None`: all): running ones get Stop,
    /// waiting ones never start.
    fn stop_workers(&mut self, orchestrator: &str, ids: Option<&[String]>) {
        let named = |w: &str| ids.is_none_or(|ids| ids.iter().any(|i| i == w));
        let queued: Vec<String> = self.worker_queue.iter().filter(|(o, w, _)| o == orchestrator && named(w)).map(|(_, w, _)| w.clone()).collect();
        self.worker_queue.retain(|(o, w, _)| !(o == orchestrator && named(w)));
        for w in queued {
            self.worker_ends.insert(w.clone(), TurnStatus::Stopped);
            if let Ok(Some(mut t)) = self.store.thread(&w) {
                if t.preview == orchestrator::WAITING_PREVIEW {
                    t.preview = "Stopped before it started".into();
                }
                t.updated_at = now_iso();
                self.save_thread(&t);
            }
        }
        let running: Vec<String> = self.turns.iter().filter(|(id, t)| t.parent.as_deref() == Some(orchestrator) && named(id)).map(|(id, _)| id.clone()).collect();
        for w in running {
            let _ = self.turn_stop(&w);
        }
    }

    /// "queued", "running", "waiting" (an approval or a card is open),
    /// "review", "done", "stopped", "failed" or "not_started".
    fn worker_status(&self, w: &Thread) -> &'static str {
        if self.worker_queue.iter().any(|(_, id, _)| *id == w.id) {
            return "queued";
        }
        if self.needs_person(&w.id) {
            return "waiting";
        }
        if self.turns.contains_key(&w.id) {
            return "running";
        }
        if w.review_at.is_some() {
            return "review";
        }
        match self.worker_ends.get(&w.id) {
            Some(TurnStatus::Done) => "done",
            Some(TurnStatus::Stopped) => "stopped",
            Some(TurnStatus::Failed) => "failed",
            None if w.status == ThreadStatus::Failed => "failed",
            None if self.store.item_count(&w.id).unwrap_or(0) == 0 => "not_started",
            None => "done",
        }
    }

    fn wait_result(&self, ids: &[String]) -> Value {
        let rows = ids.iter().map(|id| match self.store.thread(id).ok().flatten() {
            Some(w) => {
                let answer = self.store.items(id).ok().and_then(|items| items.into_iter().rev().find(|i| i.kind == ItemKind::Assistant).and_then(|i| i.text)).filter(|t| !t.trim().is_empty());
                json!({"id": id, "title": w.title, "kind": kind_of(&w), "status": self.worker_status(&w), "answer": answer.map(|a| cut(&a, orchestrator::ANSWER_CHARS))})
            }
            None => json!({"id": id, "title": "", "status": "deleted", "answer": null}),
        });
        Value::Array(rows.collect())
    }

    /// Answer the waits whose workers are all done, or whose time is up.
    fn check_waits(&mut self) {
        let now = Instant::now();
        let (ready, keep): (Vec<Wait>, Vec<Wait>) = std::mem::take(&mut self.waits).into_iter().partition(|w| now >= w.until || w.ids.iter().all(|id| !self.worker_busy(id)));
        self.waits = keep;
        for w in ready {
            let _ = w.reply.send(Ok(self.wait_result(&w.ids)));
        }
    }

    /// Answer an orchestrator's waits now, with where its workers are.
    fn answer_waits_of(&mut self, orchestrator: &str) {
        let (mine, keep): (Vec<Wait>, Vec<Wait>) = std::mem::take(&mut self.waits).into_iter().partition(|w| w.orchestrator == orchestrator);
        self.waits = keep;
        for w in mine {
            let _ = w.reply.send(Ok(self.wait_result(&w.ids)));
        }
    }

    // ---------- FeedHQ ----------

    fn feedhq_connect(&mut self) -> Value {
        let pkce = oauth::pkce();
        let state = oauth::new_state();
        let url = oauth::authorize_url(&self.cfg.feedhq_origin, self.auth.origin(), &pkce.challenge, &state);
        self.sign_ins.insert(state, (pkce.verifier, Instant::now()));
        json!({ "authorizeUrl": url })
    }

    fn oauth_redirect(&mut self, url: &str) {
        let Some(r) = oauth::parse_redirect(url) else {
            return;
        };
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
    fn titles_and_sign_in_errors() {
        assert_eq!(title_from("  Plan my pottery launch for spring, please and thanks"), "Plan my pottery launch for spring");
        assert_eq!(title_from("Hi there."), "Hi there");
        assert!(looks_signed_out("Invalid API key · Please run /login"));
        assert!(looks_signed_out("Not logged in. Run codex login."));
        assert!(!looks_signed_out("The file wasn't found."));
    }
}
