//! The profile database: a port of `src/main/database.ts`.
//!
//! Open one [`Db`] per profile with [`Db::open`]; it runs the same table
//! creation, migrations, backfills and defaults as the Electron app, so either
//! app can open the other's file. Method names mirror the TypeScript functions
//! in snake_case and return the same shapes (see [`crate::models`]).

use std::cell::Cell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rusqlite::types::Value as SqlValue;
use rusqlite::{Connection, Params};
use serde_json::Value;

use crate::time::Clock;

mod billing;
mod clients;
mod commissions;
mod entries;
mod invoices;
mod reports;
mod schema;
mod settings;
mod tax;

pub use commissions::compute_commission;

/// SQL for an unbilled, finished, billable time entry (alias `te`).
macro_rules! unbilled_sql {
    () => {
        "te.end_time IS NOT NULL AND te.is_billable = 1 AND te.is_invoiced = 0 AND te.invoice_id IS NULL"
    };
}
pub(crate) use unbilled_sql;

/// An open profile database.
///
/// Not `Sync`: keep it on one thread (or behind a `Mutex`).
pub struct Db {
    conn: Connection,
    path: PathBuf,
    clock: Clock,
    tx_depth: Cell<u32>,
}

impl Db {
    /// Open (creating if needed) the database at `path` and bring it up to date:
    /// WAL mode, foreign keys, tables, migrations, backfills and default
    /// settings (database.ts `initDatabase`). The parent folder must exist.
    pub fn open(path: impl AsRef<Path>, clock: Clock) -> Result<Db> {
        let path = path.as_ref().to_path_buf();
        let conn = Connection::open(&path).with_context(|| format!("opening {}", path.display()))?;
        // better-sqlite3's defaults: 5 s busy timeout, and (compiled with
        // SQLITE_DEFAULT_WAL_SYNCHRONOUS=1) synchronous=NORMAL under WAL.
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        let _mode: String = conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))?;
        conn.execute_batch("PRAGMA synchronous = NORMAL; PRAGMA foreign_keys = ON;")?;
        let db = Db { conn, path, clock, tx_depth: Cell::new(0) };
        db.create_tables()?;
        db.run_migrations()?;
        db.seed_defaults()?;
        Ok(db)
    }

    /// Open a profile's database (`<userData>/profiles/<id>/billable.db`).
    pub fn open_profile(profiles: &crate::profiles::Profiles, profile_id: &str, clock: Clock) -> Result<Db> {
        Db::open(profiles.profile_db_path(profile_id), clock)
    }

    /// The database file path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The clock this database uses for "now".
    pub fn clock(&self) -> &Clock {
        &self.clock
    }

    /// The underlying connection, for things this API doesn't cover.
    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    /// Close the database (database.ts `closeDatabase`). Dropping also closes.
    pub fn close(self) -> Result<()> {
        self.conn.close().map_err(|(_, e)| e.into())
    }

    /// `localToday()` by this database's clock.
    pub fn local_today(&self) -> String {
        self.clock.local_today()
    }

    /// Run `f` in a transaction like better-sqlite3's `db.transaction`
    /// (nested calls use savepoints). Rolls back if `f` fails.
    pub(crate) fn tx<T>(&self, f: impl FnOnce() -> Result<T>) -> Result<T> {
        let depth = self.tx_depth.get();
        if depth == 0 {
            self.conn.execute_batch("BEGIN")?;
        } else {
            self.conn.execute_batch(&format!("SAVEPOINT billable_{depth}"))?;
        }
        self.tx_depth.set(depth + 1);
        let result = f();
        self.tx_depth.set(depth);
        match result {
            Ok(v) => {
                if depth == 0 {
                    self.conn.execute_batch("COMMIT")?;
                } else {
                    self.conn.execute_batch(&format!("RELEASE billable_{depth}"))?;
                }
                Ok(v)
            }
            Err(e) => {
                let _ = if depth == 0 {
                    self.conn.execute_batch("ROLLBACK")
                } else {
                    self.conn.execute_batch(&format!("ROLLBACK TO billable_{depth}; RELEASE billable_{depth}"))
                };
                Err(e)
            }
        }
    }

    /// All rows of a query as column maps (like better-sqlite3 `.all()`).
    pub(crate) fn rows<P: Params>(&self, sql: &str, params: P) -> Result<Vec<RowMap>> {
        let mut stmt = self.conn.prepare_cached(sql)?;
        let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
        let mut rows = stmt.query(params)?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            let mut cols = Vec::with_capacity(names.len());
            for (i, name) in names.iter().enumerate() {
                cols.push((name.clone(), row.get::<_, SqlValue>(i)?));
            }
            out.push(RowMap { cols });
        }
        Ok(out)
    }

    /// First row of a query (like better-sqlite3 `.get()`).
    pub(crate) fn row<P: Params>(&self, sql: &str, params: P) -> Result<Option<RowMap>> {
        Ok(self.rows(sql, params)?.into_iter().next())
    }

    /// Run a statement; returns the number of changed rows (`.run().changes`).
    pub(crate) fn run<P: Params>(&self, sql: &str, params: P) -> Result<usize> {
        let mut stmt = self.conn.prepare_cached(sql)?;
        Ok(stmt.execute(params)?)
    }

    /// Run an insert; returns `lastInsertRowid`.
    pub(crate) fn insert<P: Params>(&self, sql: &str, params: P) -> Result<i64> {
        self.run(sql, params)?;
        Ok(self.conn.last_insert_rowid())
    }

    /// One column of the first row, or `None`.
    pub(crate) fn scalar<P: Params>(&self, sql: &str, params: P, col: &str) -> Result<Option<SqlValue>> {
        Ok(self.row(sql, params)?.and_then(|r| r.get(col).cloned()))
    }
}

