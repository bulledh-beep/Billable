//! `<input type="date">` and `<input type="time">` as Chromium draws them
//! on the Mac: segments ("mm/dd/yyyy", "hh:mm AM") you click into and type
//! digits or use the arrow keys on, plus the calendar button (opacity .5) that
//! opens a month picker. Pages hold an `Entity<DateInput>` and subscribe to
//! [`DateChanged`].

use chrono::{Datelike, NaiveDate, NaiveTime, Timelike};
use gpui::{
    div, prelude::*, px, App, Context, ElementId, EventEmitter, FocusHandle, Focusable, IntoElement, KeyDownEvent,
    ParentElement, Render, SharedString, Styled, Window,
};

use super::{fields::input_box, menu::popover_panel};
use crate::{
    icons::Lucide,
    style::{icon, num_font, r, text},
    theme::theme,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Date,
    Time,
}

/// Emitted when the value changes (`None` when cleared).
#[derive(Clone, Debug)]
pub enum DateChanged {
    Date(Option<NaiveDate>),
    Time(Option<NaiveTime>),
}

pub struct DateInput {
    kind: Kind,
    /// Segment values; `None` shows the placeholder ("mm", "--").
    parts: [Option<u32>; 3],
    active: Option<usize>,
    typed: String,
    focus: FocusHandle,
    calendar: Option<NaiveDate>,
    /// `h-7 text-xs`: 22.75px tall with 12px text.
    small: bool,
    /// The `max` attribute: later days can't be picked or typed.
    max: Option<NaiveDate>,
}

impl EventEmitter<DateChanged> for DateInput {}

/// The order of month/day/year and the separator in a date field, from the
/// Mac's locale, as Chromium shows them (en-CA: "yyyy-mm-dd", en-US:
/// "mm/dd/yyyy"). Indices into `parts` ([month, day, year]).
fn date_layout() -> ([usize; 3], &'static str) {
    use std::sync::OnceLock;
    static LAYOUT: OnceLock<([usize; 3], String)> = OnceLock::new();
    let (order, sep) = LAYOUT.get_or_init(|| {
        use objc2_foundation::{NSDateFormatter, NSLocale, NSString};
        let pattern = NSDateFormatter::dateFormatFromTemplate_options_locale(&NSString::from_str("yMMdd"), 0, Some(&NSLocale::currentLocale()))
            .map(|p| p.to_string())
            .unwrap_or_else(|| "MM/dd/y".into());
        let mut order = Vec::new();
        let mut sep = String::new();
        for ch in pattern.chars() {
            let part = match ch {
                'M' | 'L' => Some(0),
                'd' => Some(1),
                'y' | 'u' => Some(2),
                _ => None,
            };
            match part {
                Some(p) if !order.contains(&p) => order.push(p),
                Some(_) => {}
                None if sep.is_empty() && !order.is_empty() && !ch.is_alphabetic() && ch != '\'' => sep.push(ch),
                None => {}
            }
        }
        let order = if order.len() == 3 { [order[0], order[1], order[2]] } else { [0, 1, 2] };
        (order, if sep.is_empty() { "/".into() } else { sep })
    });
    (*order, sep.as_str())
}

impl Focusable for DateInput {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl DateInput {
    pub fn date(value: Option<NaiveDate>, cx: &mut Context<Self>) -> Self {
        let mut d = DateInput { kind: Kind::Date, parts: [None; 3], active: None, typed: String::new(), focus: cx.focus_handle(), calendar: None, small: false, max: None };
        d.set_date(value);
        d
    }

    pub fn time(value: Option<NaiveTime>, cx: &mut Context<Self>) -> Self {
        let mut d = DateInput { kind: Kind::Time, parts: [None; 3], active: None, typed: String::new(), focus: cx.focus_handle(), calendar: None, small: false, max: None };
        d.set_time(value);
        d
    }

    /// The compact size (`h-7 text-xs`), e.g. the invoice builder's "Bill through".
    pub fn small(mut self) -> Self {
        self.small = true;
        self
    }

    /// Latest allowed day (`max`), or none.
    pub fn set_max(&mut self, max: Option<NaiveDate>) {
        self.max = max;
    }

