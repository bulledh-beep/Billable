//! Dashboard, reports and unbilled-time queries.

use anyhow::{Context, Result};
use rusqlite::named_params;

use super::{unbilled_sql, Db};
use crate::models::*;
use crate::time::{iso_string, local_ms_from_parts, local_range_to_utc, local_start_of_day, local_ymd_dow};

impl Db {
    fn hours_since(&self, from_ms: f64) -> Result<f64> {
        Ok(self
            .row(
                "
    SELECT COALESCE(SUM(duration_minutes) / 60.0, 0) AS hours
    FROM time_entries WHERE end_time IS NOT NULL AND start_time >= ?
  ",
                [iso_string(from_ms as i64)],
            )?
            .map(|r| r.col("hours"))
            .unwrap_or(0.0))
    }

    /// `dashboard:stats`. Weeks start on Monday, like the Time page.
    pub fn get_dashboard_stats(&self) -> Result<DashboardStats> {
        let now = self.clock.now_ms();
        let (y, m, d, dow) = local_ymd_dow(now);
        let start_of_today = local_ms_from_parts(y as i64, m as i64 - 1, d as i64, 0, 0, 0, 0);
        let start_of_week = local_ms_from_parts(y as i64, m as i64 - 1, d as i64 - ((dow as i64 + 6) % 7), 0, 0, 0, 0);
        let start_of_month = local_ms_from_parts(y as i64, m as i64 - 1, 1, 0, 0, 0, 0);

        let unbilled = self.get_unbilled_totals()?;
        let today = self.local_today();
        let inv = self
            .row(
                "
    SELECT
      COALESCE(SUM(CASE WHEN status IN ('sent', 'overdue') THEN total END), 0) AS outstanding,
      COALESCE(SUM(CASE WHEN status IN ('sent', 'overdue') AND due_date < @today THEN total END), 0) AS overdue,
      COALESCE(SUM(CASE WHEN status = 'paid' THEN total END), 0) AS paid,
      COALESCE(SUM(CASE WHEN status = 'paid' AND substr(COALESCE(payment_date, issue_date), 1, 4) = @year THEN total END), 0) AS paid_ytd
    FROM invoices
  ",
                named_params! {"@today": today, "@year": crate::js::js_slice(&today, 4)},
            )?
            .context("invoice totals returned nothing")?;

        Ok(DashboardStats {
            hours_today: self.hours_since(start_of_today)?,
            hours_this_week: self.hours_since(start_of_week)?,
            hours_this_month: self.hours_since(start_of_month)?,
            unbilled_hours: unbilled.hours,
            unbilled_amount: unbilled.amount,
            outstanding_total: inv.col("outstanding"),
            overdue_total: inv.col("overdue"),
            paid_total: inv.col("paid"),
            paid_ytd: inv.col("paid_ytd"),
        })
    }

    /// The latest finished entries (`dashboard:recent`; the app uses 10).
    pub fn get_recent_entries(&self, limit: i64) -> Result<Vec<EntryRow>> {
        let today = self.local_today();
        let rows = self.rows(
            &format!("{} WHERE te.end_time IS NOT NULL ORDER BY te.start_time DESC LIMIT ?", super::entries::ENTRY_SELECT),
            [limit],
        )?;
        Ok(rows.iter().map(|r| EntryRow::from_row(r, &today)).collect())
    }

    /// Hours and billable value per project between two local dates (end included).
    pub fn hours_by_project(&self, start_date: &str, end_date: &str) -> Result<Vec<ProjectHours>> {
        let (from, to) = local_range_to_utc(start_date, end_date)?;
        let rows = self.rows(
            "
    SELECT p.id, p.name, p.color, c.name AS client_name,
      COALESCE(SUM(te.duration_minutes) / 60.0, 0) AS hours,
      COALESCE(SUM(CASE WHEN te.is_billable = 1 THEN te.duration_minutes * p.rate / 60.0 END), 0) AS value
    FROM time_entries te
    JOIN projects p ON te.project_id = p.id
    LEFT JOIN clients c ON c.id = p.client_id
    WHERE te.end_time IS NOT NULL AND te.start_time >= ? AND te.start_time < ?
    GROUP BY p.id
    ORDER BY hours DESC
  ",
            [from, to],
        )?;
        Ok(rows.iter().map(|r| ProjectHours::from_row(r, &[])).collect())
    }

    /// Hours and billable value per client between two local dates (end included).
    pub fn hours_by_client(&self, start_date: &str, end_date: &str) -> Result<Vec<ClientHours>> {
        let (from, to) = local_range_to_utc(start_date, end_date)?;
        let rows = self.rows(
            "
    SELECT c.id, c.name,
      COALESCE(SUM(te.duration_minutes) / 60.0, 0) AS hours,
      COALESCE(SUM(CASE WHEN te.is_billable = 1 THEN te.duration_minutes * p.rate / 60.0 END), 0) AS value
    FROM time_entries te
    JOIN projects p ON te.project_id = p.id
    JOIN clients c ON p.client_id = c.id
    WHERE te.end_time IS NOT NULL AND te.start_time >= ? AND te.start_time < ?
    GROUP BY c.id
    ORDER BY hours DESC
  ",
            [from, to],
        )?;
        Ok(rows.iter().map(|r| ClientHours::from_row(r, &[])).collect())
    }

