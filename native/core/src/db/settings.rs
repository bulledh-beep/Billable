//! Settings and tax settings.

use anyhow::Result;
use rusqlite::named_params;
use rusqlite::types::Value as SqlValue;

use super::Db;
use crate::models::*;

impl Db {
    /// Every setting (`settings:get`).
    pub fn get_settings(&self) -> Result<Settings> {
        let rows = self.rows("SELECT key, value FROM settings", [])?;
        Ok(Settings { rows: rows.iter().map(|r| (r.col("key"), r.col("value"))).collect() })
    }

    /// Store settings (values are saved as text, like `String(value)`) and
    /// return them all. Use [`crate::js::js_string`] to turn JSON values
    /// into the stored text.
    pub fn update_settings<K: AsRef<str>, V: AsRef<str>>(&self, data: &[(K, V)]) -> Result<Settings> {
        for (k, v) in data {
            self.run("INSERT OR REPLACE INTO settings (key, value) VALUES (?, ?)", [k.as_ref(), v.as_ref()])?;
        }
        self.get_settings()
    }

    /// The single tax settings row.
    pub fn get_tax_settings(&self) -> Result<Option<TaxSettings>> {
        Ok(self.row("SELECT * FROM tax_settings WHERE id = 1", [])?.map(|r| TaxSettings::from_row(&r, &[])))
    }

    /// Merge `data` over the tax settings, stamp `updated_at`, and return them.
    pub fn save_tax_settings(&self, data: &TaxSettingsPatch) -> Result<Option<TaxSettings>> {
        let existing = self.row("SELECT * FROM tax_settings WHERE id = 1", [])?;
        // { ...existing, ...data }: a field from `data` wins, else the stored value
        let pick_text = |patch: &Option<Option<String>>, col: &str| -> SqlValue {
            match patch {
                Some(v) => v.clone().map(SqlValue::Text).unwrap_or(SqlValue::Null),
                None => existing.as_ref().and_then(|r| r.get(col).cloned()).unwrap_or(SqlValue::Null),
            }
        };
        let pick_int = |patch: &Option<Option<i64>>, col: &str| -> SqlValue {
            match patch {
                Some(v) => v.map(SqlValue::Integer).unwrap_or(SqlValue::Null),
                None => existing.as_ref().and_then(|r| r.get(col).cloned()).unwrap_or(SqlValue::Null),
            }
        };
        let pick_real = |patch: &Option<Option<f64>>, col: &str| -> SqlValue {
            match patch {
                Some(v) => v.map(SqlValue::Real).unwrap_or(SqlValue::Null),
                None => existing.as_ref().and_then(|r| r.get(col).cloned()).unwrap_or(SqlValue::Null),
            }
        };
        self.run(
            "UPDATE tax_settings SET business_name = @business_name, business_address = @business_address, \
             gst_hst_number = @gst_hst_number, gst_hst_registered = @gst_hst_registered, province = @province, \
             fiscal_year_start = @fiscal_year_start, default_tax_rate = @default_tax_rate, \
             income_tax_bracket = @income_tax_bracket, currency = @currency, updated_at = @updated_at WHERE id = @id",
            named_params! {
                "@id": 1,
                "@business_name": pick_text(&data.business_name, "business_name"),
                "@business_address": pick_text(&data.business_address, "business_address"),
                "@gst_hst_number": pick_text(&data.gst_hst_number, "gst_hst_number"),
                "@gst_hst_registered": pick_int(&data.gst_hst_registered, "gst_hst_registered"),
                "@province": pick_text(&data.province, "province"),
                "@fiscal_year_start": pick_text(&data.fiscal_year_start, "fiscal_year_start"),
                "@default_tax_rate": pick_real(&data.default_tax_rate, "default_tax_rate"),
                "@income_tax_bracket": pick_real(&data.income_tax_bracket, "income_tax_bracket"),
                "@currency": pick_text(&data.currency, "currency"),
                "@updated_at": self.clock.now_iso(),
            },
        )?;
        self.get_tax_settings()
    }

    /// Hide an attention item for good (`billing:dismiss`).
    pub fn dismiss_attention(&self, key: &str) -> Result<bool> {
        let mut set = self.dismissed()?;
        let key = serde_json::Value::String(key.to_string());
        if !set.contains(&key) {
            set.push(key);
        }
        self.run(
            "INSERT OR REPLACE INTO settings (key, value) VALUES ('dismissed_attention', ?)",
            [serde_json::to_string(&set)?],
        )?;
        Ok(true)
    }

    /// The dismissed keys, as `new Set(JSON.parse(value || '[]'))` would hold them.
    pub(crate) fn dismissed(&self) -> Result<Vec<serde_json::Value>> {
        use serde_json::Value;
        let raw: Option<String> = self
            .row("SELECT value FROM settings WHERE key = 'dismissed_attention'", [])?
            .and_then(|r| r.col("value"));
        let text = raw.filter(|s| !s.is_empty()).unwrap_or_else(|| "[]".into());
        let mut out: Vec<Value> = Vec::new();
        let mut add = |v: Value| {
            if !out.contains(&v) {
                out.push(v);
            }
        };
        match serde_json::from_str::<Value>(&text) {
            Ok(Value::Array(items)) => items.into_iter().for_each(&mut add),
            // A Set built from a string holds its characters
            Ok(Value::String(s)) => s.chars().for_each(|c| add(Value::String(c.to_string()))),
            _ => {}
        }
        Ok(out)
    }
}