    pub fn set_date(&mut self, v: Option<NaiveDate>) {
        let v = match (v, self.max) {
            (Some(d), Some(m)) if d > m => Some(m),
            (v, _) => v,
        };
        self.parts = match v {
            Some(d) => [Some(d.month()), Some(d.day()), Some(d.year() as u32)],
            None => [None; 3],
        };
    }

    pub fn set_time(&mut self, v: Option<NaiveTime>) {
        self.parts = match v {
            Some(t) => {
                let h = t.hour();
                [Some(if h % 12 == 0 { 12 } else { h % 12 }), Some(t.minute()), Some(if h >= 12 { 1 } else { 0 })]
            }
            None => [None; 3],
        };
    }

    pub fn date_value(&self) -> Option<NaiveDate> {
        let d = match self.parts {
            [Some(m), Some(d), Some(y)] => NaiveDate::from_ymd_opt(y as i32, m, d),
            _ => None,
        }?;
        // Chromium treats a value past `max` as invalid (no value).
        match self.max {
            Some(m) if d > m => None,
            _ => Some(d),
        }
    }

    pub fn time_value(&self) -> Option<NaiveTime> {
        match self.parts {
            [Some(h), Some(m), Some(pm)] => NaiveTime::from_hms_opt(h % 12 + if pm == 1 { 12 } else { 0 }, m, 0),
            _ => None,
        }
    }

    fn emit(&mut self, cx: &mut Context<Self>) {
        let e = match self.kind {
            Kind::Date => DateChanged::Date(self.date_value()),
            Kind::Time => DateChanged::Time(self.time_value()),
        };
        cx.emit(e);
        cx.notify();
    }

    /// (min, max) for a segment.
    fn range(&self, i: usize) -> (u32, u32) {
        match (self.kind, i) {
            (Kind::Date, 0) => (1, 12),
            (Kind::Date, 1) => (1, 31),
            (Kind::Date, _) => (1, 9999),
            (Kind::Time, 0) => (1, 12),
            (Kind::Time, 1) => (0, 59),
            (Kind::Time, _) => (0, 1),
        }
    }

    fn step(&mut self, i: usize, delta: i32, cx: &mut Context<Self>) {
        let (lo, hi) = self.range(i);
        let today = chrono::Local::now();
        let start = match (self.kind, i) {
            (Kind::Date, 0) => today.month(),
            (Kind::Date, 1) => today.day(),
            (Kind::Date, _) => today.year() as u32,
            (Kind::Time, 0) => 12,
            _ => lo,
        };
        let v = match self.parts[i] {
            None => start,
            Some(v) => {
                let span = (hi - lo + 1) as i32;
                ((v as i32 - lo as i32 + delta).rem_euclid(span) + lo as i32) as u32
            }
        };
        self.parts[i] = Some(v);
        self.typed.clear();
        self.emit(cx);
    }

    /// Which value a display position edits (dates follow the locale's order).
    fn part_at(&self, pos: usize) -> usize {
        if self.kind == Kind::Date { date_layout().0[pos] } else { pos }
    }

    fn type_digit(&mut self, pos: usize, digit: u32, cx: &mut Context<Self>) {
        let i = self.part_at(pos);
        if self.kind == Kind::Time && i == 2 {
            return;
        }
        self.typed.push(char::from_digit(digit, 10).unwrap_or('0'));
        let (lo, hi) = self.range(i);
        let n: u32 = self.typed.parse().unwrap_or(0);
        let width = if self.kind == Kind::Date && i == 2 { 4 } else { 2 };
        let done = self.typed.len() >= width || n * 10 > hi;
        self.parts[i] = Some(if done { n.clamp(lo, hi) } else { n.max(lo.min(n)) });
        if done {
            self.typed.clear();
            if pos < 2 {
                self.active = Some(pos + 1);
            }
        }
        self.emit(cx);
    }

