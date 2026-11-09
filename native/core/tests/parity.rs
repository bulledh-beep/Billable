//! Parity with the Electron app.
//!
//! Replays every scenario in `parity/scenarios/` through this crate and
//! compares each step's result, the files left in the data folder, every
//! SQLite file (schema and rows) and `profiles.json` with what the
//! TypeScript produced (`parity/out/<name>/`, written by
//! `parity/node/run.mjs`). Scenarios the Node side hasn't run are skipped
//! with a message. See `parity/README.md`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use billable_core::js::{js_string, js_truthy};
use billable_core::time::js_date_parse;
use billable_core::*;
use chrono::{DateTime, Utc};
use rusqlite::types::Value as SqlValue;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

fn parity_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("parity")
}

fn dev_data_dir() -> PathBuf {
    match std::env::var_os("BILLABLE_DEV_DATA") {
        Some(d) => PathBuf::from(d),
        None => PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
            .join("Library/Application Support/Billable Dev"),
    }
}

fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

// ---------------------------------------------------------------- setup

fn setup(spec: &Value, user_data: &Path) -> Result<bool> {
    fs::create_dir_all(user_data)?;
    match spec {
        Value::String(s) if s == "fresh" => Ok(true),
        Value::String(s) if s == "dev" => {
            let src = dev_data_dir();
            if src.file_name().is_some_and(|n| n == "Billable") {
                bail!("refusing to read the real Billable data folder");
            }
            if !src.join("profiles.json").exists() {
                return Ok(false);
            }
            fs::copy(src.join("profiles.json"), user_data.join("profiles.json"))?;
            for entry in fs::read_dir(src.join("profiles"))? {
                let entry = entry?;
                if !entry.file_type()?.is_dir() {
                    continue;
                }
                let to = user_data.join("profiles").join(entry.file_name());
                fs::create_dir_all(&to)?;
                for f in ["billable.db", "billable.db-wal", "billable.db-shm"] {
                    if entry.path().join(f).exists() {
                        fs::copy(entry.path().join(f), to.join(f))?;
                    }
                }
            }
            Ok(true)
        }
        Value::Object(o) if o.contains_key("legacy_sql") => {
            let conn = rusqlite::Connection::open(user_data.join("billable.db"))?;
            for s in o["legacy_sql"].as_array().context("legacy_sql")? {
                conn.execute_batch(s.as_str().context("sql")?)?;
            }
            Ok(true)
        }
        other => bail!("unknown setup {other}"),
    }
}

// ---------------------------------------------------------------- normalizing

fn normalize(v: Value, user_data: &str) -> Value {
    fn walk(v: Value, key: Option<&str>, ud: &str) -> Value {
        match v {
            Value::String(s) => {
                if key.is_some_and(|k| k.to_lowercase().contains("token") || k.to_lowercase().contains("secret")) {
                    Value::String("<redacted>".into())
                } else {
                    Value::String(s.replace(ud, "<userData>"))
                }
            }
            Value::Array(a) => Value::Array(a.into_iter().map(|x| walk(x, None, ud)).collect()),
            Value::Object(o) => {
                let settings_key = o.get("key").and_then(|k| k.as_str()).map(|s| s.to_string());
                Value::Object(
                    o.into_iter()
                        .map(|(k, x)| {
                            let eff = if k == "value" && settings_key.is_some() { settings_key.clone() } else { Some(k.clone()) };
                            let nx = walk(x, eff.as_deref(), ud);
                            (k, nx)
                        })
                        .collect(),
                )
            }
            other => other,
        }
    }
    walk(v, None, user_data)
}

fn resolve_refs(v: &Value, saved: &BTreeMap<String, Value>) -> Result<Value> {
    Ok(match v {
        Value::Array(a) => Value::Array(a.iter().map(|x| resolve_refs(x, saved)).collect::<Result<_>>()?),
        Value::Object(o) if o.len() == 1 && o.contains_key("$") => {
            let path = o["$"].as_str().context("ref")?;
            let mut parts = path.split('.');
            let mut cur = saved.get(parts.next().unwrap_or("")).cloned().unwrap_or(Value::Null);
            for p in parts {
                cur = match &cur {
                    Value::Array(a) => p.parse::<usize>().ok().and_then(|i| a.get(i).cloned()).unwrap_or(Value::Null),
                    Value::Object(m) => m.get(p).cloned().unwrap_or(Value::Null),
                    _ => Value::Null,
                };
            }
            if cur.is_null() {
                bail!("unresolved ref {path}");
            }
            cur
        }
        Value::Object(o) => {
            let mut out = serde_json::Map::new();
            for (k, x) in o {
                out.insert(k.clone(), resolve_refs(x, saved)?);
            }
            Value::Object(out)
        }
        other => other.clone(),
    })
}

