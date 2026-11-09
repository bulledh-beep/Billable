//! StatusBadge.tsx: tinted pills for statuses, the quiet billing chip, and
//! the red count bubble.

use gpui::{div, px, Div, Hsla, IntoElement, ParentElement, SharedString, Styled};

use crate::{
    style::{dot, r, text},
    theme::{Theme, Tone},
};

/// `.badge`: currentColor text on a 13% tint of itself.
pub fn badge(t: &Theme, tone: Tone, label: impl Into<SharedString>, with_dot: bool) -> Div {
    let c = t.tone(tone);
    let bg_alpha = match tone {
        Tone::Accent | Tone::Amber => 0.14,
        Tone::Gray => 0.07,
        _ => 0.13,
    };
    // Tailwind's bg-*/[.13] tints the tone colour itself; gray tints fg.
    let bg = if tone == Tone::Gray { t.fg.opacity(bg_alpha) } else { tone_base(t, tone).opacity(bg_alpha) };
    let mut b = div().flex().flex_none().items_center().gap(r(1.)).h(px(22.)).px(r(2.5)).rounded_full().bg(bg);
    if with_dot {
        b = b.child(dot(if tone == Tone::Gray { t.fg3 } else { tone_base(t, tone) }, r(1.5)));
    }
    b.child(text(label, 12.0, 600, c).whitespace_nowrap().line_height(px(16.)))
}

fn tone_base(t: &Theme, tone: Tone) -> Hsla {
    match tone {
        Tone::Accent => t.accent,
        Tone::Green => t.green,
        Tone::Blue => t.blue,
        Tone::Red => t.red,
        Tone::Amber => t.amber,
        Tone::Violet => t.violet,
        Tone::Gray => t.fg,
    }
}

/// Invoice and project statuses.
pub fn status_tone(status: &str) -> (Tone, &'static str) {
    match status {
        "draft" => (Tone::Gray, "Draft"),
        "sent" => (Tone::Blue, "Sent"),
        "overdue" => (Tone::Red, "Overdue"),
        "paid" => (Tone::Green, "Paid"),
        "active" => (Tone::Green, "Active"),
        "paused" => (Tone::Amber, "Paused"),
        "complete" => (Tone::Blue, "Complete"),
        "archived" => (Tone::Gray, "Archived"),
        _ => (Tone::Gray, ""),
    }
}

pub fn status_badge(t: &Theme, status: &str) -> Div {
    let (tone, label) = status_tone(status);
    let label: SharedString = if label.is_empty() { status.to_string().into() } else { label.into() };
    badge(t, tone, label, false)
}

/// Text colour for a tone in quiet text (TONE_TEXT in StatusBadge.tsx).
pub fn tone_text(t: &Theme, tone: Tone) -> Hsla {
    match tone {
        Tone::Accent => t.accent_text,
        Tone::Green => t.green,
        Tone::Blue => t.blue,
        Tone::Red => t.red,
        Tone::Amber => t.amber,
        Tone::Violet => t.violet,
        Tone::Gray => t.fg3,
    }
}

/// Where a time entry's money is (BillingChip). `None` for unbilled time.
pub fn billing_chip(t: &Theme, state: &str, invoice_number: Option<&str>) -> Option<Div> {
    let (tone, label) = match state {
        "unbilled" | "" => return None,
        "nonbillable" => return Some(text("Not billable", 12.0, 500, t.fg4).whitespace_nowrap().line_height(px(16.))),
        "draft" => (Tone::Gray, "Draft"),
        "sent" => (Tone::Blue, "Sent"),
        "overdue" => (Tone::Red, "Overdue"),
        "paid" => (Tone::Green, "Paid"),
        "invoiced" => (Tone::Blue, "Invoiced"),
        _ => return None,
    };
    let mut chip = div().flex().flex_none().whitespace_nowrap().child(text(label, 12.0, 500, tone_text(t, tone)).line_height(px(16.)));
    if let Some(n) = invoice_number {
        chip = chip.child(text(format!(" · {n}"), 12.0, 500, t.fg3).line_height(px(16.)));
    }
    Some(chip)
}

/// The red count bubble (sidebar Billing, "Needs attention").
pub fn count_badge(t: &Theme, n: usize) -> impl IntoElement {
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .min_w(px(20.))
        .h(px(20.))
        .px(r(1.5))
        .rounded_full()
        .bg(t.red)
        .child(div().font(crate::style::num_font(700)).text_size(px(11.5)).text_color(crate::theme::hex(0xffffff)).line_height(px(20.)).child(n.to_string()))
}
