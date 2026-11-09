//! InvoiceCreate.tsx's line model: turning ticked time into invoice lines
//! (`buildLines`), hand-written lines, and the page's money formatting.

use std::collections::HashMap;

use billable_core::{js::round2, models::EntryRow};
use chrono::NaiveDate;

use crate::format::{self, friendly_day};

/// `LineStyle`: how ticked time becomes lines. `Manual` is "keep current
/// lines" for invoices written before lines were tied to time.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LineStyle {
    Entry,
    Project,
    Day,
    Manual,
}

impl LineStyle {
    pub fn as_str(self) -> &'static str {
        match self {
            LineStyle::Entry => "entry",
            LineStyle::Project => "project",
            LineStyle::Day => "day",
            LineStyle::Manual => "manual",
        }
    }

    pub fn parse(s: &str) -> Option<LineStyle> {
        match s {
            "entry" => Some(LineStyle::Entry),
            "project" => Some(LineStyle::Project),
            "day" => Some(LineStyle::Day),
            "manual" => Some(LineStyle::Manual),
            _ => None,
        }
    }
}

/// An edited description or rate on a line built from time.
#[derive(Clone, Default, Debug)]
pub struct Override {
    pub description: Option<String>,
    pub unit_price: Option<f64>,
}

/// A line added by hand (or kept from an older invoice).
#[derive(Clone, Debug)]
pub struct ManualLine {
    pub key: String,
    pub description: String,
    pub quantity: f64,
    pub unit_price: f64,
    /// Saved amount from an existing invoice, kept until hours or rate change.
    pub fixed_total: Option<f64>,
    /// `None` is "undefined" (falls back to the invoice's sole project);
    /// `Some(None)` is an explicit null.
    pub project_id: Option<Option<i64>>,
}

/// One invoice line as shown and saved.
#[derive(Clone, Debug)]
pub struct Line {
    pub key: String,
    pub description: String,
    pub quantity: f64,
    pub unit_price: f64,
    pub project_id: Option<i64>,
    pub source_key: Option<String>,
    pub generated: bool,
    pub default_rate: f64,
    pub custom: bool,
    pub fixed_total: Option<f64>,
}

impl Line {
    /// `lineTotal`: the saved amount, else hours × rate to the cent.
    pub fn total(&self) -> f64 {
        self.fixed_total.unwrap_or_else(|| round2(self.quantity * self.unit_price))
    }
}

/// `entryDay`: the local day an entry started on.
pub fn entry_day(e: &EntryRow) -> NaiveDate {
    format::local_day(&e.entry.start_time).unwrap_or_default()
}

/// `formatDay` for a day.
pub fn day_label(d: NaiveDate, today: NaiveDate) -> String {
    friendly_day(d, today)
}

struct Group<'a> {
    entries: Vec<&'a EntryRow>,
    description: String,
    project_id: i64,
    rate: f64,
}

/// `buildLines`: turn selected time into invoice lines. Overrides keep
/// edited descriptions and rates.
pub fn build_lines(entries: &[&EntryRow], style: LineStyle, overrides: &HashMap<String, Override>, today: NaiveDate) -> Vec<Line> {
    let mut projects: Vec<i64> = entries.iter().map(|e| e.entry.project_id).collect();
    projects.sort_unstable();
    projects.dedup();
    let multi_project = projects.len() > 1;
    let mut sorted: Vec<&EntryRow> = entries.to_vec();
    sorted.sort_by(|a, b| a.entry.start_time.cmp(&b.entry.start_time));

    let mut order: Vec<String> = Vec::new();
    let mut groups: HashMap<String, Group> = HashMap::new();
    for e in sorted {
        let day = entry_day(e);
        let project_name = e.project_name.clone().unwrap_or_default();
        let (key, description) = match style {
            LineStyle::Project => (format!("project:{}", e.entry.project_id), if project_name.is_empty() { "Work".to_string() } else { project_name.clone() }),
            LineStyle::Day => (format!("day:{}:{}", day.format("%Y-%m-%d"), e.entry.project_id), format!("{} · {}", day_label(day, today), project_name)),
            _ => {
                let trimmed = billable_core::js::js_trim(&e.entry.description);
                let what = if !trimmed.is_empty() {
                    trimmed.to_string()
                } else if !project_name.is_empty() {
                    project_name.clone()
                } else {
                    "Work".to_string()
                };
                let prefix = if multi_project && !e.entry.description.is_empty() { format!("{project_name}: ") } else { String::new() };
                (format!("entry:{}", e.entry.id), format!("{} · {prefix}{what}", day_label(day, today)))
            }
        };
        match groups.get_mut(&key) {
            Some(g) => g.entries.push(e),
            None => {
                order.push(key.clone());
                groups.insert(key, Group { entries: vec![e], description, project_id: e.entry.project_id, rate: e.rate.filter(|r| !r.is_nan()).unwrap_or(0.0) });
            }
        }
    }

    order
        .into_iter()
        .filter_map(|key| {
            let g = groups.remove(&key)?;
            let mut default_description = g.description.clone();
            match style {
                LineStyle::Project => {
                    let first = entry_day(g.entries[0]);
                    let last = entry_day(g.entries[g.entries.len() - 1]);
                    default_description = if first == last {
                        format!("{} · {}", g.description, day_label(first, today))
                    } else {
                        format!("{} · {} – {}", g.description, day_label(first, today), day_label(last, today))
                    };
                }
                LineStyle::Day => {
                    let mut notes: Vec<String> = Vec::new();
                    for e in &g.entries {
                        let n = billable_core::js::js_trim(&e.entry.description).to_string();
                        if !n.is_empty() && !notes.contains(&n) {
                            notes.push(n);
                        }
                    }
                    if !notes.is_empty() {
                        default_description = format!("{}: {}", g.description, notes.join("; "));
                    }
                }
                _ => {}
            }
            let minutes: f64 = g.entries.iter().map(|e| e.entry.duration_minutes).sum();
            let o = overrides.get(&key).cloned().unwrap_or_default();
            let custom = o.description.as_ref().is_some_and(|d| *d != default_description);
            Some(Line {
                key: key.clone(),
                source_key: Some(key),
                generated: true,
                project_id: Some(g.project_id),
                description: o.description.unwrap_or_else(|| default_description.clone()),
                custom,
                quantity: round2(minutes / 60.0),
                unit_price: o.unit_price.unwrap_or(g.rate),
                default_rate: g.rate,
                fixed_total: None,
            })
        })
        .collect()
}

