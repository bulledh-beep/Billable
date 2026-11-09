//! Ports of src/renderer/utils/format.ts and src/main/database.ts's date helpers.

use chrono::{DateTime, Datelike, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};

/// JavaScript's `Math.round`: halves go up (towards +infinity), unlike Rust's `round`.
pub fn js_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

/// `Intl.NumberFormat('en-US', { style: 'currency', currency: 'USD' })`,
/// split into dollars and cents so the cents can be set back.
pub fn money_parts(amount: f64) -> (String, String) {
    // Intl rounds half away from zero, which is Rust's `round`.
    let cents_total = (amount.abs() * 100.0).round() as u64;
    let whole = cents_total / 100;
    let cents = cents_total % 100;
    let digits = whole.to_string();
    let mut grouped = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    let sign = if amount < 0.0 && cents_total > 0 { "-" } else { "" };
    (format!("{sign}${grouped}"), format!(".{cents:02}"))
}

pub fn money(amount: f64) -> String {
    let (w, c) = money_parts(amount);
    w + &c
}

/// "5h 45m", "3h", "45m".
pub fn duration_short(minutes: f64) -> String {
    let total = js_round(minutes) as i64;
    let (h, m) = (total.div_euclid(60), total.rem_euclid(60));
    match (h, m) {
        (0, m) => format!("{m}m"),
        (h, 0) => format!("{h}h"),
        (h, m) => format!("{h}h {m}m"),
    }
}

pub fn hours_short(hours: f64) -> String {
    let minutes = js_round(hours * 60.0);
    if minutes == 0.0 { "0h".into() } else { duration_short(minutes) }
}

/// Stored times are ISO strings ("2026-10-19T16:00:00.000Z"), or SQLite's
/// "YYYY-MM-DD HH:MM:SS" in UTC.
pub fn parse_time(s: &str) -> Option<DateTime<Local>> {
    if let Ok(t) = DateTime::parse_from_rfc3339(s) {
        return Some(t.with_timezone(&Local));
    }
    for fmt in ["%Y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M"] {
        if let Ok(t) = NaiveDateTime::parse_from_str(s, fmt) {
            return Some(Utc.from_utc_datetime(&t).with_timezone(&Local));
        }
    }
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Local.from_local_datetime(&d.and_hms_opt(0, 0, 0)?).earliest();
    }
    None
}

pub fn local_day(s: &str) -> Option<NaiveDate> {
    parse_time(s).map(|t| t.date_naive())
}

/// `Date.toISOString()` of a local instant, for comparing with stored ISO strings.
pub fn iso_utc(t: DateTime<Local>) -> String {
    t.with_timezone(&Utc).format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

pub fn local_midnight(d: NaiveDate) -> DateTime<Local> {
    Local.from_local_datetime(&d.and_hms_opt(0, 0, 0).unwrap()).earliest().unwrap()
}

const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// "Jun 12", or "Jun 12, 2025" outside the current year.
pub fn friendly_day(d: NaiveDate, today: NaiveDate) -> String {
    let base = format!("{} {}", MONTHS[d.month0() as usize], d.day());
    if d.year() == today.year() { base } else { format!("{base}, {}", d.year()) }
}

/// The small "when" under a time entry's duration.
pub fn relative(s: &str, now: DateTime<Local>) -> String {
    let Some(t) = parse_time(s) else { return String::new() };
    let diff_ms = (now - t).num_milliseconds();
    let mins = diff_ms.div_euclid(60_000);
    let hours = diff_ms.div_euclid(3_600_000);
    let days = (now.date_naive() - t.date_naive()).num_days();
    if mins < 1 {
        "Just now".into()
    } else if mins < 60 {
        format!("{mins}m ago")
    } else if hours < 24 && days == 0 {
        format!("{hours}h ago")
    } else if days == 1 {
        "Yesterday".into()
    } else if days < 7 {
        format!("{days}d ago")
    } else {
        friendly_day(t.date_naive(), now.date_naive())
    }
}

pub fn hour_label(h: i64) -> String {
    match h % 24 {
        0 => "12 AM".into(),
        12 => "Noon".into(),
        hr if hr < 12 => format!("{hr} AM"),
        hr => format!("{} PM", hr - 12),
    }
}
