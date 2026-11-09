//! Time entries and the timer (database.ts "Time Entry Queries").

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::types::Value as SqlValue;
use rusqlite::{named_params, params};

use super::{sql_truthy, unbilled_sql, Db, FromCol, RowMap};
use crate::js::{js_parse_int, js_round};
use crate::models::*;
use crate::time::{iso_string, js_date_parse};

pub(crate) const ENTRY_SELECT: &str = "
  SELECT te.*, p.name AS project_name, p.color AS project_color, p.rate, p.client_id,
         p.status AS project_status, c.name AS client_name,
         i.invoice_number, i.status AS invoice_stored_status, i.due_date AS invoice_due_date
  FROM time_entries te
  LEFT JOIN projects p ON te.project_id = p.id
  LEFT JOIN clients c ON p.client_id = c.id
  LEFT JOIN invoices i ON i.id = te.invoice_id
";

const TIMER_SELECT: &str = "
    SELECT te.*, p.name as project_name, p.color as project_color, p.rate,
           c.name as client_name
    FROM time_entries te
    LEFT JOIN projects p ON te.project_id = p.id
    LEFT JOIN clients c ON p.client_id = c.id
";

/// Minutes on an entry so far: stored minutes plus the running segment.
fn accumulated_minutes(entry: &RowMap, now_ms: i64) -> f64 {
    let accumulated = {
        let v: f64 = entry.col("duration_minutes");
        if v.is_nan() {
            0.0
        } else {
            v
        }
    };
    if sql_truthy(entry.get("paused_at")) {
        return accumulated;
    }
    let since = if sql_truthy(entry.get("active_since")) { "active_since" } else { "start_time" };
    let active_since = match entry.get(since) {
        Some(SqlValue::Text(s)) => js_date_parse(s),
        Some(SqlValue::Integer(i)) => *i as f64,
        Some(SqlValue::Real(f)) => *f,
        // new Date(null) is the epoch; anything else unreadable is NaN
        Some(SqlValue::Null) => 0.0,
        _ => f64::NAN,
    };
    let running = (now_ms as f64 - active_since).max(0.0) / 60000.0;
    // Math.max(0, NaN) is NaN in JS
    let running = if active_since.is_nan() { f64::NAN } else { running };
    accumulated + running
}

/// database.ts `roundDuration`: `none` → cents of a minute, else up to the increment.
fn round_duration(minutes: f64, round_to: &str) -> f64 {
    if round_to == "none" {
        return js_round(minutes * 100.0) / 100.0;
    }
    let increment = js_parse_int(round_to);
    (minutes / increment).ceil() * increment
}

fn at_ms(db: &Db, at: Option<DateTime<Utc>>) -> i64 {
    at.map(|d| d.timestamp_millis()).unwrap_or_else(|| db.clock.now_ms())
}

impl Db {
    fn entry_rows(&self, sql: &str, params: impl rusqlite::Params) -> Result<Vec<EntryRow>> {
        let today = self.local_today();
        Ok(self.rows(sql, params)?.iter().map(|r| EntryRow::from_row(r, &today)).collect())
    }

    /// All time entries (or one project's), newest first, with billing state.
    pub fn list_time_entries(&self, project_id: Option<i64>) -> Result<Vec<EntryRow>> {
        match project_id.filter(|p| *p != 0) {
            Some(p) => self.entry_rows(&format!("{ENTRY_SELECT} WHERE te.project_id = ? ORDER BY te.start_time DESC"), [p]),
            None => self.entry_rows(&format!("{ENTRY_SELECT} ORDER BY te.start_time DESC"), []),
        }
    }

    pub fn get_time_entry(&self, id: i64) -> Result<Option<EntryRow>> {
        Ok(self.entry_rows(&format!("{ENTRY_SELECT} WHERE te.id = ?"), [id])?.into_iter().next())
    }

