//! Timer actions shared by the toolbar, Dashboard, Projects, the menu bar
//! and the tray (TimerManager + useTimer's toasts). Starting stops whatever
//! is running or paused first.

use billable_core::models::{EntryRow, TimerRow};
use chrono::Utc;
use gpui::App;

use crate::{
    components::{toast, toast_error, Mood, ToastIcon},
    format::{duration_short, money},
    store::{self, data_changed},
};

/// Seconds on the clock for a running or paused entry (useTimer's elapsedSeconds).
pub fn elapsed_seconds(e: &TimerRow) -> f64 {
    let accumulated = e.entry.duration_minutes.max(0.0) * 60.0;
    let running = if e.entry.paused_at.is_some() {
        0.0
    } else {
        let since = e.entry.active_since.as_deref().unwrap_or(&e.entry.start_time);
        crate::format::parse_time(since).map(|t| (chrono::Local::now() - t).num_milliseconds().max(0) as f64 / 1000.0).unwrap_or(0.0)
    };
    (accumulated + running).floor()
}

/// "01:02:03"
pub fn elapsed_text(seconds: f64) -> String {
    let s = seconds.max(0.0) as i64;
    format!("{:02}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
}

/// Money earned so far at the project's rate (0 for non-billable time).
pub fn earned(e: &TimerRow, seconds: f64) -> f64 {
    if e.entry.is_billable == 0 {
        return 0.0;
    }
    e.rate.map(|r| seconds / 3600.0 * r).unwrap_or(0.0)
}

fn stop_current(cx: &mut App) -> anyhow::Result<Option<EntryRow>> {
    let s = store::store(cx);
    let current = s.timer.active.as_ref().or(s.timer.paused.as_ref()).map(|e| e.entry.id);
    match current {
        Some(id) => Ok(s.db.stop_timer(id, Some(Utc::now()))?),
        None => Ok(None),
    }
}

/// Start a timer, with the mascot's little cheer.
pub fn start(project_id: i64, description: Option<String>, cx: &mut App) {
    let result = (|| {
        stop_current(cx)?;
        let desc = description.filter(|d| !d.trim().is_empty());
        store::db(cx).start_timer(project_id, desc.as_deref(), Some(Utc::now()))
    })();
    data_changed(cx);
    match result {
        Ok(entry) => toast(
            format!("On the clock for {}", entry.project_name.clone().unwrap_or_else(|| "your project".into())),
            ToastIcon::MascotHop(Mood::Happy),
            cx,
        ),
        Err(e) => toast_error(format!("Couldn't start the timer: {e}"), cx),
    }
}

pub fn pause(cx: &mut App) {
    let id = store::store(cx).timer.active.as_ref().map(|e| e.entry.id);
    if let Some(id) = id {
        if let Err(e) = store::db(cx).pause_timer(id, Some(Utc::now())) {
            toast_error(format!("Couldn't pause: {e}"), cx);
        }
        data_changed(cx);
    }
}

pub fn resume(cx: &mut App) {
    let id = store::store(cx).timer.paused.as_ref().map(|e| e.entry.id);
    if let Some(id) = id {
        if let Err(e) = store::db(cx).resume_timer(id, Some(Utc::now())) {
            toast_error(format!("Couldn't resume: {e}"), cx);
        }
        data_changed(cx);
    }
}

pub fn toggle_pause(cx: &mut App) {
    if store::store(cx).timer.active.is_some() {
        pause(cx)
    } else {
        resume(cx)
    }
}

/// Stop, and say what was logged (or that a sub-minute timer was dropped).
pub fn stop(cx: &mut App) {
    let result = stop_current(cx);
    data_changed(cx);
    match result {
        Ok(Some(entry)) if entry.discarded == Some(true) => toast("Timer ran under a minute, so it was discarded", ToastIcon::None, cx),
        Ok(Some(entry)) => {
            let mins = entry.entry.duration_minutes;
            let earned = if entry.entry.is_billable != 0 { entry.rate.map(|r| mins / 60.0 * r).unwrap_or(0.0) } else { 0.0 };
            let money_part = if earned > 0.0 { format!(" · {}", money(earned)) } else { String::new() };
            toast(
                format!("Logged {}{} on {}", duration_short(mins), money_part, entry.project_name.clone().unwrap_or_else(|| "the project".into())),
                ToastIcon::Mascot(Mood::Happy),
                cx,
            );
        }
        Ok(None) => {}
        Err(e) => toast_error(format!("Couldn't stop the timer: {e}"), cx),
    }
}
