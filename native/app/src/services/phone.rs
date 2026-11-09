//! phone-sync.ts: phone sync with Content HQ. The Mac stays the source of truth:
//!   - it sends Content HQ a snapshot of Billable whenever something changes,
//!   - it picks up changes made on the phone (the "actions"), applies them,
//!     and acknowledges each one in the same request as a snapshot that
//!     already includes its effect.
//!
//! It checks in every 20 seconds while Billable is running, right away when
//! the Mac wakes or unlocks, and soon after anything changes on the Mac.
//!
//! The connection belongs to the profile that was open when it was made and
//! only ever talks to the Content HQ address that issued it. Its token lives
//! in the macOS Keychain (service "Billable", one item per profile folder)
//! and is never logged or shown. `<profile>/phone-sync.json` keeps the rest
//! (`version`, `origin`, `profileId`, `connectedAt`), as the Electron app did.
//!
//! Database work happens on the main thread (the `Store`'s connection);
//! requests run on a background thread with a small blocking client.

use std::{
    cell::RefCell,
    collections::HashMap,
    ffi::c_void,
    path::{Path, PathBuf},
    ptr::NonNull,
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use billable_core::{
    js::{js_string_to_number, js_truthy},
    phone::{self, Ack, Activity, AppliedSummary},
    TimerManager,
};
use gpui::{App, AsyncApp, Task};
use serde::Serialize;
use serde_json::Value;

use crate::{
    components::{toast::toast_for, toast_error, Mood, ToastIcon},
    store,
};

const CONFIG_FILE: &str = "phone-sync.json";
const KEYCHAIN_SERVICE: &str = "Billable";
const MIN_GAP_MS: i64 = 5_200; // Content HQ allows one request every 5 seconds
const DEFAULT_INTERVAL_S: i64 = 20;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const ACCOUNT_TIMEOUT: Duration = Duration::from_secs(8);
const CHANGE_POLL_MS: i64 = 3_000;
const LOOP_MS: u64 = 250;
/// utils/contentHq.ts `CONTENT_HQ_DEFAULT_URL`
pub const CONTENT_HQ_DEFAULT_URL: &str = "https://content-hq-live.vercel.app";

// ---------- Public shapes (PhoneSyncStatus etc.) ----------

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SyncState {
    Off,
    Syncing,
    Ok,
    Offline,
    Unavailable,
    Revoked,
    Mismatch,
    Error,
}

/// `PhoneSyncStatus`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub connected: bool,
    pub state: SyncState,
    pub host: Option<String>,
    pub profile_name: Option<String>,
    pub connected_at: Option<String>,
    pub last_sync_at: Option<String>,
    pub message: Option<String>,
}

impl Default for Status {
    fn default() -> Self {
        Status { connected: false, state: SyncState::Off, host: None, profile_name: None, connected_at: None, last_sync_at: None, message: None }
    }
}

/// Content HQ asked to connect (`phone:connect-request`).
#[derive(Clone, Debug, PartialEq)]
pub struct ConnectRequest {
    pub request_id: String,
    pub host: String,
    /// The Content HQ account the key belongs to, when Content HQ says.
    pub account: Option<String>,
    pub profile_name: Option<String>,
}

/// Open at login (`phone:open-at-login`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LoginItem {
    pub available: bool,
    pub enabled: bool,
    pub needs_approval: bool,
}

/// What listeners hear about (the `phone:*` events).
#[derive(Clone, Debug)]
pub enum PhoneEvent {
    Status(Status),
    /// Changes from the phone were applied (data has already reloaded).
    Applied(Vec<AppliedSummary>),
    /// Show the confirmation (`None`: it was answered or withdrawn).
    ConnectRequest(Option<ConnectRequest>),
}

pub type Listener = Rc<dyn Fn(&PhoneEvent, &mut App)>;

// ---------- State ----------

#[derive(Clone)]
struct Conn {
    profile_id: String,
    dir: PathBuf,
    origin: String,
    token: String,
    connected_at: String,
}

struct Pending {
    token: String,
    origin: String,
    expires: i64,
}

#[derive(Default)]
struct State {
    started: bool,
    conn: Option<Conn>,
    /// Bumped whenever `conn` changes (TypeScript's `conn !== c`).
    conn_gen: u64,
    loaded_profile_id: Option<String>,
    status: Status,
    last_sent_hash: Option<u64>,
    needs_snapshot: bool,
    in_flight: bool,
    rerun: bool,
    next_due_at: Option<i64>,
    last_request_at: i64,
    not_before: i64,
    failures: i32,
    fast_retry_until: i64,
    last_changes: i64,
    device_name: Option<String>,
    pending: HashMap<String, Pending>,
    request: Option<ConnectRequest>,
    listeners: Vec<(&'static str, Listener)>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State { needs_snapshot: true, last_changes: -1, ..Default::default() });
}
static WOKE: AtomicBool = AtomicBool::new(false);

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|s| f(&mut s.borrow_mut()))
}