    /// Add a manual entry.
    pub fn create_time_entry(&self, data: &TimeEntryInput) -> Result<EntryRow> {
        let duration = match data.duration_minutes {
            Some(v) if v != 0.0 && !v.is_nan() => v,
            _ => 0.0,
        };
        let end_time = data.end_time.as_deref().filter(|s| !s.is_empty());
        let id = self.insert(
            "
    INSERT INTO time_entries (project_id, description, start_time, end_time, duration_minutes, is_billable, is_invoiced)
    VALUES (@project_id, @description, @start_time, @end_time, @duration_minutes, @is_billable, 0)
  ",
            named_params! {
                "@project_id": data.project_id,
                "@description": super::clients::str_or(data.description.as_deref(), ""),
                "@start_time": data.start_time,
                "@end_time": end_time,
                "@duration_minutes": duration,
                "@is_billable": data.is_billable.unwrap_or(1),
            },
        )?;
        self.get_time_entry(id)?.context("time entry not found after insert")
    }

    /// Edit an entry's project, text, times or billable flag. Billing links
    /// are never written here (invoices own them).
    pub fn update_time_entry(&self, id: i64, data: &TimeEntryPatch) -> Result<Option<EntryRow>> {
        let mut sets: Vec<(&str, SqlValue)> = Vec::new();
        if let Some(v) = data.project_id {
            sets.push(("project_id", SqlValue::Integer(v)));
        }
        if let Some(v) = &data.description {
            sets.push(("description", SqlValue::Text(v.clone())));
        }
        if let Some(v) = &data.start_time {
            sets.push(("start_time", SqlValue::Text(v.clone())));
        }
        if let Some(v) = &data.end_time {
            sets.push(("end_time", v.clone().map(SqlValue::Text).unwrap_or(SqlValue::Null)));
        }
        if let Some(v) = data.duration_minutes {
            sets.push(("duration_minutes", SqlValue::Real(v)));
        }
        if let Some(v) = data.is_billable {
            sets.push(("is_billable", SqlValue::Integer(v)));
        }
        self.update_columns("time_entries", id, &sets)?;
        self.get_time_entry(id)
    }

    pub fn delete_time_entry(&self, id: i64) -> Result<()> {
        self.run("DELETE FROM time_entries WHERE id = ?", [id])?;
        Ok(())
    }

    /// Mark finished, not-yet-invoiced entries billable or not ("Don't bill").
    /// Returns how many changed.
    pub fn set_entries_billable(&self, ids: &[i64], billable: bool) -> Result<i64> {
        let list: Vec<i64> = ids.iter().copied().filter(|i| *i != 0).collect();
        if list.is_empty() {
            return Ok(0);
        }
        self.tx(|| {
            let mut n = 0;
            for id in &list {
                n += self.run(
                    "
    UPDATE time_entries SET is_billable = ?
    WHERE id = ? AND end_time IS NOT NULL AND invoice_id IS NULL AND is_invoiced = 0
  ",
                    params![if billable { 1 } else { 0 }, id],
                )?;
            }
            Ok(n as i64)
        })
    }

    /// Entries that can go on an invoice for this client: everything
    /// unbilled, plus (when editing) what's already on `invoice_id`.
    pub fn list_invoiceable_entries(&self, client_id: i64, invoice_id: Option<i64>) -> Result<Vec<EntryRow>> {
        self.entry_rows(
            &format!(
                "
    {ENTRY_SELECT}
    WHERE p.client_id = @clientId AND te.end_time IS NOT NULL
      AND (({}) OR (@invoiceId IS NOT NULL AND te.invoice_id = @invoiceId))
    ORDER BY te.start_time
  ",
                unbilled_sql!()
            ),
            named_params! {"@clientId": client_id, "@invoiceId": invoice_id},
        )
    }

    // ============ Timer ============

    /// Start a running entry on a project. `at` is when it happened (changes
    /// made elsewhere); otherwise now. Doesn't stop other timers; use
    /// [`crate::TimerManager`] for that.
    pub fn start_timer(&self, project_id: i64, description: Option<&str>, at: Option<DateTime<Utc>>) -> Result<EntryRow> {
        let now = iso_string(at_ms(self, at));
        let id = self.insert(
            "
    INSERT INTO time_entries (project_id, description, start_time, active_since, is_billable)
    VALUES (?, ?, ?, ?, 1)
  ",
            params![project_id, description.unwrap_or(""), now, now],
        )?;
        self.get_time_entry(id)?.context("time entry not found after insert")
    }