fn list_files(root: &Path) -> Vec<String> {
    fn walk(dir: &Path, rel: &str, out: &mut Vec<String>) {
        let Ok(rd) = fs::read_dir(dir) else { return };
        let mut names: Vec<String> = rd.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        for name in names {
            let full = dir.join(&name);
            let r = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
            if full.is_dir() {
                out.push(format!("{r}/"));
                walk(&full, &r, out);
            } else if !(name.ends_with("-wal") || name.ends_with("-shm")) {
                out.push(r);
            }
        }
    }
    let mut out = Vec::new();
    walk(root, "", &mut out);
    out
}

fn sql_json(v: SqlValue) -> Value {
    match v {
        SqlValue::Null => Value::Null,
        SqlValue::Integer(i) => json!(i),
        SqlValue::Real(f) => json!(f),
        SqlValue::Text(s) => Value::String(s),
        SqlValue::Blob(b) => json!({"type": "Buffer", "data": b}),
    }
}

fn query_json(conn: &rusqlite::Connection, sql: &str, params: &[Value]) -> Result<Vec<Value>> {
    let mut stmt = conn.prepare(sql)?;
    let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let bound: Vec<SqlValue> = params
        .iter()
        .map(|p| match p {
            Value::Null => SqlValue::Null,
            Value::Number(n) => n.as_i64().map(SqlValue::Integer).unwrap_or(SqlValue::Real(n.as_f64().unwrap_or(0.0))),
            Value::String(s) => SqlValue::Text(s.clone()),
            other => SqlValue::Text(other.to_string()),
        })
        .collect();
    let mut rows = stmt.query(rusqlite::params_from_iter(bound))?;
    let mut out = Vec::new();
    while let Some(r) = rows.next()? {
        let mut o = serde_json::Map::new();
        for (i, n) in names.iter().enumerate() {
            o.insert(n.clone(), sql_json(r.get(i)?));
        }
        out.push(Value::Object(o));
    }
    Ok(out)
}

// ---------------------------------------------------------------- dispatch

struct Args<'a>(&'a [Value]);

impl Args<'_> {
    /// The argument, or `None` if missing (JS `undefined`).
    fn get(&self, i: usize) -> Option<&Value> {
        self.0.get(i)
    }
    fn i64(&self, i: usize) -> Result<i64> {
        self.get(i).and_then(|v| v.as_f64()).map(|f| f as i64).ok_or_else(|| anyhow!("arg {i} must be a number"))
    }
    fn opt_i64(&self, i: usize) -> Option<i64> {
        self.get(i).and_then(|v| v.as_f64()).map(|f| f as i64)
    }
    fn str(&self, i: usize) -> Result<String> {
        self.get(i).and_then(|v| v.as_str()).map(String::from).ok_or_else(|| anyhow!("arg {i} must be a string"))
    }
    fn opt_str(&self, i: usize) -> Option<String> {
        self.get(i).and_then(|v| v.as_str()).map(String::from)
    }
    fn obj<T: DeserializeOwned>(&self, i: usize) -> Result<T> {
        Ok(serde_json::from_value(self.get(i).cloned().unwrap_or_else(|| json!({})))?)
    }
    fn ids(&self, i: usize) -> Result<Vec<i64>> {
        Ok(self
            .get(i)
            .and_then(|v| v.as_array())
            .map(|a| a.iter().map(|x| x.as_f64().unwrap_or(0.0) as i64).collect())
            .unwrap_or_default())
    }
    fn at(&self, i: usize) -> Option<DateTime<Utc>> {
        let ms = js_date_parse(self.get(i)?.as_str()?);
        DateTime::<Utc>::from_timestamp_millis(ms as i64)
    }
}