/// `s.slice(0, n)` (UTF-16 units).
fn cut(s: &str, n: usize) -> String {
    phone::slice16(s, n).to_string()
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn now_iso() -> String {
    billable_core::time::iso_string(now_ms())
}

fn emit(event: PhoneEvent, cx: &mut App) {
    let listeners: Vec<Listener> = with(|s| s.listeners.iter().map(|(_, l)| l.clone()).collect());
    // Listeners run once the current update has finished: emit is often
    // reached from inside a listener's own view (the connect dialog's Connect
    // button), and updating a view while it's being updated aborts the app.
    cx.defer(move |cx| {
        for l in listeners {
            l(&event, cx);
        }
    });
}

/// Hear about status changes, applied phone changes and connect requests.
/// A listener registered again under the same `key` replaces the old one.
pub fn listen(key: &'static str, listener: Listener) {
    with(|s| {
        s.listeners.retain(|(k, _)| *k != key);
        s.listeners.push((key, listener));
    });
}

fn set_status(cx: &mut App, patch: impl FnOnce(&mut Status)) {
    let status = with(|s| {
        patch(&mut s.status);
        s.status.clone()
    });
    emit(PhoneEvent::Status(status), cx);
}

pub fn status() -> Status {
    with(|s| s.status.clone())
}

/// The confirmation waiting for an answer, if any.
pub fn pending_request() -> Option<ConnectRequest> {
    with(|s| s.request.clone())
}

// ---------- Storage ----------

fn token_ok(token: &str) -> bool {
    token.len() >= 44
        && token.len() <= 68
        && token.starts_with("bhq_")
        && token[4..].bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// One Keychain item per profile folder, so a copied data folder (like
/// "Billable Dev") never picks up the real folder's connection.
fn keychain_account(dir: &Path) -> String {
    format!("phone-sync {}", dir.display())
}

fn read_connection(profile_id: &str, dir: &Path) -> Option<Conn> {
    let raw: Value = serde_json::from_str(&std::fs::read_to_string(dir.join(CONFIG_FILE)).ok()?).ok()?;
    if raw.get("version") != Some(&Value::from(1)) || raw.get("profileId").and_then(Value::as_str) != Some(profile_id) {
        return None;
    }
    let origin = raw.get("origin").and_then(Value::as_str).filter(|o| !o.is_empty())?.to_string();
    let token = security_framework::passwords::get_generic_password(KEYCHAIN_SERVICE, &keychain_account(dir)).ok()?;
    let token = String::from_utf8(token).ok().filter(|t| token_ok(t))?;
    let connected_at = raw.get("connectedAt").and_then(Value::as_str).unwrap_or_default().to_string();
    Some(Conn { profile_id: profile_id.into(), dir: dir.into(), origin, token, connected_at })
}

fn write_connection(c: &Conn) -> anyhow::Result<()> {
    use std::io::Write as _;
    use std::os::unix::fs::OpenOptionsExt as _;
    security_framework::passwords::set_generic_password(KEYCHAIN_SERVICE, &keychain_account(&c.dir), c.token.as_bytes())?;
    let stored = serde_json::json!({
        "version": 1,
        "origin": c.origin,
        "profileId": c.profile_id,
        "connectedAt": c.connected_at,
    });
    let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(c.dir.join(CONFIG_FILE))?;
    f.write_all(serde_json::to_string_pretty(&stored)?.as_bytes())?;
    Ok(())
}

fn forget_connection(dir: &Path) {
    let _ = std::fs::remove_file(dir.join(CONFIG_FILE));
    let _ = security_framework::passwords::delete_generic_password(KEYCHAIN_SERVICE, &keychain_account(dir));
}

// ---------- Origins ----------

/// `new URL(s)`'s (protocol, host, hostname) for http(s) addresses.
fn parse_origin(s: &str) -> Option<(String, String, String)> {
    let s = s.trim();
    let (scheme, rest) = s.split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return None;
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let authority = authority.rsplit('@').next().unwrap_or("").to_ascii_lowercase();
    let (hostname, port) = match authority.rsplit_once(':') {
        Some((h, p)) if !h.ends_with(']') || authority.starts_with('[') => (h.to_string(), Some(p.to_string())),
        _ => (authority.clone(), None),
    };
    if hostname.is_empty() {
        return None;
    }
    let default_port = if scheme == "https" { "443" } else { "80" };
    let host = match port.filter(|p| !p.is_empty() && p != default_port) {
        Some(p) => {
            if !p.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            format!("{hostname}:{p}")
        }
        None => hostname.clone(),
    };
    Some((scheme, host, hostname))
}

fn host_of(origin: &str) -> Option<String> {
    parse_origin(origin).map(|(_, host, _)| host)
}

/// Content HQ's origin as Settings configures it (contenthq.ts
/// `contentHqOrigin`), e.g. `https://content-hq-live.vercel.app`.
pub fn content_hq_origin(cx: &App) -> Option<String> {
    let configured = store::db(cx).get_settings().ok().map(|s| s.text("content_hq_url").trim().to_string()).unwrap_or_default();
    let url = if configured.is_empty() { CONTENT_HQ_DEFAULT_URL.to_string() } else { configured };
    parse_origin(&url).map(|(scheme, host, _)| format!("{scheme}://{host}"))
}

// ---------- Profile ----------

fn active_profile(cx: &App) -> (String, PathBuf, Option<String>) {
    let s = store::store(cx);
    let id = s.profiles.get_active_profile_id();
    let dir = s.profiles.profile_dir(&id);
    let name = s.profiles.get_active_profile().map(|p| p.name).filter(|n| !n.is_empty());
    (id, dir, name)
}

/// Pick up the connection for whichever profile is open now.
fn load_for_active_profile(cx: &mut App) {
    let (profile_id, dir, profile_name) = active_profile(cx);
    if with(|s| s.loaded_profile_id.as_deref() == Some(profile_id.as_str())) {
        return;
    }
    let conn = read_connection(&profile_id, &dir);
    let connected = conn.is_some();
    let host = conn.as_ref().and_then(|c| host_of(&c.origin));
    let connected_at = conn.as_ref().map(|c| c.connected_at.clone()).filter(|c| !c.is_empty());
    with(|s| {
        s.loaded_profile_id = Some(profile_id);
        s.conn = conn;
        s.conn_gen += 1;
        s.last_sent_hash = None;
        s.needs_snapshot = true;
        s.failures = 0;
        s.last_changes = -1;
    });
    set_status(cx, |st| {
        let revoked = st.state == SyncState::Revoked;
        st.connected = connected;
        st.state = if connected { SyncState::Syncing } else if revoked { SyncState::Revoked } else { SyncState::Off };
        st.host = host;
        st.profile_name = profile_name;
        st.connected_at = connected_at;
        st.last_sync_at = None;
        st.message = if connected || !revoked { None } else { st.message.clone() };
    });
}

// ---------- Scheduling ----------

fn schedule(delay_ms: i64) {
    with(|s| {
        if s.conn.is_none() {
            return;
        }
        let due = (now_ms() + delay_ms).max(s.last_request_at + MIN_GAP_MS).max(s.not_before);
        // Keep an earlier run if one is already set
        if s.next_due_at.is_some_and(|d| d <= due) {
            return;
        }
        s.next_due_at = Some(due);
    })
}

fn sync_soon() {
    schedule(0)
}

fn backoff_ms(s: &State) -> i64 {
    if now_ms() < s.fast_retry_until {
        return 5_000;
    }
    let exp = (s.failures - 1).min(4);
    (300_000f64).min(20_000.0 * 2f64.powi(exp)) as i64
}

// ---------- Talking to Content HQ ----------

struct Reply {
    status: u16,
    retry_after: Option<String>,
    body: String,
}

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(timeout))
        .user_agent(format!("Billable/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

fn read_reply(mut res: ureq::http::Response<ureq::Body>) -> anyhow::Result<Reply> {
    let status = res.status().as_u16();
    let retry_after = res.headers().get("retry-after").and_then(|v| v.to_str().ok()).map(str::to_string);
    let body = res.body_mut().read_to_string().unwrap_or_default();
    Ok(Reply { status, retry_after, body })
}

/// POST JSON with the connection's bearer token (blocking).
fn post(origin: &str, token: &str, path: &str, body: &str) -> anyhow::Result<Reply> {
    let res = agent(REQUEST_TIMEOUT)
        .post(format!("{origin}{path}"))
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .send(body.as_bytes())?;
    read_reply(res)
}

/// `scutil --get ComputerName`, else the host name without `.local`.
fn computer_name() -> String {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let out = std::process::Command::new("/usr/sbin/scutil").args(["--get", "ComputerName"]).output();
        let _ = tx.send(out.ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()));
    });
    let name = rx.recv_timeout(Duration::from_secs(2)).ok().flatten().filter(|n| !n.is_empty()).unwrap_or_else(|| {
        let mut buf = [0u8; 256];
        let ok = unsafe { libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) } == 0;
        let host = if ok { String::from_utf8_lossy(&buf[..buf.iter().position(|b| *b == 0).unwrap_or(0)]).to_string() } else { String::new() };
        if host.to_ascii_lowercase().ends_with(".local") {
            host[..host.len() - 6].to_string()
        } else {
            host
        }
    });
    let name = if name.is_empty() { "Mac".to_string() } else { name };
    cut(&name, 60)
}