    fn on_key(&mut self, e: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(pos) = self.active else { return };
        let i = pos;
        let part = self.part_at(pos);
        let key = e.keystroke.key.as_str();
        match key {
            "up" => self.step(part, 1, cx),
            "down" => self.step(part, -1, cx),
            "left" => {
                self.active = Some(i.saturating_sub(1));
                self.typed.clear();
                cx.notify();
            }
            "right" | "/" | ":" | "-" | "." => {
                self.active = Some((i + 1).min(2));
                self.typed.clear();
                cx.notify();
            }
            "tab" if !e.keystroke.modifiers.shift && i < 2 => {
                self.active = Some(i + 1);
                self.typed.clear();
                cx.stop_propagation();
                cx.notify();
            }
            "tab" if e.keystroke.modifiers.shift && i > 0 => {
                self.active = Some(i - 1);
                self.typed.clear();
                cx.stop_propagation();
                cx.notify();
            }
            "backspace" | "delete" => {
                self.parts[part] = None;
                self.typed.clear();
                self.emit(cx);
            }
            "a" if self.kind == Kind::Time && i == 2 => {
                self.parts[2] = Some(0);
                self.emit(cx);
            }
            "p" if self.kind == Kind::Time && i == 2 => {
                self.parts[2] = Some(1);
                self.emit(cx);
            }
            k if k.len() == 1 && k.chars().next().is_some_and(|c| c.is_ascii_digit()) => {
                self.type_digit(i, k.parse().unwrap_or(0), cx);
            }
            _ => {}
        }
    }

    fn segment_text(&self, i: usize) -> String {
        match (self.kind, i, self.parts[i]) {
            (Kind::Date, 0, None) => "mm".into(),
            (Kind::Date, 1, None) => "dd".into(),
            (Kind::Date, _, None) => "yyyy".into(),
            (Kind::Date, 2, Some(v)) => format!("{v:04}"),
            (Kind::Time, 2, None) => "--".into(),
            (Kind::Time, 2, Some(v)) => if v == 1 { "PM".into() } else { "AM".into() },
            (Kind::Time, _, None) => "--".into(),
            (_, _, Some(v)) => format!("{v:02}"),
        }
    }
}

impl Render for DateInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(cx);
        let focused = self.focus.is_focused(window);
        if !focused && self.active.is_some() {
            self.active = None;
            self.typed.clear();
        }
        let sep = if self.kind == Kind::Date { date_layout().1 } else { ":" };
        let mut row = div().flex().items_center().font(num_font(500)).text_size(px(if self.small { 12.0 } else { 14.0 })).text_color(t.fg).flex_1();
        for i in 0..3 {
            let part = self.part_at(i);
            let selected = focused && self.active == Some(i);
            let mut seg = div()
                .id(ElementId::Name(format!("seg-{i}").into()))
                // Chromium pads each field 1px on either side.
                .px(px(1.))
                .rounded(px(2.))
                .when(selected, |s| s.bg(t.accent.opacity(0.28)))
                .on_mouse_down(gpui::MouseButton::Left, cx.listener(move |this, _, window, cx| {
                    this.active = Some(i);
                    this.typed.clear();
                    this.focus.focus(window, cx);
                    cx.notify();
                }))
                .child(self.segment_text(part));
            if self.kind == Kind::Time && i == 2 {
                seg = seg.ml(px(4.));
            }
            row = row.child(seg);
            if i == 0 || (i == 1 && self.kind == Kind::Date) {
                row = row.child(sep);
            }
        }
        let mut field = input_box(&t, focused, false)
            .when(self.small, |b| b.h(r(7.)).line_height(px(16.)))
            .id("date-input")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key))
            .on_mouse_down(gpui::MouseButton::Left, cx.listener(|this, _, window, cx| {
                if this.active.is_none() {
                    this.active = Some(0);
                }
                this.focus.focus(window, cx);
                cx.notify();
            }))
            .child(row);
        field = field.child(
            div()
                .id("date-picker-button")
                .ml(px(2.))
                .cursor_pointer()
                .on_mouse_down(gpui::MouseButton::Left, cx.listener(|this, _, _, cx| {
                    cx.stop_propagation();
                    if this.kind == Kind::Date {
                        let base = this.date_value().unwrap_or_else(|| chrono::Local::now().date_naive());
                        this.calendar = if this.calendar.is_some() { None } else { Some(base.with_day(1).unwrap_or(base)) };
                        cx.notify();
                    }
                }))
                .child(picker_indicator(self.kind)),
        );
        let mut wrap = div().relative().w_full().child(field);
        if let Some(month) = self.calendar {
            wrap = wrap.child(calendar(month, self.date_value(), cx));
        }
        wrap
    }
}

