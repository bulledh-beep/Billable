//! The money rules from Commissions.tsx: status labels and colours, the
//! payout a job counts for, and the mutually exclusive stage buckets
//! (Active, Owed, Invoiced, Paid) used by the tabs, metrics and table.

use billable_core::models::Commission;
use gpui::{Entity, Hsla};
use gpui_component::input::InputState;

use crate::{format::js_round, theme::Theme};

pub const STATUS_OPTIONS: [(&str, &str); 7] = [
    ("appointment_set", "Appointment set"),
    ("appointment_attended", "Attended"),
    ("closed_waiting", "Closed, unpaid"),
    ("paid", "Paid"),
    ("lost", "Lost"),
    ("cancelled", "Cancelled"),
    ("needs_review", "Needs review"),
];

pub const PAYMENT_OPTIONS: [(&str, &str); 3] = [("unpaid", "Unpaid"), ("pending", "Pending"), ("paid", "Paid")];

pub fn status_label(s: &str) -> String {
    STATUS_OPTIONS.iter().find(|o| o.0 == s).map(|o| o.1.to_string()).unwrap_or_else(|| s.to_string())
}

pub fn payment_label(p: &str) -> String {
    PAYMENT_OPTIONS.iter().find(|o| o.0 == p).map(|o| o.1.to_string()).unwrap_or_else(|| p.to_string())
}

/// STATUS_STYLE: the badge colour for a deal status.
pub fn status_color(t: &Theme, s: &str) -> Hsla {
    match s {
        "appointment_set" => t.blue,
        "appointment_attended" => t.violet,
        "closed_waiting" => t.amber,
        "paid" => t.green,
        "lost" => t.red,
        "cancelled" => t.fg3,
        "needs_review" => t.accent_text,
        _ => t.fg,
    }
}

/// INVOICE_STATUS_STYLE
pub fn invoice_status_color(t: &Theme, s: &str) -> Hsla {
    match s {
        "draft" => t.fg2,
        "sent" => t.blue,
        "paid" => t.green,
        "cancelled" => t.red,
        _ => t.fg,
    }
}

/// "draft" → "Draft".
pub fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

pub fn category_label(category: &str) -> &'static str {
    match category {
        "mixed" => "Solar and roofing",
        "solar" => "Solar",
        _ => "Roofing",
    }
}

/// The payout a job counts for: the override wins; a job in the roofing
/// review gap has none.
pub fn effective(c: &Commission) -> Option<f64> {
    if let Some(o) = c.manual_override {
        return Some(o);
    }
    if c.needs_review != 0 {
        return None;
    }
    Some(c.calculated_commission)
}

pub fn effective_or_zero(c: &Commission) -> f64 {
    effective(c).unwrap_or(0.0)
}

pub fn is_paid(c: &Commission) -> bool {
    c.payment_status == "paid"
}

pub fn is_invoiced(c: &Commission) -> bool {
    !is_paid(c) && c.invoice_status.as_deref() == Some("invoiced")
}

pub fn is_owed(c: &Commission) -> bool {
    !is_paid(c) && c.status == "closed_waiting" && c.invoice_status.as_deref() != Some("invoiced")
}

pub fn is_active(c: &Commission) -> bool {
    !is_paid(c) && matches!(c.status.as_str(), "appointment_set" | "appointment_attended" | "needs_review")
}

pub fn is_earned(c: &Commission) -> bool {
    c.status == "closed_waiting" || c.status == "paid" || is_paid(c)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stage {
    Active,
    Owed,
    Invoiced,
    Paid,
    All,
}

pub const STAGE_TABS: [(Stage, &str); 5] =
    [(Stage::Active, "Active"), (Stage::Owed, "Owed"), (Stage::Invoiced, "Invoiced"), (Stage::Paid, "Paid"), (Stage::All, "All")];

pub fn stage_label(s: Stage) -> &'static str {
    STAGE_TABS.iter().find(|t| t.0 == s).map(|t| t.1).unwrap_or("")
}

