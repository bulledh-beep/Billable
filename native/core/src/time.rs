//! "Now" and the JavaScript date behaviour the Electron code relies on.
//!
//! Times are milliseconds since the Unix epoch, like a JS `Date`. Local dates
//! use the process time zone (`TZ`, else the system zone), like the Electron
//! main process. Parsing follows V8's `Date.parse`: ISO date-only strings are
//! UTC, ISO date-times without an offset and SQLite-style
//! `"YYYY-MM-DD HH:MM:SS"` strings are local time.

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

use chrono::{DateTime, Datelike, Local, NaiveDate, NaiveDateTime, NaiveTime, Offset, TimeZone, Timelike, Utc};

/// Source of "now". [`Clock::system`] in the app; [`Clock::fixed`] in tests so
/// results are deterministic.
#[derive(Clone, Debug, Default)]
pub struct Clock {
    fixed: Option<Arc<AtomicI64>>,
}

impl Clock {
    /// The real wall clock.
    pub fn system() -> Self {
        Clock { fixed: None }
    }

    /// A clock stopped at `ms` (epoch milliseconds). Clones share the time.
    pub fn fixed(ms: i64) -> Self {
        Clock { fixed: Some(Arc::new(AtomicI64::new(ms))) }
    }

    /// A clock stopped at an ISO-8601 instant such as `2026-11-08T21:09:55.123Z`.
    pub fn fixed_at(iso: &str) -> anyhow::Result<Self> {
        let ms = js_date_parse(iso);
        anyhow::ensure!(ms.is_finite(), "invalid time {iso}");
        Ok(Self::fixed(ms as i64))
    }

    /// Move a fixed clock to `ms`. No effect on the system clock.
    pub fn set_ms(&self, ms: i64) {
        if let Some(f) = &self.fixed {
            f.store(ms, Ordering::SeqCst);
        }
    }

    /// Move a fixed clock forward. No effect on the system clock.
    pub fn advance_ms(&self, delta: i64) {
        if let Some(f) = &self.fixed {
            f.fetch_add(delta, Ordering::SeqCst);
        }
    }

    /// Now, in epoch milliseconds (`Date.now()`).
    pub fn now_ms(&self) -> i64 {
        match &self.fixed {
            Some(f) => f.load(Ordering::SeqCst),
            None => Utc::now().timestamp_millis(),
        }
    }

    /// Now as a UTC date-time (millisecond precision).
    pub fn now(&self) -> DateTime<Utc> {
        ms_to_utc(self.now_ms())
    }

    /// `new Date().toISOString()`.
    pub fn now_iso(&self) -> String {
        iso_string(self.now_ms())
    }

    /// The user's calendar day, `YYYY-MM-DD` (database.ts `localToday`).
    pub fn local_today(&self) -> String {
        local_date(self.now_ms() as f64)
    }

    /// `new Date().getFullYear()` in local time.
    pub fn local_year(&self) -> i32 {
        local_parts(self.now_ms()).0
    }
}

pub(crate) fn ms_to_utc(ms: i64) -> DateTime<Utc> {
    DateTime::<Utc>::from_timestamp_millis(ms).unwrap_or_default()
}

/// `Date.prototype.toISOString()`, e.g. `2026-11-08T21:09:55.123Z`.
pub fn iso_string(ms: i64) -> String {
    let d = ms_to_utc(ms);
    let year = d.year();
    let y = if (0..=9999).contains(&year) {
        format!("{year:04}")
    } else if year < 0 {
        format!("-{:06}", -year)
    } else {
        format!("+{year:06}")
    };
    format!(
        "{y}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        d.month(),
        d.day(),
        d.hour(),
        d.minute(),
        d.second(),
        d.timestamp_subsec_millis()
    )
}

