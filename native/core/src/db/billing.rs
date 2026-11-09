//! The Billing page overview: pipeline, ready-to-bill, needs-attention.

use anyhow::Result;
use serde_json::Value;

use super::{unbilled_sql, Db, RowMap};
use crate::js::{js_number_to_string, js_round, js_slice, js_string_to_number};
use crate::models::*;
use crate::time::{days_between, js_date_parse, local_date, sqlite_time_ms};

const STALE_UNBILLED_DAYS: i64 = 30;
const STALE_DRAFT_DAYS: i64 = 3;
const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// `"Jun 12"`, or `"Jun 12, 2025"` outside the current year.
pub fn friendly_day(ymd: &str, today: &str) -> String {
    let parts: Vec<f64> = js_slice(ymd, 10).split('-').map(js_string_to_number).collect();
    let get = |i: usize| parts.get(i).copied().unwrap_or(f64::NAN);
    let (y, m, d) = (get(0), get(1), get(2));
    let falsy = |v: f64| v == 0.0 || v.is_nan();
    if falsy(y) || falsy(m) || falsy(d) {
        return ymd.to_string();
    }
    let month = if m.fract() == 0.0 && (1.0..=12.0).contains(&m) { MONTHS[m as usize - 1] } else { "undefined" };
    let base = format!("{month} {}", js_number_to_string(d));
    let year = js_number_to_string(y);
    if year == js_slice(today, 4) {
        base
    } else {
        format!("{base}, {year}")
    }
}

/// `${n}` for a day count that may be `NaN` in JS.
fn days_text(n: Option<i64>) -> String {
    n.map(|v| v.to_string()).unwrap_or_else(|| "NaN".into())
}

const NAME_NOISE: [&str; 16] = [
    "the", "inc", "incorporated", "ltd", "limited", "llc", "llp", "co", "corp", "corporation", "company", "society",
    "association", "group", "foundation", "and",
];

/// Lowercase, `&` → `and`, punctuation out, legal-form words out.
pub fn normalize_name(name: &str) -> String {
    let lowered = name.to_lowercase().replace('&', " and ");
    let mut cleaned = String::with_capacity(lowered.len());
    let mut in_junk = false;
    for c in lowered.chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() || c == ' ' {
            cleaned.push(c);
            in_junk = false;
        } else if !in_junk {
            cleaned.push(' ');
            in_junk = true;
        }
    }
    cleaned.split(' ').filter(|w| !w.is_empty() && !NAME_NOISE.contains(w)).collect::<Vec<_>>().join(" ")
}

impl Db {
    /// Pairs of clients that look like the same organization.
    fn find_duplicate_clients(&self, dismissed: &[Value]) -> Result<Vec<AttentionItem>> {
        let clients: Vec<(i64, String, String)> = self
            .rows("SELECT id, name, company FROM clients", [])?
            .iter()
            .map(|r| (r.col("id"), r.col("name"), r.col("company")))
            .collect();
        let mut out = Vec::new();
        for i in 0..clients.len() {
            for j in i + 1..clients.len() {
                let (a, b) = (&clients[i], &clients[j]);
                let (na, nb) = (normalize_name(&a.1), normalize_name(&b.1));
                let (ca, cb) = (normalize_name(&a.2), normalize_name(&b.2));
                let same = (!na.is_empty() && na == nb)
                    || (!ca.is_empty() && ca == nb)
                    || (!cb.is_empty() && cb == na)
                    || (!ca.is_empty() && !cb.is_empty() && ca == cb);
                if !same {
                    continue;
                }
                let key = format!("dup:{}-{}", a.0.min(b.0), a.0.max(b.0));
                if dismissed.contains(&Value::String(key.clone())) {
                    continue;
                }
                out.push(AttentionItem {
                    key,
                    kind: "duplicate_clients".into(),
                    tone: "neutral".into(),
                    title: format!("{} and {}", a.1, b.1),
                    detail: "These look like the same client".into(),
                    client_ids: Some(vec![a.0, b.0]),
                    client_names: Some(vec![a.1.clone(), b.1.clone()]),
                    ..Default::default()
                });
            }
        }
        Ok(out)
    }

