//! Appointment-setting commissions and commission invoices.

use std::collections::BTreeSet;

use anyhow::{bail, Context, Result};
use rusqlite::named_params;
use rusqlite::types::Value as SqlValue;

use super::clients::str_or;
use super::Db;
use crate::js::js_round;
use crate::models::*;

/// The commission rules, run on every create/update:
///
/// - Solar: kW × $50.
/// - Roofing: ≤ $20,000 → $250; ≥ $30,000 → $500; in between → needs
///   review (0) unless a manual override (≥ 0) is given.
pub fn compute_commission(
    job_type: Option<&str>,
    system_size_kw: Option<f64>,
    contract_amount: Option<f64>,
    manual_override: Option<f64>,
) -> CommissionCalc {
    let has_override = manual_override.is_some_and(|v| !v.is_nan() && v >= 0.0);
    if job_type == Some("roofing") {
        let amt = super::clients::num_or0(contract_amount);
        if amt <= 20000.0 {
            return CommissionCalc { calculated: 250.0, needs_review: 0 };
        }
        if amt >= 30000.0 {
            return CommissionCalc { calculated: 500.0, needs_review: 0 };
        }
        return CommissionCalc { calculated: 0.0, needs_review: if has_override { 0 } else { 1 } };
    }
    let kw = super::clients::num_or0(system_size_kw);
    let amount = js_round(kw * 50.0 * 100.0) / 100.0;
    CommissionCalc { calculated: if amount > 0.0 { amount } else { 0.0 }, needs_review: 0 }
}

/// `Math.max(0, Number(x))` for a manual override (NaN stays NaN → stored NULL).
fn clamp_override(v: Option<f64>) -> Option<f64> {
    v.map(|x| if x.is_nan() || x > 0.0 { x } else { 0.0 })
}

/// Effective payout for totals: override wins; review gap → 0.
fn commission_effective(c: &Commission) -> f64 {
    if let Some(o) = c.manual_override {
        return o;
    }
    if c.needs_review != 0 {
        return 0.0;
    }
    if c.calculated_commission.is_nan() {
        0.0
    } else {
        c.calculated_commission
    }
}

fn opt_text(v: &Option<String>) -> SqlValue {
    v.clone().map(SqlValue::Text).unwrap_or(SqlValue::Null)
}

impl Db {
    /// All jobs, latest appointment first.
    pub fn list_commissions(&self) -> Result<Vec<Commission>> {
        Ok(self
            .rows("SELECT * FROM commissions ORDER BY appointment_date DESC, id DESC", [])?
            .iter()
            .map(|r| Commission::from_row(r, &[]))
            .collect())
    }

    pub fn get_commission(&self, id: i64) -> Result<Option<Commission>> {
        Ok(self.row("SELECT * FROM commissions WHERE id = ?", [id])?.map(|r| Commission::from_row(&r, &[])))
    }

    pub fn create_commission(&self, data: &CommissionInput) -> Result<Commission> {
        let calc = compute_commission(
            data.job_type.as_deref(),
            data.system_size_kw,
            data.contract_amount,
            data.manual_override,
        );
        let id = self.insert(
            "
    INSERT INTO commissions (
      client_name, job_type, appointment_date, closer_name, status, payment_status,
      system_size_kw, contract_amount, calculated_commission, manual_override, needs_review, notes
    ) VALUES (
      @client_name, @job_type, @appointment_date, @closer_name, @status, @payment_status,
      @system_size_kw, @contract_amount, @calculated_commission, @manual_override, @needs_review, @notes
    )
  ",
            named_params! {
                "@client_name": data.client_name,
                "@job_type": str_or(data.job_type.as_deref(), "solar"),
                "@appointment_date": data.appointment_date.as_deref().filter(|s| !s.is_empty()),
                "@closer_name": str_or(data.closer_name.as_deref(), ""),
                "@status": str_or(data.status.as_deref(), "appointment_set"),
                "@payment_status": str_or(data.payment_status.as_deref(), "unpaid"),
                "@system_size_kw": data.system_size_kw,
                "@contract_amount": data.contract_amount,
                "@calculated_commission": calc.calculated,
                "@manual_override": clamp_override(data.manual_override),
                "@needs_review": calc.needs_review,
                "@notes": str_or(data.notes.as_deref(), ""),
            },
        )?;
        self.get_commission(id)?.context("commission not found after insert")
    }