    /// Finish an entry, applying the rounding setting. A timer that ran
    /// under a minute is deleted instead and returned with `discarded`.
    /// `None` if the entry doesn't exist.
    pub fn stop_timer(&self, id: i64, at: Option<DateTime<Utc>>) -> Result<Option<EntryRow>> {
        let Some(entry) = self.row("SELECT * FROM time_entries WHERE id = ?", [id])? else { return Ok(None) };
        let now = at_ms(self, at);
        let minutes = accumulated_minutes(&entry, now);

        // Under a minute is almost always a mis-click or a quick project
        // switch. Rounding would turn it into billable time, so drop it.
        if minutes < 1.0 {
            let snapshot = self.get_time_entry(id)?;
            self.run("DELETE FROM time_entries WHERE id = ?", [id])?;
            return Ok(snapshot.map(|mut s| {
                s.entry.end_time = Some(iso_string(now));
                s.entry.duration_minutes = 0.0;
                s.discarded = Some(true);
                s
            }));
        }

        let round_to = self
            .scalar("SELECT value FROM settings WHERE key = 'time_rounding'", [], "value")?
            .map(|v| <String as FromCol>::from_col(Some(&v)))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "none".into());
        let rounded = round_duration(minutes, &round_to);
        self.run(
            "
    UPDATE time_entries
    SET end_time = ?, duration_minutes = ?, paused_at = NULL, active_since = NULL
    WHERE id = ?
  ",
            params![iso_string(now), rounded, id],
        )?;
        self.get_time_entry(id)
    }

    /// Pause a running entry, banking its minutes so far. A finished or
    /// already-paused entry is returned unchanged; `None` if missing.
    pub fn pause_timer(&self, id: i64, at: Option<DateTime<Utc>>) -> Result<Option<EntryRow>> {
        let Some(entry) = self.row("SELECT * FROM time_entries WHERE id = ?", [id])? else { return Ok(None) };
        if sql_truthy(entry.get("end_time")) || sql_truthy(entry.get("paused_at")) {
            return self.get_time_entry(id);
        }
        let now = at_ms(self, at);
        let minutes = accumulated_minutes(&entry, now);
        self.run(
            "
    UPDATE time_entries
    SET duration_minutes = ?, paused_at = ?, active_since = NULL
    WHERE id = ?
  ",
            params![minutes, iso_string(now), id],
        )?;
        self.get_time_entry(id)
    }

    /// Resume a paused entry. Anything else is returned unchanged; `None` if missing.
    pub fn resume_timer(&self, id: i64, at: Option<DateTime<Utc>>) -> Result<Option<EntryRow>> {
        let Some(entry) = self.row("SELECT * FROM time_entries WHERE id = ?", [id])? else { return Ok(None) };
        if sql_truthy(entry.get("end_time")) || !sql_truthy(entry.get("paused_at")) {
            return self.get_time_entry(id);
        }
        let now = iso_string(at_ms(self, at));
        self.run(
            "
    UPDATE time_entries
    SET paused_at = NULL, active_since = ?
    WHERE id = ?
  ",
            params![now, id],
        )?;
        self.get_time_entry(id)
    }

    /// The running entry, if any (newest if several).
    pub fn get_active_timer(&self) -> Result<Option<TimerRow>> {
        Ok(self
            .row(
                &format!(
                    "{TIMER_SELECT}    WHERE te.end_time IS NULL AND te.paused_at IS NULL
    ORDER BY te.start_time DESC
    LIMIT 1
  "
                ),
                [],
            )?
            .map(|r| TimerRow::from_row(&r)))
    }

    /// The paused entry, if any (most recently paused).
    pub fn get_paused_timer(&self) -> Result<Option<TimerRow>> {
        Ok(self
            .row(
                &format!(
                    "{TIMER_SELECT}    WHERE te.end_time IS NULL AND te.paused_at IS NOT NULL
    ORDER BY te.paused_at DESC
    LIMIT 1
  "
                ),
                [],
            )?
            .map(|r| TimerRow::from_row(&r)))
    }
}
