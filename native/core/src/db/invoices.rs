//! Invoices, lines and the time linked to them (database.ts "Invoice Queries").

use std::collections::HashSet;

use anyhow::{bail, Context, Result};
use rusqlite::named_params;
use rusqlite::types::Value as SqlValue;

use super::clients::{num_or0, str_or};
use super::{sql_truthy, Db};
use crate::js::js_number_to_string;
use crate::models::*;
use crate::time::year_of;

const INVOICE_SELECT: &str = "
  SELECT i.*, c.name AS client_name, c.company AS client_company,
         c.email AS client_email, c.address AS client_address,
         p.name AS project_name,
         (SELECT COUNT(*) FROM time_entries te WHERE te.invoice_id = i.id) AS entry_count,
         (SELECT COALESCE(SUM(te.duration_minutes), 0) / 60.0 FROM time_entries te WHERE te.invoice_id = i.id) AS entry_hours,
         (SELECT COUNT(DISTINCT it.project_id) FROM invoice_items it WHERE it.invoice_id = i.id AND it.project_id IS NOT NULL) AS project_count
  FROM invoices i
  LEFT JOIN clients c ON i.client_id = c.id
  LEFT JOIN projects p ON i.project_id = p.id
";

const DOUBLE_BILLING: &str =
    "Some of the selected time is already on another invoice or was deleted. Reopen the invoice and try again.";

/// Which projects the linked time spans.
struct Linked {
    project_id: Option<i64>,
    project_count: usize,
}

fn opt_text(v: &Option<String>) -> SqlValue {
    v.clone().map(SqlValue::Text).unwrap_or(SqlValue::Null)
}

fn opt_int(v: &Option<i64>) -> SqlValue {
    v.map(SqlValue::Integer).unwrap_or(SqlValue::Null)
}

impl Db {
    /// All invoices, newest issue date first, with the shown status. Pass a
    /// status (`draft` | `sent` | `overdue` | `paid`) to filter.
    pub fn list_invoices(&self, status: Option<&str>) -> Result<Vec<Invoice>> {
        let today = self.local_today();
        let rows = self.rows(&format!("{INVOICE_SELECT} ORDER BY i.issue_date DESC, i.id DESC"), [])?;
        let all = rows.iter().map(|r| Invoice::from_row(r, &today));
        Ok(match status.filter(|s| !s.is_empty()) {
            Some(s) => all.filter(|i| i.invoice.status == s).collect(),
            None => all.collect(),
        })
    }

    /// One invoice with its lines (`items`) and linked time (`entries`).
    pub fn get_invoice(&self, id: i64) -> Result<Option<Invoice>> {
        let today = self.local_today();
        let Some(row) = self.row(&format!("{INVOICE_SELECT} WHERE i.id = ?"), [id])? else { return Ok(None) };
        let mut invoice = Invoice::from_row(&row, &today);
        invoice.items = Some(
            self.rows("SELECT * FROM invoice_items WHERE invoice_id = ? ORDER BY id", [id])?
                .iter()
                .map(|r| InvoiceItem::from_row(r, &[]))
                .collect(),
        );
        invoice.entries = Some(
            self.rows(
                "
      SELECT te.*, p.name AS project_name, p.color AS project_color, p.rate
      FROM time_entries te LEFT JOIN projects p ON p.id = te.project_id
      WHERE te.invoice_id = ?
      ORDER BY te.start_time
    ",
                [id],
            )?
            .iter()
            .map(InvoiceEntry::from_row)
            .collect(),
        );
        Ok(Some(invoice))
    }

    fn insert_invoice_items(&self, invoice_id: i64, items: &[InvoiceItemInput]) -> Result<()> {
        for it in items {
            self.run(
                "
    INSERT INTO invoice_items (invoice_id, description, quantity, unit_price, total, project_id, source_key, custom_description)
    VALUES (@invoice_id, @description, @quantity, @unit_price, @total, @project_id, @source_key, @custom_description)
  ",
                named_params! {
                    "@invoice_id": invoice_id,
                    "@description": it.description.clone().unwrap_or_default(),
                    "@quantity": num_or0(it.quantity),
                    "@unit_price": num_or0(it.unit_price),
                    "@total": num_or0(it.total),
                    "@project_id": it.project_id.filter(|p| *p != 0),
                    "@source_key": it.source_key.as_deref().filter(|s| !s.is_empty()),
                    "@custom_description": if it.custom_description { 1 } else { 0 },
                },
            )?;
        }
        Ok(())
    }