    /// Full edit; merged over the stored job and recalculated. `None` if missing.
    pub fn update_commission(&self, id: i64, data: &CommissionPatch) -> Result<Option<Commission>> {
        let Some(existing) = self.get_commission(id)? else { return Ok(None) };
        // { ...existing, ...data }
        let client_name = data.client_name.clone().unwrap_or(existing.client_name);
        let job_type = data.job_type.clone().unwrap_or(existing.job_type);
        let appointment_date = data.appointment_date.clone().unwrap_or(existing.appointment_date);
        let closer_name = data.closer_name.clone().unwrap_or(Some(existing.closer_name));
        let status = data.status.clone().unwrap_or(existing.status);
        let payment_status = data.payment_status.clone().unwrap_or(existing.payment_status);
        let system_size_kw = data.system_size_kw.unwrap_or(existing.system_size_kw);
        let contract_amount = data.contract_amount.unwrap_or(existing.contract_amount);
        let manual_override = data.manual_override.unwrap_or(existing.manual_override);
        let notes = data.notes.clone().unwrap_or(Some(existing.notes));

        let calc = compute_commission(Some(&job_type), system_size_kw, contract_amount, manual_override);
        self.run(
            "
    UPDATE commissions SET
      client_name = @client_name,
      job_type = @job_type,
      appointment_date = @appointment_date,
      closer_name = @closer_name,
      status = @status,
      payment_status = @payment_status,
      system_size_kw = @system_size_kw,
      contract_amount = @contract_amount,
      calculated_commission = @calculated_commission,
      manual_override = @manual_override,
      needs_review = @needs_review,
      notes = @notes,
      updated_at = datetime('now')
    WHERE id = @id
  ",
            named_params! {
                "@id": id,
                "@client_name": client_name,
                "@job_type": job_type,
                "@appointment_date": appointment_date.filter(|s| !s.is_empty()),
                "@closer_name": str_or(closer_name.as_deref(), ""),
                "@status": status,
                "@payment_status": payment_status,
                "@system_size_kw": system_size_kw,
                "@contract_amount": contract_amount,
                "@calculated_commission": calc.calculated,
                "@manual_override": clamp_override(manual_override),
                "@needs_review": calc.needs_review,
                "@notes": str_or(notes.as_deref(), ""),
            },
        )?;
        self.get_commission(id)
    }

    pub fn delete_commission(&self, id: i64) -> Result<()> {
        self.run("DELETE FROM commissions WHERE id = ?", [id])?;
        Ok(())
    }

    /// Lifecycle-only update for quick/bulk actions (status, payment and
    /// invoice tracking); doesn't recompute the amount.
    pub fn patch_commission(&self, id: i64, patch: &CommissionStatusPatch) -> Result<Option<Commission>> {
        let mut sets: Vec<(&str, SqlValue)> = Vec::new();
        if let Some(v) = &patch.status {
            sets.push(("status", opt_text(v)));
        }
        if let Some(v) = &patch.payment_status {
            sets.push(("payment_status", opt_text(v)));
        }
        if let Some(v) = &patch.invoice_status {
            sets.push(("invoice_status", opt_text(v)));
        }
        if let Some(v) = &patch.invoiced_at {
            sets.push(("invoiced_at", opt_text(v)));
        }
        if let Some(v) = &patch.paid_at {
            sets.push(("paid_at", opt_text(v)));
        }
        if let Some(v) = &patch.invoice_id {
            sets.push(("invoice_id", v.map(SqlValue::Integer).unwrap_or(SqlValue::Null)));
        }
        if sets.is_empty() {
            return self.get_commission(id);
        }
        let assignments: Vec<String> = sets.iter().map(|(c, _)| format!("{c} = ?")).collect();
        let sql =
            format!("UPDATE commissions SET {}, updated_at = datetime('now') WHERE id = ?", assignments.join(", "));
        let mut values: Vec<SqlValue> = sets.into_iter().map(|(_, v)| v).collect();
        values.push(SqlValue::Integer(id));
        self.run(&sql, rusqlite::params_from_iter(values))?;
        self.get_commission(id)
    }

    /// [`Db::patch_commission`] on several jobs at once; returns all jobs.
    pub fn bulk_patch_commissions(&self, ids: &[i64], patch: &CommissionStatusPatch) -> Result<Vec<Commission>> {
        self.tx(|| {
            for id in ids {
                self.patch_commission(*id, patch)?;
            }
            Ok(())
        })?;
        self.list_commissions()
    }

    // ============ Commission invoices ============

    pub fn list_commission_invoices(&self) -> Result<Vec<CommissionInvoice>> {
        Ok(self
            .rows("SELECT * FROM commission_invoices ORDER BY created_at DESC, id DESC", [])?
            .iter()
            .map(|r| CommissionInvoice::from_row(r, &[]))
            .collect())
    }

