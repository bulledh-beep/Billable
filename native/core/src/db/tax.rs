//! Tax overview, invoices by tax year, and expenses.

use anyhow::{bail, Context, Result};
use rusqlite::named_params;
use rusqlite::types::Value as SqlValue;

use super::clients::{num_or0, str_or};
use super::{json_to_sql, Db};
use crate::models::*;
use crate::time::year_of;

impl Db {
    /// Income, GST/HST, expenses and monthly income for one tax year.
    pub fn get_tax_overview(&self, tax_year: i64) -> Result<TaxOverview> {
        let income = self
            .row(
                "
    SELECT
      COALESCE(SUM(total), 0) as total_invoiced,
      COALESCE(SUM(CASE WHEN status = 'paid' THEN total ELSE 0 END), 0) as total_paid,
      COALESCE(SUM(CASE WHEN status IN ('sent', 'overdue') THEN total ELSE 0 END), 0) as total_outstanding,
      COALESCE(SUM(CASE WHEN status = 'paid' AND gst_hst_applicable = 1 THEN gst_hst_amount ELSE 0 END), 0) as gst_collected_paid,
      COALESCE(SUM(CASE WHEN gst_hst_applicable = 1 THEN gst_hst_amount ELSE 0 END), 0) as gst_collected_total,
      COUNT(*) as invoice_count,
      COALESCE(SUM(CASE WHEN status = 'paid' THEN 1 ELSE 0 END), 0) as paid_count
    FROM invoices
    WHERE tax_year = ?
  ",
                [tax_year],
            )?
            .context("income query returned nothing")?;

        let expenses_by_category: Vec<ExpenseBucket> = self
            .rows(
                "
    SELECT category, COALESCE(SUM(amount), 0) as total, COUNT(*) as count
    FROM expenses
    WHERE tax_year = ?
    GROUP BY category
    ORDER BY total DESC
  ",
                [tax_year],
            )?
            .iter()
            .map(|r| ExpenseBucket::from_row(r, &[]))
            .collect();
        let total_expenses = expenses_by_category.iter().fold(0.0, |s, r| s + r.total);

        let monthly_income = self
            .rows(
                "
    SELECT
      strftime('%Y-%m', COALESCE(payment_date, issue_date)) as month,
      COALESCE(SUM(CASE WHEN status = 'paid' THEN total ELSE 0 END), 0) as paid,
      COALESCE(SUM(total), 0) as invoiced
    FROM invoices
    WHERE tax_year = ?
    GROUP BY month
    ORDER BY month
  ",
                [tax_year],
            )?
            .iter()
            .map(|r| MonthlyIncome::from_row(r, &[]))
            .collect();

        Ok(TaxOverview {
            tax_year,
            total_invoiced: income.col("total_invoiced"),
            total_paid: income.col("total_paid"),
            total_outstanding: income.col("total_outstanding"),
            gst_collected_paid: income.col("gst_collected_paid"),
            gst_collected_total: income.col("gst_collected_total"),
            invoice_count: income.col("invoice_count"),
            paid_count: income.col("paid_count"),
            expenses_by_category,
            total_expenses,
            monthly_income,
        })
    }

    /// Invoices in a tax year by issue date (stored status), for exports.
    pub fn list_invoices_by_year(&self, tax_year: i64) -> Result<Vec<YearInvoice>> {
        let rows = self.rows(
            "
    SELECT i.*, c.name as client_name, c.company as client_company
    FROM invoices i
    LEFT JOIN clients c ON i.client_id = c.id
    WHERE i.tax_year = ?
    ORDER BY i.issue_date
  ",
            [tax_year],
        )?;
        Ok(rows
            .iter()
            .map(|r| YearInvoice {
                invoice: InvoiceRow::from_row(r, &["client_name", "client_company"]),
                client_name: r.col("client_name"),
                client_company: r.col("client_company"),
            })
            .collect())
    }

    // ============ Expenses ============

