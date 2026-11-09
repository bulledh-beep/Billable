//! `phone-snapshot.ts`: the copy of Billable that Content HQ shows on the
//! phone (snapshot schema version 1). Content HQ never does billing math, so
//! every figure comes from the same queries Billable's own pages use. Client
//! emails, addresses, invoice lines and payment details stay on the Mac.

use std::collections::HashMap;

use anyhow::Result;
use chrono::{Datelike, Local, Offset, TimeZone, Timelike};
use serde::Serialize;
use serde_json::Value;

use super::{is_ymd, slice16, JsNum};
use crate::db::{Db, FromCol};
use crate::js::{js_round, js_string, js_trim, js_truthy};
use crate::time::{js_date_parse, local_add_days, local_date, local_start_of_day};
use crate::timer::TimerManager;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ExpenseCategory {
    pub id: &'static str,
    pub label: &'static str,
}

pub const EXPENSE_CATEGORIES: [ExpenseCategory; 8] = [
    ExpenseCategory { id: "equipment", label: "Equipment" },
    ExpenseCategory { id: "software", label: "Software" },
    ExpenseCategory { id: "home_office", label: "Home office" },
    ExpenseCategory { id: "phone_internet", label: "Phone and internet" },
    ExpenseCategory { id: "travel", label: "Travel" },
    ExpenseCategory { id: "meals", label: "Meals" },
    ExpenseCategory { id: "professional_development", label: "Professional development" },
    ExpenseCategory { id: "other", label: "Other" },
];

const RECENT_DAYS: i64 = 60;
const MAX_ENTRIES: usize = 5000;
const MAX_INVOICES: usize = 2000;
const MAX_EXPENSES: usize = 5000;
const MAX_CLIENTS: usize = 1000;
const MAX_PROJECTS: usize = 2000;
const MAX_NOTE: usize = 500;
const MAX_MONEY: f64 = 10_000_000.0;

const PROJECT_STATUSES: [&str; 4] = ["active", "paused", "complete", "archived"];
const INVOICE_STATUSES: [&str; 4] = ["draft", "sent", "overdue", "paid"];