/// Local (year, month 1-12, day, hour, minute, second, millisecond) of an instant.
fn local_parts(ms: i64) -> (i32, u32, u32, u32, u32, u32, u32) {
    let d = ms_to_utc(ms).with_timezone(&Local);
    (d.year(), d.month(), d.day(), d.hour(), d.minute(), d.second(), d.timestamp_subsec_millis())
}

/// database.ts `localDate(d)`: the local calendar day `YYYY-MM-DD`
/// (`"NaN-NaN-NaN"` for an invalid date, as in JS).
pub fn local_date(ms: f64) -> String {
    if !ms.is_finite() {
        return "NaN-NaN-NaN".into();
    }
    let (y, m, d, ..) = local_parts(ms as i64);
    format!("{y}-{m:02}-{d:02}")
}

/// Local wall time → epoch ms, resolving DST like V8: a time skipped by a
/// spring-forward uses the offset from before the change; a repeated time
/// uses the earlier instant.
pub(crate) fn local_to_ms(naive: NaiveDateTime) -> f64 {
    use chrono::LocalResult;
    let ms = match Local.from_local_datetime(&naive) {
        LocalResult::Single(d) => d.timestamp_millis(),
        LocalResult::Ambiguous(a, b) => a.timestamp_millis().min(b.timestamp_millis()),
        LocalResult::None => {
            let before = naive - chrono::Duration::days(1);
            let offset = match Local.from_local_datetime(&before) {
                LocalResult::Single(d) | LocalResult::Ambiguous(d, _) => d.offset().fix().local_minus_utc(),
                LocalResult::None => 0,
            };
            naive.and_utc().timestamp_millis() - offset as i64 * 1000
        }
    };
    ms as f64
}

/// `MakeDay`/`MakeDate` with overflow (day 31 of February rolls into March).
fn make_naive(year: i64, month: i64, day: i64, h: i64, mi: i64, s: i64, ms: i64) -> Option<NaiveDateTime> {
    let ym = year * 12 + (month - 1);
    let (y, m0) = (ym.div_euclid(12), ym.rem_euclid(12));
    let first = NaiveDate::from_ymd_opt(i32::try_from(y).ok()?, m0 as u32 + 1, 1)?;
    let date = first.checked_add_signed(chrono::Duration::days(day - 1))?;
    let base = date.and_time(NaiveTime::MIN);
    base.checked_add_signed(chrono::Duration::milliseconds(((h * 60 + mi) * 60 + s) * 1000 + ms))
}

/// `new Date(y, m0, d, h, mi, s, ms)` (local time, month 0-based, with overflow).
pub(crate) fn local_ms_from_parts(year: i64, month0: i64, day: i64, h: i64, mi: i64, s: i64, ms: i64) -> f64 {
    match make_naive(year, month0 + 1, day, h, mi, s, ms) {
        Some(n) => local_to_ms(n),
        None => f64::NAN,
    }
}

/// `d.setDate(d.getDate() + days)` on a local date-time.
pub(crate) fn local_add_days(ms: f64, days: i64) -> f64 {
    if !ms.is_finite() {
        return f64::NAN;
    }
    let (y, m, d, h, mi, s, milli) = local_parts(ms as i64);
    local_ms_from_parts(y as i64, m as i64 - 1, d as i64 + days, h as i64, mi as i64, s as i64, milli as i64)
}

/// Local midnight of the day containing `ms` (`d.setHours(0, 0, 0, 0)`).
pub(crate) fn local_start_of_day(ms: i64) -> f64 {
    let (y, m, d, ..) = local_parts(ms);
    local_ms_from_parts(y as i64, m as i64 - 1, d as i64, 0, 0, 0, 0)
}

/// Local (year, month 1-12, day, weekday 0=Sunday).
pub(crate) fn local_ymd_dow(ms: i64) -> (i32, u32, u32, u32) {
    let d = ms_to_utc(ms).with_timezone(&Local);
    (d.year(), d.month(), d.day(), d.weekday().num_days_from_sunday())
}

