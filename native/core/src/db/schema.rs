//! Tables, migrations, backfills and defaults (database.ts `createTables`,
//! `runMigrations`, `backfillInvoiceLinks`, `seedDefaults`). The SQL text is
//! byte-for-byte the Electron app's, so `sqlite_master` matches too.

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;
use rusqlite::params;

use super::Db;
use crate::time::{iso_string, sqlite_time_ms};

impl Db {
    pub(super) fn create_tables(&self) -> Result<()> {
        self.conn.execute_batch(CREATE_TABLES)?;
        Ok(())
    }

    /// `ALTER TABLE … ADD COLUMN` unless the column exists.
    fn add_column_if_missing(&self, table: &str, column: &str, def: &str) -> Result<()> {
        let cols = self.rows(&format!("PRAGMA table_info({table})"), [])?;
        if !cols.iter().any(|c| c.col::<String>("name") == column) {
            self.conn.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {column} {def}"))?;
        }
        Ok(())
    }

    /// Idempotent migrations, safe to re-run on every launch.
    pub(super) fn run_migrations(&self) -> Result<()> {
        // Timer pause/resume state
        self.add_column_if_missing("time_entries", "paused_at", "TEXT")?;
        self.add_column_if_missing("time_entries", "active_since", "TEXT")?;
        self.run(
            "
    UPDATE time_entries
    SET active_since = start_time
    WHERE end_time IS NULL AND paused_at IS NULL AND active_since IS NULL
  ",
            [],
        )?;

        // Tax & expense tracking
        self.conn.execute_batch(TAX_TABLES)?;
        self.run("INSERT OR IGNORE INTO tax_settings (id, business_name) VALUES (1, '')", [])?;
        self.add_column_if_missing("invoices", "tax_year", "INTEGER")?;
        self.add_column_if_missing("invoices", "payment_date", "TEXT")?;
        self.add_column_if_missing("invoices", "payment_method", "TEXT")?;
        self.add_column_if_missing("invoices", "currency", "TEXT DEFAULT 'CAD'")?;
        self.add_column_if_missing("invoices", "gst_hst_applicable", "INTEGER DEFAULT 0")?;
        self.add_column_if_missing("invoices", "gst_hst_number", "TEXT")?;
        self.add_column_if_missing("invoices", "gst_hst_rate", "REAL DEFAULT 0")?;
        self.add_column_if_missing("invoices", "gst_hst_amount", "REAL DEFAULT 0")?;
        self.run(
            "UPDATE invoices SET tax_year = CAST(strftime('%Y', issue_date) AS INTEGER) WHERE tax_year IS NULL",
            [],
        )?;

        // Commission tracking
        self.conn.execute_batch(COMMISSION_TABLES)?;
        self.add_column_if_missing("commissions", "invoice_id", "INTEGER")?;
        self.add_column_if_missing("commissions", "invoice_status", "TEXT DEFAULT 'not_invoiced'")?;
        self.add_column_if_missing("commissions", "invoiced_at", "TEXT")?;
        self.add_column_if_missing("commissions", "paid_at", "TEXT")?;

        // Billing links: every time entry knows which invoice it is on
        self.add_column_if_missing("time_entries", "invoice_id", "INTEGER REFERENCES invoices(id) ON DELETE SET NULL")?;
        self.add_column_if_missing("invoices", "sent_at", "TEXT")?;
        self.add_column_if_missing("invoices", "line_style", "TEXT")?;
        self.add_column_if_missing("invoice_items", "project_id", "INTEGER")?;
        self.add_column_if_missing("invoice_items", "source_key", "TEXT")?;
        self.add_column_if_missing("invoice_items", "custom_description", "INTEGER DEFAULT 0")?;
        self.conn.execute_batch("CREATE INDEX IF NOT EXISTS idx_time_entries_invoice ON time_entries(invoice_id)")?;
        self.conn.execute_batch("CREATE INDEX IF NOT EXISTS idx_time_entries_project ON time_entries(project_id)")?;

        // Changes made on the phone through Content HQ
        self.conn.execute_batch(PHONE_ACTIONS_TABLE)?;
        self.backfill_invoice_links()
    }