struct Runner {
    user_data: PathBuf,
    clock: Clock,
    profiles: Profiles,
    db: Option<Db>,
    timer: Option<TimerManager>,
}

fn to_json<T: serde::Serialize>(v: T) -> Result<Value> {
    Ok(serde_json::to_value(v)?)
}

impl Runner {
    fn db(&self) -> Result<&Db> {
        self.db.as_ref().ok_or_else(|| anyhow!("The database connection is not open"))
    }

    fn timer(&mut self) -> Result<(&Db, &mut TimerManager)> {
        let db = self.db.as_ref().ok_or_else(|| anyhow!("The database connection is not open"))?;
        let t = self.timer.as_mut().ok_or_else(|| anyhow!("no timer"))?;
        Ok((db, t))
    }

    fn call(&mut self, op: &str, a: Args) -> Result<Value> {
        let p = &self.profiles;
        Ok(match op {
            // ---- harness
            "now" => {
                self.clock.set_ms(js_date_parse(&a.str(0)?) as i64);
                Value::Null
            }
            "advance" => {
                self.clock.advance_ms(a.i64(0)?);
                Value::Null
            }
            "open" => {
                self.db = None;
                let id = a.opt_str(0).filter(|s| !s.is_empty()).unwrap_or_else(|| p.get_active_profile_id());
                self.db = Some(Db::open_profile(p, &id, self.clock.clone())?);
                Value::Null
            }
            "close" => {
                if let Some(db) = self.db.take() {
                    db.close()?;
                }
                Value::Null
            }
            "sql" => {
                let params: Vec<Value> = a.get(1).and_then(|v| v.as_array()).cloned().unwrap_or_default();
                Value::Array(query_json(self.db()?.connection(), &a.str(0)?, &params)?)
            }
            "exec" => {
                self.db()?.connection().execute_batch(&a.str(0)?)?;
                Value::Null
            }
            "export" => {
                let path = self.user_data.join(a.str(0)?);
                self.db()?.export_to(&path)?;
                Value::String(path.to_string_lossy().into_owned())
            }
            "files" => to_json(list_files(&self.user_data))?,
            "timer.new" => {
                self.timer = Some(TimerManager::new(self.db()?)?);
                Value::Null
            }
            "timer.start" => {
                let (project, desc, at) = (a.i64(0)?, a.opt_str(1), a.at(2));
                let (db, t) = self.timer()?;
                to_json(t.start(db, project, desc.as_deref(), at)?)?
            }
            "timer.stop" => {
                let at = a.at(0);
                let (db, t) = self.timer()?;
                to_json(t.stop(db, at)?)?
            }
            "timer.pause" => {
                let at = a.at(0);
                let (db, t) = self.timer()?;
                to_json(t.pause(db, at)?)?
            }
            "timer.resume" => {
                let at = a.at(0);
                let (db, t) = self.timer()?;
                to_json(t.resume(db, at)?)?
            }
            "timer.toggle" => {
                let (db, t) = self.timer()?;
                to_json(t.toggle_pause(db)?)?
            }
            "timer.state" => to_json(self.timer.as_ref().context("no timer")?.state())?,
            "profile.list" => to_json(p.list())?,

            // ---- profiles.ts
            "listProfiles" => to_json(p.list_profiles())?,
            "getActiveProfileId" => to_json(p.get_active_profile_id())?,
            "getActiveProfile" => to_json(p.get_active_profile())?,
            "setActiveProfileId" => to_json(p.set_active_profile_id(&a.str(0)?)?)?,
            "createProfile" => to_json(p.create_profile(&a.str(0)?, a.opt_str(1).as_deref())?)?,
            "renameProfile" => to_json(p.rename_profile(&a.str(0)?, &a.str(1)?)?)?,
            "updateProfileColor" => to_json(p.update_profile_color(&a.str(0)?, &a.str(1)?)?)?,
            "clearProfileAvatar" => to_json(p.clear_profile_avatar(&a.str(0)?)?)?,
            "deleteProfile" => to_json(p.delete_profile(&a.str(0)?)?)?,
            "migrateLegacyIfNeeded" => to_json(p.migrate_legacy_if_needed()?)?,

            // ---- database.ts
            "localToday" => to_json(self.clock.local_today())?,
            "effectiveInvoiceStatus" => {
                let today = a.opt_str(2).unwrap_or_else(|| self.clock.local_today());
                to_json(effective_invoice_status(a.opt_str(0).as_deref(), a.opt_str(1).as_deref(), &today))?
            }
            "backupDatabase" => to_json(self.db()?.backup_database(&a.str(0)?))?,
            "listClients" => to_json(self.db()?.list_clients()?)?,
            "getClient" => to_json(self.db()?.get_client(a.i64(0)?)?)?,
            "createClient" => to_json(self.db()?.create_client(&a.obj(0)?)?)?,
            "updateClient" => to_json(self.db()?.update_client(a.i64(0)?, &a.obj(1)?)?)?,
            "deleteClient" => to_json(self.db()?.delete_client(a.i64(0)?)?)?,
            "mergeClients" => to_json(self.db()?.merge_clients(a.i64(0)?, a.i64(1)?)?)?,
            "listProjects" => to_json(self.db()?.list_projects(a.opt_i64(0))?)?,
            "getProject" => to_json(self.db()?.get_project(a.i64(0)?)?)?,
            "createProject" => to_json(self.db()?.create_project(&a.obj(0)?)?)?,
            "updateProject" => to_json(self.db()?.update_project(a.i64(0)?, &a.obj(1)?)?)?,
            "deleteProject" => to_json(self.db()?.delete_project(a.i64(0)?)?)?,
            "listTimeEntries" => to_json(self.db()?.list_time_entries(a.opt_i64(0))?)?,
            "getTimeEntry" => to_json(self.db()?.get_time_entry(a.i64(0)?)?)?,
            "createTimeEntry" => to_json(self.db()?.create_time_entry(&a.obj(0)?)?)?,
            "updateTimeEntry" => to_json(self.db()?.update_time_entry(a.i64(0)?, &a.obj(1)?)?)?,
            "deleteTimeEntry" => to_json(self.db()?.delete_time_entry(a.i64(0)?)?)?,
            "setEntriesBillable" => {
                to_json(self.db()?.set_entries_billable(&a.ids(0)?, a.get(1).is_some_and(js_truthy))?)?
            }
            "listInvoiceableEntries" => to_json(self.db()?.list_invoiceable_entries(a.i64(0)?, a.opt_i64(1))?)?,
            "startTimer" => to_json(self.db()?.start_timer(a.i64(0)?, a.opt_str(1).as_deref(), a.at(2))?)?,
            "stopTimer" => to_json(self.db()?.stop_timer(a.i64(0)?, a.at(1))?)?,
            "pauseTimer" => to_json(self.db()?.pause_timer(a.i64(0)?, a.at(1))?)?,
            "resumeTimer" => to_json(self.db()?.resume_timer(a.i64(0)?, a.at(1))?)?,
            "getActiveTimer" => to_json(self.db()?.get_active_timer()?)?,
            "getPausedTimer" => to_json(self.db()?.get_paused_timer()?)?,
            "listInvoices" => to_json(self.db()?.list_invoices(a.opt_str(0).as_deref())?)?,
            "getInvoice" => to_json(self.db()?.get_invoice(a.i64(0)?)?)?,
            "createInvoice" => to_json(self.db()?.create_invoice(&a.obj(0)?)?)?,
            "updateInvoice" => to_json(self.db()?.update_invoice(a.i64(0)?, &a.obj(1)?)?)?,
            "deleteInvoice" => to_json(self.db()?.delete_invoice(a.i64(0)?)?)?,
            "markInvoicesSent" => to_json(self.db()?.mark_invoices_sent(&a.ids(0)?, a.opt_str(1).as_deref())?)?,
            "markInvoicesPaid" => to_json(self.db()?.mark_invoices_paid(
                &a.ids(0)?,
                a.opt_str(1).as_deref(),
                a.opt_str(2).as_deref(),
            )?)?,
            "markInvoiceUnpaid" => to_json(self.db()?.mark_invoice_unpaid(a.i64(0)?)?)?,
            "getSettings" => to_json(self.db()?.get_settings()?)?,
            "updateSettings" => {
                let pairs: Vec<(String, String)> = a
                    .get(0)
                    .and_then(|v| v.as_object())
                    .map(|o| o.iter().map(|(k, v)| (k.clone(), js_string(v))).collect())
                    .unwrap_or_default();
                to_json(self.db()?.update_settings(&pairs)?)?
            }
            "getDashboardStats" => to_json(self.db()?.get_dashboard_stats()?)?,
            "getRecentEntries" => to_json(self.db()?.get_recent_entries(a.opt_i64(0).unwrap_or(10))?)?,
            "hoursByProject" => to_json(self.db()?.hours_by_project(&a.str(0)?, &a.str(1)?)?)?,
            "hoursByClient" => to_json(self.db()?.hours_by_client(&a.str(0)?, &a.str(1)?)?)?,
            "earningsByMonth" => to_json(self.db()?.earnings_by_month(&a.str(0)?, &a.str(1)?)?)?,
            "getUnbilledEntries" => to_json(self.db()?.get_unbilled_entries(a.i64(0)?)?)?,
            "getTodayHours" => to_json(self.db()?.get_today_hours()?)?,
            "getUnbilledTotals" => to_json(self.db()?.get_unbilled_totals()?)?,
            "getRecentProjects" => to_json(self.db()?.get_recent_projects(a.opt_i64(0).unwrap_or(5))?)?,
            "getUnbilledEntriesByClient" => to_json(self.db()?.get_unbilled_entries_by_client(a.i64(0)?)?)?,
            "getUnbilledEntriesForProjects" => to_json(self.db()?.get_unbilled_entries_for_projects(&a.ids(0)?)?)?,
            "getBillingOverview" => to_json(self.db()?.get_billing_overview()?)?,
            "dismissAttention" => to_json(self.db()?.dismiss_attention(&a.str(0)?)?)?,
            "getTaxOverview" => to_json(self.db()?.get_tax_overview(a.i64(0)?)?)?,
            "listInvoicesByYear" => to_json(self.db()?.list_invoices_by_year(a.i64(0)?)?)?,
            "getTaxSettings" => to_json(self.db()?.get_tax_settings()?)?,
            "saveTaxSettings" => to_json(self.db()?.save_tax_settings(&a.obj(0)?)?)?,
            "listExpenses" => to_json(self.db()?.list_expenses(a.opt_i64(0))?)?,
            "getExpense" => to_json(self.db()?.get_expense(a.i64(0)?)?)?,
            "createExpense" => to_json(self.db()?.create_expense(&a.obj(0)?)?)?,
            "updateExpense" => to_json(self.db()?.update_expense(a.i64(0)?, &a.obj(1)?)?)?,
            "deleteExpense" => to_json(self.db()?.delete_expense(a.i64(0)?)?)?,
            "computeCommission" => {
                let d = a.get(0).cloned().unwrap_or_else(|| json!({}));
                let num = |k: &str| d.get(k).and_then(|v| v.as_f64());
                to_json(compute_commission(
                    d.get("job_type").and_then(|v| v.as_str()),
                    num("system_size_kw"),
                    num("contract_amount"),
                    num("manual_override"),
                ))?
            }
            "listCommissions" => to_json(self.db()?.list_commissions()?)?,
            "getCommission" => to_json(self.db()?.get_commission(a.i64(0)?)?)?,
            "createCommission" => to_json(self.db()?.create_commission(&a.obj(0)?)?)?,
            "updateCommission" => to_json(self.db()?.update_commission(a.i64(0)?, &a.obj(1)?)?)?,
            "deleteCommission" => to_json(self.db()?.delete_commission(a.i64(0)?)?)?,
            "patchCommission" => to_json(self.db()?.patch_commission(a.i64(0)?, &a.obj(1)?)?)?,
            "bulkPatchCommissions" => to_json(self.db()?.bulk_patch_commissions(&a.ids(0)?, &a.obj(1)?)?)?,
            "listCommissionInvoices" => to_json(self.db()?.list_commission_invoices()?)?,
            "getCommissionInvoice" => to_json(self.db()?.get_commission_invoice(a.i64(0)?)?)?,
            "createCommissionInvoice" => to_json(self.db()?.create_commission_invoice(&a.obj(0)?)?)?,
            "updateCommissionInvoiceStatus" => {
                to_json(self.db()?.update_commission_invoice_status(a.i64(0)?, &a.str(1)?)?)?
            }
            "deleteCommissionInvoice" => to_json(self.db()?.delete_commission_invoice(a.i64(0)?)?)?,
            other => bail!("unknown op {other}"),
        })
    }
}