/// `::-webkit-calendar-picker-indicator` at `opacity: 0.5`. Chromium draws a
/// small filled-header calendar (light under `color-scheme: dark`, which the
/// app's `invert(1)` turns dark again), so in both themes it reads as a dim
/// near-black glyph, about 11×11 with two tabs on top. Drawn in a 14px box.
fn picker_indicator(kind: Kind) -> gpui::AnyElement {
    let ink = gpui::black().opacity(0.5);
    if kind == Kind::Time {
        return icon(Lucide::Clock).size(px(14.)).text_color(ink).into_any_element();
    }
    div()
        .relative()
        .size(px(14.))
        .flex_none()
        .child(
            div()
                .absolute()
                .left(px(1.))
                .top(px(1.))
                .w(px(11.))
                .h(px(11.))
                .border_1()
                // The thin outline is antialiased in Chromium, so it reads lighter.
                .border_color(ink.opacity(0.7))
                .rounded(px(1.5))
                .child(div().h(px(2.)).bg(ink)),
        )
        .child(div().absolute().left(px(3.)).top_0().w(px(1.)).h(px(1.)).bg(ink.opacity(0.5)))
        .child(div().absolute().left(px(10.)).top_0().w(px(1.)).h(px(1.)).bg(ink.opacity(0.5)))
        .into_any_element()
}

/// The month picker under a date field.
fn calendar(month: NaiveDate, selected: Option<NaiveDate>, cx: &mut Context<DateInput>) -> impl IntoElement {
    let t = theme(cx);
    let title: SharedString = month.format("%B %Y").to_string().into();
    let first_weekday = month.weekday().num_days_from_sunday() as i64;
    let start = month - chrono::Duration::days(first_weekday);
    let today = chrono::Local::now().date_naive();
    let nav = |id: &'static str, glyph: Lucide, delta: i32, cx: &mut Context<DateInput>| {
        div()
            .id(id)
            .size(px(24.))
            .rounded_full()
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .hover(|s| s.bg(gpui::black().opacity(0.06)))
            .on_click(cx.listener(move |this, _, _, cx| {
                if let Some(m) = this.calendar {
                    this.calendar = if delta > 0 { m.checked_add_months(chrono::Months::new(1)) } else { m.checked_sub_months(chrono::Months::new(1)) };
                    cx.notify();
                }
            }))
            .child(icon(glyph).size(px(14.)).text_color(t.fg2))
    };
    let mut grid = div().flex().flex_col().gap(px(2.));
    let mut head = div().flex();
    for d in ["S", "M", "T", "W", "T", "F", "S"] {
        head = head.child(text(d, 11.0, 600, t.fg3).w(px(32.)).text_center());
    }
    grid = grid.child(head);
    for week in 0..6 {
        let mut row = div().flex();
        for day in 0..7 {
            let date = start + chrono::Duration::days(week * 7 + day);
            let in_month = date.month() == month.month();
            let is_sel = Some(date) == selected;
            let hover = t.fg.opacity(0.07);
            row = row.child(
                div()
                    .id(ElementId::Name(format!("cal-{date}").into()))
                    .w(px(32.))
                    .h(px(28.))
                    .rounded(px(8.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .when(is_sel, |d| d.bg(t.accent))
                    .when(!is_sel, |d| d.hover(move |s| s.bg(hover)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.set_date(Some(date));
                        this.calendar = None;
                        this.emit(cx);
                    }))
                    .child(text(
                        date.day().to_string(),
                        13.0,
                        if date == today { 700 } else { 500 },
                        if is_sel { crate::theme::hex(0xffffff) } else if in_month { t.fg } else { t.fg4 },
                    )),
            );
        }
        grid = grid.child(row);
    }
    let panel = popover_panel(&t)
        .id("calendar")
        .occlude()
        .on_mouse_down_out(cx.listener(|this, _, _, cx| {
            this.calendar = None;
            cx.notify();
        }))
        .p(r(3.))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .mb(r(2.))
                .child(text(title, 14.0, 700, t.fg))
                .child(div().flex().gap(px(2.)).child(nav("cal-prev", Lucide::ChevronLeft, -1, cx)).child(nav("cal-next", Lucide::ChevronRight, 1, cx))),
        )
        .child(grid);
    div().absolute().top_full().left_0().child(gpui::deferred(gpui::anchored().offset(gpui::point(px(0.), px(6.))).snap_to_window_with_margin(px(8.)).child(panel)).with_priority(30))
}