    pub(super) fn seed_defaults(&self) -> Result<()> {
        let defaults: [(&str, &str); 13] = [
            ("business_name", ""),
            ("business_email", ""),
            ("business_address", ""),
            ("business_logo", ""),
            ("tax_id", ""),
            ("default_currency", "USD"),
            ("default_rate", "100"),
            ("invoice_prefix", "INV-"),
            ("invoice_next_number", "1001"),
            ("payment_methods", r#"[{"name":"e-Transfer","email":""}]"#),
            ("default_payment_method", "e-Transfer"),
            ("time_rounding", "none"),
            ("theme", "dark"),
        ];
        for (k, v) in defaults {
            self.run("INSERT OR IGNORE INTO settings (key, value) VALUES (?, ?)", params![k, v])?;
        }
        Ok(())
    }

    fn mark_links_done(&self, note: &str) -> Result<()> {
        let value = format!("{} {}", self.clock.now_iso(), note);
        self.run("INSERT OR REPLACE INTO settings (key, value) VALUES ('billing_links_v1', ?)", [value])?;
        Ok(())
    }

    /// One-time upgrade from the old yes/no "invoiced" flag to real links.
    /// Each previously-invoiced entry goes to its project's invoice (the first
    /// issued on or after the work ended when there are several). The
    /// database is backed up first; if that fails the upgrade waits for the
    /// next launch.
    fn backfill_invoice_links(&self) -> Result<()> {
        if self.row("SELECT value FROM settings WHERE key = 'billing_links_v1'", [])?.is_some() {
            return Ok(());
        }
        let n: i64 = self
            .row(
                "
    SELECT (SELECT COUNT(*) FROM invoices) + (SELECT COUNT(*) FROM time_entries WHERE is_invoiced = 1) AS n
  ",
                [],
            )?
            .map(|r| r.col("n"))
            .unwrap_or(0);
        if n == 0 {
            return self.mark_links_done("empty");
        }

        let Some(backup) = self.backup_database("before-billing-upgrade") else { return Ok(()) };

        self.tx(|| {
            struct Flagged {
                id: i64,
                project_id: i64,
                created_at: Option<String>,
                end_time: Option<String>,
                client_id: i64,
                project_name: String,
            }
            struct Inv {
                id: i64,
                client_id: i64,
                project_id: Option<i64>,
                created_at: Option<String>,
            }
            let flagged: Vec<Flagged> = self
                .rows(
                    "
      SELECT te.id, te.project_id, te.created_at, te.end_time, p.client_id, p.name AS project_name
      FROM time_entries te JOIN projects p ON p.id = te.project_id
      WHERE te.is_invoiced = 1 AND te.invoice_id IS NULL
    ",
                    [],
                )?
                .iter()
                .map(|r| Flagged {
                    id: r.col("id"),
                    project_id: r.col("project_id"),
                    created_at: r.col("created_at"),
                    end_time: r.col("end_time"),
                    client_id: r.col("client_id"),
                    project_name: r.col("project_name"),
                })
                .collect();
            let invoices: Vec<Inv> = self
                .rows("SELECT id, client_id, project_id, created_at FROM invoices", [])?
                .iter()
                .map(|r| Inv {
                    id: r.col("id"),
                    client_id: r.col("client_id"),
                    project_id: r.col("project_id"),
                    created_at: r.col("created_at"),
                })
                .collect();

            // The old multi-project builder stored no project on the invoice
            // but prefixed every line with "[Project name]".
            let mut coverage: HashMap<i64, Vec<String>> = HashMap::new();
            for inv in invoices.iter().filter(|i| i.project_id.is_none()) {
                let lines = self.rows("SELECT description FROM invoice_items WHERE invoice_id = ?", [inv.id])?;
                coverage.insert(inv.id, lines.iter().map(|r| r.col::<String>("description")).collect());
            }
            let covers = |inv: &Inv, e: &Flagged| {
                inv.project_id == Some(e.project_id)
                    || (inv.project_id.is_none()
                        && coverage
                            .get(&inv.id)
                            .is_some_and(|ds| ds.iter().any(|d| d.starts_with(&format!("[{}]", e.project_name)))))
            };

            for e in &flagged {
                let candidates: Vec<&Inv> =
                    invoices.iter().filter(|i| i.client_id == e.client_id && covers(i, e)).collect();
                if candidates.is_empty() {
                    continue;
                }
                // The old code flagged time when an invoice was created, so the
                // right invoice is the first one made after this entry ended.
                let ready = sqlite_time_ms(e.created_at.as_deref()).max(sqlite_time_ms(e.end_time.as_deref()));
                let mut after: Vec<&&Inv> = candidates
                    .iter()
                    .filter(|i| sqlite_time_ms(i.created_at.as_deref()) >= ready - 1000.0)
                    .collect();
                after.sort_by(|a, b| {
                    sqlite_time_ms(a.created_at.as_deref())
                        .partial_cmp(&sqlite_time_ms(b.created_at.as_deref()))
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                let target = after.first().map(|i| **i).or(if candidates.len() == 1 { Some(candidates[0]) } else { None });
                if let Some(t) = target {
                    self.run("UPDATE time_entries SET invoice_id = ? WHERE id = ?", params![t.id, e.id])?;
                }
            }

            // Older line items belong to their invoice's single project
            self.run(
                "
      UPDATE invoice_items
      SET project_id = (SELECT project_id FROM invoices WHERE invoices.id = invoice_items.invoice_id)
      WHERE project_id IS NULL
    ",
                [],
            )?;
            // Multi-project lines name their project in a "[Project]" prefix
            let orphan_lines = self.rows(
                "
      SELECT it.id, it.description, i.client_id FROM invoice_items it
      JOIN invoices i ON i.id = it.invoice_id
      WHERE it.project_id IS NULL AND i.project_id IS NULL
    ",
                [],
            )?;
            for line in &orphan_lines {
                let desc: String = line.col("description");
                let projects =
                    self.rows("SELECT id, name FROM projects WHERE client_id = ?", [line.col::<i64>("client_id")])?;
                if let Some(p) = projects.iter().find(|p| desc.starts_with(&format!("[{}]", p.col::<String>("name")))) {
                    self.run(
                        "UPDATE invoice_items SET project_id = ? WHERE id = ?",
                        params![p.col::<i64>("id"), line.col::<i64>("id")],
                    )?;
                }
            }

            // "Overdue" is now worked out from the due date, so store it as sent
            self.run("UPDATE invoices SET status = 'sent' WHERE status = 'overdue'", [])?;

            let name = backup.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            self.mark_links_done(&format!("backup={name}"))
        })
    }

    /// Consistent copy of the open database next to it
    /// (`billable-backup-<tag>-<stamp>.db`, works with WAL). Returns the
    /// backup path, or `None` if it failed (database.ts `backupDatabase`).
    pub fn backup_database(&self, tag: &str) -> Option<PathBuf> {
        let dir = self.path.parent()?.to_path_buf();
        let stamp: String = iso_string(self.clock.now_ms())
            .chars()
            .map(|c| if c == ':' || c == '.' { '-' } else { c })
            .take(19)
            .collect();
        let file = dir.join(format!("billable-backup-{tag}-{stamp}.db"));
        match self.vacuum_into(&file) {
            Ok(()) => Some(file),
            Err(err) => {
                eprintln!("Database backup failed: {err}");
                None
            }
        }
    }

    fn vacuum_into(&self, file: &std::path::Path) -> Result<()> {
        let quoted = file.to_string_lossy().replace('\'', "''");
        self.conn.execute_batch(&format!("VACUUM INTO '{quoted}'"))?;
        Ok(())
    }

    /// Replace this profile's database with `source` (Settings → Import, the
    /// `settings:import-db` handler minus the dialog and relaunch). Checks
    /// it's a SQLite file, backs the current data up first
    /// (`before-import`), closes, and copies it in. Reopen with [`Db::open`]
    /// afterwards; stop the timer before calling, as the original does.
    /// Returns the backup path.
    pub fn import_from(self, source: impl AsRef<std::path::Path>) -> Result<PathBuf> {
        use std::io::Read;
        let source = source.as_ref();
        let mut header = [0u8; 16];
        let n = std::fs::File::open(source)?.read(&mut header)?;
        if n < 15 || &header[..15] != b"SQLite format 3" {
            anyhow::bail!("That file is not a Billable database");
        }
        let Some(backup) = self.backup_database("before-import") else {
            anyhow::bail!("Couldn't back up your current data first, so nothing was replaced.");
        };
        let target = self.path.clone();
        self.close()?;
        for ext in ["-wal", "-shm"] {
            let sib = PathBuf::from(format!("{}{ext}", target.display()));
            if sib.exists() {
                std::fs::remove_file(sib)?;
            }
        }
        std::fs::copy(source, &target)?;
        Ok(backup)
    }

    /// Export the whole database to `file` as one consistent file, replacing
    /// any file already there (Settings → Export).
    pub fn export_to(&self, file: impl AsRef<std::path::Path>) -> Result<()> {
        let file = file.as_ref();
        if file.exists() {
            std::fs::remove_file(file)?;
        }
        self.vacuum_into(file)
    }
}

// Copied verbatim from database.ts (whitespace matters: SQLite stores it).
const TAX_TABLES: &str = r#"
    CREATE TABLE IF NOT EXISTS tax_settings (
      id INTEGER PRIMARY KEY CHECK (id = 1),
      business_name TEXT DEFAULT '',
      business_address TEXT DEFAULT '',
      gst_hst_number TEXT DEFAULT '',
      gst_hst_registered INTEGER DEFAULT 0,
      province TEXT DEFAULT '',
      fiscal_year_start TEXT DEFAULT '01-01',
      default_tax_rate REAL DEFAULT 0,
      income_tax_bracket REAL DEFAULT 25,
      currency TEXT DEFAULT 'CAD',
      updated_at TEXT DEFAULT (datetime('now'))
    );

    CREATE TABLE IF NOT EXISTS expenses (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      date TEXT NOT NULL,
      category TEXT NOT NULL DEFAULT 'other',
      description TEXT DEFAULT '',
      amount REAL DEFAULT 0,
      tax_year INTEGER NOT NULL,
      receipt_note TEXT DEFAULT '',
      receipt_id INTEGER,
      created_at TEXT DEFAULT (datetime('now'))
    );
  "#;

const COMMISSION_TABLES: &str = r#"
    CREATE TABLE IF NOT EXISTS commissions (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      client_name TEXT NOT NULL,
      job_type TEXT NOT NULL DEFAULT 'solar' CHECK(job_type IN ('solar', 'roofing')),
      appointment_date TEXT,
      closer_name TEXT DEFAULT '',
      status TEXT NOT NULL DEFAULT 'appointment_set'
        CHECK(status IN ('appointment_set','appointment_attended','closed_waiting','paid','lost','cancelled','needs_review')),
      payment_status TEXT NOT NULL DEFAULT 'unpaid'
        CHECK(payment_status IN ('unpaid','pending','paid')),
      system_size_kw REAL,
      contract_amount REAL,
      calculated_commission REAL DEFAULT 0,
      manual_override REAL,
      needs_review INTEGER DEFAULT 0,
      notes TEXT DEFAULT '',
      created_at TEXT DEFAULT (datetime('now')),
      updated_at TEXT DEFAULT (datetime('now'))
    );

    CREATE TABLE IF NOT EXISTS commission_invoices (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      invoice_number TEXT NOT NULL,
      category TEXT NOT NULL DEFAULT 'mixed' CHECK(category IN ('solar','roofing','mixed')),
      date_from TEXT,
      date_to TEXT,
      status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','sent','paid','cancelled')),
      job_count INTEGER DEFAULT 0,
      total REAL DEFAULT 0,
      notes TEXT DEFAULT '',
      created_at TEXT DEFAULT (datetime('now')),
      updated_at TEXT DEFAULT (datetime('now')),
      paid_at TEXT
    );
  "#;

const PHONE_ACTIONS_TABLE: &str = r#"
    CREATE TABLE IF NOT EXISTS phone_actions (
      id TEXT PRIMARY KEY,
      kind TEXT NOT NULL,
      status TEXT NOT NULL CHECK(status IN ('applied', 'rejected')),
      summary TEXT NOT NULL DEFAULT '',
      message TEXT,
      created_billable_id TEXT,
      occurred_at TEXT,
      applied_at TEXT NOT NULL,
      acked INTEGER NOT NULL DEFAULT 0
    );
    CREATE INDEX IF NOT EXISTS idx_phone_actions_acked ON phone_actions(acked);
  "#;

const CREATE_TABLES: &str = r#"
    CREATE TABLE IF NOT EXISTS clients (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      name TEXT NOT NULL,
      company TEXT DEFAULT '',
      email TEXT DEFAULT '',
      address TEXT DEFAULT '',
      default_rate REAL DEFAULT 0,
      currency TEXT DEFAULT 'USD',
      created_at TEXT DEFAULT (datetime('now'))
    );

    CREATE TABLE IF NOT EXISTS projects (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      client_id INTEGER NOT NULL,
      name TEXT NOT NULL,
      description TEXT DEFAULT '',
      rate REAL DEFAULT 0,
      status TEXT DEFAULT 'active' CHECK(status IN ('active', 'paused', 'complete', 'archived')),
      color TEXT DEFAULT '#F5A623',
      created_at TEXT DEFAULT (datetime('now')),
      FOREIGN KEY (client_id) REFERENCES clients(id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS time_entries (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      project_id INTEGER NOT NULL,
      description TEXT DEFAULT '',
      start_time TEXT NOT NULL,
      end_time TEXT,
      duration_minutes REAL DEFAULT 0,
      paused_at TEXT,
      active_since TEXT,
      is_billable INTEGER DEFAULT 1,
      is_invoiced INTEGER DEFAULT 0,
      created_at TEXT DEFAULT (datetime('now')),
      FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS invoices (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      project_id INTEGER,
      client_id INTEGER NOT NULL,
      invoice_number TEXT NOT NULL UNIQUE,
      issue_date TEXT NOT NULL,
      due_date TEXT NOT NULL,
      status TEXT DEFAULT 'draft' CHECK(status IN ('draft', 'sent', 'paid', 'overdue')),
      subtotal REAL DEFAULT 0,
      tax_rate REAL DEFAULT 0,
      total REAL DEFAULT 0,
      notes TEXT DEFAULT '',
      pdf_path TEXT,
      created_at TEXT DEFAULT (datetime('now')),
      FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE SET NULL,
      FOREIGN KEY (client_id) REFERENCES clients(id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS invoice_items (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      invoice_id INTEGER NOT NULL,
      description TEXT NOT NULL,
      quantity REAL DEFAULT 0,
      unit_price REAL DEFAULT 0,
      total REAL DEFAULT 0,
      FOREIGN KEY (invoice_id) REFERENCES invoices(id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS settings (
      key TEXT PRIMARY KEY,
      value TEXT
    );
  "#;