struct RustRun {
    steps: Vec<Value>,
    wall: (i64, i64),
}

fn run_rust(scenario: &Value, user_data: &Path) -> Result<Option<RustRun>> {
    if !setup(&scenario["setup"], user_data)? {
        return Ok(None);
    }
    let clock = Clock::fixed_at(scenario["now"].as_str().context("now")?)?;
    let mut r = Runner {
        user_data: user_data.to_path_buf(),
        profiles: Profiles::new(user_data, clock.clone()),
        clock,
        db: None,
        timer: None,
    };
    let ud = user_data.to_string_lossy().into_owned();
    let mut saved = BTreeMap::new();
    let mut steps = Vec::new();
    let start = now_ms();
    for (i, raw) in scenario["steps"].as_array().context("steps")?.iter().enumerate() {
        let (op, args, save_as) = match raw {
            Value::Array(a) => (a[0].as_str().unwrap_or("").to_string(), a[1..].to_vec(), None),
            Value::Object(o) => (
                o["op"].as_str().unwrap_or("").to_string(),
                o.get("args").and_then(|v| v.as_array()).cloned().unwrap_or_default(),
                o.get("as").and_then(|v| v.as_str()).map(String::from),
            ),
            _ => bail!("bad step {raw}"),
        };
        let result = resolve_refs(&Value::Array(args), &saved).and_then(|args| {
            let list = args.as_array().cloned().unwrap_or_default();
            r.call(&op, Args(&list))
        });
        let record = match result {
            Ok(v) => {
                let v = normalize(v, &ud);
                if let Some(name) = &save_as {
                    saved.insert(name.clone(), v.clone());
                }
                json!({"i": i, "op": op, "ok": true, "value": v})
            }
            Err(e) => json!({"i": i, "op": op, "ok": false, "error": e.to_string()}),
        };
        steps.push(record);
    }
    if let Some(db) = r.db.take() {
        db.close()?;
    }
    Ok(Some(RustRun { steps, wall: (start, now_ms()) }))
}

