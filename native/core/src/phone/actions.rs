//! `phone-actions.ts`: changes made on the phone, applied to Billable
//! (actions schema version 1), and the `phone_actions` bookkeeping from
//! `phone-sync.ts`. Everything here arrives over the network, so each
//! payload is checked field by field before it touches the database. A
//! change that can't be applied is rejected with a short reason the phone
//! shows.

use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::types::Value as SqlValue;
use serde::Serialize;
use serde_json::{Map, Value};

use super::snapshot::{payment_method_names, EXPENSE_CATEGORIES};
use super::{is_ymd, len16, slice16};
use crate::db::Db;
use crate::js::{js_number_to_string, js_round, js_trim};
use crate::models::{ExpenseInput, Project, ProjectInput, ProjectPatch, TimeEntry, TimeEntryInput, TimeEntryPatch};
use crate::time::{iso_string, js_date_parse};
use crate::timer::TimerManager;

/// One action as Content HQ delivers it. `payload` and `occurred_at` are
/// whatever arrived (`Value::Null` when missing).
#[derive(Debug, Clone)]
pub struct IncomingAction {
    pub id: String,
    pub kind: String,
    pub payload: Value,
    pub occurred_at: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionResult {
    /// `applied` | `rejected`
    pub status: &'static str,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_billable_id: Option<String>,
}

impl ActionResult {
    fn applied(summary: impl Into<String>) -> Self {
        ActionResult { status: "applied", summary: summary.into(), message: None, created_billable_id: None }
    }
    fn created(mut self, id: impl ToString) -> Self {
        self.created_billable_id = Some(id.to_string());
        self
    }
}

/// What the app shows for each new action (the `phone:applied` event).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AppliedSummary {
    pub status: &'static str,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

enum Fail {
    Reject(String),
    Error(anyhow::Error),
}

impl From<anyhow::Error> for Fail {
    fn from(e: anyhow::Error) -> Self {
        Fail::Error(e)
    }
}

type Run<T> = std::result::Result<T, Fail>;

fn reject<T>(message: &str) -> Run<T> {
    Err(Fail::Reject(message.to_string()))
}

const UNREADABLE: &str = "Billable couldn't read this change.";

// ---------- Field checks ----------

fn obj<'a>(value: Option<&'a Value>, allowed: &[&str]) -> Run<&'a Map<String, Value>> {
    let Some(Value::Object(o)) = value else { return reject(UNREADABLE) };
    if o.keys().any(|k| !allowed.contains(&k.as_str())) {
        return reject(UNREADABLE);
    }
    Ok(o)
}

/// `/^\d{1,12}$/`
fn is_id(v: Option<&Value>) -> Option<i64> {
    match v {
        Some(Value::String(s)) if (1..=12).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_digit()) => s.parse().ok(),
        _ => None,
    }
}

fn is_uuid(v: Option<&Value>) -> Option<&str> {
    let Some(Value::String(s)) = v else { return None };
    let b = s.as_bytes();
    let ok = b.len() == 36
        && b.iter().enumerate().all(|(i, c)| if [8, 13, 18, 23].contains(&i) { *c == b'-' } else { c.is_ascii_hexdigit() });
    ok.then_some(s.as_str())
}

fn day(v: Option<&Value>) -> Run<String> {
    match v {
        Some(Value::String(s)) if is_ymd(s) && js_date_parse(&format!("{s}T00:00:00")).is_finite() => Ok(s.clone()),
        _ => reject("Billable couldn't read the date."),
    }
}

/// `None` for missing/null, else the trimmed note (at most 500 characters).
fn optional_note(v: Option<&Value>) -> Run<Option<String>> {
    match v {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if len16(s) <= 500 => Ok(Some(js_trim(s).to_string())),
        _ => reject("That note is too long."),
    }
}

fn num(v: Option<&Value>, min: f64, max: f64, what: &str) -> Run<f64> {
    match v.and_then(Value::as_f64) {
        Some(n) if matches!(v, Some(Value::Number(_))) && n.is_finite() && n >= min && n <= max => Ok(n),
        _ => Err(Fail::Reject(format!("Billable couldn't read the {what}."))),
    }
}

fn present(o: &Map<String, Value>, key: &str) -> bool {
    o.contains_key(key)
}

// ---------- Lookups ----------

