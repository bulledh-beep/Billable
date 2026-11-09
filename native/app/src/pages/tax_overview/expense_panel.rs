//! ExpensePanel.tsx: the slide-in panel for adding, editing and deleting an
//! expense. It slides in from the right over a dimmed window (framer-motion's
//! spring, damping 28, stiffness 280) and back out when closed. The page keeps
//! `Option<Entity<ExpensePanel>>`, subscribes to [`ExpensePanelEvent`], and
//! calls [`ExpensePanel::close`] to play the exit before dropping it.

use std::time::Instant;

use billable_core::models::{Expense, ExpenseInput, ExpensePatch};
use chrono::{Datelike, Local, NaiveDate};
use gpui::{
    anchored, deferred, div, point, prelude::*, px, AppContext, Context, Entity, EventEmitter, IntoElement, ParentElement,
    PromptLevel, Render, SharedString, Styled, Subscription, Window,
};
use gpui_component::input::InputState;

use crate::{
    components::{
        client_form::trim_number,
        fields::TextField,
        select, toast_error, toast_success, Button, DateChanged, DateInput,
    },
    icons::Lucide,
    motion::{self, cubic_bezier, spring},
    store,
    style::{hint, label, r, text},
    theme::theme,
};

pub enum ExpensePanelEvent {
    Saved,
    Deleted,
    Closed,
}

/// `CATEGORIES`: value, label, hint.
pub const CATEGORIES: [(&str, &str, Option<&str>); 8] = [
    ("equipment", "Equipment and hardware", None),
    ("software", "Software and subscriptions", None),
    ("home_office", "Home office", Some("Enter only the share of your home used for work.")),
    ("phone_internet", "Phone and internet", None),
    ("travel", "Travel", None),
    ("meals", "Meals and entertainment", Some("Only half is deductible, so enter the deductible half.")),
    ("professional_development", "Professional development", None),
    ("other", "Other", None),
];

/// `yearOf(date)`: the date's year, or this year.
fn year_of(date: &str) -> i64 {
    js_parse_int(date.get(..4).unwrap_or(date)).filter(|y| *y != 0).unwrap_or_else(|| Local::now().year() as i64)
}

/// `parseInt`: leading digits (with a sign), or NaN (`None`).
fn js_parse_int(s: &str) -> Option<i64> {
    let s = s.trim_start();
    let (sign, rest) = match s.strip_prefix('-') {
        Some(r) => (-1, r),
        None => (1, s.strip_prefix('+').unwrap_or(s)),
    };
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse::<i64>().ok().map(|v| sign * v)
}

/// `parseFloat(value) || 0` for a number field.
pub fn number_value(s: &str) -> f64 {
    let s = s.trim();
    let mut end = 0;
    let mut seen_dot = false;
    let mut seen_digit = false;
    let mut seen_exp = false;
    let bytes = s.as_bytes();
    while end < bytes.len() {
        let c = bytes[end] as char;
        let ok = match c {
            '0'..='9' => {
                seen_digit = true;
                true
            }
            '.' if !seen_dot && !seen_exp => {
                seen_dot = true;
                true
            }
            '-' | '+' => end == 0 || matches!(bytes[end - 1] as char, 'e' | 'E'),
            'e' | 'E' if seen_digit && !seen_exp => {
                seen_exp = true;
                true
            }
            _ => false,
        };
        if !ok {
            break;
        }
        end += 1;
    }
    let mut num = &s[..end];
    while !num.is_empty() && num.parse::<f64>().is_err() {
        num = &num[..num.len() - 1];
    }
    let v = num.parse::<f64>().unwrap_or(0.0);
    if v.is_nan() { 0.0 } else { v }
}

pub struct ExpensePanel {
    expense: Option<Expense>,
    date: Entity<DateInput>,
    /// The `<input type="date">` value: "YYYY-MM-DD", or "" when incomplete.
    date_value: String,
    category: String,
    description: Entity<InputState>,
    amount: Entity<InputState>,
    receipt: Entity<InputState>,
    opened: Instant,
    /// When closing began, and (x, opacity, backdrop) at that moment.
    closing: Option<(Instant, (f32, f32, f32))>,
    _subs: Vec<Subscription>,
}

impl EventEmitter<ExpensePanelEvent> for ExpensePanel {}

const TRAVEL: f32 = 320.0;
/// How long the exit runs before the page drops the panel: framer-motion
/// removes it once the spring settles (within 0.5px), about half a second.
pub const EXIT_SECS: f32 = 0.5;