// ---------------------------------------------------------------- comparing

/// Differences we accept on purpose: (op, text in the message, why).
const KNOWN_DIFFERENCES: &[(&str, &str, &str)] = &[
    ("timer.state", ".billing_state: extra in rust", "after a resync the Rust timer holds the full time:list entry shape"),
    ("timer.state", ".client_id: extra in rust", "after a resync the Rust timer holds the full time:list entry shape"),
    ("timer.state", ".project_status: extra in rust", "after a resync the Rust timer holds the full time:list entry shape"),
    ("timer.state", ".invoice_number: extra in rust", "after a resync the Rust timer holds the full time:list entry shape"),
    ("timer.state", ".invoice_status: extra in rust", "after a resync the Rust timer holds the full time:list entry shape"),
    ("timer.state", ".invoice_stored_status: extra in rust", "after a resync the Rust timer holds the full time:list entry shape"),
    ("timer.state", ".invoice_due_date: extra in rust", "after a resync the Rust timer holds the full time:list entry shape"),
    ("saveTaxSettings", ".province: node null vs rust \"\"", "a NULL in a text column the app always fills reads as \"\""),
];

struct Cmp {
    node_wall: (i64, i64),
    rust_wall: (i64, i64),
    /// Profile ids that differ only by their random suffix (node → rust).
    id_map: Vec<(String, String)>,
    out: Vec<String>,
}