fn project(db: &Db, r: Option<&Value>) -> Run<Project> {
    let r = obj(r, &["billableId", "actionId"])?;
    let project_id = if let (Some(id), false) = (is_id(r.get("billableId")), present(r, "actionId")) {
        id
    } else if let (Some(action_id), false) = (is_uuid(r.get("actionId")), present(r, "billableId")) {
        // A job created on the phone: find what Billable made for it
        let row = db.row("SELECT status, created_billable_id FROM phone_actions WHERE id = ?", [action_id.to_lowercase()])?;
        let Some(row) = row else { return reject("The new job hasn't reached Billable yet.") };
        let status: String = row.col("status");
        let created: Option<String> = row.col("created_billable_id");
        match created.filter(|c| !c.is_empty()) {
            Some(c) if status == "applied" => c.parse::<i64>().unwrap_or(i64::MIN),
            _ => return reject("The new job wasn't created, so this couldn't be added."),
        }
    } else {
        return reject("Billable couldn't tell which job this is for.");
    };
    match db.get_project(project_id)? {
        Some(p) => Ok(p),
        None => reject("That job no longer exists in Billable."),
    }
}

/// The last moment the Mac changed this timer: its start, resume or pause.
fn last_touched(entry: &TimeEntry) -> f64 {
    let since = entry.active_since.as_deref().filter(|s| !s.is_empty()).unwrap_or(&entry.start_time);
    let start = js_date_parse(since);
    let paused = match entry.paused_at.as_deref().filter(|s| !s.is_empty()) {
        Some(p) => js_date_parse(p),
        None => 0.0,
    };
    let z = |x: f64| if x.is_nan() || x == 0.0 { 0.0 } else { x };
    z(start).max(z(paused))
}

fn fmt_minutes(m: f64) -> String {
    let h = (m / 60.0).floor();
    let mm = js_round(m % 60.0);
    let (hs, ms) = (js_number_to_string(h), js_number_to_string(mm));
    if h != 0.0 && !h.is_nan() {
        if mm != 0.0 && !mm.is_nan() {
            format!("{hs}h {ms}m")
        } else {
            format!("{hs}h")
        }
    } else {
        format!("{ms}m")
    }
}

/// `$` + `n.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 })`:
/// ICU rounds the shortest decimal form half away from zero, then groups thousands.
pub(crate) fn fmt_money(n: f64) -> String {
    let neg = n < 0.0;
    let s = format!("{}", n.abs());
    let (int, frac) = s.split_once('.').unwrap_or((&s, ""));
    let mut digits: Vec<u8> = int.bytes().map(|b| b - b'0').collect();
    let f: Vec<u8> = frac.bytes().map(|b| b - b'0').collect();
    digits.push(*f.first().unwrap_or(&0));
    digits.push(*f.get(1).unwrap_or(&0));
    if f.get(2).is_some_and(|d| *d >= 5) {
        let mut i = digits.len();
        loop {
            if i == 0 {
                digits.insert(0, 1);
                break;
            }
            i -= 1;
            if digits[i] == 9 {
                digits[i] = 0;
            } else {
                digits[i] += 1;
                break;
            }
        }
    }
    let split = digits.len() - 2;
    let int_digits: String = digits[..split].iter().map(|d| (b'0' + d) as char).collect();
    let frac_digits: String = digits[split..].iter().map(|d| (b'0' + d) as char).collect();
    let int_digits = int_digits.trim_start_matches('0');
    let int_digits = if int_digits.is_empty() { "0" } else { int_digits };
    let mut grouped = String::new();
    for (i, c) in int_digits.chars().enumerate() {
        if i > 0 && (int_digits.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(c);
    }
    format!("${}{grouped}.{frac_digits}", if neg { "-" } else { "" })
}

fn describe_kind(kind: &str) -> &'static str {
    match kind {
        "timer.start" => "Start a timer",
        "timer.pause" => "Pause the timer",
        "timer.resume" => "Resume the timer",
        "timer.stop" => "Stop the timer",
        "time.add" => "Log time",
        "project.create" => "Create a job",
        "project.setStatus" => "Change a job",
        "expense.add" => "Add an expense",
        "invoice.markSent" => "Mark an invoice sent",
        "invoice.markPaid" => "Record a payment",
        _ => "Unknown change",
    }
}

fn utc(ms: f64) -> DateTime<Utc> {
    DateTime::<Utc>::from_timestamp_millis(ms as i64).unwrap_or_default()
}

fn job_name(name: &Option<String>) -> String {
    name.clone().filter(|n| !n.is_empty()).unwrap_or_else(|| "a job".into())
}

// ---------- Applying ----------