struct Cursor<'a> {
    b: &'a [u8],
    i: usize,
}

impl Cursor<'_> {
    fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }
    fn eat(&mut self, c: u8) -> bool {
        if self.peek() == Some(c) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    /// Exactly `n` digits.
    fn digits(&mut self, n: usize) -> Option<i64> {
        let s = self.b.get(self.i..self.i + n)?;
        if !s.iter().all(u8::is_ascii_digit) {
            return None;
        }
        self.i += n;
        Some(s.iter().fold(0, |v, d| v * 10 + (d - b'0') as i64))
    }
    /// One or more digits; returns (value, count).
    fn some_digits(&mut self, max: usize) -> Option<(i64, usize)> {
        let start = self.i;
        while self.i - start < max && self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.i += 1;
        }
        let n = self.i - start;
        (n > 0).then(|| (self.b[start..self.i].iter().fold(0, |v, d| v * 10 + (d - b'0') as i64), n))
    }
    fn done(&self) -> bool {
        self.i >= self.b.len()
    }
}

/// Milliseconds from a fraction's digits (`.5` → 500, `.6956789` → 695).
fn fraction_ms(c: &mut Cursor) -> Option<i64> {
    let start = c.i;
    while c.peek().is_some_and(|ch| ch.is_ascii_digit()) {
        c.i += 1;
    }
    let digits = &c.b[start..c.i];
    if digits.is_empty() {
        return None;
    }
    let mut ms = 0;
    for k in 0..3 {
        ms = ms * 10 + digits.get(k).map(|d| (d - b'0') as i64).unwrap_or(0);
    }
    Some(ms)
}

/// `Z`, `±HH:MM` or `±HHMM` → offset seconds east of UTC.
fn offset_secs(c: &mut Cursor) -> Option<Option<i64>> {
    match c.peek() {
        Some(b'Z') | Some(b'z') => {
            c.i += 1;
            Some(Some(0))
        }
        Some(sign @ (b'+' | b'-')) => {
            c.i += 1;
            let h = c.digits(2)?;
            c.eat(b':');
            let m = c.digits(2)?;
            let s = (h * 60 + m) * 60;
            Some(Some(if sign == b'-' { -s } else { s }))
        }
        _ => Some(None),
    }
}

#[allow(clippy::too_many_arguments)]
fn finish(year: i64, month: i64, day: i64, h: i64, mi: i64, s: i64, ms: i64, offset: Option<i64>) -> f64 {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || h > 24 || mi > 59 || s > 59 {
        return f64::NAN;
    }
    if h == 24 && (mi > 0 || s > 0 || ms > 0) {
        return f64::NAN;
    }
    let Some(naive) = make_naive(year, month, day, h, mi, s, ms) else { return f64::NAN };
    match offset {
        Some(off) => (naive.and_utc().timestamp_millis() - off * 1000) as f64,
        None => local_to_ms(naive),
    }
}

/// ES5 ISO format: `YYYY[-MM[-DD]][THH:MM[:SS[.fff]][Z|±HH:MM]]`.
fn parse_iso(s: &str) -> Option<f64> {
    let mut c = Cursor { b: s.as_bytes(), i: 0 };
    let year = c.digits(4)?;
    let (mut month, mut day) = (1, 1);
    if c.eat(b'-') {
        month = c.digits(2)?;
        if c.eat(b'-') {
            day = c.digits(2)?;
        }
    }
    if c.done() {
        // Date-only forms are UTC
        return Some(finish(year, month, day, 0, 0, 0, 0, Some(0)));
    }
    if !(c.eat(b'T') || c.eat(b't')) {
        return None;
    }
    let h = c.digits(2)?;
    if !c.eat(b':') {
        return None;
    }
    let mi = c.digits(2)?;
    let (mut sec, mut ms) = (0, 0);
    if c.eat(b':') {
        sec = c.digits(2)?;
        if c.eat(b'.') {
            ms = fraction_ms(&mut c)?;
        }
    }
    let offset = offset_secs(&mut c)?;
    if !c.done() {
        return None;
    }
    Some(finish(year, month, day, h, mi, sec, ms, offset))
}

