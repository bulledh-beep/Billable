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

/// Standard base64 (profile photos in profiles.json).
pub fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut buf, mut bits) = (0u32, 0u32);
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' | b'\n' | b'\r' | b' ' => continue,
            _ => return None,
        } as u32;
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// Parse "YYYY-MM-DD" as a local day, SQLite "YYYY-MM-DD HH:MM:SS" as UTC, else ISO (toDate).
pub fn to_day(s: &str) -> Option<NaiveDate> {
    if s.len() == 10 {
        if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
            return Some(d);
        }
    }
    local_day(s)
}

/// Whole days from `s` to today (positive = in the past).
pub fn days_since(s: &str) -> i64 {
    to_day(s).map(|d| (Local::now().date_naive() - d).num_days()).unwrap_or(0)
}

/// "today", "yesterday", "12 days ago", "in 3 days".
pub fn relative_days(s: &str) -> String {
    match days_since(s) {
        0 => "today".into(),
        1 => "yesterday".into(),
        -1 => "tomorrow".into(),
        n if n > 1 => format!("{n} days ago"),
        n => format!("in {} days", -n),
    }
}

/// "Oct 19, 2026" (formatDate).
pub fn format_date(s: &str) -> String {
    to_day(s).map(|d| format!("{} {}, {}", MONTHS[d.month0() as usize], d.day(), d.year())).unwrap_or_default()
}

/// "Oct 19", or "Oct 19, 2025" outside this year (formatDay).
pub fn format_day(s: &str) -> String {
    to_day(s).map(|d| friendly_day(d, Local::now().date_naive())).unwrap_or_default()
}

/// "3:05 PM" (formatTime).
pub fn format_time(s: &str) -> String {
    parse_time(s).map(|t| t.format("%-I:%M %p").to_string()).unwrap_or_default()
}

/// "$1,154" / "$12.4k" for tight spots (formatMoneyCompact).
pub fn money_compact(amount: f64) -> String {
    if amount.abs() >= 10_000.0 {
        let k = format!("{:.1}", amount / 1000.0);
        return format!("${}k", k.trim_end_matches(".0"));
    }
    let (whole, _) = money_parts(js_round(amount));
    whole
}

/// "JC" for "Jane Cooper" (getInitials).
pub fn initials(name: &str) -> String {
    name.split(' ').filter_map(|w| w.chars().next()).collect::<String>().to_uppercase().chars().take(2).collect()
}

/// JavaScript's string hash used for avatar colours (charCode + ((h << 5) - h)), as an i32.
pub fn js_name_hash(name: &str) -> i32 {
    let mut h: i32 = 0;
    for c in name.encode_utf16() {
        h = (c as i32).wrapping_add(h.wrapping_shl(5).wrapping_sub(h));
    }
    h
}