/// Apply one action. Timer changes happen at the moment they were tapped
/// (`occurredAt`), so a timer started on the phone at 9:00 counts from 9:00
/// even if the Mac was asleep until noon.
pub fn apply_action(db: &Db, timer: &mut TimerManager, action: &IncomingAction) -> ActionResult {
    let now = db.clock().now_ms() as f64;
    let mut at = match &action.occurred_at {
        Value::String(s) => js_date_parse(s),
        _ => f64::NAN,
    };
    if !at.is_finite() {
        at = now;
    }
    // A phone clock that runs ahead can't put work in the future
    if at > now {
        at = now;
    }
    match run(db, timer, action, at) {
        Ok(r) => r,
        Err(Fail::Reject(message)) => ActionResult {
            status: "rejected",
            summary: describe_kind(&action.kind).into(),
            message: Some(message),
            created_billable_id: None,
        },
        Err(Fail::Error(e)) => {
            eprintln!("[phone] action failed {} {e:#}", action.kind);
            ActionResult {
                status: "rejected",
                summary: describe_kind(&action.kind).into(),
                message: Some("Billable hit a problem applying this change.".into()),
                created_billable_id: None,
            }
        }
    }
}

fn run(db: &Db, timer: &mut TimerManager, action: &IncomingAction, at: f64) -> Run<ActionResult> {
    let empty = Value::Object(Map::new());
    let payload = if action.payload.is_null() { &empty } else { &action.payload };
    let payload = Some(payload);
    let at_utc = Some(utc(at));

    match action.kind.as_str() {
        "timer.start" => {
            let p = obj(payload, &["project", "note"])?;
            let job = project(db, p.get("project"))?;
            let text = optional_note(p.get("note"))?.unwrap_or_default();
            let current = timer.active().or(timer.paused());
            if current.is_some_and(|c| last_touched(c) > at) {
                return reject("A timer was started on your Mac after this, so Billable kept that one.");
            }
            let entry = timer.start(db, job.id, Some(&text), at_utc)?;
            Ok(ActionResult::applied(format!("Started a timer on {}", job.name)).created(entry.id))
        }

        "timer.pause" => {
            obj(payload, &[])?;
            let Some(running) = timer.active().cloned() else { return reject("No timer was running on your Mac.") };
            if last_touched(&running) > at {
                return reject("The timer on your Mac started after this.");
            }
            timer.pause(db, at_utc)?;
            Ok(ActionResult::applied(format!("Paused the timer on {}", job_name(&running.project_name))))
        }

        "timer.resume" => {
            obj(payload, &[])?;
            let Some(paused) = timer.paused().cloned() else { return reject("No timer was paused on your Mac.") };
            if last_touched(&paused) > at {
                return reject("The timer on your Mac was paused after this.");
            }
            timer.resume(db, at_utc)?;
            Ok(ActionResult::applied(format!("Resumed the timer on {}", job_name(&paused.project_name))))
        }

        "timer.stop" => {
            let p = obj(payload, &["note"])?;
            let text = optional_note(p.get("note"))?;
            let Some(current) = timer.active().or(timer.paused()).cloned() else { return reject("No timer was running on your Mac.") };
            if last_touched(&current) > at {
                return reject("The timer on your Mac changed after this.");
            }
            if let Some(text) = text {
                db.update_time_entry(current.id, &TimeEntryPatch { description: Some(text), ..Default::default() })?;
            }
            let entry = timer.stop(db, at_utc)?;
            let name = job_name(&current.project_name);
            if entry.as_ref().is_some_and(|e| e.discarded == Some(true)) {
                let mut r = ActionResult::applied(format!("Stopped the timer on {name}"));
                r.message = Some("It ran under a minute, so it wasn\u{2019}t kept.".into());
                return Ok(r);
            }
            let mins = entry.as_ref().map(|e| e.duration_minutes).filter(|m| !m.is_nan()).unwrap_or(0.0);
            let mut r = ActionResult::applied(format!("Stopped the timer on {name} at {}", fmt_minutes(mins)));
            r.created_billable_id = entry.map(|e| e.id.to_string());
            Ok(r)
        }

        "time.add" => {
            let p = obj(payload, &["project", "date", "startTime", "minutes", "note", "billable"])?;
            let job = project(db, p.get("project"))?;
            let date = day(p.get("date"))?;
            let mut start_time = "09:00".to_string();
            match p.get("startTime") {
                None | Some(Value::Null) => {}
                Some(Value::String(s)) if valid_hhmm(s) => start_time = s.clone(),
                _ => return reject("Billable couldn't read the start time."),
            }
            let mins = num(p.get("minutes"), 1.0, 1440.0, "duration")?;
            if mins.fract() != 0.0 {
                return reject("Billable couldn't read the duration.");
            }
            let Some(Value::Bool(billable)) = p.get("billable") else {
                return reject("Billable couldn't tell if this time is billable.");
            };
            let start = js_date_parse(&format!("{date}T{start_time}:00"));
            let end = start + mins * 60_000.0;
            let description = optional_note(p.get("note"))?.unwrap_or_default();
            let entry = db.create_time_entry(&TimeEntryInput {
                project_id: job.id,
                description: Some(description),
                start_time: iso_string(start as i64),
                end_time: Some(iso_string(end as i64)),
                duration_minutes: Some(mins),
                is_billable: Some(if *billable { 1 } else { 0 }),
            })?;
            Ok(ActionResult::applied(format!("Logged {} on {}", fmt_minutes(mins), job.name)).created(entry.entry.id))
        }

        "project.create" => {
            let p = obj(payload, &["clientBillableId", "name", "rate"])?;
            let Some(client_id) = is_id(p.get("clientBillableId")) else {
                return reject("Billable couldn't tell which client this is for.");
            };
            let Some(client) = db.get_client(client_id)? else { return reject("That client no longer exists in Billable.") };
            let name = match p.get("name") {
                Some(Value::String(s)) if !js_trim(s).is_empty() && len16(js_trim(s)) <= 120 => js_trim(s).to_string(),
                _ => return reject("The job needs a name up to 120 characters."),
            };
            let settings = db.get_settings()?;
            let rate = match p.get("rate") {
                None | Some(Value::Null) => {
                    let or0 = |x: f64| if x.is_nan() { 0.0 } else { x };
                    let c = or0(client.default_rate);
                    if c != 0.0 {
                        c
                    } else {
                        or0(settings.default_rate())
                    }
                }
                v => num(v, 0.0, 100000.0, "rate")?,
            };
            let created = db.create_project(&ProjectInput {
                client_id: client.id,
                name,
                rate: Some(rate),
                status: Some("active".into()),
                ..Default::default()
            })?;
            Ok(ActionResult::applied(format!("Created the job {} for {}", created.name, client.name)).created(created.id))
        }

        "project.setStatus" => {
            let p = obj(payload, &["project", "status"])?;
            let job = project(db, p.get("project"))?;
            let status = match p.get("status") {
                Some(Value::String(s)) if ["active", "paused", "complete", "archived"].contains(&s.as_str()) => s.clone(),
                _ => return reject("Billable couldn't read the job status."),
            };
            db.update_project(job.id, &ProjectPatch { status: Some(status.clone()), ..Default::default() })?;
            Ok(ActionResult::applied(format!("Marked {} {status}", job.name)))
        }

        "expense.add" => {
            let p = obj(payload, &["date", "category", "amount", "description"])?;
            let date = day(p.get("date"))?;
            let category = match p.get("category") {
                Some(Value::String(s)) => EXPENSE_CATEGORIES.iter().find(|c| c.id == s),
                _ => None,
            };
            let Some(category) = category else { return reject("Billable doesn't have that expense category.") };
            let amount = js_round(num(p.get("amount"), 0.01, 100000.0, "amount")? * 100.0) / 100.0;
            let description = optional_note(p.get("description"))?.unwrap_or_default();
            let created = db.create_expense(&ExpenseInput {
                date: Some(date),
                category: Some(category.id.into()),
                amount: Some(amount),
                description: Some(description),
                ..Default::default()
            })?;
            Ok(ActionResult::applied(format!("Added a {} {} expense", fmt_money(amount), category.label.to_lowercase())).created(created.id))
        }

        "invoice.markSent" => {
            let p = obj(payload, &["invoiceBillableId", "sentDate"])?;
            let Some(id) = is_id(p.get("invoiceBillableId")) else { return reject("Billable couldn't tell which invoice this is.") };
            let sent_date = day(p.get("sentDate"))?;
            let Some(inv) = db.get_invoice(id)? else { return reject("That invoice no longer exists in Billable.") };
            if inv.stored_status.as_deref() != Some("draft") {
                return Ok(ActionResult::applied(format!("{} was already sent", inv.invoice.invoice_number)));
            }
            db.mark_invoices_sent(&[inv.invoice.id], Some(&sent_date))?;
            Ok(ActionResult::applied(format!("Marked {} sent", inv.invoice.invoice_number)))
        }

        "invoice.markPaid" => {
            let p = obj(payload, &["invoiceBillableIds", "paidDate", "method"])?;
            let ids: Vec<i64> = match p.get("invoiceBillableIds") {
                Some(Value::Array(items)) if (1..=50).contains(&items.len()) => {
                    let ids: Vec<Option<i64>> = items.iter().map(|v| is_id(Some(v))).collect();
                    if ids.iter().any(Option::is_none) {
                        return reject("Billable couldn't tell which invoices were paid.");
                    }
                    ids.into_iter().flatten().collect()
                }
                _ => return reject("Billable couldn't tell which invoices were paid."),
            };
            let paid_date = day(p.get("paidDate"))?;
            let method = match p.get("method") {
                Some(Value::String(m)) if payment_method_names(db)?.contains(m) => m.clone(),
                _ => return reject("Billable doesn't have that payment method."),
            };
            let mut invoices = Vec::new();
            for id in &ids {
                invoices.push(db.get_invoice(*id)?);
            }
            if invoices.iter().any(Option::is_none) {
                return reject("One of those invoices no longer exists in Billable.");
            }
            let open: Vec<_> = invoices.into_iter().flatten().filter(|i| i.stored_status.as_deref() != Some("paid")).collect();
            if open.is_empty() {
                return Ok(ActionResult::applied("Those invoices were already paid"));
            }
            let open_ids: Vec<i64> = open.iter().map(|i| i.invoice.id).collect();
            db.mark_invoices_paid(&open_ids, Some(&paid_date), Some(&method))?;
            let total: f64 = open.iter().map(|i| if i.invoice.total.is_nan() { 0.0 } else { i.invoice.total }).sum();
            let names = if open.len() == 1 { open[0].invoice.invoice_number.clone() } else { format!("{} invoices", open.len()) };
            Ok(ActionResult::applied(format!("Recorded a {} payment for {names}", fmt_money(total))))
        }

        _ => reject("This version of Billable doesn\u{2019}t know that change. Update Billable on your Mac."),
    }
}