/// Whether macOS thinks the Mac has a network (Chromium's `net.isOnline()`
/// asks SCNetworkReachability the same way).
fn is_online() -> bool {
    #[repr(C)]
    struct SockaddrIn {
        sin_len: u8,
        sin_family: u8,
        sin_port: u16,
        sin_addr: u32,
        sin_zero: [u8; 8],
    }
    #[link(name = "SystemConfiguration", kind = "framework")]
    extern "C" {
        fn SCNetworkReachabilityCreateWithAddress(allocator: *const c_void, address: *const SockaddrIn) -> *const c_void;
        fn SCNetworkReachabilityGetFlags(target: *const c_void, flags: *mut u32) -> u8;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFRelease(cf: *const c_void);
    }
    let addr = SockaddrIn { sin_len: std::mem::size_of::<SockaddrIn>() as u8, sin_family: 2, sin_port: 0, sin_addr: 0, sin_zero: [0; 8] };
    unsafe {
        let target = SCNetworkReachabilityCreateWithAddress(std::ptr::null(), &addr);
        if target.is_null() {
            return true;
        }
        let mut flags = 0u32;
        let got = SCNetworkReachabilityGetFlags(target, &mut flags) != 0;
        CFRelease(target);
        !got || (flags & (1 << 1) != 0 && flags & (1 << 2) == 0)
    }
}