// ---------- The snapshot (field order as the TypeScript builds it) ----------

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub generated_at: String,
    pub currency: String,
    pub timer: Option<TimerSnap>,
    pub totals: Totals,
    pub clients: Vec<ClientSnap>,
    pub projects: Vec<ProjectSnap>,
    pub entries: Vec<EntrySnap>,
    pub invoices: Vec<InvoiceSnap>,
    pub expenses: Vec<ExpenseSnap>,
    pub expense_categories: Vec<ExpenseCategory>,
    pub payment_methods: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimerSnap {
    pub entry_id: String,
    pub project_billable_id: String,
    pub note: Option<String>,
    pub started_at: Option<String>,
    /// `running` | `paused`
    pub state: &'static str,
    pub accumulated_minutes: JsNum,
    pub active_since: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    pub unbilled_amount: JsNum,
    pub unbilled_hours: JsNum,
    pub awaiting_amount: JsNum,
    pub awaiting_count: i64,
    pub overdue_amount: JsNum,
    pub overdue_count: i64,
    pub paid_this_month: JsNum,
    pub paid_this_year: JsNum,
    pub hours_today: JsNum,
    pub hours_this_week: JsNum,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientSnap {
    pub billable_id: String,
    pub name: String,
    pub company: Option<String>,
    pub rate: JsNum,
    pub unbilled_amount: JsNum,
    pub outstanding_amount: JsNum,
    pub overdue_amount: JsNum,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSnap {
    pub billable_id: String,
    pub client_billable_id: String,
    pub name: String,
    pub status: String,
    pub rate: JsNum,
    pub color: String,
    pub tracked_hours: JsNum,
    pub unbilled_hours: JsNum,
    pub unbilled_amount: JsNum,
    pub last_activity_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntrySnap {
    pub billable_id: String,
    pub project_billable_id: String,
    pub date: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub minutes: JsNum,
    pub note: Option<String>,
    pub billable: bool,
    pub invoiced: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceSnap {
    pub billable_id: String,
    pub number: String,
    pub client_billable_id: String,
    pub project_billable_ids: Vec<String>,
    pub status: String,
    pub issue_date: String,
    pub due_date: String,
    pub sent_at: Option<String>,
    pub paid_date: Option<String>,
    pub total: JsNum,
    pub hours: JsNum,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpenseSnap {
    pub billable_id: String,
    pub date: String,
    pub category: String,
    pub description: Option<String>,
    pub amount: JsNum,
}

// ---------- Field helpers (same names as the TypeScript) ----------

fn pad2(n: u32) -> String {
    format!("{n:02}")
}

/// ISO 8601 in the Mac's timezone, with its offset: `2026-09-24T18:00:00-06:00`.
pub fn iso_local(ms: f64) -> String {
    let d = match Local.timestamp_millis_opt(ms as i64).single() {
        Some(d) => d,
        None => return String::new(),
    };
    let off = d.offset().fix().local_minus_utc() / 60;
    let sign = if off >= 0 { '+' } else { '-' };
    let abs = off.unsigned_abs();
    format!(
        "{}-{}-{}T{}:{}:{}{}{}:{}",
        d.year(),
        pad2(d.month()),
        pad2(d.day()),
        pad2(d.hour()),
        pad2(d.minute()),
        pad2(d.second()),
        sign,
        pad2(abs / 60),
        pad2(abs % 60)
    )
}

/// Stored times come as ISO strings, SQLite UTC `YYYY-MM-DD HH:MM:SS`, or plain dates.
pub(crate) fn parse_stored(value: Option<&str>) -> Option<f64> {
    let s = value.filter(|s| !s.is_empty())?;
    let b = s.as_bytes();
    let sqlite_prefix = b.len() >= 16
        && is_ymd(&s[..10])
        && b[10] == b' '
        && b[11..13].iter().all(u8::is_ascii_digit)
        && b[13] == b':'
        && b[14..16].iter().all(u8::is_ascii_digit);
    let ms = if is_ymd(s) {
        js_date_parse(&format!("{s}T00:00:00")) // a local day
    } else if sqlite_prefix {
        js_date_parse(&format!("{}Z", s.replacen(' ', "T", 1)))
    } else {
        js_date_parse(s)
    };
    ms.is_finite().then_some(ms)
}

fn time_or_null(value: Option<&str>) -> Option<String> {
    parse_stored(value).map(iso_local)
}

fn day_or_null(value: Option<&str>) -> Option<String> {
    let s = slice16(value.unwrap_or(""), 10);
    is_ymd(s).then(|| s.to_string())
}

fn num(n: f64) -> f64 {
    if n.is_nan() {
        0.0
    } else {
        n
    }
}

fn money(n: f64) -> JsNum {
    let v = js_round(num(n) * 100.0) / 100.0;
    JsNum(v.max(0.0).min(MAX_MONEY))
}

fn hours(n: f64) -> JsNum {
    JsNum((js_round(num(n) * 100.0) / 100.0).max(0.0))
}

fn minutes(n: f64) -> JsNum {
    JsNum((js_round(num(n) * 100.0) / 100.0).max(0.0).min(1440.0))
}

/// Text that can be empty goes as null, as Content HQ's contract asks.
fn note(s: &str) -> Option<String> {
    let v = slice16(js_trim(s), MAX_NOTE);
    (!v.is_empty()).then(|| v.to_string())
}

/// Content HQ requires names to be non-empty and caps their length; one odd
/// row must not stop every sync.
fn label(s: &str, max: usize, fallback: &str) -> String {
    let t = js_trim(s);
    slice16(if t.is_empty() { fallback } else { t }, max).to_string()
}

fn truthy(s: &Option<String>) -> bool {
    s.as_deref().is_some_and(|v| !v.is_empty())
}

/// The payment methods the phone may pick from (names only).
pub fn payment_method_names(db: &Db) -> Result<Vec<String>> {
    let settings = db.get_settings()?;
    let raw = settings.get("payment_methods").filter(|s| !s.is_empty()).unwrap_or("[]");
    let mut names: Vec<String> = match serde_json::from_str::<Value>(raw) {
        Ok(Value::Array(items)) => items
            .iter()
            .map(|m| {
                let name = match m {
                    Value::Object(o) => o.get("name").cloned().unwrap_or(Value::Null),
                    _ => Value::Null,
                };
                let text = if js_truthy(&name) { js_string(&name) } else { String::new() };
                slice16(js_trim(&text), 60).to_string()
            })
            .filter(|s| !s.is_empty())
            .collect(),
        _ => vec![],
    };
    if names.is_empty() {
        let d = settings.get("default_payment_method").filter(|s| !s.is_empty()).unwrap_or("e-Transfer");
        names = vec![slice16(d, 60).to_string()];
    }
    let mut seen = std::collections::HashSet::new();
    names.retain(|n| seen.insert(n.clone()));
    names.truncate(20);
    Ok(names)
}

/// The settings' currency if it's a three-letter code, else CAD.
pub fn snapshot_currency(db: &Db) -> Result<String> {
    let settings = db.get_settings()?;
    let code = settings.text("default_currency").to_uppercase();
    Ok(if code.len() == 3 && code.bytes().all(|b| b.is_ascii_uppercase()) { code } else { "CAD".into() })
}

fn timer_snapshot(timer: &TimerManager) -> Option<TimerSnap> {
    let running = timer.active();
    let e = running.or(timer.paused())?;
    let since = if truthy(&e.active_since) { e.active_since.as_deref() } else { Some(e.start_time.as_str()) };
    Some(TimerSnap {
        entry_id: e.id.to_string(),
        project_billable_id: e.project_id.to_string(),
        note: note(&e.description),
        started_at: time_or_null(Some(&e.start_time)),
        state: if running.is_some() { "running" } else { "paused" },
        accumulated_minutes: JsNum((js_round(num(e.duration_minutes) * 100.0) / 100.0).max(0.0)),
        active_since: if running.is_some() { time_or_null(since) } else { None },
    })
}

/// `buildSnapshot(timer)`: everything Content HQ shows, as of the database's clock.
pub fn build_snapshot(db: &Db, timer: &TimerManager) -> Result<Snapshot> {
    let now = db.clock().now_ms();
    let today = db.local_today();
    let month = slice16(&today, 7).to_string();
    let stats = db.get_dashboard_stats()?;
    let overview = db.get_billing_overview()?;
    let p = &overview.pipeline;
    let all_invoices = &overview.invoices;

    let paid_this_month: f64 = all_invoices
        .iter()
        .filter(|i| {
            let day = if truthy(&i.invoice.payment_date) { i.invoice.payment_date.clone().unwrap_or_default() } else { i.invoice.issue_date.clone() };
            i.invoice.status == "paid" && day.starts_with(&month)
        })
        .map(|i| num(i.invoice.total))
        .sum();

    let clients = db
        .list_clients()?
        .into_iter()
        .take(MAX_CLIENTS)
        .map(|c| ClientSnap {
            billable_id: c.id.to_string(),
            name: label(&c.name, 200, "Unnamed client"),
            company: Some(slice16(js_trim(&c.company), 200).to_string()).filter(|s| !s.is_empty()),
            rate: money(c.default_rate),
            unbilled_amount: money(c.unbilled_amount),
            outstanding_amount: money(c.outstanding_amount),
            overdue_amount: money(c.overdue_amount),
        })
        .collect();

    let projects = db
        .list_projects(None)?
        .into_iter()
        .take(MAX_PROJECTS)
        .map(|pr| {
            let color_ok = pr.color.len() == 7 && pr.color.starts_with('#') && pr.color[1..].bytes().all(|b| b.is_ascii_hexdigit());
            ProjectSnap {
                billable_id: pr.id.to_string(),
                client_billable_id: pr.client_id.to_string(),
                name: label(&pr.name, 200, "Untitled job"),
                status: if PROJECT_STATUSES.contains(&pr.status.as_str()) { pr.status.clone() } else { "active".into() },
                rate: money(pr.rate),
                color: if color_ok { pr.color.clone() } else { "#F5A623".into() },
                tracked_hours: hours(pr.total_hours),
                unbilled_hours: hours(pr.unbilled_hours),
                unbilled_amount: money(pr.unbilled_amount),
                last_activity_at: time_or_null(pr.last_activity.as_deref()),
            }
        })
        .collect();

    // Every unbilled entry, plus everything from the last 60 days
    let since = local_add_days(local_start_of_day(now), -RECENT_DAYS);
    let entries = db
        .list_time_entries(None)?
        .into_iter()
        .filter(|e| truthy(&e.entry.end_time))
        .filter(|e| e.billing_state == "unbilled" || parse_stored(Some(&e.entry.start_time)).unwrap_or(0.0) >= since)
        .take(MAX_ENTRIES)
        .map(|e| {
            let start = parse_stored(Some(&e.entry.start_time));
            EntrySnap {
                billable_id: e.entry.id.to_string(),
                project_billable_id: e.entry.project_id.to_string(),
                date: start.map(local_date).unwrap_or_else(|| today.clone()),
                started_at: iso_local(start.unwrap_or(now as f64)),
                ended_at: time_or_null(e.entry.end_time.as_deref()),
                minutes: minutes(e.entry.duration_minutes),
                note: note(&e.entry.description),
                billable: e.entry.is_billable != 0,
                invoiced: e.entry.invoice_id.is_some_and(|i| i != 0) || e.entry.is_invoiced != 0,
            }
        })
        .collect();

    // Projects each invoice's lines are for, in the order SQLite returns them
    let mut invoice_projects: HashMap<i64, Vec<String>> = HashMap::new();
    for row in db.rows("SELECT DISTINCT invoice_id, project_id FROM invoice_items WHERE project_id IS NOT NULL", [])? {
        let invoice_id: i64 = row.col("invoice_id");
        let project_id = <String as FromCol>::from_col(row.get("project_id"));
        let set = invoice_projects.entry(invoice_id).or_default();
        if !set.contains(&project_id) {
            set.push(project_id);
        }
    }
    let invoices = all_invoices
        .iter()
        .take(MAX_INVOICES)
        .map(|inv| {
            let i = &inv.invoice;
            let mut ids = invoice_projects.get(&i.id).cloned().unwrap_or_default();
            if let Some(pid) = i.project_id.filter(|p| *p != 0) {
                let pid = pid.to_string();
                if !ids.contains(&pid) {
                    ids.push(pid);
                }
            }
            ids.truncate(200);
            ProjectIdsInvoice { inv, ids }.into_snap(&today)
        })
        .collect();

    let year = db.clock().local_year() as i64;
    let mut expense_rows = db.list_expenses(Some(year))?;
    expense_rows.extend(db.list_expenses(Some(year - 1))?);
    let expenses = expense_rows
        .into_iter()
        .take(MAX_EXPENSES)
        .map(|x| ExpenseSnap {
            billable_id: x.id.to_string(),
            date: day_or_null(Some(&x.date)).unwrap_or_else(|| today.clone()),
            category: if EXPENSE_CATEGORIES.iter().any(|c| c.id == x.category) { x.category.clone() } else { "other".into() },
            description: note(&x.description),
            amount: money(x.amount),
        })
        .collect();

    Ok(Snapshot {
        generated_at: iso_local(now as f64),
        currency: snapshot_currency(db)?,
        timer: timer_snapshot(timer),
        totals: Totals {
            unbilled_amount: money(p.unbilled_amount),
            unbilled_hours: hours(p.unbilled_hours),
            awaiting_amount: money(p.awaiting_amount),
            awaiting_count: p.awaiting_count,
            overdue_amount: money(p.overdue_amount),
            overdue_count: p.overdue_count,
            paid_this_month: money(paid_this_month),
            paid_this_year: money(p.paid_ytd_amount),
            hours_today: hours(stats.hours_today),
            hours_this_week: hours(stats.hours_this_week),
        },
        clients,
        projects,
        entries,
        invoices,
        expenses,
        expense_categories: EXPENSE_CATEGORIES.to_vec(),
        payment_methods: payment_method_names(db)?,
    })
}

struct ProjectIdsInvoice<'a> {
    inv: &'a crate::models::Invoice,
    ids: Vec<String>,
}

impl ProjectIdsInvoice<'_> {
    fn into_snap(self, today: &str) -> InvoiceSnap {
        let i = &self.inv.invoice;
        InvoiceSnap {
            billable_id: i.id.to_string(),
            number: label(&i.invoice_number, 60, &format!("Invoice {}", i.id)),
            client_billable_id: i.client_id.to_string(),
            project_billable_ids: self.ids,
            status: if INVOICE_STATUSES.contains(&i.status.as_str()) { i.status.clone() } else { "draft".into() },
            issue_date: day_or_null(Some(&i.issue_date)).unwrap_or_else(|| today.to_string()),
            due_date: day_or_null(Some(&i.due_date)).or_else(|| day_or_null(Some(&i.issue_date))).unwrap_or_else(|| today.to_string()),
            sent_at: time_or_null(i.sent_at.as_deref()),
            paid_date: if i.status == "paid" { day_or_null(i.payment_date.as_deref()) } else { None },
            total: money(i.total),
            hours: hours(self.inv.entry_hours),
        }
    }
}