fn sqlite_now_ms(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    let shape = b.len() == 19
        && b.iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            10 => *c == b' ',
            13 | 16 => *c == b':',
            _ => c.is_ascii_digit(),
        });
    if !shape {
        return None;
    }
    let ms = js_date_parse(&format!("{}Z", s.replacen(' ', "T", 1)));
    ms.is_finite().then_some(ms as i64)
}

impl Cmp {
    fn map_ids(&self, s: &str) -> String {
        let mut s = s.to_string();
        for (a, b) in &self.id_map {
            s = s.replace(a.as_str(), b.as_str());
        }
        s
    }

    fn strings_match(&self, a: &str, b: &str) -> bool {
        if a == b || self.map_ids(a) == b {
            return true;
        }
        // datetime('now') written by SQLite during each run
        let within = |s: &str, w: (i64, i64)| sqlite_now_ms(s).is_some_and(|t| t >= w.0 - 2000 && t <= w.1 + 2000);
        within(a, self.node_wall) && within(b, self.rust_wall)
    }

    fn value(&mut self, path: &str, a: &Value, b: &Value) {
        if self.out.len() > 60 {
            return;
        }
        match (a, b) {
            (Value::Number(x), Value::Number(y)) => {
                if x.as_f64() != y.as_f64() {
                    self.out.push(format!("{path}: node {x} vs rust {y}"));
                }
            }
            (Value::String(x), Value::String(y)) => {
                if !self.strings_match(x, y) {
                    self.out.push(format!("{path}: node {x:?} vs rust {y:?}"));
                }
            }
            (Value::Array(x), Value::Array(y)) => {
                if x.len() != y.len() {
                    self.out.push(format!("{path}: node has {} items, rust {}", x.len(), y.len()));
                }
                for (i, (p, q)) in x.iter().zip(y).enumerate() {
                    self.value(&format!("{path}[{i}]"), p, q);
                }
            }
            (Value::Object(x), Value::Object(y)) => {
                for k in x.keys() {
                    if !y.contains_key(k) {
                        self.out.push(format!("{path}.{k}: missing in rust (node {})", x[k]));
                    }
                }
                for k in y.keys() {
                    if !x.contains_key(k) {
                        self.out.push(format!("{path}.{k}: extra in rust ({})", y[k]));
                    }
                }
                for (k, p) in x {
                    if let Some(q) = y.get(k) {
                        self.value(&format!("{path}.{k}"), p, q);
                    }
                }
            }
            _ if a == b => {}
            _ => self.out.push(format!("{path}: node {a} vs rust {b}")),
        }
    }