    /// Everything the Billing page and Dashboard need: money in each stage,
    /// unbilled work grouped by client, and a short list of things to act on.
    pub fn get_billing_overview(&self) -> Result<BillingOverview> {
        let today = self.local_today();
        let year = js_slice(&today, 4).to_string();

        let rows: Vec<RowMap> = self.rows(
            concat!(
                "
    SELECT p.id AS project_id, p.name AS project_name, p.color AS project_color,
           p.status AS project_status, p.rate, c.id AS client_id, c.name AS client_name,
           COUNT(te.id) AS entry_count,
           SUM(te.duration_minutes) / 60.0 AS hours,
           SUM(te.duration_minutes * p.rate / 60.0) AS amount,
           MIN(te.start_time) AS oldest, MAX(te.start_time) AS newest
    FROM time_entries te
    JOIN projects p ON p.id = te.project_id
    JOIN clients c ON c.id = p.client_id
    WHERE ",
                unbilled_sql!(),
                "
    GROUP BY p.id
    ORDER BY c.name COLLATE NOCASE, oldest
  "
            ),
            [],
        )?;

        // Group ready-to-bill work by client (first-seen order)
        let mut unbilled_projects: Vec<UnbilledProject> = Vec::new();
        let mut groups: Vec<UnbilledClient> = Vec::new();
        for r in &rows {
            let mut row = UnbilledProject::from_row(r, &[]);
            let oldest: Option<String> = r.col("oldest");
            row.oldest_day = local_date(oldest.as_deref().map(js_date_parse).unwrap_or(0.0));
            row.age_days = days_between(&row.oldest_day, &today);
            let gi = match groups.iter().position(|g| g.client_id == row.client_id) {
                Some(i) => i,
                None => {
                    groups.push(UnbilledClient {
                        client_id: row.client_id,
                        client_name: row.client_name.clone(),
                        amount: 0.0,
                        hours: 0.0,
                        entry_count: 0,
                        oldest_day: row.oldest_day.clone(),
                        projects: Vec::new(),
                    });
                    groups.len() - 1
                }
            };
            let g = &mut groups[gi];
            g.amount += row.amount;
            g.hours += row.hours;
            g.entry_count += row.entry_count;
            if row.oldest_day < g.oldest_day {
                g.oldest_day = row.oldest_day.clone();
            }
            g.projects.push(row.clone());
            unbilled_projects.push(row);
        }
        let mut ready_to_bill = groups;
        ready_to_bill.sort_by(|a, b| b.amount.partial_cmp(&a.amount).unwrap_or(std::cmp::Ordering::Equal));

        let invoices = self.list_invoices(None)?;
        let sum = |list: &[&Invoice]| list.iter().fold(0.0, |s, i| s + if i.total.is_nan() { 0.0 } else { i.total });
        let drafts: Vec<&Invoice> = invoices.iter().filter(|i| i.status == "draft").collect();
        let awaiting: Vec<&Invoice> = invoices.iter().filter(|i| i.status == "sent").collect();
        let overdue: Vec<&Invoice> = invoices.iter().filter(|i| i.status == "overdue").collect();
        let paid_this_year: Vec<&Invoice> = invoices
            .iter()
            .filter(|i| {
                let day = i.payment_date.as_deref().filter(|d| !d.is_empty()).unwrap_or(&i.issue_date);
                i.status == "paid" && day.starts_with(&year)
            })
            .collect();

        let pipeline = BillingPipeline {
            unbilled_amount: ready_to_bill.iter().fold(0.0, |s, g| s + g.amount),
            unbilled_hours: ready_to_bill.iter().fold(0.0, |s, g| s + g.hours),
            unbilled_entries: ready_to_bill.iter().map(|g| g.entry_count).sum(),
            unbilled_projects: unbilled_projects.len() as i64,
            draft_amount: sum(&drafts),
            draft_count: drafts.len() as i64,
            awaiting_amount: sum(&awaiting),
            awaiting_count: awaiting.len() as i64,
            overdue_amount: sum(&overdue),
            overdue_count: overdue.len() as i64,
            paid_ytd_amount: sum(&paid_this_year),
            paid_ytd_count: paid_this_year.len() as i64,
        };

        // ---- Needs attention ----
        let mut attention: Vec<AttentionItem> = Vec::new();
        let dismissed = self.dismissed()?;

        for inv in &overdue {
            let days = inv.days_past_due;
            attention.push(AttentionItem {
                key: format!("overdue:{}", inv.id),
                kind: "overdue".into(),
                tone: "danger".into(),
                title: format!("{} is overdue", inv.invoice_number),
                detail: format!(
                    "{} · was due {} · {} day{} late",
                    inv.client_name.as_deref().filter(|n| !n.is_empty()).unwrap_or("Client"),
                    friendly_day(&inv.due_date, &today),
                    days_text(days),
                    if days == Some(1) { "" } else { "s" }
                ),
                amount: Some(inv.total),
                invoice_id: Some(inv.id),
                ..Default::default()
            });
        }

        for row in &unbilled_projects {
            let closed = row.project_status == "complete" || row.project_status == "archived";
            let stale = row.age_days.is_some_and(|d| d >= STALE_UNBILLED_DAYS);
            if !closed && !stale {
                continue;
            }
            attention.push(AttentionItem {
                key: format!("unbilled:{}", row.project_id),
                kind: "unbilled".into(),
                tone: "warning".into(),
                title: row.project_name.clone(),
                detail: format!(
                    "{} · unbilled since {}{}",
                    row.client_name,
                    friendly_day(&row.oldest_day, &today),
                    if closed { format!(" · project is marked {}", row.project_status) } else { String::new() }
                ),
                amount: Some(row.amount),
                project_id: Some(row.project_id),
                client_id: Some(row.client_id),
                closed: Some(closed),
                oldest_day: Some(row.oldest_day.clone()),
                ..Default::default()
            });
        }

        for inv in &drafts {
            let created_ms = sqlite_time_ms(Some(&inv.created_at));
            let created =
                if created_ms != 0.0 { local_date(created_ms) } else { js_slice(&inv.issue_date, 10).to_string() };
            let age = days_between(&created, &today);
            // NaN < 3 is false in JS, so an unreadable date still shows
            if age.is_some_and(|a| a < STALE_DRAFT_DAYS) {
                continue;
            }
            attention.push(AttentionItem {
                key: format!("draft:{}", inv.id),
                kind: "draft".into(),
                tone: "neutral".into(),
                title: format!("{} is still a draft", inv.invoice_number),
                detail: format!(
                    "{} · created {} days ago · not marked as sent",
                    inv.client_name.as_deref().filter(|n| !n.is_empty()).unwrap_or("Client"),
                    days_text(age)
                ),
                amount: Some(inv.total),
                invoice_id: Some(inv.id),
                ..Default::default()
            });
        }

        let accidental = self.rows(
            concat!(
                "
    SELECT te.id, te.start_time, te.end_time, te.duration_minutes, p.name AS project_name, p.rate,
           (julianday(te.end_time) - julianday(te.start_time)) * 86400.0 AS wall_seconds
    FROM time_entries te JOIN projects p ON p.id = te.project_id
    WHERE ",
                unbilled_sql!(),
                "
      AND (julianday(te.end_time) - julianday(te.start_time)) * 86400.0 < 60
      AND te.duration_minutes <= 30
  "
            ),
            [],
        )?;
        for e in &accidental {
            let wall: f64 = e.col("wall_seconds");
            let secs = js_round(wall).max(1.0);
            let minutes: f64 = e.col("duration_minutes");
            let rate: f64 = e.col("rate");
            let start: String = e.col("start_time");
            attention.push(AttentionItem {
                key: format!("timer:{}", e.col::<i64>("id")),
                kind: "accidental_timer".into(),
                tone: "neutral".into(),
                title: e.col("project_name"),
                detail: format!(
                    "Timer ran {} second{} on {} and was rounded up to {}m",
                    js_number_to_string(secs),
                    if secs == 1.0 { "" } else { "s" },
                    friendly_day(&local_date(js_date_parse(&start)), &today),
                    js_number_to_string(js_round(minutes))
                ),
                amount: Some((minutes / 60.0) * rate),
                entry_id: Some(e.col("id")),
                ..Default::default()
            });
        }

        attention.extend(self.find_duplicate_clients(&dismissed)?);
        attention.retain(|a| !dismissed.contains(&Value::String(a.key.clone())));

        Ok(BillingOverview { today, pipeline, ready_to_bill, attention, invoices })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_normalize_like_js() {
        assert_eq!(normalize_name("The Harbour Arts Society"), "harbour arts");
        assert_eq!(normalize_name("Smith & Sons, Inc."), "smith sons");
        assert_eq!(normalize_name("  ACME  "), "acme");
        assert_eq!(normalize_name("Café Ltd"), "caf");
    }

    #[test]
    fn friendly_days() {
        assert_eq!(friendly_day("2026-06-12", "2026-11-08"), "Jun 12");
        assert_eq!(friendly_day("2025-06-02", "2026-11-08"), "Jun 2, 2025");
        assert_eq!(friendly_day("garbage", "2026-11-08"), "garbage");
        assert_eq!(friendly_day("2026-13-01", "2026-11-08"), "undefined 1");
    }
}