/// `/^([01]\d|2[0-3]):[0-5]\d$/`
fn valid_hhmm(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 5
        && b[2] == b':'
        && b.iter().enumerate().all(|(i, c)| i == 2 || c.is_ascii_digit())
        && ((b[0] <= b'1') || (b[0] == b'2' && b[1] <= b'3'))
        && b[3] <= b'5'
}

// ---------- A batch from Content HQ (phone-sync.ts applyIncoming) ----------

#[derive(Debug)]
struct RolledBack(ActionResult);

impl std::fmt::Display for RolledBack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("rolled back")
    }
}
impl std::error::Error for RolledBack {}

fn opt_text(s: Option<String>) -> SqlValue {
    s.map(SqlValue::Text).unwrap_or(SqlValue::Null)
}

fn record(db: &Db, a: &IncomingAction, r: &ActionResult) -> Result<()> {
    db.run(
        "
    INSERT OR IGNORE INTO phone_actions (id, kind, status, summary, message, created_billable_id, occurred_at, applied_at, acked)
    VALUES (?, ?, ?, ?, ?, ?, ?, ?, 0)
  ",
        rusqlite::params![
            a.id,
            slice16(&a.kind, 40),
            r.status,
            slice16(&r.summary, 200),
            opt_text(r.message.as_deref().filter(|m| !m.is_empty()).map(|m| slice16(m, 200).to_string())),
            opt_text(r.created_billable_id.clone().filter(|c| !c.is_empty())),
            opt_text(match &a.occurred_at {
                Value::String(s) => Some(slice16(s, 40).to_string()),
                _ => None,
            }),
            db.clock().now_iso(),
        ],
    )?;
    Ok(())
}