    fn database(&mut self, label: &str, node: &Path, rust: &Path) -> Result<()> {
        let open = |p: &Path| rusqlite::Connection::open(p);
        let (n, r) = (open(node)?, open(rust)?);
        let schema = "SELECT type, name, tbl_name, sql FROM sqlite_master ORDER BY type, name";
        let (ns, rs) = (query_json(&n, schema, &[])?, query_json(&r, schema, &[])?);
        self.value(&format!("{label} sqlite_master"), &Value::Array(ns.clone()), &Value::Array(rs));
        for t in ns.iter().filter(|t| t["type"] == "table") {
            let name = t["name"].as_str().unwrap_or_default();
            let sql = format!("SELECT rowid AS __rowid, * FROM \"{name}\" ORDER BY rowid");
            let (nr, rr) = (query_json(&n, &sql, &[])?, query_json(&r, &sql, &[])?);
            self.value(&format!("{label} {name}"), &Value::Array(nr), &Value::Array(rr));
        }
        Ok(())
    }
}

fn compare(
    name: &str,
    node: &Value,
    rust: &RustRun,
    node_ud: &Path,
    rust_ud: &Path,
    known: &mut Vec<String>,
) -> Result<Vec<String>> {
    let wall = |v: &Value| (v[0].as_i64().unwrap_or(0), v[1].as_i64().unwrap_or(0));
    let mut c = Cmp { node_wall: wall(&node["wall"]), rust_wall: rust.wall, id_map: Vec::new(), out: Vec::new() };
    let node_steps = node["steps"].as_array().context("node steps")?;
    if node_steps.len() != rust.steps.len() {
        c.out.push(format!("{name}: node ran {} steps, rust {}", node_steps.len(), rust.steps.len()));
    }
    for (n, r) in node_steps.iter().zip(&rust.steps) {
        let label = format!("{name} step {} {}", n["i"], n["op"].as_str().unwrap_or(""));
        if n["op"] == "createProfile" && n["ok"] == true && r["ok"] == true {
            let (a, b) = (n["value"]["id"].as_str().unwrap_or(""), r["value"]["id"].as_str().unwrap_or(""));
            let base = |s: &str| s.rsplit_once('-').map(|(x, _)| x.to_string()).unwrap_or_default();
            if a != b && base(a) == base(b) && !base(a).is_empty() {
                c.id_map.push((a.to_string(), b.to_string()));
            }
        }
        let before = c.out.len();
        match (n["ok"].as_bool(), r["ok"].as_bool()) {
            (Some(true), Some(true)) => c.value(&label, &n["value"], &r["value"]),
            (Some(false), Some(false)) => c.value(&label, &n["error"], &r["error"]),
            _ => c.out.push(format!(
                "{label}: node {} vs rust {}",
                if n["ok"] == true { format!("ok {}", n["value"]) } else { format!("error {}", n["error"]) },
                if r["ok"] == true { format!("ok {}", r["value"]) } else { format!("error {}", r["error"]) },
            )),
        }
        let op = n["op"].as_str().unwrap_or("");
        let step_diffs = c.out.split_off(before);
        for d in step_diffs {
            match KNOWN_DIFFERENCES.iter().find(|(o, text, _)| *o == op && d.contains(text)) {
                Some((_, _, why)) => known.push(format!("{d} ({why})")),
                None => c.out.push(d),
            }
        }
    }

    // Files left behind
    let rust_files = list_files(rust_ud);
    c.value(&format!("{name} files"), &node["files"], &json!(rust_files));

    // Every SQLite file, and profiles.json
    for f in node["files"].as_array().into_iter().flatten().filter_map(|f| f.as_str()) {
        let mapped = c.map_ids(f);
        if f.ends_with(".db") && rust_ud.join(&mapped).exists() {
            c.database(&format!("{name} {f}"), &node_ud.join(f), &rust_ud.join(&mapped))?;
        }
    }
    if node_ud.join("profiles.json").exists() {
        let a = c.map_ids(&fs::read_to_string(node_ud.join("profiles.json"))?);
        let b = fs::read_to_string(rust_ud.join("profiles.json")).unwrap_or_default();
        if a != b {
            c.out.push(format!("{name} profiles.json differs:\nnode: {a}\nrust: {b}"));
        }
    }
    Ok(c.out)
}