impl ExpensePanel {
    pub fn new(expense: Option<Expense>, default_tax_year: i64, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let today = Local::now().date_naive();
        let (date, category, description, amount, receipt) = match &expense {
            Some(e) => (e.date.get(..10).unwrap_or(&e.date).to_string(), e.category.clone(), e.description.clone(), e.amount, e.receipt_note.clone()),
            None => {
                // Jan 1 of the chosen tax year if it isn't this year, else today
                let date = if default_tax_year == today.year() as i64 { today.format("%Y-%m-%d").to_string() } else { format!("{default_tax_year}-01-01") };
                (date, "other".to_string(), String::new(), 0.0, String::new())
            }
        };
        let parsed = NaiveDate::parse_from_str(&date, "%Y-%m-%d").ok();
        let date_input = cx.new(|cx| DateInput::date(parsed, cx));
        let field = |value: String, placeholder: &'static str, window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| InputState::new(window, cx).placeholder(placeholder).default_value(value))
        };
        let description = field(description, "Adobe Creative Cloud, March", window, cx);
        let amount = field(if amount != 0.0 { trim_number(amount) } else { String::new() }, "0.00", window, cx);
        let receipt = field(receipt, "Dropbox/Receipts/2026/March", window, cx);
        let subs = vec![cx.subscribe(&date_input, |p, _, e: &DateChanged, cx| {
            if let DateChanged::Date(d) = e {
                p.date_value = d.map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_default();
                cx.notify();
            }
        })];
        // autoFocus on the description
        description.update(cx, |s, cx| s.focus(window, cx));
        ExpensePanel {
            expense,
            date: date_input,
            date_value: if parsed.is_some() { date } else { String::new() },
            category,
            description,
            amount,
            receipt,
            opened: Instant::now(),
            closing: None,
            _subs: subs,
        }
    }

    pub fn is_closing(&self) -> bool {
        self.closing.is_some()
    }

    /// Start the exit animation (the page drops the panel when it's done).
    pub fn close(&mut self, cx: &mut Context<Self>) {
        if self.closing.is_none() {
            let now = Instant::now();
            self.closing = Some((now, self.open_state(now)));
            cx.notify();
        }
    }

    /// (x offset, panel opacity, backdrop opacity) while opening.
    fn open_state(&self, now: Instant) -> (f32, f32, f32) {
        if motion::reduced() {
            return (0.0, 1.0, 1.0);
        }
        let t = (now - self.opened).as_secs_f32();
        let s = spring(t, 280.0, 28.0, 1.0);
        let fade = cubic_bezier(0.25, 0.1, 0.35, 1.0);
        (TRAVEL * (1.0 - s), s.clamp(0.0, 1.0), if t >= 0.3 { 1.0 } else { fade(t / 0.3) })
    }

    fn state(&self, now: Instant) -> ((f32, f32, f32), bool) {
        match self.closing {
            None => {
                let t = (now - self.opened).as_secs_f32();
                (self.open_state(now), !motion::reduced() && t < 1.0)
            }
            Some((at, (x0, o0, b0))) => {
                if motion::reduced() {
                    return ((TRAVEL, 0.0, 0.0), false);
                }
                let t = (now - at).as_secs_f32();
                let s = spring(t, 280.0, 28.0, 1.0);
                let fade = cubic_bezier(0.25, 0.1, 0.35, 1.0);
                let b = if t >= 0.3 { 0.0 } else { b0 * (1.0 - fade(t / 0.3)) };
                ((x0 + (TRAVEL - x0) * s, (o0 * (1.0 - s)).clamp(0.0, 1.0), b), t < EXIT_SECS)
            }
        }
    }

    fn emit_close(&mut self, cx: &mut Context<Self>) {
        if self.closing.is_none() {
            cx.emit(ExpensePanelEvent::Closed);
        }
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let description = self.description.read(cx).value().to_string();
        if description.trim().is_empty() {
            toast_error("Add a description", cx);
            return;
        }
        let amount = number_value(&self.amount.read(cx).value());
        if !(amount > 0.0) {
            toast_error("Enter an amount above zero", cx);
            return;
        }
        let receipt_note = self.receipt.read(cx).value().to_string();
        let date = self.date_value.clone();
        let tax_year = year_of(&date);
        let category = self.category.clone();
        let result = match &self.expense {
            Some(e) => {
                let id = e.id;
                let patch = ExpensePatch {
                    date: Some(date),
                    category: Some(category),
                    description: Some(description),
                    amount: Some(amount),
                    tax_year: Some(Some(tax_year)),
                    receipt_note: Some(receipt_note),
                    ..Default::default()
                };
                store::write(cx, |db| db.update_expense(id, &patch).map(|_| ())).map(|_| "Expense updated")
            }
            None => {
                let input = ExpenseInput {
                    date: Some(date),
                    category: Some(category),
                    description: Some(description),
                    amount: Some(amount),
                    tax_year: Some(tax_year),
                    receipt_note: Some(receipt_note),
                    receipt_id: None,
                };
                store::write(cx, |db| db.create_expense(&input).map(|_| ())).map(|_| "Expense added")
            }
        };
        match result {
            Ok(msg) => {
                toast_success(msg, cx);
                cx.emit(ExpensePanelEvent::Saved);
            }
            Err(e) => toast_error(format!("Failed: {e}"), cx),
        }
    }

    fn delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.expense.as_ref().map(|e| e.id) else { return };
        // confirm(): the Mac's own alert, as Electron shows it
        let answer = window.prompt(PromptLevel::Info, "Delete this expense? This can't be undone.", None, &["OK", "Cancel"], cx);
        cx.spawn(async move |this, cx| {
            if answer.await != Ok(0) {
                return;
            }
            let _ = this.update(cx, |_, cx| match store::write(cx, |db| db.delete_expense(id)) {
                Ok(()) => {
                    toast_success("Expense deleted", cx);
                    cx.emit(ExpensePanelEvent::Deleted);
                }
                Err(e) => toast_error(format!("Failed: {e}"), cx),
            });
        })
        .detach();
    }
}