/// `Number(v)` for a JSON value.
fn js_number(v: Option<&Value>) -> f64 {
    match v {
        None => f64::NAN,
        Some(Value::Null) => 0.0,
        Some(Value::Bool(b)) => *b as i32 as f64,
        Some(Value::Number(n)) => n.as_f64().unwrap_or(f64::NAN),
        Some(Value::String(s)) => js_string_to_number(s),
        Some(_) => f64::NAN,
    }
}

fn total_changes(cx: &App) -> Option<i64> {
    store::db(cx).connection().query_row("SELECT total_changes() AS n", [], |r| r.get(0)).ok()
}

/// What a tick prepared on the main thread for the request.
struct Prepared {
    acks: Vec<Ack>,
    snapshot: Option<String>,
    hash: u64,
    profile_name: String,
}

fn prepare(cx: &App, c: &Conn) -> anyhow::Result<Prepared> {
    let db = store::db(cx);
    let acks = phone::unacked_acks(db)?;
    let timer = TimerManager::new(db)?;
    let snapshot = serde_json::to_string(&phone::build_snapshot(db, &timer)?)?;
    // The hash leaves out generatedAt (always the first field)
    let rest = snapshot.split_once(",\"currency\"").map(|(_, r)| r).unwrap_or(&snapshot);
    let hash = {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        rest.hash(&mut h);
        h.finish()
    };
    let include = with(|s| s.needs_snapshot || s.last_sent_hash != Some(hash)) || !acks.is_empty();
    let (_, _, name) = active_profile(cx);
    let profile_name = cut(&name.unwrap_or_else(|| c.profile_id.clone()), 200);
    Ok(Prepared { acks, snapshot: include.then_some(snapshot), hash, profile_name })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo<'a> {
    version: &'a str,
    profile_id: &'a str,
    profile_name: &'a str,
    device_name: &'a str,
}

fn sync_body(c: &Conn, p: &Prepared, device_name: &str) -> String {
    let app = AppInfo { version: env!("CARGO_PKG_VERSION"), profile_id: &c.profile_id, profile_name: &p.profile_name, device_name };
    format!(
        "{{\"schemaVersion\":1,\"app\":{},\"snapshot\":{},\"acks\":{}}}",
        serde_json::to_string(&app).unwrap_or_default(),
        p.snapshot.as_deref().unwrap_or("null"),
        serde_json::to_string(&p.acks).unwrap_or_else(|_| "[]".into())
    )
}

fn tick(cx: &mut App) {
    if with(|s| s.in_flight) {
        with(|s| s.rerun = true);
        return;
    }
    load_for_active_profile(cx);
    let Some(c) = with(|s| s.conn.clone()) else { return };
    // Content HQ asked us to wait (429 Retry-After)
    let not_before = with(|s| s.not_before);
    if now_ms() < not_before {
        schedule(not_before - now_ms());
        return;
    }
    let gen = with(|s| {
        s.in_flight = true;
        s.conn_gen
    });
    set_status(cx, |s| s.state = if s.state == SyncState::Ok { SyncState::Ok } else { SyncState::Syncing });

    let prepared = prepare(cx, &c);
    let device_name = with(|s| s.device_name.clone());
    with(|s| s.last_request_at = now_ms());
    let request = {
        let c = c.clone();
        cx.background_executor().spawn(async move {
            let prepared = prepared?;
            let device_name = device_name.unwrap_or_else(computer_name);
            let reply = post(&c.origin, &c.token, "/api/billable/sync", &sync_body(&c, &prepared, &device_name));
            Ok::<_, anyhow::Error>((prepared, device_name, reply?))
        })
    };
    cx.spawn(async move |cx: &mut AsyncApp| {
        let result = request.await;
        cx.update(|cx| {
            let follow_up = finish(cx, &c, gen, result);
            with(|s| s.in_flight = false);
            let changes = total_changes(cx);
            let (connected, rerun) = with(|s| {
                if let Some(n) = changes {
                    s.last_changes = n;
                }
                let rerun = s.rerun;
                s.rerun = false;
                (s.conn.is_some(), rerun)
            });
            if connected {
                if rerun {
                    sync_soon()
                } else if let Some(ms) = follow_up {
                    schedule(ms)
                }
            }
        });
    })
    .detach();
}