    /// Make `ids` exactly the entries on this invoice. Entries that dropped
    /// off go back to unbilled. Only unbilled entries (or ones already here)
    /// can be added; anything else fails so the save rolls back instead of
    /// double billing.
    fn set_invoice_entries(&self, invoice_id: i64, ids: &[i64]) -> Result<Linked> {
        let mut seen = HashSet::new();
        let list: Vec<i64> = ids.iter().copied().filter(|i| *i != 0 && seen.insert(*i)).collect();
        let current: Vec<i64> = self
            .rows("SELECT id FROM time_entries WHERE invoice_id = ?", [invoice_id])?
            .iter()
            .map(|r| r.col("id"))
            .collect();
        for id in current {
            if !seen.contains(&id) {
                self.run("UPDATE time_entries SET invoice_id = NULL, is_invoiced = 0 WHERE id = ?", [id])?;
            }
        }
        for id in &list {
            let changed = self.run(
                "
    UPDATE time_entries SET invoice_id = @invoiceId, is_invoiced = 1
    WHERE id = @id AND end_time IS NOT NULL
      AND (invoice_id = @invoiceId OR (invoice_id IS NULL AND is_invoiced = 0))
  ",
                named_params! {"@invoiceId": invoice_id, "@id": id},
            )?;
            if changed == 0 {
                bail!(DOUBLE_BILLING);
            }
        }
        let projects = self.rows("SELECT DISTINCT project_id FROM time_entries WHERE invoice_id = ?", [invoice_id])?;
        Ok(Linked {
            project_id: if projects.len() == 1 { projects[0].col("project_id") } else { None },
            project_count: projects.len(),
        })
    }

    /// Point the invoice at its project when all linked time shares one.
    fn sync_invoice_project(&self, invoice_id: i64, linked: Linked) -> Result<()> {
        if linked.project_count == 0 {
            return Ok(());
        }
        self.run(
            "UPDATE invoices SET project_id = ? WHERE id = ?",
            rusqlite::params![linked.project_id, invoice_id],
        )?;
        Ok(())
    }

    /// The next free invoice number and the counter value after it.
    fn next_invoice_number(&self) -> Result<(String, f64)> {
        let settings = self.get_settings()?;
        let prefix = settings.get("invoice_prefix").unwrap_or("INV-").to_string();
        let mut n = if settings.contains("invoice_next_number") { settings.invoice_next_number() } else { 0.0 };
        if n == 0.0 || n.is_nan() {
            n = 1001.0;
        }
        while self.row("SELECT 1 FROM invoices WHERE invoice_number = ?", [format!("{prefix}{}", js_number_to_string(n))])?.is_some()
        {
            n += 1.0;
        }
        Ok((format!("{prefix}{}", js_number_to_string(n)), n + 1.0))
    }