    /// Invoiced by issue month and paid by payment month, `YYYY-MM-DD` bounds
    /// inclusive. Drafts are left out.
    pub fn earnings_by_month(&self, start_date: &str, end_date: &str) -> Result<Vec<MonthEarnings>> {
        let rows = self.rows(
            "
    SELECT month, COALESCE(SUM(paid), 0) AS paid, COALESCE(SUM(invoiced), 0) AS invoiced
    FROM (
      SELECT substr(issue_date, 1, 7) AS month, 0 AS paid, total AS invoiced
      FROM invoices WHERE status != 'draft' AND issue_date >= @s AND issue_date <= @e
      UNION ALL
      SELECT substr(COALESCE(payment_date, issue_date), 1, 7) AS month, total AS paid, 0 AS invoiced
      FROM invoices WHERE status = 'paid'
        AND COALESCE(payment_date, issue_date) >= @s AND COALESCE(payment_date, issue_date) <= @e
    )
    GROUP BY month
    ORDER BY month
  ",
            named_params! {"@s": start_date, "@e": end_date},
        )?;
        Ok(rows.iter().map(|r| MonthEarnings::from_row(r, &[])).collect())
    }

    /// A project's unbilled, finished, billable entries, oldest first.
    pub fn get_unbilled_entries(&self, project_id: i64) -> Result<Vec<UnbilledEntry>> {
        let rows = self.rows(
            concat!(
                "
    SELECT te.*, p.name as project_name, p.rate
    FROM time_entries te
    JOIN projects p ON te.project_id = p.id
    WHERE te.project_id = ? AND ",
                unbilled_sql!(),
                "
    ORDER BY te.start_time
  "
            ),
            [project_id],
        )?;
        Ok(rows.iter().map(UnbilledEntry::from_row).collect())
    }

    /// Hours logged today (local day).
    pub fn get_today_hours(&self) -> Result<f64> {
        self.hours_since(local_start_of_day(self.clock.now_ms()))
    }

    /// Unbilled hours and their value at current rates.
    pub fn get_unbilled_totals(&self) -> Result<UnbilledTotals> {
        let r = self
            .row(
                concat!(
                    "
    SELECT COALESCE(SUM(te.duration_minutes) / 60.0, 0) AS hours,
           COALESCE(SUM(te.duration_minutes * p.rate / 60.0), 0) AS amount
    FROM time_entries te JOIN projects p ON p.id = te.project_id
    WHERE ",
                    unbilled_sql!(),
                    "
  "
                ),
                [],
            )?
            .context("unbilled totals returned nothing")?;
        Ok(UnbilledTotals { hours: r.col("hours"), amount: r.col("amount") })
    }

    /// Active projects with time, most recently used first (the app uses 5).
    pub fn get_recent_projects(&self, limit: i64) -> Result<Vec<RecentProject>> {
        let rows = self.rows(
            "
    SELECT DISTINCT p.id, p.name, p.client_id, p.color, p.status, p.rate,
           c.name as client_name,
           MAX(te.start_time) as last_used
    FROM time_entries te
    JOIN projects p ON te.project_id = p.id
    LEFT JOIN clients c ON p.client_id = c.id
    WHERE p.status = 'active'
    GROUP BY p.id
    ORDER BY last_used DESC
    LIMIT ?
  ",
            [limit],
        )?;
        Ok(rows.iter().map(|r| RecentProject::from_row(r, &[])).collect())
    }

    /// A client's unbilled entries across projects, oldest first.
    pub fn get_unbilled_entries_by_client(&self, client_id: i64) -> Result<Vec<UnbilledEntry>> {
        let rows = self.rows(
            concat!(
                "
    SELECT te.*, p.name as project_name, p.rate
    FROM time_entries te
    JOIN projects p ON te.project_id = p.id
    WHERE p.client_id = ? AND ",
                unbilled_sql!(),
                "
    ORDER BY te.start_time
  "
            ),
            [client_id],
        )?;
        Ok(rows.iter().map(UnbilledEntry::from_row).collect())
    }

    /// Unbilled entries for several projects, by project name then time.
    pub fn get_unbilled_entries_for_projects(&self, project_ids: &[i64]) -> Result<Vec<UnbilledEntry>> {
        if project_ids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = vec!["?"; project_ids.len()].join(",");
        let sql = format!(
            "
    SELECT te.*, p.name as project_name, p.rate
    FROM time_entries te
    JOIN projects p ON te.project_id = p.id
    WHERE te.project_id IN ({placeholders}) AND {}
    ORDER BY p.name, te.start_time
  ",
            unbilled_sql!()
        );
        let rows = self.rows(&sql, rusqlite::params_from_iter(project_ids))?;
        Ok(rows.iter().map(UnbilledEntry::from_row).collect())
    }
}