/// Handle a sync's outcome; returns when to check in next (`None`: don't).
fn finish(cx: &mut App, c: &Conn, gen: u64, result: anyhow::Result<(Prepared, String, Reply)>) -> Option<i64> {
    let default = Some(DEFAULT_INTERVAL_S * 1000);
    // The profile may have been switched while the request was out
    if with(|s| s.conn_gen) != gen {
        return default;
    }
    let outcome = result.and_then(|(prepared, device_name, reply)| {
        with(|s| s.device_name = Some(device_name));
        handle_reply(cx, c, prepared, reply)
    });
    match outcome {
        Ok(next) => next,
        Err(e) => {
            if with(|s| s.conn_gen) != gen {
                return default;
            }
            let offline = !is_online();
            eprintln!("[phone] sync failed: {e}");
            let next = with(|s| {
                s.failures += 1;
                backoff_ms(s)
            });
            set_status(cx, |s| {
                s.state = SyncState::Offline;
                s.message = Some(if offline {
                    "Your Mac is offline. Billable will sync when it\u{2019}s back.".into()
                } else {
                    "Couldn\u{2019}t reach Content HQ. Billable will keep trying.".into()
                });
            });
            Some(next)
        }
    }
}

fn handle_reply(cx: &mut App, c: &Conn, prepared: Prepared, reply: Reply) -> anyhow::Result<Option<i64>> {
    match reply.status {
        401 => {
            forget_connection(&c.dir);
            with(|s| {
                s.conn = None;
                s.conn_gen += 1;
            });
            set_status(cx, |s| {
                s.connected = false;
                s.state = SyncState::Revoked;
                s.host = None;
                s.connected_at = None;
                s.message = Some("This Mac was disconnected in Content HQ. Connect it again from Content HQ\u{2019}s settings.".into());
            });
            return Ok(None);
        }
        409 => {
            set_status(cx, |s| {
                s.state = SyncState::Mismatch;
                s.message = Some("Content HQ is linked to a different Billable profile. Disconnect and connect again from this profile.".into());
            });
            return Ok(Some(10 * 60_000));
        }
        404 => {
            set_status(cx, |s| {
                s.state = SyncState::Unavailable;
                s.message = Some("Content HQ doesn\u{2019}t have phone sync turned on yet.".into());
            });
            return Ok(Some(5 * 60_000));
        }
        429 => {
            let n = reply.retry_after.as_deref().map(js_string_to_number).unwrap_or(0.0);
            let wait = (if n.is_nan() || n == 0.0 { 10.0 } else { n }).clamp(5.0, 300.0);
            let ms = (wait * 1000.0) as i64;
            with(|s| s.not_before = now_ms() + ms);
            return Ok(Some(ms));
        }
        200..=299 => {}
        status => {
            eprintln!("[phone] sync refused ({status}): {}", cut(&reply.body, 500));
            let next = with(|s| {
                s.failures += 1;
                backoff_ms(s)
            });
            set_status(cx, |s| {
                s.state = SyncState::Error;
                s.message = Some(format!("Content HQ didn\u{2019}t accept the sync (error {status}). Billable will keep trying."));
            });
            return Ok(Some(next.max(60_000)));
        }
    }

    let json: Option<Value> = serde_json::from_str(&reply.body).ok();
    let actions = json.as_ref().filter(|j| j.get("ok") == Some(&Value::Bool(true))).and_then(|j| j.get("actions")).and_then(Value::as_array);
    let (Some(json), Some(actions)) = (json.as_ref(), actions) else {
        let next = with(|s| {
            s.failures += 1;
            backoff_ms(s)
        });
        set_status(cx, |s| {
            s.state = SyncState::Error;
            s.message = Some("Content HQ sent back something Billable couldn\u{2019}t read.".into());
        });
        return Ok(Some(next.max(60_000)));
    };

    // Accepted: this snapshot and these acks are stored
    let db = store::db(cx);
    if !prepared.acks.is_empty() {
        let ids: Vec<String> = prepared.acks.iter().map(|a| a.action_id.clone()).collect();
        phone::mark_acked(db, &ids)?;
    }
    let needs_snapshot = json.get("needsSnapshot").is_some_and(js_truthy);
    with(|s| {
        if prepared.snapshot.is_some() {
            s.last_sent_hash = Some(prepared.hash);
        }
        s.needs_snapshot = needs_snapshot;
        s.failures = 0;
    });
    let synced = now_iso();
    set_status(cx, |s| {
        s.state = SyncState::Ok;
        s.message = None;
        s.last_sync_at = Some(synced);
    });

    // Only apply to the profile this connection belongs to
    let (profile_id, _, _) = active_profile(cx);
    if profile_id != c.profile_id {
        return Ok(Some(0));
    }
    let db = store::db(cx);
    let mut timer = TimerManager::new(db)?;
    let applied = phone::apply_incoming(db, &mut timer, actions)?;
    if !applied.is_empty() {
        announce_applied(&applied, cx);
        return Ok(Some(0)); // report back straight away, with the new snapshot
    }
    if needs_snapshot {
        return Ok(Some(0));
    }
    let next = js_number(json.get("nextSyncSeconds"));
    let secs = if next.is_finite() { next.clamp(10.0, 300.0) } else { DEFAULT_INTERVAL_S as f64 };
    Ok(Some((secs * 1000.0) as i64))
}