/// V8's legacy fallback for the shapes that occur in practice:
/// `YYYY-M-D`, `YYYY/M/D`, optionally followed by ` H:MM[:SS[.f]]` and a zone.
fn parse_legacy(s: &str) -> Option<f64> {
    let s = s.trim_matches(|c: char| c.is_ascii_whitespace());
    let mut c = Cursor { b: s.as_bytes(), i: 0 };
    let (year, _) = c.some_digits(6)?;
    let sep = c.peek()?;
    if sep != b'-' && sep != b'/' {
        return None;
    }
    c.i += 1;
    let (month, _) = c.some_digits(2)?;
    if !c.eat(sep) {
        return None;
    }
    let (day, _) = c.some_digits(2)?;
    let (mut h, mut mi, mut sec, mut ms, mut offset) = (0, 0, 0, 0, None);
    if !c.done() {
        if !(c.eat(b' ') || c.eat(b'T') || c.eat(b't')) {
            return None;
        }
        while c.eat(b' ') {}
        let (hh, _) = c.some_digits(2)?;
        if !c.eat(b':') {
            return None;
        }
        mi = c.digits(2)?;
        h = hh;
        if c.eat(b':') {
            sec = c.digits(2)?;
            if c.eat(b'.') {
                ms = fraction_ms(&mut c)?;
            }
        }
        while c.eat(b' ') {}
        let rest = &s[c.i..];
        if rest.eq_ignore_ascii_case("utc") || rest.eq_ignore_ascii_case("gmt") {
            offset = Some(0);
            c.i = s.len();
        } else {
            offset = offset_secs(&mut c)?;
        }
        if !c.done() {
            return None;
        }
    }
    Some(finish(year, month, day, h, mi, sec, ms, offset))
}

/// `Date.parse(s)`: epoch ms, or `NaN` when JS can't read it.
pub fn js_date_parse(s: &str) -> f64 {
    parse_iso(s).or_else(|| parse_legacy(s)).unwrap_or(f64::NAN)
}

/// database.ts `sqliteTimeMs`: SQLite `"YYYY-MM-DD HH:MM:SS"` (UTC) or ISO
/// strings to ms; 0 when missing or unreadable.
pub fn sqlite_time_ms(value: Option<&str>) -> f64 {
    let Some(s) = value.filter(|s| !s.is_empty()) else { return 0.0 };
    let b = s.as_bytes();
    let sqlite_shape = b.len() >= 16
        && b[..4].iter().all(u8::is_ascii_digit)
        && b[4] == b'-'
        && b[5..7].iter().all(u8::is_ascii_digit)
        && b[7] == b'-'
        && b[8..10].iter().all(u8::is_ascii_digit)
        && b[10] == b' '
        && b[11..13].iter().all(u8::is_ascii_digit)
        && b[13] == b':'
        && b[14..16].iter().all(u8::is_ascii_digit);
    let t = if sqlite_shape { js_date_parse(&format!("{}Z", s.replacen(' ', "T", 1))) } else { js_date_parse(s) };
    if t.is_finite() {
        t
    } else {
        0.0
    }
}

/// database.ts `localRangeToUtc`: UTC instants bounding a local-date range,
/// end date inclusive. Errors like `toISOString()` on an invalid date.
pub fn local_range_to_utc(start_date: &str, end_date: &str) -> anyhow::Result<(String, String)> {
    let start = js_date_parse(&format!("{start_date}T00:00:00"));
    let end = local_add_days(js_date_parse(&format!("{end_date}T00:00:00")), 1);
    anyhow::ensure!(start.is_finite() && end.is_finite(), "Invalid time value");
    Ok((iso_string(start as i64), iso_string(end as i64)))
}

