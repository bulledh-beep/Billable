//! Clients and projects (database.ts "Client Queries" / "Project Queries").

use anyhow::{anyhow, bail, Context, Result};
use rusqlite::types::Value as SqlValue;
use rusqlite::{named_params, params};

use super::{unbilled_sql, Db};
use crate::js::{js_round, str_truthy};
use crate::models::*;

macro_rules! client_aggregates {
    () => {
        concat!(
            "
  (SELECT COUNT(*) FROM projects p WHERE p.client_id = c.id) AS project_count,
  (SELECT COUNT(*) FROM projects p WHERE p.client_id = c.id AND p.status = 'active') AS active_project_count,
  (SELECT COALESCE(SUM(te.duration_minutes * p.rate / 60.0), 0)
     FROM time_entries te JOIN projects p ON p.id = te.project_id
     WHERE p.client_id = c.id AND ",
            unbilled_sql!(),
            ") AS unbilled_amount,
  (SELECT COALESCE(SUM(te.duration_minutes) / 60.0, 0)
     FROM time_entries te JOIN projects p ON p.id = te.project_id
     WHERE p.client_id = c.id AND ",
            unbilled_sql!(),
            ") AS unbilled_hours,
  (SELECT MIN(te.start_time)
     FROM time_entries te JOIN projects p ON p.id = te.project_id
     WHERE p.client_id = c.id AND ",
            unbilled_sql!(),
            ") AS oldest_unbilled,
  (SELECT COALESCE(SUM(i.total), 0) FROM invoices i
     WHERE i.client_id = c.id AND i.status IN ('sent', 'overdue')) AS outstanding_amount,
  (SELECT COALESCE(SUM(i.total), 0) FROM invoices i
     WHERE i.client_id = c.id AND i.status IN ('sent', 'overdue') AND i.due_date < @today) AS overdue_amount,
  (SELECT COALESCE(SUM(i.total), 0) FROM invoices i
     WHERE i.client_id = c.id AND i.status = 'paid') AS paid_amount,
  (SELECT COUNT(*) FROM invoices i WHERE i.client_id = c.id) AS invoice_count,
  (SELECT MAX(te.start_time)
     FROM time_entries te JOIN projects p ON p.id = te.project_id
     WHERE p.client_id = c.id) AS last_activity
"
        )
    };
}

macro_rules! project_select {
    () => {
        concat!(
            "
  SELECT p.*, c.name AS client_name, c.company AS client_company,
    (SELECT COALESCE(SUM(te.duration_minutes), 0) / 60.0 FROM time_entries te
       WHERE te.project_id = p.id AND te.end_time IS NOT NULL) AS total_hours,
    (SELECT COALESCE(SUM(te.duration_minutes), 0) / 60.0 FROM time_entries te
       WHERE te.project_id = p.id AND ",
            unbilled_sql!(),
            ") AS unbilled_hours,
    (SELECT COUNT(*) FROM time_entries te
       WHERE te.project_id = p.id AND ",
            unbilled_sql!(),
            ") AS unbilled_entries,
    (SELECT MIN(te.start_time) FROM time_entries te
       WHERE te.project_id = p.id AND ",
            unbilled_sql!(),
            ") AS oldest_unbilled,
    (SELECT COALESCE(SUM(it.total), 0) FROM invoice_items it
       WHERE it.project_id = p.id) AS invoiced_amount,
    (SELECT COALESCE(SUM(it.total), 0) FROM invoice_items it JOIN invoices i ON i.id = it.invoice_id
       WHERE it.project_id = p.id AND i.status = 'paid') AS paid_amount,
    (SELECT MAX(te.start_time) FROM time_entries te WHERE te.project_id = p.id) AS last_activity
  FROM projects p
  LEFT JOIN clients c ON p.client_id = c.id
"
        )
    };
}

/// `Number(x) || 0` for an optional number.
pub(crate) fn num_or0(x: Option<f64>) -> f64 {
    match x {
        Some(v) if v != 0.0 && !v.is_nan() => v,
        _ => 0.0,
    }
}

/// `s || fallback` for an optional string.
pub(crate) fn str_or<'a>(s: Option<&'a str>, fallback: &'a str) -> &'a str {
    if str_truthy(s) {
        s.unwrap_or(fallback)
    } else {
        fallback
    }
}

fn project_from_row(r: &super::RowMap) -> Project {
    let mut p = Project::from_row(r, &[]);
    p.unbilled_amount = js_round(p.unbilled_hours * p.rate * 100.0) / 100.0;
    p.billed_total = p.invoiced_amount;
    p
}

impl Db {
    // ============ Clients ============

    /// All clients by name, with money totals.
    pub fn list_clients(&self) -> Result<Vec<Client>> {
        let today = self.local_today();
        let rows = self.rows(
            concat!("SELECT c.*, ", client_aggregates!(), " FROM clients c ORDER BY c.name COLLATE NOCASE"),
            named_params! {"@today": today},
        )?;
        Ok(rows.iter().map(|r| Client::from_row(r, &[])).collect())
    }

    pub fn get_client(&self, id: i64) -> Result<Option<Client>> {
        let today = self.local_today();
        let row = self.row(
            concat!("SELECT c.*, ", client_aggregates!(), " FROM clients c WHERE c.id = @id"),
            named_params! {"@today": today, "@id": id},
        )?;
        Ok(row.map(|r| Client::from_row(&r, &[])))
    }

    /// Fold one client into another: projects and invoices move to the
    /// target, then the source is removed.
    pub fn merge_clients(&self, source_id: i64, target_id: i64) -> Result<MergeResult> {
        if source_id == 0 || target_id == 0 || source_id == target_id {
            bail!("Pick two different clients to merge");
        }
        self.tx(|| {
            let source = self.row("SELECT id FROM clients WHERE id = ?", [source_id])?;
            let target = self.row("SELECT id FROM clients WHERE id = ?", [target_id])?;
            if source.is_none() || target.is_none() {
                bail!("Client not found");
            }
            let moved_projects = self.run("UPDATE projects SET client_id = ? WHERE client_id = ?", [target_id, source_id])?;
            let moved_invoices = self.run("UPDATE invoices SET client_id = ? WHERE client_id = ?", [target_id, source_id])?;
            self.run("DELETE FROM clients WHERE id = ?", [source_id])?;
            Ok(MergeResult { moved_projects: moved_projects as i64, moved_invoices: moved_invoices as i64 })
        })
    }

    pub fn create_client(&self, data: &ClientInput) -> Result<Client> {
        let id = self.insert(
            "
    INSERT INTO clients (name, company, email, address, default_rate, currency)
    VALUES (@name, @company, @email, @address, @default_rate, @currency)
  ",
            named_params! {
                "@name": data.name,
                "@company": str_or(data.company.as_deref(), ""),
                "@email": str_or(data.email.as_deref(), ""),
                "@address": str_or(data.address.as_deref(), ""),
                "@default_rate": num_or0(data.default_rate),
                "@currency": str_or(data.currency.as_deref(), "CAD"),
            },
        )?;
        self.get_client(id)?.context("client not found after insert")
    }

    /// Edit a client. A changed `default_rate` cascades to this client's
    /// projects still on the old rate (unbilled time only; invoice lines keep
    /// their prices). `None` if the client doesn't exist.
    pub fn update_client(&self, id: i64, data: &ClientPatch) -> Result<Option<ClientUpdate>> {
        let Some(existing) = self.row("SELECT * FROM clients WHERE id = ?", [id])? else { return Ok(None) };

        let mut sets: Vec<(&str, SqlValue)> = Vec::new();
        let text = |v: &Option<String>| v.clone().map(SqlValue::Text);
        if let Some(v) = text(&data.name) {
            sets.push(("name", v));
        }
        if let Some(v) = text(&data.company) {
            sets.push(("company", v));
        }
        if let Some(v) = text(&data.email) {
            sets.push(("email", v));
        }
        if let Some(v) = text(&data.address) {
            sets.push(("address", v));
        }
        if let Some(v) = data.default_rate {
            sets.push(("default_rate", SqlValue::Real(v)));
        }
        if let Some(v) = text(&data.currency) {
            sets.push(("currency", v));
        }
        self.update_columns("clients", id, &sets)?;

        let mut cascaded = 0;
        if let Some(rate) = data.default_rate {
            let old: Option<f64> = existing.col("default_rate");
            if rate != old.unwrap_or(0.0) {
                let old_value = existing.get("default_rate").cloned().unwrap_or(SqlValue::Null);
                cascaded = self.run(
                    "
      UPDATE projects
      SET rate = ?
      WHERE client_id = ? AND rate = ?
    ",
                    params![rate, id, old_value],
                )?;
            }
        }
        Ok(self.get_client(id)?.map(|client| ClientUpdate { client, cascaded_projects: cascaded as i64 }))
    }

    pub fn delete_client(&self, id: i64) -> Result<()> {
        self.run("DELETE FROM clients WHERE id = ?", [id])?;
        Ok(())
    }

    /// `UPDATE table SET a = ?, b = ? WHERE id = ?` for the given columns.
    pub(crate) fn update_columns(&self, table: &str, id: i64, sets: &[(&str, SqlValue)]) -> Result<()> {
        if sets.is_empty() {
            return Ok(());
        }
        let assignments: Vec<String> = sets.iter().map(|(c, _)| format!("{c} = ?")).collect();
        let sql = format!("UPDATE {table} SET {} WHERE id = ?", assignments.join(", "));
        let mut values: Vec<SqlValue> = sets.iter().map(|(_, v)| v.clone()).collect();
        values.push(SqlValue::Integer(id));
        self.run(&sql, rusqlite::params_from_iter(values))?;
        Ok(())
    }

    // ============ Projects ============

    /// Projects, newest first; only one client's when `client_id` is given.
    pub fn list_projects(&self, client_id: Option<i64>) -> Result<Vec<Project>> {
        let rows = match client_id.filter(|c| *c != 0) {
            Some(c) => self.rows(concat!(project_select!(), " WHERE p.client_id = ? ORDER BY p.created_at DESC"), [c])?,
            None => self.rows(concat!(project_select!(), " ORDER BY p.created_at DESC"), [])?,
        };
        Ok(rows.iter().map(project_from_row).collect())
    }

    pub fn get_project(&self, id: i64) -> Result<Option<Project>> {
        Ok(self.row(concat!(project_select!(), " WHERE p.id = ?"), [id])?.as_ref().map(project_from_row))
    }

    pub fn create_project(&self, data: &ProjectInput) -> Result<Project> {
        let id = self.insert(
            "
    INSERT INTO projects (client_id, name, description, rate, status, color)
    VALUES (@client_id, @name, @description, @rate, @status, @color)
  ",
            named_params! {
                "@client_id": data.client_id,
                "@name": data.name,
                "@description": str_or(data.description.as_deref(), ""),
                "@rate": num_or0(data.rate),
                "@status": str_or(data.status.as_deref(), "active"),
                "@color": str_or(data.color.as_deref(), "#F5A623"),
            },
        )?;
        self.get_project(id)?.ok_or_else(|| anyhow!("project not found after insert"))
    }

    pub fn update_project(&self, id: i64, data: &ProjectPatch) -> Result<Option<Project>> {
        let mut sets: Vec<(&str, SqlValue)> = Vec::new();
        if let Some(v) = data.client_id {
            sets.push(("client_id", SqlValue::Integer(v)));
        }
        if let Some(v) = &data.name {
            sets.push(("name", SqlValue::Text(v.clone())));
        }
        if let Some(v) = &data.description {
            sets.push(("description", SqlValue::Text(v.clone())));
        }
        if let Some(v) = data.rate {
            sets.push(("rate", SqlValue::Real(v)));
        }
        if let Some(v) = &data.status {
            sets.push(("status", SqlValue::Text(v.clone())));
        }
        if let Some(v) = &data.color {
            sets.push(("color", SqlValue::Text(v.clone())));
        }
        self.update_columns("projects", id, &sets)?;
        self.get_project(id)
    }

    pub fn delete_project(&self, id: i64) -> Result<()> {
        self.run("DELETE FROM projects WHERE id = ?", [id])?;
        Ok(())
    }
}