/// Reload what's on screen (timer included) and say what happened (App.tsx's
/// `phone:applied` toast).
fn announce_applied(results: &[AppliedSummary], cx: &mut App) {
    store::data_changed(cx);
    let rejected: Vec<&AppliedSummary> = results.iter().filter(|r| r.status == "rejected").collect();
    let text = if results.len() == 1 {
        match rejected.first() {
            Some(r) => format!(
                "Couldn\u{2019}t apply a change from your phone: {}",
                r.message.clone().filter(|m| !m.is_empty()).unwrap_or_else(|| r.summary.clone())
            ),
            None => format!("From your phone: {}", results[0].summary),
        }
    } else {
        format!(
            "{} changes from your phone{}",
            results.len(),
            if rejected.is_empty() { String::new() } else { format!(", {} couldn\u{2019}t be applied", rejected.len()) }
        )
    };
    toast_for(text, ToastIcon::Mascot(if rejected.is_empty() { Mood::Wave } else { Mood::Worried }), 4000, cx);
    emit(PhoneEvent::Applied(results.to_vec()), cx);
}

// ---------- Connecting ----------

/// A random UUID v4 (`crypto.randomUUID()`).
fn random_uuid() -> String {
    let mut b = [0u8; 16];
    if security_framework::random::SecRandom::default().copy_bytes(&mut b).is_err() {
        let t = now_ms().to_le_bytes();
        b[..8].copy_from_slice(&t);
    }
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &h[..8], &h[8..12], &h[12..16], &h[16..20], &h[20..])
}

/// Which Content HQ account a token belongs to, asked of Content HQ itself,
/// so the confirmation can name it. Versions without the route answer 404,
/// and the confirmation goes without the email. `(email, refused)`.
fn account_for_token(token: &str, origin: &str) -> (Option<String>, bool) {
    let res = agent(ACCOUNT_TIMEOUT)
        .get(format!("{origin}/api/billable/account"))
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/json")
        .call();
    let Ok(reply) = res.map_err(anyhow::Error::from).and_then(read_reply) else { return (None, false) };
    if reply.status == 401 {
        return (None, true);
    }
    if !(200..300).contains(&reply.status) {
        return (None, false);
    }
    let email = serde_json::from_str::<Value>(&reply.body)
        .ok()
        .filter(|j| j.get("ok") == Some(&Value::Bool(true)))
        .and_then(|j| j.get("email").and_then(Value::as_str).map(|e| cut(billable_core::js::js_trim(e), 200)))
        .filter(|e| !e.is_empty());
    (email, false)
}

/// A `billable://connect?token=…` link from Content HQ's own page in
/// Billable's window. Only call this for links from that view, whose page is
/// on [`content_hq_origin`]: from anywhere else a link could hand Billable
/// someone else's token and send your data to their account. Nothing is
/// stored until the person confirms in [`PhoneConnectModal`].
///
/// [`PhoneConnectModal`]: super::phone_connect::PhoneConnectModal
pub fn accept_connect_token(token: &str, cx: &mut App) {
    ensure_started(cx);
    let Some(origin) = content_hq_origin(cx) else { return };
    request_connect(token.to_string(), origin, cx);
}

fn request_connect(token: String, origin: String, cx: &mut App) {
    if !token_ok(&token) {
        return;
    }
    let Some((scheme, host, hostname)) = parse_origin(&origin) else { return };
    let local = scheme == "http" && (hostname == "localhost" || hostname == "127.0.0.1");
    if scheme != "https" && !local {
        return;
    }
    let origin = format!("{scheme}://{host}");
    crate::platform::show_main_window(cx);
    with(|s| s.last_request_at = now_ms());
    let lookup = {
        let (token, origin) = (token.clone(), origin.clone());
        cx.background_executor().spawn(async move { account_for_token(&token, &origin) })
    };
    cx.spawn(async move |cx: &mut AsyncApp| {
        let (account, refused) = lookup.await;
        cx.update(|cx| {
            if refused {
                toast_error("Content HQ didn\u{2019}t recognise that connection. Click Connect this Mac again.", cx);
                return;
            }
            let (_, _, profile_name) = active_profile(cx);
            let request_id = random_uuid();
            let request = ConnectRequest { request_id: request_id.clone(), host, account, profile_name };
            with(|s| {
                let now = now_ms();
                s.pending.retain(|_, p| p.expires >= now);
                s.pending.insert(request_id, Pending { token, origin, expires: now + 5 * 60_000 });
                s.request = Some(request.clone());
            });
            emit(PhoneEvent::ConnectRequest(Some(request)), cx);
        });
    })
    .detach();
}

/// The person answered the confirmation. Returns the status afterwards.
pub fn confirm_connect(request_id: &str, accept: bool, cx: &mut App) -> Status {
    let pending = with(|s| {
        if s.request.as_ref().is_some_and(|r| r.request_id == request_id) {
            s.request = None;
        }
        s.pending.remove(request_id)
    });
    emit(PhoneEvent::ConnectRequest(None), cx);
    let Some(pending) = pending.filter(|p| accept && p.expires >= now_ms()) else { return status() };
    let (profile_id, dir, _) = active_profile(cx);
    let c = Conn { profile_id, dir, origin: pending.origin, token: pending.token, connected_at: now_iso() };
    if let Err(e) = write_connection(&c) {
        eprintln!("[phone] couldn't store the connection: {e}");
        set_status(cx, |s| {
            s.state = SyncState::Error;
            s.message = Some("Billable couldn\u{2019}t use the Keychain to store the connection safely.".into());
        });
        return status();
    }
    with(|s| s.loaded_profile_id = None); // reload from disk
    load_for_active_profile(cx);
    sync_soon();
    status()
}