    /// Create an invoice with the next number, its lines, and linked time.
    pub fn create_invoice(&self, data: &InvoiceInput) -> Result<Invoice> {
        let id = self.tx(|| {
            let (number, next) = self.next_invoice_number()?;
            let status = data.status.as_deref().filter(|s| *s == "sent" || *s == "paid").unwrap_or("draft");
            let today = self.local_today();
            let tax_year = match data.tax_year {
                Some(y) if y != 0 => y,
                _ => year_of(Some(&data.issue_date), &self.clock),
            };
            let invoice_id = self.insert(
                "
      INSERT INTO invoices (
        project_id, client_id, invoice_number, issue_date, due_date, status,
        subtotal, tax_rate, total, notes, tax_year, currency,
        gst_hst_applicable, gst_hst_number, gst_hst_rate, gst_hst_amount,
        sent_at, line_style, payment_date, payment_method
      ) VALUES (
        @project_id, @client_id, @invoice_number, @issue_date, @due_date, @status,
        @subtotal, @tax_rate, @total, @notes, @tax_year, @currency,
        @gst_hst_applicable, @gst_hst_number, @gst_hst_rate, @gst_hst_amount,
        @sent_at, @line_style, @payment_date, @payment_method
      )
    ",
                named_params! {
                    "@project_id": data.project_id,
                    "@client_id": data.client_id,
                    "@invoice_number": number,
                    "@issue_date": data.issue_date,
                    "@due_date": data.due_date,
                    "@status": status,
                    "@subtotal": num_or0(data.subtotal),
                    "@tax_rate": num_or0(data.tax_rate),
                    "@total": num_or0(data.total),
                    "@notes": str_or(data.notes.as_deref(), ""),
                    "@tax_year": tax_year,
                    "@currency": str_or(data.currency.as_deref(), "CAD"),
                    "@gst_hst_applicable": if data.gst_hst_applicable { 1 } else { 0 },
                    "@gst_hst_number": data.gst_hst_number.as_deref().filter(|s| !s.is_empty()),
                    "@gst_hst_rate": num_or0(data.gst_hst_rate),
                    "@gst_hst_amount": num_or0(data.gst_hst_amount),
                    "@sent_at": if status == "draft" { None } else { Some(today.as_str()) },
                    "@line_style": data.line_style.as_deref().filter(|s| !s.is_empty()),
                    "@payment_date": if status == "paid" { Some(str_or(data.payment_date.as_deref(), &today)) } else { None },
                    "@payment_method": if status == "paid" { data.payment_method.as_deref().filter(|s| !s.is_empty()) } else { None },
                },
            )?;
            self.insert_invoice_items(invoice_id, &data.items)?;
            if let Some(ids) = data.entry_ids.as_ref().filter(|ids| !ids.is_empty()) {
                let linked = self.set_invoice_entries(invoice_id, ids)?;
                self.sync_invoice_project(invoice_id, linked)?;
            }
            self.run(
                "INSERT OR REPLACE INTO settings (key, value) VALUES ('invoice_next_number', ?)",
                [js_number_to_string(next)],
            )?;
            Ok(invoice_id)
        })?;
        self.get_invoice(id)?.context("invoice not found after insert")
    }

    /// Edit an invoice. Setting `paid` fills in today's payment date and
    /// `sent` today's sent date when missing; `overdue` is stored as `sent`.
    pub fn update_invoice(&self, id: i64, data: &InvoicePatch) -> Result<Option<Invoice>> {
        self.tx(|| {
            let Some(existing) = self.row("SELECT * FROM invoices WHERE id = ?", [id])? else { return Ok(()) };
            let mut inv = data.clone();
            if inv.issue_date.as_deref().is_some_and(|d| !d.is_empty()) && inv.tax_year.is_none() {
                inv.tax_year = Some(Some(year_of(inv.issue_date.as_deref(), &self.clock)));
            }
            if inv.status.as_deref() == Some("overdue") {
                inv.status = Some("sent".into());
            }
            if inv.status.as_deref() == Some("paid") && inv.payment_date.is_none() && !sql_truthy(existing.get("payment_date")) {
                inv.payment_date = Some(Some(self.local_today()));
            }
            if inv.status.as_deref() == Some("sent") && !sql_truthy(existing.get("sent_at")) && inv.sent_at.is_none() {
                inv.sent_at = Some(Some(self.local_today()));
            }

            let mut sets: Vec<(&str, SqlValue)> = Vec::new();
            let text = |v: &String| SqlValue::Text(v.clone());
            if let Some(v) = &inv.project_id {
                sets.push(("project_id", opt_int(v)));
            }
            if let Some(v) = inv.client_id {
                sets.push(("client_id", SqlValue::Integer(v)));
            }
            if let Some(v) = &inv.issue_date {
                sets.push(("issue_date", text(v)));
            }
            if let Some(v) = &inv.due_date {
                sets.push(("due_date", text(v)));
            }
            if let Some(v) = &inv.status {
                sets.push(("status", text(v)));
            }
            if let Some(v) = inv.subtotal {
                sets.push(("subtotal", SqlValue::Real(v)));
            }
            if let Some(v) = inv.tax_rate {
                sets.push(("tax_rate", SqlValue::Real(v)));
            }
            if let Some(v) = inv.total {
                sets.push(("total", SqlValue::Real(v)));
            }
            if let Some(v) = &inv.notes {
                sets.push(("notes", text(v)));
            }
            if let Some(v) = &inv.pdf_path {
                sets.push(("pdf_path", opt_text(v)));
            }
            if let Some(v) = &inv.tax_year {
                sets.push(("tax_year", opt_int(v)));
            }
            if let Some(v) = &inv.payment_date {
                sets.push(("payment_date", opt_text(v)));
            }
            if let Some(v) = &inv.payment_method {
                sets.push(("payment_method", opt_text(v)));
            }
            if let Some(v) = &inv.currency {
                sets.push(("currency", text(v)));
            }
            if let Some(v) = inv.gst_hst_applicable {
                sets.push(("gst_hst_applicable", SqlValue::Integer(v)));
            }
            if let Some(v) = &inv.gst_hst_number {
                sets.push(("gst_hst_number", opt_text(v)));
            }
            if let Some(v) = inv.gst_hst_rate {
                sets.push(("gst_hst_rate", SqlValue::Real(v)));
            }
            if let Some(v) = inv.gst_hst_amount {
                sets.push(("gst_hst_amount", SqlValue::Real(v)));
            }
            if let Some(v) = &inv.sent_at {
                sets.push(("sent_at", opt_text(v)));
            }
            if let Some(v) = &inv.line_style {
                sets.push(("line_style", opt_text(v)));
            }
            self.update_columns("invoices", id, &sets)?;

            if let Some(items) = &inv.items {
                self.run("DELETE FROM invoice_items WHERE invoice_id = ?", [id])?;
                self.insert_invoice_items(id, items)?;
            }
            if let Some(ids) = &inv.entry_ids {
                let linked = self.set_invoice_entries(id, ids)?;
                self.sync_invoice_project(id, linked)?;
            }
            Ok(())
        })?;
        self.get_invoice(id)
    }