/// Whole days from one `YYYY-MM-DD` to another, `None` if either is unreadable.
pub fn days_between(from_day: &str, to_day: &str) -> Option<i64> {
    let a = js_date_parse(&format!("{to_day}T00:00:00"));
    let b = js_date_parse(&format!("{from_day}T00:00:00"));
    crate::js::js_round_i64((a - b) / 86_400_000.0)
}

/// Invoice status as the user sees it: a sent invoice past its due date is
/// overdue (database.ts `effectiveInvoiceStatus`).
pub fn effective_invoice_status(stored: Option<&str>, due_date: Option<&str>, today: &str) -> &'static str {
    match stored {
        Some("paid") => "paid",
        Some("draft") => "draft",
        _ => match due_date {
            Some(d) if !d.is_empty() && crate::js::js_slice(d, 10) < today => "overdue",
            _ => "sent",
        },
    }
}

/// database.ts `yearOf`: year of a `YYYY-MM-DD` string, else the current local year.
pub fn year_of(date: Option<&str>, clock: &Clock) -> i64 {
    let y = crate::js::js_parse_int(crate::js::js_slice(date.unwrap_or(""), 4));
    if y.is_finite() && y > 1900.0 {
        y as i64
    } else {
        clock.local_year() as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// These cases were checked against Node 24 (V8) with TZ=America/Edmonton.
    #[test]
    fn date_parse_matches_v8() {
        std::env::set_var("TZ", "America/Edmonton");
        let cases: &[(&str, f64)] = &[
            ("2026-03-15T01:19:53.695Z", 1773537593695.0),
            ("2026-03-15", 1773532800000.0),
            ("2026-03-15T00:00:00", 1773554400000.0),
            ("2026-03-15 01:19:53", 1773559193000.0),
            ("2026-03-15 01:19:53Z", 1773537593000.0),
            ("2026-03-15T01:19", 1773559140000.0),
            ("2026-02-30", 1772409600000.0),
            ("2026-02-30T00:00:00", 1772434800000.0),
            ("2026/03/15", 1773554400000.0),
            ("2026-3-5", 1772694000000.0),
            ("2026-03-15T01:19:53.6956789Z", 1773537593695.0),
            ("2026-03-15T24:00:00", 1773640800000.0),
            ("2026-03-15T01:19:53+02:00", 1773530393000.0),
            ("2026-03-15T01:19:53+0200", 1773530393000.0),
            (" 2026-03-15", 1773554400000.0),
            ("2026-03-15 ", 1773554400000.0),
            ("2026", 1767225600000.0),
            ("2026-03", 1772323200000.0),
            ("2026-03-15 01:19", 1773559140000.0),
            ("2026-03-15T01:19:53.5", 1773559193500.0),
            ("2026-03-08T02:30:00", 1772962200000.0),
            ("2026-11-01T01:30:00", 1793518200000.0),
            ("2026-03-15 1:19:53", 1773559193000.0),
            ("2026-03-15T01:19:53z", 1773537593000.0),
            ("2026-03-15t01:19:53Z", 1773537593000.0),
            ("2026-04-31", 1777593600000.0),
        ];
        for (s, want) in cases {
            assert_eq!(js_date_parse(s), *want, "{s}");
        }
        for s in ["2026-13-01", "abc", "", "2026-03-15T25:00:00", "NaN-NaN-NaNT00:00:00"] {
            assert!(js_date_parse(s).is_nan(), "{s}");
        }
    }

    #[test]
    fn iso_and_sqlite_times() {
        assert_eq!(iso_string(1773537593695), "2026-03-15T01:19:53.695Z");
        assert_eq!(sqlite_time_ms(Some("2026-03-15 01:19:53")), 1773537593000.0);
        assert_eq!(sqlite_time_ms(Some("")), 0.0);
        assert_eq!(sqlite_time_ms(Some("garbage")), 0.0);
    }
}