/// `formatMoney(n, currency)`: en-CA for CAD (which shows "$"), en-US for
/// the rest, always two decimals.
pub fn money_in(amount: f64, currency: &str) -> String {
    let amount = if amount.is_finite() { amount } else { 0.0 };
    let (whole, cents) = format::money_parts(amount);
    let symbol = match currency {
        "CAD" | "USD" => return whole + &cents,
        "EUR" => "€".to_string(),
        "GBP" => "£".to_string(),
        "JPY" => "¥".to_string(),
        "AUD" => "A$".to_string(),
        other => format!("{other}\u{a0}"),
    };
    whole.replacen('$', &symbol, 1) + &cents
}

/// `Number.prototype.toFixed(digits)`: the nearest decimal, ties away from zero.
pub fn to_fixed(x: f64, digits: usize) -> String {
    if !x.is_finite() {
        return billable_core::js::js_number_to_string(x);
    }
    // The exact decimal expansion (to far more places than a double holds),
    // then rounded by hand so exact ties go up like JS rather than to even.
    let exact = format!("{:.60}", x.abs());
    let (int_part, frac) = exact.split_once('.').unwrap_or((&exact, ""));
    let keep: String = frac.chars().take(digits).collect();
    let rest = &frac[keep.len()..];
    let round_up = rest.chars().next().is_some_and(|c| c >= '5');
    let mut digits_str: Vec<u8> = format!("{int_part}{keep}").into_bytes();
    if round_up {
        let mut i = digits_str.len();
        loop {
            if i == 0 {
                digits_str.insert(0, b'1');
                break;
            }
            i -= 1;
            if digits_str[i] == b'9' {
                digits_str[i] = b'0';
            } else {
                digits_str[i] += 1;
                break;
            }
        }
    }
    let s = String::from_utf8(digits_str).unwrap_or_default();
    let split = s.len() - digits;
    let body = if digits == 0 { s.clone() } else { format!("{}.{}", &s[..split], &s[split..]) };
    if x < 0.0 {
        // JS keeps the sign for any negative input, even when it rounds to zero ("-0.00").
        format!("-{body}")
    } else {
        body
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_fixed_matches_js() {
        assert_eq!(to_fixed(65.0, 2), "65.00");
        assert_eq!(to_fixed(72.5, 2), "72.50");
        assert_eq!(to_fixed(0.125, 2), "0.13");
        assert_eq!(to_fixed(1.005, 2), "1.00");
        assert_eq!(to_fixed(9.999, 2), "10.00");
        assert_eq!(to_fixed(0.0, 2), "0.00");
        assert_eq!(to_fixed(-1.5, 0), "-2");
        assert_eq!(to_fixed(-0.001, 2), "-0.00");
    }

    #[test]
    fn money_formats() {
        assert_eq!(money_in(1234.5, "CAD"), "$1,234.50");
        assert_eq!(money_in(-5.0, "USD"), "-$5.00");
        assert_eq!(money_in(10.0, "EUR"), "€10.00");
    }
}