/// Disconnect this profile, and tell Content HQ so it deletes its copy.
/// Resolves to the new status and whether Content HQ heard.
pub fn disconnect(cx: &mut App) -> Task<(Status, bool)> {
    ensure_started(cx);
    load_for_active_profile(cx);
    let Some(c) = with(|s| s.conn.take()) else { return Task::ready((status(), true)) };
    with(|s| {
        s.conn_gen += 1;
        s.next_due_at = None;
        s.last_request_at = now_ms();
    });
    forget_connection(&c.dir);
    set_status(cx, |s| {
        s.connected = false;
        s.state = SyncState::Off;
        s.host = None;
        s.connected_at = None;
        s.last_sync_at = None;
        s.message = None;
    });
    let request = cx.background_executor().spawn(async move {
        match post(&c.origin, &c.token, "/api/billable/disconnect", "{}") {
            Ok(r) => (200..300).contains(&r.status) || r.status == 401,
            Err(_) => false, // offline
        }
    });
    cx.spawn(async move |_| (status(), request.await))
}

/// "Sync now".
pub fn sync_now(cx: &mut App) -> Status {
    ensure_started(cx);
    load_for_active_profile(cx);
    with(|s| s.failures = 0);
    sync_soon();
    status()
}

/// Recent changes from the phone (`phone:activity`).
pub fn activity(limit: i64, cx: &App) -> Vec<Activity> {
    phone::list_activity(store::db(cx), limit)
}

// ---------- Open at login ----------

mod login {
    use objc2::{
        msg_send,
        rc::Retained,
        runtime::{AnyClass, AnyObject},
    };
    use objc2_foundation::NSBundle;

    #[link(name = "ServiceManagement", kind = "framework")]
    extern "C" {}

    /// Only an installed app (a bundle) can be a login item (Electron's `app.isPackaged`).
    fn packaged() -> bool {
        let path = NSBundle::mainBundle().bundlePath().to_string();
        path.ends_with(".app")
    }

    fn service() -> Option<Retained<AnyObject>> {
        let cls = AnyClass::get(c"SMAppService")?;
        unsafe { msg_send![cls, mainAppService] }
    }

    pub fn get() -> super::LoginItem {
        if !packaged() {
            return super::LoginItem::default();
        }
        let Some(svc) = service() else { return super::LoginItem::default() };
        // SMAppServiceStatus: 0 not registered, 1 enabled, 2 requires approval, 3 not found
        let status: isize = unsafe { msg_send![&*svc, status] };
        super::LoginItem { available: true, enabled: status == 1, needs_approval: status == 2 }
    }

    pub fn set(enabled: bool) -> super::LoginItem {
        if packaged() {
            if let Some(svc) = service() {
                let none = std::ptr::null_mut::<*mut AnyObject>();
                let ok: bool = if enabled {
                    unsafe { msg_send![&*svc, registerAndReturnError: none] }
                } else {
                    unsafe { msg_send![&*svc, unregisterAndReturnError: none] }
                };
                if !ok {
                    eprintln!("[phone] couldn't change the login item");
                }
            }
        }
        get()
    }
}

pub fn open_at_login() -> LoginItem {
    login::get()
}

pub fn set_open_at_login(enabled: bool) -> LoginItem {
    login::set(enabled)
}

// ---------- Start ----------

/// Back from sleep or the lock screen: NSWorkspace's wake notification and
/// the distributed `com.apple.screenIsUnlocked` (what Electron's
/// powerMonitor `resume` / `unlock-screen` use).
fn observe_wake() {
    use block2::RcBlock;
    use objc2_app_kit::{NSWorkspace, NSWorkspaceDidWakeNotification};
    use objc2_foundation::{NSDistributedNotificationCenter, NSNotification, NSString};

    let block = RcBlock::new(|_: NonNull<NSNotification>| WOKE.store(true, Ordering::SeqCst));
    unsafe {
        let center = NSWorkspace::sharedWorkspace().notificationCenter();
        std::mem::forget(center.addObserverForName_object_queue_usingBlock(Some(NSWorkspaceDidWakeNotification), None, None, &block));
        let unlocked = NSString::from_str("com.apple.screenIsUnlocked");
        let dist = NSDistributedNotificationCenter::defaultCenter();
        std::mem::forget(dist.addObserverForName_object_queue_usingBlock(Some(&unlocked), None, None, &block));
    }
}