    /// Expenses, newest first; one tax year's when given.
    pub fn list_expenses(&self, tax_year: Option<i64>) -> Result<Vec<Expense>> {
        let rows = match tax_year.filter(|y| *y != 0) {
            Some(y) => self.rows("SELECT * FROM expenses WHERE tax_year = ? ORDER BY date DESC, id DESC", [y])?,
            None => self.rows("SELECT * FROM expenses ORDER BY date DESC, id DESC", [])?,
        };
        Ok(rows.iter().map(|r| Expense::from_row(r, &[])).collect())
    }

    pub fn get_expense(&self, id: i64) -> Result<Option<Expense>> {
        Ok(self.row("SELECT * FROM expenses WHERE id = ?", [id])?.map(|r| Expense::from_row(&r, &[])))
    }

    pub fn create_expense(&self, data: &ExpenseInput) -> Result<Expense> {
        let today = self.local_today();
        let date = str_or(data.date.as_deref(), &today).to_string();
        let tax_year = match data.tax_year {
            Some(y) if y != 0 => y,
            _ => year_of(Some(&date), &self.clock),
        };
        let id = self.insert(
            "
    INSERT INTO expenses (date, category, description, amount, tax_year, receipt_note, receipt_id)
    VALUES (@date, @category, @description, @amount, @tax_year, @receipt_note, @receipt_id)
  ",
            named_params! {
                "@date": date,
                "@category": str_or(data.category.as_deref(), "other"),
                "@description": str_or(data.description.as_deref(), ""),
                "@amount": num_or0(data.amount),
                "@tax_year": tax_year,
                "@receipt_note": str_or(data.receipt_note.as_deref(), ""),
                "@receipt_id": data.receipt_id.filter(|r| *r != 0),
            },
        )?;
        self.get_expense(id)?.context("expense not found after insert")
    }

    /// Edit an expense. A new date without a tax year re-derives the year.
    pub fn update_expense(&self, id: i64, data: &ExpensePatch) -> Result<Option<Expense>> {
        let mut sets: Vec<(String, SqlValue)> = Vec::new();
        let text = |v: &String| SqlValue::Text(v.clone());
        if let Some(v) = &data.date {
            sets.push(("date".into(), text(v)));
        }
        if let Some(v) = &data.category {
            sets.push(("category".into(), text(v)));
        }
        if let Some(v) = &data.description {
            sets.push(("description".into(), text(v)));
        }
        if let Some(v) = data.amount {
            sets.push(("amount".into(), SqlValue::Real(v)));
        }
        if let Some(v) = &data.tax_year {
            sets.push(("tax_year".into(), v.map(SqlValue::Integer).unwrap_or(SqlValue::Null)));
        }
        if let Some(v) = &data.receipt_note {
            sets.push(("receipt_note".into(), text(v)));
        }
        if let Some(v) = &data.receipt_id {
            sets.push(("receipt_id".into(), v.map(SqlValue::Integer).unwrap_or(SqlValue::Null)));
        }
        for (k, v) in &data.extra {
            if k == "id" || k == "created_at" {
                continue;
            }
            if !k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                bail!("no such column: {k}");
            }
            sets.push((k.clone(), json_to_sql(v)?));
        }
        if sets.is_empty() {
            return self.get_expense(id);
        }
        // Re-derive tax_year if the date changed
        let tax_year_falsy = matches!(data.tax_year, None | Some(None) | Some(Some(0)));
        if data.date.as_deref().is_some_and(|d| !d.is_empty()) && tax_year_falsy {
            let year = year_of(data.date.as_deref(), &self.clock);
            sets.retain(|(k, _)| k != "tax_year");
            sets.push(("tax_year".into(), SqlValue::Integer(year)));
        }
        let refs: Vec<(&str, SqlValue)> = sets.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
        self.update_columns("expenses", id, &refs)?;
        self.get_expense(id)
    }

    pub fn delete_expense(&self, id: i64) -> Result<()> {
        self.run("DELETE FROM expenses WHERE id = ?", [id])?;
        Ok(())
    }
}
