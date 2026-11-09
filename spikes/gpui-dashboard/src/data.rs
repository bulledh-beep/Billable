//! What the Dashboard shows, read straight from Billable's own SQLite file.
//! Ports of getDashboardStats, getBillingOverview, listTimeEntries and
//! listProjects in src/main/database.ts. The database is opened read-only.

use std::{collections::HashSet, path::PathBuf};

use anyhow::{Context as _, Result};
use chrono::{Datelike, Local, NaiveDate};
use rusqlite::{Connection, OpenFlags};

use crate::format::{friendly_day, iso_utc, js_round, local_day, local_midnight, parse_time};

#[derive(Clone)]
pub struct Profile {
    pub name: String,
    pub color: String,
}

#[derive(Clone, Copy, PartialEq)]
pub enum BillingState {
    Unbilled,
    NonBillable,
    Draft,
    Sent,
    Overdue,
    Paid,
    Invoiced,
}

#[derive(Clone)]
pub struct Entry {
    pub id: i64,
    pub project_id: i64,
    pub description: String,
    pub start_time: String,
    pub end_time: Option<String>,
    pub duration_minutes: f64,
    pub paused_at: Option<String>,
    pub active_since: Option<String>,
    pub is_billable: bool,
    pub project_name: String,
    pub project_color: String,
    pub rate: f64,
    pub client_name: String,
    pub invoice_number: Option<String>,
    pub billing_state: BillingState,
}

#[derive(Clone)]
pub struct Project {
    pub id: i64,
    pub name: String,
    pub color: String,
    pub client_name: String,
}

#[derive(Clone, Copy, PartialEq)]
pub enum AttentionKind {
    Unbilled,
    Overdue,
    Draft,
    AccidentalTimer,
    DuplicateClients,
}

#[derive(Clone)]
pub struct Attention {
    pub key: String,
    pub kind: AttentionKind,
    pub title: String,
    pub detail: String,
    pub amount: Option<f64>,
}

#[derive(Clone, Default)]
pub struct Pipeline {
    pub unbilled_amount: f64,
    pub unbilled_hours: f64,
    pub draft_amount: f64,
    pub draft_count: usize,
    pub awaiting_amount: f64,
    pub awaiting_count: usize,
    pub overdue_amount: f64,
    pub overdue_count: usize,
    pub paid_ytd_amount: f64,
    pub paid_ytd_count: usize,
}

#[derive(Clone)]
pub struct Dashboard {
    pub profile: Profile,
    pub hours_today: f64,
    pub hours_this_month: f64,
    pub pipeline: Pipeline,
    pub attention: Vec<Attention>,
    pub entries: Vec<Entry>,
    pub projects: Vec<Project>,
    pub active: Option<Entry>,
}

fn data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("BILLABLE_USER_DATA") {
        return PathBuf::from(dir);
    }
    PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("Library/Application Support/Billable")
}

fn active_profile() -> Result<(String, Profile)> {
    let raw = std::fs::read_to_string(data_dir().join("profiles.json")).context("read profiles.json")?;
    let json: serde_json::Value = serde_json::from_str(&raw)?;
    let id = json["active"].as_str().unwrap_or("default").to_string();
    let p = json["profiles"].as_array().and_then(|list| list.iter().find(|p| p["id"] == id.as_str()));
    let profile = Profile {
        name: p.and_then(|p| p["name"].as_str()).unwrap_or("Profile").to_string(),
        color: p.and_then(|p| p["color"].as_str()).unwrap_or("#F5A623").to_string(),
    };
    Ok((id, profile))
}

fn effective_status(stored: &str, due: Option<&str>, today: NaiveDate) -> &'static str {
    match stored {
        "paid" => "paid",
        "draft" => "draft",
        _ => match due.and_then(|d| NaiveDate::parse_from_str(&d[..d.len().min(10)], "%Y-%m-%d").ok()) {
            Some(d) if d < today => "overdue",
            _ => "sent",
        },
    }
}

const UNBILLED_SQL: &str = "te.end_time IS NOT NULL AND te.is_billable = 1 AND te.is_invoiced = 0 AND te.invoice_id IS NULL";

const NAME_NOISE: &[&str] = &[
    "the", "inc", "incorporated", "ltd", "limited", "llc", "llp", "co", "corp", "corporation", "company", "society",
    "association", "group", "foundation", "and",
];

