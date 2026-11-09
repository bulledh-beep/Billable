//! macOS glue: the main window, the menu bar item, the app menu, the global
//! shortcut, `billable://` links and the About panel.

pub mod menus;
pub mod pdf;
pub mod tray;

use std::{cell::RefCell, time::Duration};

use global_hotkey::{
    hotkey::{Code, HotKey, Modifiers},
    GlobalHotKeyEvent, GlobalHotKeyManager,
};
use gpui::{App, AnyWindowHandle, Window};

use crate::{store, timer};

thread_local! {
    static PENDING_SIGNIN: RefCell<Option<String>> = const { RefCell::new(None) };
    static PENDING_CONNECT: RefCell<Option<String>> = const { RefCell::new(None) };
    static LINKS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static MAIN: RefCell<Option<AnyWindowHandle>> = const { RefCell::new(None) };
    static HOTKEYS: RefCell<Option<GlobalHotKeyManager>> = const { RefCell::new(None) };
}

pub fn set_main_window(handle: AnyWindowHandle) {
    MAIN.with(|m| *m.borrow_mut() = Some(handle));
}

/// Run `f` in the main window, if it's open.
pub fn with_main_window(cx: &mut App, f: impl FnOnce(&mut Window, &mut App)) {
    let Some(handle) = MAIN.with(|m| *m.borrow()) else { return };
    let mut f = Some(f);
    let _ = handle.update(cx, |_, window, cx| {
        if let Some(f) = f.take() {
            f(window, cx)
        }
    });
}

/// Bring the window forward, opening it again if it was closed (the app
/// keeps running in the menu bar, like Electron's).
pub fn show_main_window(cx: &mut App) {
    let alive = MAIN.with(|m| *m.borrow()).is_some_and(|h| cx.windows().contains(&h));
    if alive {
        with_main_window(cx, |w, _| w.activate_window());
    } else {
        crate::open_main_window(cx);
    }
    cx.activate(true);
}

/// `billable://` links from macOS arrive before GPUI can act on them; they
/// wait here until the next tick.
pub fn queue_link(url: String) {
    LINKS.with(|l| l.borrow_mut().push(url));
}

/// contenthq.ts: `billable://switch?to=billable|content` opens that side of
/// the window. (`billable://connect` is only accepted from Billable's own
/// Content HQ view, so it is handled there, not here.)
fn handle_links(cx: &mut App) {
    let links: Vec<String> = LINKS.with(|l| std::mem::take(&mut *l.borrow_mut()));
    for url in links {
        let Some(rest) = url.strip_prefix("billable://") else { continue };
        let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
        if path.trim_end_matches('/') == "signin" {
            // billable://signin?code=…&next=… from the Mac's browser: Google is
            // done, so finish signing in inside the window (finishSignIn).
            let param = |k: &str| query.split('&').find_map(|kv| kv.strip_prefix(&format!("{k}="))).map(|v| v.to_string());
            let code = param("code").unwrap_or_default();
            let next = param("next").filter(|n| n.starts_with('/') && !n.starts_with("//")).unwrap_or_else(|| "/home".into());
            show_main_window(cx);
            with_main_window(cx, |w, cx| crate::shell::navigate_path("/content", w, cx));
            if code.len() >= 8 && code.len() <= 128 && code.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
                let home = crate::shell::content_hq::home_url(cx);
                let callback = format!("{}/auth/callback?code={code}&next={next}", home.trim_end_matches('/'));
                PENDING_SIGNIN.with(|p| *p.borrow_mut() = Some(callback));
            }
            continue;
        }
        if path.trim_end_matches('/') == "switch" {
            let to = query.split('&').find_map(|kv| kv.strip_prefix("to=")).unwrap_or("billable");
            show_main_window(cx);
            let target = if to == "content" { "/content" } else { "/" };
            with_main_window(cx, move |w, cx| crate::shell::navigate_path(target, w, cx));
        }
    }
}

/// A `billable://connect?token=` link from Content HQ's own page in the
/// window: hand the token to phone sync to confirm.
pub fn phone_connect_from_view(token: String, _cx: &mut App) {
    PENDING_CONNECT.with(|p| *p.borrow_mut() = Some(token));
}

/// The connect token waiting for phone sync, if any.
pub fn take_phone_connect() -> Option<String> {
    PENDING_CONNECT.with(|p| p.borrow_mut().take())
}

/// The standard About panel (Electron's `role: 'about'`).
pub fn about_panel(_cx: &mut App) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSApplication;
    if let Some(mtm) = MainThreadMarker::new() {
        let app = NSApplication::sharedApplication(mtm);
        unsafe { app.orderFrontStandardAboutPanel(None) };
    }
}

/// "Check for Updates…" in the app menu.
pub fn updater_check_from_menu(cx: &mut App) {
    show_main_window(cx);
    crate::services::updater::check_from_menu(cx);
}

/// ⌘⇧Space anywhere: stop a running timer, else bring Billable forward.
fn register_hotkey() {
    match GlobalHotKeyManager::new() {
        Ok(manager) => {
            let key = HotKey::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::Space);
            if let Err(e) = manager.register(key) {
                eprintln!("Couldn't register ⌘⇧Space: {e}");
            }
            HOTKEYS.with(|h| *h.borrow_mut() = Some(manager));
        }
        Err(e) => eprintln!("Global shortcuts unavailable: {e}"),
    }
}

/// Set up the menus, menu bar item and shortcut, then poll their events
/// (they arrive on channels) and keep the tray's clock ticking.
pub fn init(cx: &mut App) {
    menus::init(cx);
    tray::init(cx);
    register_hotkey();
    cx.spawn(async move |cx| {
        let mut last_rev = 0u64;
        let mut ticks = 0u32;
        loop {
            cx.background_executor().timer(Duration::from_millis(100)).await;
            ticks += 1;
            cx.update(|cx| {
                    tray::handle_events(cx);
                    crate::shell::content_hq::handle_view_links(cx);
                    handle_links(cx);
                    if crate::shell::content_hq::view_exists() {
                        if let Some(url) = PENDING_SIGNIN.with(|p| p.borrow_mut().take()) {
                            crate::shell::content_hq::load(&url);
                        }
                    }
                    while let Ok(e) = GlobalHotKeyEvent::receiver().try_recv() {
                        if e.state == global_hotkey::HotKeyState::Pressed {
                            if store::store(cx).timer.active.is_some() {
                                timer::stop(cx);
                            } else {
                                show_main_window(cx);
                            }
                        }
                    }
                    let rev = store::rev(cx);
                    let running = store::store(cx).timer.active.is_some();
                    if rev != last_rev {
                        last_rev = rev;
                        menus::refresh(cx);
                        tray::refresh(cx);
                    } else if running && ticks % 10 == 0 {
                        tray::refresh(cx);
                    }
            });
        }
    })
    .detach();
}