/// A result row: column names and values in select order. Like a
/// better-sqlite3 row object, a repeated column name resolves to the last one.
#[derive(Debug, Clone)]
pub(crate) struct RowMap {
    cols: Vec<(String, SqlValue)>,
}

impl RowMap {
    pub(crate) fn get(&self, name: &str) -> Option<&SqlValue> {
        self.cols.iter().rev().find(|(n, _)| n == name).map(|(_, v)| v)
    }

    /// Read a column with the tolerant conversions of [`FromCol`].
    pub(crate) fn col<T: FromCol>(&self, name: &str) -> T {
        T::from_col(self.get(name))
    }

    /// Columns not listed in `known` or `also_known`, as JSON (last wins).
    pub(crate) fn extra(&self, known: &[&str], also_known: &[&str]) -> BTreeMap<String, Value> {
        let mut out = BTreeMap::new();
        for (name, v) in &self.cols {
            if !known.contains(&name.as_str()) && !also_known.contains(&name.as_str()) {
                out.insert(name.clone(), sql_to_json(v));
            }
        }
        out
    }
}

/// A SQLite value as better-sqlite3 hands it to JS (then `JSON.stringify`).
pub(crate) fn sql_to_json(v: &SqlValue) -> Value {
    match v {
        SqlValue::Null => Value::Null,
        SqlValue::Integer(i) => Value::from(*i),
        SqlValue::Real(f) => serde_json::Number::from_f64(*f).map(Value::Number).unwrap_or(Value::Null),
        SqlValue::Text(s) => Value::String(s.clone()),
        SqlValue::Blob(b) => serde_json::json!({ "type": "Buffer", "data": b }),
    }
}

/// Tolerant column conversion: a `NULL` (or missing column) where the app
/// always writes a value reads as 0 / `""`; numbers stored as text are parsed.
pub(crate) trait FromCol: Sized {
    fn from_col(v: Option<&SqlValue>) -> Self;
}

impl FromCol for f64 {
    fn from_col(v: Option<&SqlValue>) -> Self {
        match v {
            Some(SqlValue::Integer(i)) => *i as f64,
            Some(SqlValue::Real(f)) => *f,
            Some(SqlValue::Text(s)) => {
                let n = crate::js::js_string_to_number(s);
                if n.is_nan() {
                    0.0
                } else {
                    n
                }
            }
            _ => 0.0,
        }
    }
}

impl FromCol for i64 {
    fn from_col(v: Option<&SqlValue>) -> Self {
        match v {
            Some(SqlValue::Integer(i)) => *i,
            other => {
                let f = f64::from_col(other);
                if f.is_finite() {
                    f as i64
                } else {
                    0
                }
            }
        }
    }
}

impl FromCol for String {
    fn from_col(v: Option<&SqlValue>) -> Self {
        match v {
            Some(SqlValue::Text(s)) => s.clone(),
            Some(SqlValue::Integer(i)) => i.to_string(),
            Some(SqlValue::Real(f)) => crate::js::js_number_to_string(*f),
            Some(SqlValue::Blob(b)) => String::from_utf8_lossy(b).into_owned(),
            _ => String::new(),
        }
    }
}

impl<T: FromCol> FromCol for Option<T> {
    fn from_col(v: Option<&SqlValue>) -> Self {
        match v {
            None | Some(SqlValue::Null) => None,
            some => Some(T::from_col(some)),
        }
    }
}

/// JS truthiness of a column value (`null`, 0, `''` are falsy).
pub(crate) fn sql_truthy(v: Option<&SqlValue>) -> bool {
    match v {
        None | Some(SqlValue::Null) => false,
        Some(SqlValue::Integer(i)) => *i != 0,
        Some(SqlValue::Real(f)) => *f != 0.0 && !f.is_nan(),
        Some(SqlValue::Text(s)) => !s.is_empty(),
        Some(SqlValue::Blob(_)) => true,
    }
}

/// A JSON value bound as a SQLite parameter, the way better-sqlite3 binds JS
/// values (numbers as doubles unless integral, `null` as NULL). Booleans,
/// arrays and objects can't be bound, as in better-sqlite3.
pub(crate) fn json_to_sql(v: &Value) -> Result<SqlValue> {
    Ok(match v {
        Value::Null => SqlValue::Null,
        Value::Number(n) => match n.as_i64() {
            Some(i) => SqlValue::Integer(i),
            None => SqlValue::Real(n.as_f64().unwrap_or(f64::NAN)),
        },
        Value::String(s) => SqlValue::Text(s.clone()),
        Value::Bool(_) | Value::Array(_) | Value::Object(_) => {
            anyhow::bail!("SQLite3 can only bind numbers, strings, bigints, buffers, and null")
        }
    })
}