/// Apply new actions in order (at most 100). Ones already applied are only
/// acknowledged again. Each runs in its own transaction: a rejected change
/// is rolled back, then recorded as rejected.
pub fn apply_incoming(db: &Db, timer: &mut TimerManager, list: &[Value]) -> Result<Vec<AppliedSummary>> {
    let mut results = Vec::new();
    for raw in list.iter().take(100) {
        let Some(o) = raw.as_object() else { continue };
        let (Some(Value::String(id)), Some(Value::String(kind))) = (o.get("id"), o.get("kind")) else { continue };
        if id.len() != 36 || !id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-') {
            continue;
        }
        let a = IncomingAction {
            id: id.to_lowercase(),
            kind: kind.clone(),
            payload: o.get("payload").cloned().unwrap_or(Value::Null),
            occurred_at: o.get("occurredAt").cloned().unwrap_or(Value::Null),
        };
        if db.row("SELECT id FROM phone_actions WHERE id = ?", [&a.id])?.is_some() {
            db.run("UPDATE phone_actions SET acked = 0 WHERE id = ?", [&a.id])?;
            continue;
        }
        let outcome = db.tx(|| {
            let r = apply_action(db, timer, &a);
            // Undo anything a rejected change half-did, then record the rejection
            if r.status == "rejected" {
                return Err(RolledBack(r).into());
            }
            record(db, &a, &r)?;
            Ok(r)
        });
        let r = match outcome {
            Ok(r) => r,
            Err(err) => {
                let r = match err.downcast::<RolledBack>() {
                    Ok(RolledBack(r)) => r,
                    Err(e) => {
                        eprintln!("[phone] could not record action {e:#}");
                        ActionResult {
                            status: "rejected",
                            summary: "Change from your phone".into(),
                            message: Some("Billable hit a problem applying this change.".into()),
                            created_billable_id: None,
                        }
                    }
                };
                record(db, &a, &r)?;
                if a.kind.starts_with("timer.") {
                    timer.sync_from_database(db)?;
                }
                r
            }
        };
        results.push(AppliedSummary { status: r.status, summary: r.summary, message: r.message });
    }
    Ok(results)
}

