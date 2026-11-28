//! FeedHQ's Agents screen: the bridge between the Content HQ page in the
//! window and the agents engine (the billable-agents crate, PROTOCOL.md).
//!
//! - The page posts protocol envelopes with `window.ipc.postMessage(json)`.
//!   Only Content HQ's own origin is heard (content_hq.rs checks the frame's
//!   address before a message gets here).
//! - The engine starts on the page's first message, or when phone sync is
//!   connected (so the website and phone can reach this Mac).
//! - Its replies and events go back with
//!   `window.__feedhqAgents.receive(<json string>)`, only while Content HQ's
//!   own page is showing. Until the page says `hello` (again after each
//!   load) they wait here, at most 500.
//! - `billable://oauth?…` (FeedHQ sign-in finishing) goes to the engine.
//! - While phone sync is connected, a remote link relays the website's and
//!   phone's requests through FeedHQ with phone sync's token.
//! - At quit every agent process is stopped.
//!
//! Its data lives in `<user data>/agents.db` (never billable.db), its agents
//! work in `<user data>/agents-workspace`, and the FeedHQ sign-in is kept in
//! the Keychain under service `app.billable.feedhq-agents`, one item per data
//! folder. Agents never see a FeedHQ token, and the person's own Claude Code
//! and Codex settings are never edited.

use std::{
    cell::RefCell,
    collections::VecDeque,
    sync::{mpsc::Receiver, Arc},
    time::{Duration, Instant},
};

use billable_agents::{util::origin_of, Engine, EngineConfig, RemoteLink, RemoteState, SecretStore, KEYCHAIN_SERVICE};
use gpui::{App, AsyncApp};

use crate::shell::content_hq;

/// Output held for a page that hasn't said hello yet.
const MAX_HELD: usize = 500;
/// How often output is delivered while the engine runs (streaming text).
const FAST_TICK: Duration = Duration::from_millis(30);
const IDLE_TICK: Duration = Duration::from_millis(250);
/// How often phone sync's connection is looked at.
const REMOTE_CHECK: Duration = Duration::from_secs(1);
/// Messages per evaluate_script call.
const PER_SCRIPT: usize = 64;

/// The FeedHQ sign-in in the Keychain. Accounts carry the data folder, so a
/// dev folder (BILLABLE_USER_DATA) never shares the real one's sign-in.
struct KeychainSecrets {
    folder: String,
}

impl KeychainSecrets {
    fn account(&self, key: &str) -> String {
        format!("{} {key}", self.folder)
    }
}

impl SecretStore for KeychainSecrets {
    fn get(&self, key: &str) -> Option<String> {
        let bytes = security_framework::passwords::get_generic_password(KEYCHAIN_SERVICE, &self.account(key)).ok()?;
        String::from_utf8(bytes).ok()
    }

    fn set(&self, key: &str, secret: &str) -> anyhow::Result<()> {
        security_framework::passwords::set_generic_password(KEYCHAIN_SERVICE, &self.account(key), secret.as_bytes())?;
        Ok(())
    }

    fn delete(&self, key: &str) {
        let _ = security_framework::passwords::delete_generic_password(KEYCHAIN_SERVICE, &self.account(key));
    }
}

#[derive(Default)]
struct Bridge {
    started: bool,
    engine: Option<Engine>,
    output: Option<Receiver<String>>,
    /// The engine couldn't start; said once, not retried every tick.
    failed: bool,
    /// Messages from the page, handled on the next tick.
    incoming: Vec<String>,
    /// Output waiting for the page's hello.
    held: VecDeque<String>,
    /// The page said hello since it last loaded.
    ready: bool,
    /// The remote link and the phone-sync connection it was made for.
    remote: Option<(u64, RemoteLink)>,
    /// A connection whose token FeedHQ refused: not tried again.
    refused_gen: Option<u64>,
    last_remote_check: Option<Instant>,
}

thread_local! {
    static BRIDGE: RefCell<Bridge> = RefCell::new(Bridge::default());
}

fn with<R>(f: impl FnOnce(&mut Bridge) -> R) -> R {
    BRIDGE.with(|b| f(&mut b.borrow_mut()))
}

/// A message from Content HQ's page (the web view's IPC handler, main
/// thread). Kept for the next tick.
pub fn from_page(message: String) {
    with(|b| b.incoming.push(message));
}

/// The page started loading: hold output until the new page says hello.
/// What was waiting was for the old page, which the new one asks again.
pub fn page_loading() {
    with(|b| {
        b.ready = false;
        b.held.clear();
    });
}

/// `billable://oauth?…`: FeedHQ's sign-in coming back. The engine only
/// accepts one that matches a sign-in it started (state and PKCE).
pub fn handle_oauth_link(url: &str, cx: &mut App) -> bool {
    if !url.starts_with("billable://oauth") {
        return false;
    }
    match ensure_engine(cx) {
        Some(engine) => {
            engine.handle_oauth_redirect(url);
        }
        None => eprintln!("[agents] a FeedHQ sign-in came back, but the agents engine isn't running"),
    }
    true
}

fn feedhq_origin(cx: &App) -> String {
    crate::services::phone::content_hq_origin(cx)
        .or_else(|| origin_of(&content_hq::home_url(cx)))
        .unwrap_or_else(|| content_hq::DEFAULT_URL.to_string())
}