#[test]
fn parity_with_typescript() {
    let dir = parity_dir();
    let mut files: Vec<PathBuf> = fs::read_dir(dir.join("scenarios"))
        .expect("parity/scenarios")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    files.sort();

    let tmp_root = std::env::temp_dir().join(format!("billable-core-parity-{}", std::process::id()));
    let mut problems = Vec::new();
    let mut ran = 0;
    for file in files {
        let scenario: Value = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
        let name = scenario["name"].as_str().unwrap().to_string();
        let out = dir.join("out").join(&name);
        let Ok(raw) = fs::read_to_string(out.join("results.json")) else {
            eprintln!("parity: skipping {name}: no Node results (run parity/node/run.mjs, see parity/README.md)");
            continue;
        };
        let node: Value = serde_json::from_str(&raw).unwrap();
        let tz = scenario["tz"].as_str().unwrap();
        assert_eq!(node["tz"].as_str(), Some(tz), "{name}: Node ran with a different TZ");
        std::env::set_var("TZ", tz);

        let user_data = tmp_root.join(&name).join("userData");
        let _ = fs::remove_dir_all(&user_data);
        let Some(rust) = run_rust(&scenario, &user_data).unwrap() else {
            eprintln!("parity: skipping {name}: setup data not available");
            continue;
        };
        let mut known = Vec::new();
        let diffs = compare(&name, &node, &rust, &out.join("userData"), &user_data, &mut known).unwrap();
        eprintln!(
            "parity: {name}: {} steps, {} differences, {} known differences",
            rust.steps.len(),
            diffs.len(),
            known.len()
        );
        for k in &known {
            eprintln!("parity:   known: {k}");
        }
        if diffs.is_empty() {
            let _ = fs::remove_dir_all(tmp_root.join(&name));
        } else {
            let dump = tmp_root.join(&name).join("rust-results.json");
            fs::write(&dump, serde_json::to_string_pretty(&rust.steps).unwrap()).unwrap();
            eprintln!("parity: {name}: Rust results in {}", dump.display());
        }
        problems.extend(diffs);
        ran += 1;
    }
    if ran == 0 {
        eprintln!("parity: no scenarios had Node results; nothing compared");
    }
    assert!(problems.is_empty(), "parity differences:\n{}", problems.join("\n"));
}