pub fn in_stage(c: &Commission, stage: Stage) -> bool {
    match stage {
        Stage::All => true,
        Stage::Active => is_active(c),
        Stage::Owed => is_owed(c),
        Stage::Invoiced => is_invoiced(c),
        Stage::Paid => is_paid(c),
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tone {
    Potential,
    Owed,
    Invoiced,
    Paid,
    Review,
}

/// TONE_TEXT
pub fn tone_color(t: &Theme, tone: Tone) -> Hsla {
    match tone {
        Tone::Potential => t.fg2,
        Tone::Owed => t.amber,
        Tone::Invoiced => t.blue,
        Tone::Paid => t.green,
        Tone::Review => t.amber,
    }
}

/// Where a job's money is: (label, value, tone).
pub fn payout_info(c: &Commission) -> (&'static str, Option<f64>, Tone) {
    if c.needs_review != 0 && c.manual_override.is_none() {
        return ("Needs review", None, Tone::Review);
    }
    let v = effective_or_zero(c);
    if is_paid(c) {
        ("Paid", Some(v), Tone::Paid)
    } else if is_invoiced(c) {
        ("Invoiced", Some(v), Tone::Invoiced)
    } else if is_owed(c) {
        ("Owed", Some(v), Tone::Owed)
    } else {
        ("Potential", Some(v), Tone::Potential)
    }
}

/// `previewCommission`: the payout the form's numbers give (strings are
/// `<input type=number>` values, "" when empty or invalid).
pub fn preview_commission(job_type: &str, kw: &str, contract: &str, manual: &str) -> (Option<f64>, bool) {
    if !manual.is_empty() && js_number(manual) >= 0.0 {
        return (Some(js_number(manual)), false);
    }
    if job_type == "roofing" {
        if contract.is_empty() {
            return (None, false);
        }
        let amt = js_number(contract);
        if amt <= 20000.0 {
            return (Some(250.0), false);
        }
        if amt >= 30000.0 {
            return (Some(500.0), false);
        }
        return (None, true);
    }
    if kw.is_empty() {
        return (None, false);
    }
    (Some(js_round(js_number(kw) * 50.0 * 100.0) / 100.0), false)
}

/// `Number(s) || 0` for a valid number string.
pub fn js_number(s: &str) -> f64 {
    s.trim().parse::<f64>().ok().filter(|v| !v.is_nan()).unwrap_or(0.0)
}

/// Whether `s` is an HTML "valid floating-point number" (what a number
/// input accepts as its value).
pub fn valid_number(s: &str) -> bool {
    let b = s.as_bytes();
    let n = b.len();
    let mut i = 0;
    if i < n && b[i] == b'-' {
        i += 1;
    }
    let int_start = i;
    while i < n && b[i].is_ascii_digit() {
        i += 1;
    }
    let int_digits = i - int_start;
    let mut frac_digits = 0;
    if i < n && b[i] == b'.' {
        i += 1;
        let fs = i;
        while i < n && b[i].is_ascii_digit() {
            i += 1;
        }
        frac_digits = i - fs;
        if frac_digits == 0 {
            return false;
        }
    }
    if int_digits == 0 && frac_digits == 0 {
        return false;
    }
    if i < n && (b[i] == b'e' || b[i] == b'E') {
        i += 1;
        if i < n && (b[i] == b'-' || b[i] == b'+') {
            i += 1;
        }
        let es = i;
        while i < n && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == es {
            return false;
        }
    }
    i == n
}

/// A number field's value as `<input type=number>` reports it: the text when
/// it's a valid number, else "".
pub fn number_value(state: &Entity<InputState>, cx: &gpui::App) -> String {
    let v = state.read(cx).value().to_string();
    if valid_number(&v) {
        v
    } else {
        String::new()
    }
}

/// Characters Chromium lets you type into a number input.
pub fn number_chars(s: &str) -> bool {
    s.chars().all(|c| c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | 'e' | 'E'))
}

/// `String(n)` for a stored number: 6 → "6", 6.16 → "6.16".
pub fn js_string(v: f64) -> String {
    crate::components::client_form::trim_number(v)
}

/// `todayISO()`: today's local date as YYYY-MM-DD.
pub fn today_iso() -> String {
    chrono::Local::now().date_naive().format("%Y-%m-%d").to_string()
}

/// "Jun 12" or "—" for a job's appointment.
pub fn appointment_day(c: &Commission) -> String {
    match c.appointment_date.as_deref() {
        Some(d) if !d.is_empty() => crate::format::format_day(d),
        _ => "—".into(),
    }
}

/// The detail line under a job in the invoice lists: "Jun 12 · 6.16 kW".
pub fn job_detail(c: &Commission) -> String {
    let size = if c.job_type == "solar" {
        format!("{} kW", js_string(c.system_size_kw.unwrap_or(0.0)))
    } else {
        crate::format::money(c.contract_amount.unwrap_or(0.0))
    };
    format!("{} · {}", appointment_day(c), size)
}
