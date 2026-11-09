//! The menu bar item (src/main/tray.ts): the timer's elapsed time as its
//! title while running, a template icon that pulses each second, and the
//! same menus (pause/stop/switch project, quick starts, today's hours, ready
//! to bill, open, quit).

use std::cell::RefCell;

use tray_icon::{
    menu::{IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu},
    Icon, TrayIcon, TrayIconBuilder,
};

use gpui::App;

use super::{show_main_window, with_main_window};
use crate::{format::money, shell::navigate_path, store, timer};

const IDLE: &[u8] = include_bytes!("../../../../resources/trayIconTemplate@2x.png");
const ACTIVE: &[u8] = include_bytes!("../../../../resources/trayIconActiveTemplate@2x.png");

thread_local! {
    static TRAY: RefCell<Option<TrayIcon>> = const { RefCell::new(None) };
    static PULSE: RefCell<bool> = const { RefCell::new(false) };
}

fn icon(png: &[u8]) -> Option<Icon> {
    let img = image::load_from_memory_with_format(png, image::ImageFormat::Png).ok()?.into_rgba8();
    let (w, h) = img.dimensions();
    Icon::from_rgba(img.into_raw(), w, h).ok()
}

pub fn init(cx: &mut App) {
    let tray = TrayIconBuilder::new()
        .with_icon(icon(IDLE).expect("tray icon"))
        .with_icon_as_template(true)
        .with_tooltip("Billable")
        .with_menu(Box::new(build_menu(cx)))
        .build();
    match tray {
        Ok(t) => TRAY.with(|c| *c.borrow_mut() = Some(t)),
        Err(e) => eprintln!("Couldn't add the menu bar item: {e}"),
    }
}

/// `0:42:13`, the menu bar title while a timer runs (TimerManager.formatElapsed).
fn title(seconds: f64) -> String {
    let s = seconds.max(0.0) as i64;
    format!("{}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
}

/// `1h 02m 05s` / `2m 05s` (formatElapsedReadable).
fn readable(seconds: f64) -> String {
    let s = seconds.max(0.0) as i64;
    let (h, m, sec) = (s / 3600, (s % 3600) / 60, s % 60);
    if h > 0 { format!("{h}h {m:02}m {sec:02}s") } else { format!("{m}m {sec:02}s") }
}

fn item(id: &str, label: &str, enabled: bool) -> MenuItem {
    MenuItem::with_id(id, label, enabled, None)
}

fn build_menu(cx: &App) -> Menu {
    let s = store::store(cx);
    let db = &s.db;
    let projects: Vec<_> = db.list_projects(None).unwrap_or_default().into_iter().filter(|p| p.status == "active").collect();
    let today = db.get_today_hours().unwrap_or(0.0);
    let unbilled = db.get_unbilled_totals().map(|u| u.amount).unwrap_or(0.0);
    let ready = item("ready", &if unbilled > 0.0 { format!("Ready to bill: {}", money(unbilled)) } else { "Nothing waiting to be billed".into() }, true);
    let label = |p: &billable_core::models::Project| match &p.client_name {
        Some(c) if !c.is_empty() => format!("{} ({c})", p.name),
        _ => p.name.clone(),
    };
    let menu = Menu::new();
    let sep = || PredefinedMenuItem::separator();
    let today_item = item("today", &format!("Today: {today:.1} hrs tracked"), false);
    let open = item("open", "Open Billable", true);
    let quit = item("quit", "Quit Billable", true);

    if let Some(active) = &s.timer.active {
        let secs = timer::elapsed_seconds(active);
        let project = active.project_name.clone().unwrap_or_else(|| "Unknown Project".into());
        let client = active.client_name.clone().filter(|c| !c.is_empty()).map(|c| format!(" — {c}")).unwrap_or_default();
        let switch = Submenu::new("Switch Project", true);
        for p in projects.iter().filter(|p| p.id != active.entry.project_id) {
            let _ = switch.append(&item(&format!("start:{}", p.id), &label(p), true));
        }
        let (pause, stop) = (item("pause", "Pause Timer", true), item("stop", "Stop Timer", true));
        let items: Vec<&dyn IsMenuItem> = vec![&pause, &stop];
        let _ = menu.append_items(&items);
        let _ = menu.append(&sep());
        let _ = menu.append(&item("status", &format!("\u{25CF}  {project}{client}"), false));
        let _ = menu.append(&item("elapsed", &format!("     {}", readable(secs)), false));
        let _ = menu.append(&sep());
        let _ = menu.append(&switch);
        let _ = menu.append(&sep());
    } else if let Some(paused) = &s.timer.paused {
        let secs = timer::elapsed_seconds(paused);
        let project = paused.project_name.clone().unwrap_or_else(|| "Unknown Project".into());
        let client = paused.client_name.clone().filter(|c| !c.is_empty()).map(|c| format!(" — {c}")).unwrap_or_default();
        let _ = menu.append(&item("resume", "Resume Timer", true));
        let _ = menu.append(&item("stop", "Stop Timer", true));
        let _ = menu.append(&sep());
        let _ = menu.append(&item("status", &format!("⏸  {project}{client}"), false));
        let _ = menu.append(&item("elapsed", &format!("     {}", readable(secs)), false));
        let _ = menu.append(&sep());
    } else {
        let start = Submenu::new("Start Timer", true);
        for p in &projects {
            let _ = start.append(&item(&format!("start:{}", p.id), &label(p), true));
        }
        let _ = menu.append(&start);
        let _ = menu.append(&sep());
        let recent = db.get_recent_projects(5).unwrap_or_default();
        if !recent.is_empty() {
            for p in &recent {
                let _ = menu.append(&item(&format!("start:{}", p.id), &p.name, true));
            }
            let _ = menu.append(&sep());
        }
    }
    let _ = menu.append(&open);
    let _ = menu.append(&today_item);
    let _ = menu.append(&ready);
    let _ = menu.append(&sep());
    let _ = menu.append(&quit);
    menu
}

/// Called every second and after data changes: title, pulsing icon, menu.
pub fn refresh(cx: &App) {
    let s = store::store(cx);
    let running = s.timer.active.as_ref().map(timer::elapsed_seconds);
    let menu = build_menu(cx);
    TRAY.with(|t| {
        let Some(tray) = t.borrow().as_ref().cloned() else { return };
        match running {
            Some(secs) => {
                tray.set_title(Some(title(secs)));
                let pulse = PULSE.with(|p| {
                    let v = !*p.borrow();
                    *p.borrow_mut() = v;
                    v
                });
                let _ = tray.set_icon_with_as_template(icon(if pulse { ACTIVE } else { IDLE }), true);
            }
            None => {
                tray.set_title(None::<String>);
                let _ = tray.set_icon_with_as_template(icon(IDLE), true);
            }
        }
        tray.set_menu(Some(Box::new(menu)));
    });
}

/// Act on clicks in the menu bar item's menu.
pub fn handle_events(cx: &mut App) {
    while let Ok(event) = MenuEvent::receiver().try_recv() {
        let id = event.id().0.clone();
        match id.as_str() {
            "pause" => timer::pause(cx),
            "resume" => timer::resume(cx),
            "stop" => timer::stop(cx),
            "open" => show_main_window(cx),
            "ready" => {
                show_main_window(cx);
                with_main_window(cx, |w, cx| navigate_path("/billing", w, cx));
            }
            "quit" => cx.quit(),
            other => {
                if let Some(pid) = other.strip_prefix("start:").and_then(|p| p.parse::<i64>().ok()) {
                    timer::start(pid, None, cx);
                }
            }
        }
    }
}