/// An acknowledgement sent with the next sync.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ack {
    pub action_id: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_billable_id: Option<String>,
}

/// Results Content HQ hasn't confirmed yet (oldest first, at most 200).
pub fn unacked_acks(db: &Db) -> Result<Vec<Ack>> {
    Ok(db
        .rows("SELECT id, status, message, created_billable_id FROM phone_actions WHERE acked = 0 ORDER BY applied_at LIMIT 200", [])?
        .iter()
        .map(|r| Ack {
            action_id: r.col("id"),
            status: r.col("status"),
            message: r.col::<Option<String>>("message").filter(|m| !m.is_empty()).map(|m| slice16(&m, 200).to_string()),
            created_billable_id: r.col::<Option<String>>("created_billable_id").filter(|c| !c.is_empty()),
        })
        .collect())
}

/// Content HQ stored these acknowledgements.
pub fn mark_acked(db: &Db, action_ids: &[String]) -> Result<()> {
    db.tx(|| {
        for id in action_ids {
            db.run("UPDATE phone_actions SET acked = 1 WHERE id = ?", [id])?;
        }
        Ok(())
    })
}

/// One change from the phone, for Settings (`phone:activity`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Activity {
    pub id: String,
    pub kind: String,
    pub status: String,
    pub summary: String,
    pub message: Option<String>,
    pub occurred_at: Option<String>,
    pub applied_at: String,
}

/// The latest changes from the phone, newest first (`limit` 1–100). Empty if
/// the table can't be read.
pub fn list_activity(db: &Db, limit: i64) -> Vec<Activity> {
    db.rows(
        "
      SELECT id, kind, status, summary, message, occurred_at, applied_at
      FROM phone_actions ORDER BY applied_at DESC LIMIT ?
    ",
        [limit.clamp(1, 100)],
    )
    .map(|rows| {
        rows.iter()
            .map(|r| Activity {
                id: r.col("id"),
                kind: r.col("kind"),
                status: r.col("status"),
                summary: r.col("summary"),
                message: r.col("message"),
                occurred_at: r.col("occurred_at"),
                applied_at: r.col("applied_at"),
            })
            .collect()
    })
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_and_minutes_read_like_the_app() {
        assert_eq!(fmt_money(1234.5), "$1,234.50");
        assert_eq!(fmt_money(1.005), "$1.01");
        assert_eq!(fmt_money(0.1 + 0.2), "$0.30");
        assert_eq!(fmt_money(999.995), "$1,000.00");
        assert_eq!(fmt_money(0.0), "$0.00");
        assert_eq!(fmt_money(1234567.0), "$1,234,567.00");
        assert_eq!(fmt_minutes(90.0), "1h 30m");
        assert_eq!(fmt_minutes(120.0), "2h");
        assert_eq!(fmt_minutes(45.0), "45m");
        assert_eq!(fmt_minutes(119.5), "1h 60m"); // as the TypeScript prints it
        assert!(valid_hhmm("09:30") && valid_hhmm("23:59") && !valid_hhmm("24:00") && !valid_hhmm("9:30") && !valid_hhmm("12:60"));
    }
}