impl Render for ExpensePanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(cx);
        let viewport = window.viewport_size();
        let now = Instant::now();
        let ((x, opacity, backdrop), animating) = self.state(now);
        if animating {
            window.request_animation_frame();
        }
        let closing = self.closing.is_some();
        let editing = self.expense.is_some();
        let this = cx.entity().downgrade();

        let header = div()
            .flex()
            .flex_none()
            .items_center()
            .justify_between()
            .px(r(5.))
            .h(px(52.))
            .border_b_1()
            .border_color(t.line)
            .child(
                div()
                    .child(text(if editing { "Edit expense" } else { "New expense" }, 14.0, 600, t.fg).line_height(px(20.)))
                    .child(text(format!("Counts toward {}", year_of(&self.date_value)), 12.0, 500, t.fg3).line_height(px(16.))),
            )
            .child(Button::icon_button("expense-panel-close", Lucide::X).small().icon_color(t.fg3).on_click(cx.listener(|p, _, _, cx| p.emit_close(cx))));

        let hint_text = CATEGORIES.iter().find(|c| c.0 == self.category).and_then(|c| c.2);
        let w = this.clone();
        let mut category = div().child(label(&t, "Category")).child(select(
            "expense-category",
            CATEGORIES.iter().map(|(v, l, _)| (v.to_string(), SharedString::from(*l))).collect(),
            &self.category,
            move |v, _, cx| {
                let _ = w.update(cx, |p, cx| {
                    p.category = v;
                    cx.notify();
                });
            },
            cx,
        ));
        if let Some(h) = hint_text {
            category = category.child(hint(&t, h));
        }

        let body = div()
            .id("expense-panel-body")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p(r(5.))
            .flex()
            .flex_col()
            .gap(r(4.))
            .child(div().child(label(&t, "Date")).child(self.date.clone()))
            .child(category)
            .child(div().child(label(&t, "Description")).child(TextField::new(&self.description)))
            .child(div().child(label(&t, "Amount")).child(TextField::new(&self.amount).leading_text("$").tabular()))
            .child(
                div()
                    .child(label(&t, "Receipt location"))
                    .child(TextField::new(&self.receipt))
                    .child(hint(&t, "Where you keep the receipt, so you can find it at tax time.")),
            );

        let left = if editing {
            Button::ghost("expense-delete", "Delete")
                .icon(Lucide::Trash2)
                .icon_color(t.accent_text)
                .hover_text(t.red)
                .on_click(cx.listener(|p, _, window, cx| p.delete(window, cx)))
                .into_any_element()
        } else {
            div().into_any_element()
        };
        let footer = div()
            .flex()
            .flex_none()
            .items_center()
            .justify_between()
            .gap(r(3.))
            .px(r(5.))
            .py(r(3.))
            .border_t_1()
            .border_color(t.line)
            .child(left)
            .child(
                div()
                    .flex()
                    .gap(r(2.))
                    .child(Button::secondary("expense-cancel", "Cancel").on_click(cx.listener(|p, _, _, cx| p.emit_close(cx))))
                    .child(
                        Button::primary("expense-save", if editing { "Save changes" } else { "Add expense" })
                            .on_click(cx.listener(|p, _, _, cx| p.save(cx))),
                    ),
            );

        let panel = div()
            .id("expense-panel")
            .absolute()
            .top_0()
            .bottom_0()
            .right(px(-x))
            .w(px(400.))
            .bg(t.panel)
            .border_l_1()
            .border_color(t.line)
            .shadow(t.shadow_pop())
            .opacity(opacity)
            .flex()
            .flex_col()
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(header)
            .child(body)
            .child(footer);

        let mut backdrop_el = div().id("expense-backdrop").absolute().inset_0().bg(gpui::black().opacity(0.4 * backdrop));
        if !closing {
            backdrop_el = backdrop_el.on_click(cx.listener(|p, _, _, cx| p.emit_close(cx)));
        }
        let layer = div()
            .id("expense-layer")
            .relative()
            .w(viewport.width)
            .h(viewport.height)
            .occlude()
            .child(backdrop_el)
            .child(panel)
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation());
        deferred(anchored().position(point(px(0.), px(0.))).child(layer)).with_priority(5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn js_numbers() {
        assert_eq!(number_value("12.5"), 12.5);
        assert_eq!(number_value(""), 0.0);
        assert_eq!(number_value("abc"), 0.0);
        assert_eq!(number_value("3e2"), 300.0);
        assert_eq!(year_of("2024-03-01"), 2024);
        assert_eq!(year_of(""), Local::now().year() as i64);
    }
}