/// The engine, started the first time it's needed.
fn ensure_engine(cx: &mut App) -> Option<Engine> {
    if let Some(e) = with(|b| b.engine.clone()) {
        return Some(e);
    }
    if with(|b| b.failed) {
        return None;
    }
    let data = billable_core::profiles::user_data_dir();
    let secrets: Arc<dyn SecretStore> = Arc::new(KeychainSecrets { folder: data.display().to_string() });
    let cfg = EngineConfig::new(data.join("agents.db"), data.join("agents-workspace"), &feedhq_origin(cx), secrets);
    match Engine::new(cfg) {
        Ok((engine, output)) => {
            with(|b| {
                b.engine = Some(engine.clone());
                b.output = Some(output);
            });
            Some(engine)
        }
        Err(e) => {
            eprintln!("[agents] the agents engine didn't start: {e:#}");
            with(|b| b.failed = true);
            None
        }
    }
}

fn is_hello(message: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(message).ok().and_then(|v| v.get("method").and_then(|m| m.as_str()).map(|m| m == "hello")).unwrap_or(false)
}

/// The script that hands messages to the page.
fn delivery_script(messages: &[String]) -> String {
    let calls: Vec<String> =
        messages.iter().map(|m| format!("window.__feedhqAgents && window.__feedhqAgents.receive({})", serde_json::to_string(m).unwrap_or_else(|_| "\"\"".into()))).collect();
    calls.join(";\n")
}

/// Content HQ's own page is the one showing (not its Supabase sign-in or
/// anything else the view may be on).
fn page_is_home(cx: &App) -> bool {
    let home = origin_of(&content_hq::home_url(cx));
    home.is_some() && content_hq::page_url().and_then(|u| origin_of(&u)) == home
}

fn tick(cx: &mut App) {
    // The page's messages
    let incoming = with(|b| std::mem::take(&mut b.incoming));
    if !incoming.is_empty() {
        if let Some(engine) = ensure_engine(cx) {
            for message in incoming {
                if is_hello(&message) {
                    with(|b| b.ready = true);
                }
                engine.handle(&message);
            }
        }
    }
    // The engine's output
    let fresh: Vec<String> = with(|b| b.output.as_ref().map(|rx| rx.try_iter().collect()).unwrap_or_default());
    let deliver = with(|b| {
        b.held.extend(fresh);
        while b.held.len() > MAX_HELD {
            b.held.pop_front();
        }
        b.ready && !b.held.is_empty()
    });
    if deliver && page_is_home(cx) {
        let batch: Vec<String> = with(|b| b.held.drain(..).collect());
        for chunk in batch.chunks(PER_SCRIPT) {
            content_hq::script(&delivery_script(chunk));
        }
    }
    remote_tick(cx);
}

/// Keep a remote link running while phone sync is connected.
fn remote_tick(cx: &mut App) {
    let due = with(|b| {
        let due = b.last_remote_check.is_none_or(|t| t.elapsed() >= REMOTE_CHECK);
        if due {
            b.last_remote_check = Some(Instant::now());
        }
        due
    });
    if !due {
        return;
    }
    let (gen, creds) = crate::services::phone::remote_credentials();
    // A link for an older connection, or one FeedHQ refused, stops.
    with(|b| {
        let stale = b.remote.as_ref().is_some_and(|(g, link)| *g != gen || creds.is_none() || link.state() != RemoteState::Running);
        if stale {
            if let Some((g, link)) = b.remote.take() {
                if link.state() == RemoteState::Refused {
                    b.refused_gen = Some(g);
                }
                link.stop();
            }
        }
    });
    let Some((origin, token)) = creds else { return };
    if with(|b| b.remote.is_some() || b.refused_gen == Some(gen)) {
        return;
    }
    let Some(engine) = ensure_engine(cx) else { return };
    let link = RemoteLink::start(Arc::new(engine), &origin, &token);
    with(|b| b.remote = Some((gen, link)));
}

/// Start the bridge's loop (once, at launch) and stop the agents at quit.
pub fn start(cx: &mut App) {
    if with(|b| std::mem::replace(&mut b.started, true)) {
        return;
    }
    cx.on_app_quit(|_| {
        shutdown();
        async {}
    })
    .detach();
    cx.spawn(async move |cx: &mut AsyncApp| loop {
        let running = with(|b| b.engine.is_some());
        cx.background_executor().timer(if running { FAST_TICK } else { IDLE_TICK }).await;
        cx.update(tick);
    })
    .detach();
}

/// Stop the remote link and every agent process (at quit).
fn shutdown() {
    let (engine, remote) = with(|b| (b.engine.take(), b.remote.take()));
    if let Some((_, link)) = remote {
        link.stop();
    }
    if let Some(engine) = engine {
        engine.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delivery_is_a_string_literal() {
        let js = delivery_script(&[r#"{"v":1,"type":"event","event":{"type":"item.delta","delta":"a\"b</script> "}}"#.to_string()]);
        assert!(js.starts_with("window.__feedhqAgents && window.__feedhqAgents.receive(\"{\\\"v\\\":1"));
        assert!(js.ends_with("\")"));
        assert_eq!(delivery_script(&["{}".into(), "{}".into()]).matches("receive(").count(), 2);
        assert!(is_hello(r#"{"v":1,"type":"request","id":"1","method":"hello","params":{}}"#));
        assert!(!is_hello(r#"{"method":"threads.list"}"#));
    }
}
