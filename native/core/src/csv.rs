//! The CSV text the Electron export handlers wrote (ipc.ts
//! `reports:export-csv`, `tax:export-invoices-csv`, `tax:export-expenses-csv`):
//! every cell quoted with `"` doubled, lines joined by `\n`, no trailing newline.

use anyhow::Result;
use rusqlite::types::Value as SqlValue;

use crate::js::js_number_to_string;
use crate::Db;

/// `"${String(v ?? '').replace(/"/g, '""')}"`
fn quote(cell: &str) -> String {
    format!("\"{}\"", cell.replace('"', "\"\""))
}

/// `reports:export-csv`: the header names as-is, then one quoted row per record.
pub fn csv_text(headers: &[String], rows: &[Vec<String>]) -> String {
    let mut lines = vec![headers.join(",")];
    for row in rows {
        let cells: Vec<String> = (0..headers.len()).map(|i| quote(row.get(i).map(String::as_str).unwrap_or(""))).collect();
        lines.push(cells.join(","));
    }
    lines.join("\n")
}

/// `String(value ?? '')` for a value as better-sqlite3 returned it.
fn js_cell(v: Option<&SqlValue>) -> String {
    match v {
        None | Some(SqlValue::Null) => String::new(),
        Some(SqlValue::Integer(i)) => js_number_to_string(*i as f64),
        Some(SqlValue::Real(f)) => js_number_to_string(*f),
        Some(SqlValue::Text(s)) => s.clone(),
        Some(SqlValue::Blob(b)) => String::from_utf8_lossy(b).into_owned(),
    }
}

const INVOICE_COLS: [&str; 15] = [
    "invoice_number",
    "issue_date",
    "due_date",
    "tax_year",
    "client_name",
    "status",
    "subtotal",
    "gst_hst_applicable",
    "gst_hst_rate",
    "gst_hst_amount",
    "tax_rate",
    "total",
    "currency",
    "payment_date",
    "payment_method",
];

const EXPENSE_COLS: [&str; 6] = ["date", "category", "description", "amount", "tax_year", "receipt_note"];

fn table_csv(cols: &[&str], rows: &[crate::db::RowMap]) -> String {
    let mut lines = vec![cols.join(",")];
    for r in rows {
        lines.push(cols.iter().map(|c| quote(&js_cell(r.get(c)))).collect::<Vec<_>>().join(","));
    }
    lines.join("\n")
}

impl Db {
    /// `tax:export-invoices-csv`: the year's invoices (`listInvoicesByYear`),
    /// or `None` when there are none (the handler then wrote nothing).
    pub fn invoices_csv(&self, tax_year: i64) -> Result<Option<String>> {
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
        Ok((!rows.is_empty()).then(|| table_csv(&INVOICE_COLS, &rows)))
    }

    /// `tax:export-expenses-csv`: the year's expenses (`listExpenses`), or `None`.
    pub fn expenses_csv(&self, tax_year: i64) -> Result<Option<String>> {
        let rows = if tax_year != 0 {
            self.rows("SELECT * FROM expenses WHERE tax_year = ? ORDER BY date DESC, id DESC", [tax_year])?
        } else {
            self.rows("SELECT * FROM expenses ORDER BY date DESC, id DESC", [])?
        };
        Ok((!rows.is_empty()).then(|| table_csv(&EXPENSE_COLS, &rows)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_like_the_app() {
        let csv = csv_text(&["a".into(), "b".into()], &[vec!["x \"y\"".into(), "1,2\n3".into()], vec!["".into(), "z".into()]]);
        assert_eq!(csv, "a,b\n\"x \"\"y\"\"\",\"1,2\n3\"\n\"\",\"z\"");
    }

    #[test]
    fn cells_print_like_js() {
        assert_eq!(js_cell(Some(&SqlValue::Real(100.0))), "100");
        assert_eq!(js_cell(Some(&SqlValue::Real(12.5))), "12.5");
        assert_eq!(js_cell(Some(&SqlValue::Integer(1))), "1");
        assert_eq!(js_cell(Some(&SqlValue::Null)), "");
        assert_eq!(js_cell(None), "");
    }

    #[test]
    fn empty_years_export_nothing() {
        let dir = std::env::temp_dir().join(format!("billable-csv-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = Db::open(dir.join("billable.db"), crate::Clock::system()).unwrap();
        assert_eq!(db.invoices_csv(1999).unwrap(), None);
        assert_eq!(db.expenses_csv(1999).unwrap(), None);
        drop(db);
        let _ = std::fs::remove_dir_all(dir);
    }
}