/// Start phone sync (idempotent): load the open profile's connection, check
/// in, and keep checking in. Call once the `Store` is set.
pub fn start(cx: &mut App) {
    if with(|s| std::mem::replace(&mut s.started, true)) {
        return;
    }
    load_for_active_profile(cx);
    sync_soon();
    observe_wake();
    cx.spawn(async move |cx: &mut AsyncApp| {
        let mut last_poll = now_ms();
        loop {
            cx.background_executor().timer(Duration::from_millis(LOOP_MS)).await;
            cx.update(|cx| {
                // billable://connect from Content HQ's view (platform/mod.rs)
                if let Some(token) = crate::platform::take_phone_connect() {
                    accept_connect_token(&token, cx);
                }
                if WOKE.swap(false, Ordering::SeqCst) {
                    // Catch up now, retrying quickly while the network comes back
                    with(|s| {
                        s.fast_retry_until = now_ms() + 2 * 60_000;
                        s.failures = 0;
                    });
                    schedule(2_000);
                }
                let now = now_ms();
                if now - last_poll >= CHANGE_POLL_MS {
                    last_poll = now;
                    poll_changes(cx);
                }
                let due = with(|s| match s.next_due_at {
                    Some(d) if now >= d => {
                        s.next_due_at = None;
                        true
                    }
                    _ => false,
                });
                if due {
                    tick(cx);
                }
            });
        }
    })
    .detach();
}

/// Same as [`start`]; the Settings hook and the connect modal call it so the
/// service runs even if nobody started it explicitly.
pub fn ensure_started(cx: &mut App) {
    start(cx)
}

/// Anything changed on the Mac (or a profile switch) → sync soon.
fn poll_changes(cx: &mut App) {
    let (profile_id, _, _) = active_profile(cx);
    if with(|s| s.loaded_profile_id.as_deref() != Some(profile_id.as_str())) {
        with(|s| s.next_due_at = None);
        load_for_active_profile(cx);
        if with(|s| s.conn.is_some()) {
            sync_soon();
        }
        return;
    }
    if with(|s| s.conn.is_none() || s.in_flight) {
        return;
    }
    let Some(n) = total_changes(cx) else { return };
    let changed = with(|s| {
        let changed = s.last_changes != -1 && n != s.last_changes && s.status.state != SyncState::Offline;
        s.last_changes = n;
        changed
    });
    if changed {
        schedule(2_000);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origins_parse_like_url() {
        assert_eq!(parse_origin("https://Content-HQ-Live.vercel.app/home?x=1").map(|o| o.1), Some("content-hq-live.vercel.app".into()));
        assert_eq!(parse_origin("https://example.com:443/").map(|o| o.1), Some("example.com".into()));
        assert_eq!(parse_origin("http://127.0.0.1:8787").map(|o| (o.1, o.2)), Some(("127.0.0.1:8787".into(), "127.0.0.1".into())));
        assert!(parse_origin("content-hq.example").is_none());
        assert!(parse_origin("ftp://example.com").is_none());
    }

    #[test]
    fn tokens_and_ids() {
        assert!(token_ok(&format!("bhq_{}", "a".repeat(40))));
        assert!(!token_ok(&format!("bhq_{}", "a".repeat(39))));
        assert!(!token_ok(&format!("bhq_{}", "a".repeat(65))));
        assert!(!token_ok(&format!("bhq_{}!", "a".repeat(41))));
        let id = random_uuid();
        assert_eq!(id.len(), 36);
        assert_eq!(&id[14..15], "4");
    }

    /// A tiny HTTP server on 127.0.0.1 answering each connection with the next canned reply.
    fn serve(replies: Vec<(u16, &'static str)>) -> (String, std::sync::mpsc::Receiver<String>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for (status, body) in replies {
                let (mut s, _) = listener.accept().unwrap();
                let mut buf = vec![0u8; 65536];
                let n = s.read(&mut buf).unwrap();
                let _ = tx.send(String::from_utf8_lossy(&buf[..n]).to_string());
                let _ = write!(s, "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            }
        });
        (origin, rx)
    }

    #[test]
    fn account_lookup_and_posts() {
        let token = format!("bhq_{}", "a".repeat(40));
        let (origin, rx) = serve(vec![
            (200, r#"{"ok":true,"email":"  me@example.test  "}"#),
            (401, r#"{"ok":false}"#),
            (404, r#"{"ok":false}"#),
            (200, r#"{"ok":true}"#),
        ]);
        assert_eq!(account_for_token(&token, &origin), (Some("me@example.test".into()), false));
        let req = rx.recv().unwrap();
        assert!(req.starts_with("GET /api/billable/account "), "{req}");
        assert!(req.contains(&format!("authorization: Bearer {token}")) || req.contains(&format!("Authorization: Bearer {token}")));
        assert_eq!(account_for_token(&token, &origin), (None, true));
        assert_eq!(account_for_token(&token, &origin), (None, false));
        let reply = post(&origin, &token, "/api/billable/disconnect", "{}").unwrap();
        assert_eq!(reply.status, 200);
        let req = rx.iter().nth(2).unwrap();
        assert!(req.starts_with("POST /api/billable/disconnect ") && req.ends_with("{}"), "{req}");
        // Nothing listening: an error, not a panic
        assert!(post("http://127.0.0.1:9", &token, "/api/billable/sync", "{}").is_err());
    }
}
