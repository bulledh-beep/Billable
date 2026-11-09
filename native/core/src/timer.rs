//! The timer's state machine (port of `src/main/timer-manager.ts` without the
//! Electron wiring). The UI layer owns ticking, the menu bar title, tray and
//! notifications; call these methods and refresh from the returned entries.
//!
//! Rules carried over: starting a timer finishes whatever is running or
//! paused first; stopping applies the rounding setting; a timer stopped
//! under a minute is deleted (the result has `discarded: Some(true)`).

use anyhow::Result;
use chrono::{DateTime, Utc};

use crate::db::Db;
use crate::models::{EntryRow, TimerState};
use crate::time::js_date_parse;

/// Holds the running and paused entries for the open profile.
#[derive(Debug, Default, Clone)]
pub struct TimerManager {
    active: Option<EntryRow>,
    paused: Option<EntryRow>,
}

impl TimerManager {
    /// Pick up a timer left running or paused in `db` (as at launch).
    pub fn new(db: &Db) -> Result<Self> {
        let mut t = TimerManager::default();
        t.sync_from_database(db)?;
        Ok(t)
    }

    /// Reload state from `db`, e.g. after switching profiles.
    pub fn sync_from_database(&mut self, db: &Db) -> Result<()> {
        self.active = match db.get_active_timer()? {
            Some(t) => db.get_time_entry(t.id)?,
            None => None,
        };
        self.paused = match db.get_paused_timer()? {
            Some(t) => db.get_time_entry(t.id)?,
            None => None,
        };
        Ok(())
    }

    /// Start timing a project. Stops the running timer and finishes a paused
    /// one first. `at` is when it happened (changes made elsewhere), else now.
    pub fn start(
        &mut self,
        db: &Db,
        project_id: i64,
        description: Option<&str>,
        at: Option<DateTime<Utc>>,
    ) -> Result<EntryRow> {
        if self.active.is_some() {
            self.stop_internal(db, at)?;
        }
        // Starting another project while paused finishes the paused entry.
        // The explicit Resume action keeps it.
        if self.paused.is_some() {
            self.stop_paused_internal(db, at)?;
        }
        let entry = db.start_timer(project_id, description, at)?;
        self.active = Some(entry.clone());
        self.paused = None;
        Ok(entry)
    }

    /// Stop the running timer, else finish the paused one. `None` if neither.
    pub fn stop(&mut self, db: &Db, at: Option<DateTime<Utc>>) -> Result<Option<EntryRow>> {
        if self.active.is_some() {
            return self.stop_internal(db, at);
        }
        if self.paused.is_some() {
            return self.stop_paused_internal(db, at);
        }
        Ok(None)
    }

    /// Pause the running timer. `None` if nothing is running.
    pub fn pause(&mut self, db: &Db, at: Option<DateTime<Utc>>) -> Result<Option<EntryRow>> {
        let Some(active) = &self.active else { return Ok(None) };
        let entry = db.pause_timer(active.id, at)?;
        self.active = None;
        self.paused = entry.clone();
        Ok(entry)
    }

    /// Resume the paused timer. `None` if nothing is paused.
    pub fn resume(&mut self, db: &Db, at: Option<DateTime<Utc>>) -> Result<Option<EntryRow>> {
        let Some(paused) = &self.paused else { return Ok(None) };
        let entry = db.resume_timer(paused.id, at)?;
        self.paused = None;
        self.active = entry.clone();
        Ok(entry)
    }

    /// Pause if running, resume if paused.
    pub fn toggle_pause(&mut self, db: &Db) -> Result<Option<EntryRow>> {
        if self.active.is_some() {
            self.pause(db, None)
        } else if self.paused.is_some() {
            self.resume(db, None)
        } else {
            Ok(None)
        }
    }

    fn stop_internal(&mut self, db: &Db, at: Option<DateTime<Utc>>) -> Result<Option<EntryRow>> {
        let Some(id) = self.active.as_ref().map(|e| e.id) else { return Ok(None) };
        let entry = db.stop_timer(id, at)?;
        self.active = None;
        Ok(entry)
    }

    fn stop_paused_internal(&mut self, db: &Db, at: Option<DateTime<Utc>>) -> Result<Option<EntryRow>> {
        let Some(id) = self.paused.as_ref().map(|e| e.id) else { return Ok(None) };
        let entry = db.stop_timer(id, at)?;
        self.paused = None;
        Ok(entry)
    }

    /// The running entry (as of when it started or resumed).
    pub fn active(&self) -> Option<&EntryRow> {
        self.active.as_ref()
    }

    /// The paused entry.
    pub fn paused(&self) -> Option<&EntryRow> {
        self.paused.as_ref()
    }

    /// `{ active, paused }` (`time:state`).
    pub fn state(&self) -> TimerState {
        TimerState { active: self.active.clone(), paused: self.paused.clone() }
    }
}

/// Whole seconds on an entry at `now_ms`: banked minutes plus the running
/// segment since `active_since` (or `start_time` when it's empty). A `None`
/// `active_since` (paused or stopped) adds nothing.
pub fn elapsed_seconds(entry: &crate::models::TimeEntry, now_ms: i64) -> i64 {
    let minutes = if entry.duration_minutes.is_nan() { 0.0 } else { entry.duration_minutes };
    let accumulated = minutes.max(0.0) * 60.0;
    let running = match entry.active_since.as_deref() {
        None => 0.0,
        Some(since) => {
            let s = if since.is_empty() { entry.start_time.as_str() } else { since };
            let ms = js_date_parse(s);
            if ms.is_nan() {
                f64::NAN
            } else {
                (now_ms as f64 - ms).max(0.0) / 1000.0
            }
        }
    };
    let total = (accumulated + running).floor();
    if total.is_finite() {
        total as i64
    } else {
        0
    }
}

/// `h:mm:ss`, the menu bar title.
pub fn format_elapsed(entry: &crate::models::TimeEntry, now_ms: i64) -> String {
    let t = elapsed_seconds(entry, now_ms);
    format!("{}:{:02}:{:02}", t / 3600, (t % 3600) / 60, t % 60)
}

/// `1h 05m 09s` or `5m 09s`.
pub fn format_elapsed_readable(entry: &crate::models::TimeEntry, now_ms: i64) -> String {
    let t = elapsed_seconds(entry, now_ms);
    let (h, m, s) = (t / 3600, (t % 3600) / 60, t % 60);
    if h > 0 {
        format!("{h}h {m:02}m {s:02}s")
    } else {
        format!("{m}m {s:02}s")
    }
}