    /// One commission invoice with its jobs.
    pub fn get_commission_invoice(&self, id: i64) -> Result<Option<CommissionInvoiceDetail>> {
        let Some(row) = self.row("SELECT * FROM commission_invoices WHERE id = ?", [id])? else { return Ok(None) };
        let jobs = self
            .rows("SELECT * FROM commissions WHERE invoice_id = ? ORDER BY job_type, appointment_date", [id])?
            .iter()
            .map(|r| Commission::from_row(r, &[]))
            .collect();
        Ok(Some(CommissionInvoiceDetail { invoice: CommissionInvoice::from_row(&row, &[]), jobs }))
    }

    /// Invoice a set of jobs: totals their payouts, numbers it `COMM-<1000+id>`
    /// and stamps the jobs as invoiced.
    pub fn create_commission_invoice(&self, data: &CommissionInvoiceInput) -> Result<Option<CommissionInvoiceDetail>> {
        let mut jobs = Vec::new();
        for id in &data.job_ids {
            if let Some(job) = self.get_commission(*id)? {
                jobs.push(job);
            }
        }
        if jobs.is_empty() {
            bail!("No jobs selected for the invoice");
        }
        let total = jobs.iter().fold(0.0, |s, c| s + commission_effective(c));
        let types: BTreeSet<&str> = jobs.iter().map(|j| j.job_type.as_str()).collect();
        let category = match data.category.as_deref() {
            Some(c) if !c.is_empty() && c != "both" => c.to_string(),
            _ => {
                if types.len() > 1 {
                    "mixed".into()
                } else if types.contains("solar") {
                    "solar".into()
                } else {
                    "roofing".into()
                }
            }
        };

        let new_id = self.tx(|| {
            let id = self.insert(
                "
      INSERT INTO commission_invoices (invoice_number, category, date_from, date_to, status, job_count, total, notes)
      VALUES ('PENDING', @category, @date_from, @date_to, 'draft', @job_count, @total, @notes)
    ",
                named_params! {
                    "@category": category,
                    "@date_from": data.date_from.as_deref().filter(|s| !s.is_empty()),
                    "@date_to": data.date_to.as_deref().filter(|s| !s.is_empty()),
                    "@job_count": jobs.len() as i64,
                    "@total": total,
                    "@notes": str_or(data.notes.as_deref(), ""),
                },
            )?;
            let number = format!("COMM-{:04}", 1000 + id);
            self.run("UPDATE commission_invoices SET invoice_number = ? WHERE id = ?", rusqlite::params![number, id])?;
            let now = self.local_today();
            for j in &jobs {
                self.run(
                    "
      UPDATE commissions SET invoice_id = ?, invoice_status = 'invoiced', invoiced_at = ?, updated_at = datetime('now')
      WHERE id = ?
    ",
                    rusqlite::params![id, now, j.id],
                )?;
            }
            Ok(id)
        })?;
        self.get_commission_invoice(new_id)
    }

    /// Set a commission invoice's status. `paid` also marks every job on it paid.
    pub fn update_commission_invoice_status(&self, id: i64, status: &str) -> Result<Option<CommissionInvoiceDetail>> {
        if status == "paid" {
            let now = self.local_today();
            self.tx(|| {
                self.run(
                    "UPDATE commission_invoices SET status = 'paid', paid_at = ?, updated_at = datetime('now') WHERE id = ?",
                    rusqlite::params![now, id],
                )?;
                self.run(
                    "
        UPDATE commissions SET payment_status = 'paid', status = 'paid', invoice_status = 'paid', paid_at = ?, updated_at = datetime('now')
        WHERE invoice_id = ?
      ",
                    rusqlite::params![now, id],
                )?;
                Ok(())
            })?;
        } else {
            self.run(
                "UPDATE commission_invoices SET status = ?, updated_at = datetime('now') WHERE id = ?",
                rusqlite::params![status, id],
            )?;
        }
        self.get_commission_invoice(id)
    }

    /// Delete a commission invoice; its unpaid jobs go back to "not invoiced".
    pub fn delete_commission_invoice(&self, id: i64) -> Result<()> {
        self.tx(|| {
            self.run(
                "
      UPDATE commissions SET invoice_id = NULL, invoice_status = 'not_invoiced', invoiced_at = NULL, updated_at = datetime('now')
      WHERE invoice_id = ? AND payment_status != 'paid'
    ",
                [id],
            )?;
            // Paid jobs keep their paid state but lose the deleted invoice link
            self.run("UPDATE commissions SET invoice_id = NULL WHERE invoice_id = ?", [id])?;
            self.run("DELETE FROM commission_invoices WHERE id = ?", [id])?;
            Ok(())
        })
    }
}