fn normalize_name(name: &str) -> String {
    let lowered = name.to_lowercase().replace('&', " and ");
    let cleaned: String = lowered.chars().map(|c| if c.is_ascii_lowercase() || c.is_ascii_digit() || c == ' ' { c } else { ' ' }).collect();
    cleaned.split_whitespace().filter(|w| !NAME_NOISE.contains(w)).collect::<Vec<_>>().join(" ")
}

struct Invoice {
    id: i64,
    number: String,
    status: &'static str,
    total: f64,
    due_date: Option<String>,
    issue_date: String,
    payment_date: Option<String>,
    created_at: Option<String>,
    client_name: Option<String>,
}

pub fn load() -> Result<Dashboard> {
    let (profile_id, profile) = active_profile()?;
    let path = data_dir().join("profiles").join(&profile_id).join("billable.db");
    let db = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)
        .with_context(|| format!("open {}", path.display()))?;

    let now = Local::now();
    let today = now.date_naive();
    let year = today.year().to_string();

    // ---- Hours (getDashboardStats) ----
    let hours_since = |from: chrono::DateTime<Local>| -> Result<f64> {
        Ok(db.query_row(
            "SELECT COALESCE(SUM(duration_minutes) / 60.0, 0) FROM time_entries WHERE end_time IS NOT NULL AND start_time >= ?",
            [iso_utc(from)],
            |r| r.get(0),
        )?)
    };
    let hours_today = hours_since(local_midnight(today))?;
    let hours_this_month = hours_since(local_midnight(today.with_day(1).unwrap()))?;

    // ---- Unbilled work by project (getBillingOverview) ----
    struct UnbilledRow {
        project_id: i64,
        project_name: String,
        project_status: String,
        client_name: String,
        hours: f64,
        amount: f64,
        oldest_day: NaiveDate,
    }
    let mut stmt = db.prepare(&format!(
        "SELECT p.id, p.name, p.status, c.name, SUM(te.duration_minutes) / 60.0, SUM(te.duration_minutes * p.rate / 60.0), MIN(te.start_time)
         FROM time_entries te JOIN projects p ON p.id = te.project_id JOIN clients c ON c.id = p.client_id
         WHERE {UNBILLED_SQL} GROUP BY p.id ORDER BY c.name COLLATE NOCASE, MIN(te.start_time)"
    ))?;
    let unbilled: Vec<UnbilledRow> = stmt
        .query_map([], |r| {
            let oldest: String = r.get(6)?;
            Ok(UnbilledRow {
                project_id: r.get(0)?,
                project_name: r.get(1)?,
                project_status: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                client_name: r.get(3)?,
                hours: r.get(4)?,
                amount: r.get(5)?,
                oldest_day: local_day(&oldest).unwrap_or(today),
            })
        })?
        .collect::<Result<_, _>>()?;

    // ---- Invoices, newest first, with overdue worked out ----
    let mut stmt = db.prepare(
        "SELECT i.invoice_number, i.status, i.total, i.due_date, i.issue_date, i.payment_date, i.created_at, c.name, i.id
         FROM invoices i LEFT JOIN clients c ON i.client_id = c.id ORDER BY i.issue_date DESC, i.id DESC",
    )?;
    let invoices: Vec<Invoice> = stmt
        .query_map([], |r| {
            let stored: String = r.get(1)?;
            let due: Option<String> = r.get(3)?;
            Ok(Invoice {
                id: r.get(8)?,
                number: r.get(0)?,
                status: effective_status(&stored, due.as_deref(), today),
                total: r.get::<_, Option<f64>>(2)?.unwrap_or(0.0),
                due_date: due,
                issue_date: r.get(4)?,
                payment_date: r.get(5)?,
                created_at: r.get(6)?,
                client_name: r.get(7)?,
            })
        })?
        .collect::<Result<_, _>>()?;

    let of = |s: &'static str| invoices.iter().filter(move |i| i.status == s);
    let paid_ytd: Vec<&Invoice> = of("paid")
        .filter(|i| i.payment_date.as_deref().unwrap_or(&i.issue_date).starts_with(&year))
        .collect();
    let pipeline = Pipeline {
        unbilled_amount: unbilled.iter().map(|u| u.amount).sum(),
        unbilled_hours: unbilled.iter().map(|u| u.hours).sum(),
        draft_amount: of("draft").map(|i| i.total).sum(),
        draft_count: of("draft").count(),
        awaiting_amount: of("sent").map(|i| i.total).sum(),
        awaiting_count: of("sent").count(),
        overdue_amount: of("overdue").map(|i| i.total).sum(),
        overdue_count: of("overdue").count(),
        paid_ytd_amount: paid_ytd.iter().map(|i| i.total).sum(),
        paid_ytd_count: paid_ytd.len(),
    };

    // ---- Needs attention ----
    let dismissed: HashSet<String> = db
        .query_row("SELECT value FROM settings WHERE key = 'dismissed_attention'", [], |r| r.get::<_, String>(0))
        .ok()
        .and_then(|v| serde_json::from_str::<Vec<String>>(&v).ok())
        .unwrap_or_default()
        .into_iter()
        .collect();
    let days_between = |from: NaiveDate, to: NaiveDate| (to - from).num_days();
    let mut attention = Vec::new();

    for inv in of("overdue") {
        let due = inv.due_date.as_deref().and_then(|d| NaiveDate::parse_from_str(&d[..10], "%Y-%m-%d").ok()).unwrap_or(today);
        let late = days_between(due, today);
        attention.push(Attention {
            key: format!("overdue:{}", inv.id),
            kind: AttentionKind::Overdue,
            title: format!("{} is overdue", inv.number),
            detail: format!(
                "{} · was due {} · {} day{} late",
                inv.client_name.as_deref().unwrap_or("Client"),
                friendly_day(due, today),
                late,
                if late == 1 { "" } else { "s" }
            ),
            amount: Some(inv.total),
        });
    }
    for row in &unbilled {
        let closed = row.project_status == "complete" || row.project_status == "archived";
        if !closed && days_between(row.oldest_day, today) < 30 {
            continue;
        }
        attention.push(Attention {
            key: format!("unbilled:{}", row.project_id),
            kind: AttentionKind::Unbilled,
            title: row.project_name.clone(),
            detail: format!(
                "{} · unbilled since {}{}",
                row.client_name,
                friendly_day(row.oldest_day, today),
                if closed { format!(" · project is marked {}", row.project_status) } else { String::new() }
            ),
            amount: Some(row.amount),
        });
    }
    for inv in of("draft") {
        let created = inv
            .created_at
            .as_deref()
            .and_then(local_day)
            .or_else(|| NaiveDate::parse_from_str(&inv.issue_date[..10], "%Y-%m-%d").ok())
            .unwrap_or(today);
        let age = days_between(created, today);
        if age < 3 {
            continue;
        }
        attention.push(Attention {
            key: format!("draft:{}", inv.id),
            kind: AttentionKind::Draft,
            title: format!("{} is still a draft", inv.number),
            detail: format!("{} · created {age} days ago · not marked as sent", inv.client_name.as_deref().unwrap_or("Client")),
            amount: Some(inv.total),
        });
    }
    let mut stmt = db.prepare(&format!(
        "SELECT te.id, te.start_time, te.duration_minutes, p.name, p.rate, (julianday(te.end_time) - julianday(te.start_time)) * 86400.0
         FROM time_entries te JOIN projects p ON p.id = te.project_id
         WHERE {UNBILLED_SQL} AND (julianday(te.end_time) - julianday(te.start_time)) * 86400.0 < 60 AND te.duration_minutes <= 30"
    ))?;
    let accidental = stmt.query_map([], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, f64>(2)?, r.get::<_, String>(3)?, r.get::<_, f64>(4)?, r.get::<_, f64>(5)?))
    })?;
    for row in accidental {
        let (id, start, minutes, project, rate, wall) = row?;
        let secs = js_round(wall).max(1.0) as i64;
        attention.push(Attention {
            key: format!("timer:{id}"),
            kind: AttentionKind::AccidentalTimer,
            title: project,
            detail: format!(
                "Timer ran {secs} second{} on {} and was rounded up to {}m",
                if secs == 1 { "" } else { "s" },
                friendly_day(local_day(&start).unwrap_or(today), today),
                js_round(minutes)
            ),
            amount: Some(minutes / 60.0 * rate),
        });
    }
    let mut stmt = db.prepare("SELECT id, name, COALESCE(company, '') FROM clients")?;
    let clients: Vec<(i64, String, String)> = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<Result<_, _>>()?;
    for (i, a) in clients.iter().enumerate() {
        for b in &clients[i + 1..] {
            let (na, nb, ca, cb) = (normalize_name(&a.1), normalize_name(&b.1), normalize_name(&a.2), normalize_name(&b.2));
            let same = (!na.is_empty() && na == nb) || (!ca.is_empty() && ca == nb) || (!cb.is_empty() && cb == na) || (!ca.is_empty() && !cb.is_empty() && ca == cb);
            if !same {
                continue;
            }
            attention.push(Attention {
                key: format!("dup:{}-{}", a.0.min(b.0), a.0.max(b.0)),
                kind: AttentionKind::DuplicateClients,
                title: format!("{} and {}", a.1, b.1),
                detail: "These look like the same client".into(),
                amount: None,
            });
        }
    }
    attention.retain(|a| !dismissed.contains(&a.key));

    // ---- Time entries (listTimeEntries) ----
    let mut stmt = db.prepare(
        "SELECT te.id, te.project_id, COALESCE(te.description, ''), te.start_time, te.end_time, COALESCE(te.duration_minutes, 0),
                te.paused_at, te.active_since, te.is_billable, te.is_invoiced, te.invoice_id,
                COALESCE(p.name, ''), COALESCE(p.color, '#AFAFAF'), COALESCE(p.rate, 0), COALESCE(c.name, ''),
                i.invoice_number, i.status, i.due_date
         FROM time_entries te
         LEFT JOIN projects p ON te.project_id = p.id
         LEFT JOIN clients c ON p.client_id = c.id
         LEFT JOIN invoices i ON i.id = te.invoice_id
         ORDER BY te.start_time DESC",
    )?;
    let entries: Vec<Entry> = stmt
        .query_map([], |r| {
            let invoice_id: Option<i64> = r.get(10)?;
            let invoice_number: Option<String> = r.get(15)?;
            let stored: Option<String> = r.get(16)?;
            let due: Option<String> = r.get(17)?;
            let is_invoiced: bool = r.get::<_, Option<i64>>(9)?.unwrap_or(0) != 0;
            let is_billable: bool = r.get::<_, Option<i64>>(8)?.unwrap_or(1) != 0;
            let billing_state = if invoice_id.is_some() && invoice_number.is_some() {
                match effective_status(stored.as_deref().unwrap_or("draft"), due.as_deref(), today) {
                    "paid" => BillingState::Paid,
                    "draft" => BillingState::Draft,
                    "overdue" => BillingState::Overdue,
                    _ => BillingState::Sent,
                }
            } else if is_invoiced {
                BillingState::Invoiced
            } else if !is_billable {
                BillingState::NonBillable
            } else {
                BillingState::Unbilled
            };
            Ok(Entry {
                id: r.get(0)?,
                project_id: r.get(1)?,
                description: r.get(2)?,
                start_time: r.get(3)?,
                end_time: r.get(4)?,
                duration_minutes: r.get(5)?,
                paused_at: r.get(6)?,
                active_since: r.get(7)?,
                is_billable,
                project_name: r.get(11)?,
                project_color: r.get(12)?,
                rate: r.get(13)?,
                client_name: r.get(14)?,
                invoice_number,
                billing_state,
            })
        })?
        .collect::<Result<_, _>>()?;
    let active = entries.iter().find(|e| e.end_time.is_none()).cloned();

    // ---- Active projects, most recently worked on first ----
    let mut stmt = db.prepare(
        "SELECT p.id, p.name, COALESCE(p.color, '#F5A623'), COALESCE(c.name, ''),
                COALESCE((SELECT MAX(te.start_time) FROM time_entries te WHERE te.project_id = p.id), p.created_at)
         FROM projects p LEFT JOIN clients c ON p.client_id = c.id WHERE p.status = 'active'",
    )?;
    let mut projects: Vec<(Project, String)> = stmt
        .query_map([], |r| Ok((Project { id: r.get(0)?, name: r.get(1)?, color: r.get(2)?, client_name: r.get(3)? }, r.get::<_, Option<String>>(4)?.unwrap_or_default())))?
        .collect::<Result<_, _>>()?;
    projects.sort_by(|a, b| b.1.cmp(&a.1));
    let projects = projects.into_iter().map(|(p, _)| p).collect();

    Ok(Dashboard { profile, hours_today, hours_this_month, pipeline, attention, entries, projects, active })
}

/// Seconds on a running or paused timer (useTimer's elapsedSeconds).
pub fn elapsed_seconds(e: &Entry) -> f64 {
    let accumulated = e.duration_minutes.max(0.0) * 60.0;
    let running = if e.paused_at.is_some() {
        0.0
    } else {
        let since = e.active_since.as_deref().unwrap_or(&e.start_time);
        parse_time(since).map(|t| (Local::now() - t).num_milliseconds().max(0) as f64 / 1000.0).unwrap_or(0.0)
    };
    (accumulated + running).floor()
}