    /// Delete an invoice. Its time goes back to unbilled so it can be billed again.
    pub fn delete_invoice(&self, id: i64) -> Result<DeleteInvoiceResult> {
        self.tx(|| {
            let r = self
                .row(
                    "
      SELECT COUNT(*) AS n, COALESCE(SUM(duration_minutes), 0) AS mins
      FROM time_entries WHERE invoice_id = ?
    ",
                    [id],
                )?
                .context("count query returned nothing")?;
            self.run("UPDATE time_entries SET invoice_id = NULL, is_invoiced = 0 WHERE invoice_id = ?", [id])?;
            self.run("DELETE FROM invoices WHERE id = ?", [id])?;
            Ok(DeleteInvoiceResult {
                released_entries: r.col("n"),
                released_hours: r.col::<f64>("mins") / 60.0,
            })
        })
    }

    /// Mark drafts as sent (others are skipped). `sent_date` defaults to
    /// today. Returns how many changed.
    pub fn mark_invoices_sent(&self, ids: &[i64], sent_date: Option<&str>) -> Result<i64> {
        let today = self.local_today();
        let date = str_or(sent_date, &today).to_string();
        self.tx(|| {
            let mut n = 0;
            for id in ids {
                n += self.run(
                    "
    UPDATE invoices SET status = 'sent', sent_at = COALESCE(sent_at, ?)
    WHERE id = ? AND status = 'draft'
  ",
                    rusqlite::params![date, id],
                )?;
            }
            Ok(n as i64)
        })
    }

    /// Record payment on invoices not already paid. Returns how many changed.
    pub fn mark_invoices_paid(&self, ids: &[i64], payment_date: Option<&str>, payment_method: Option<&str>) -> Result<i64> {
        let today = self.local_today();
        let date = str_or(payment_date, &today).to_string();
        let method = payment_method.filter(|m| !m.is_empty());
        self.tx(|| {
            let mut n = 0;
            for id in ids {
                n += self.run(
                    "
    UPDATE invoices
    SET status = 'paid', payment_date = @date, payment_method = @method,
        sent_at = COALESCE(sent_at, issue_date)
    WHERE id = @id AND status != 'paid'
  ",
                    named_params! {"@id": id, "@date": date, "@method": method},
                )?;
            }
            Ok(n as i64)
        })
    }

    /// Undo a payment recorded by mistake.
    pub fn mark_invoice_unpaid(&self, id: i64) -> Result<Option<Invoice>> {
        self.run(
            "
    UPDATE invoices SET status = 'sent', payment_date = NULL, payment_method = NULL,
      sent_at = COALESCE(sent_at, issue_date)
    WHERE id = ?
  ",
            [id],
        )?;
        self.get_invoice(id)
    }
}
