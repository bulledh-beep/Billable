//! EntryEditor.tsx as a modal view: add or edit a time entry, keeping
//! start, end and duration in sync. OWNER: the Time port. It saves itself and
//! emits [`EntryEditorEvent::Saved`].
//!
//! Start or end changes → the duration follows. A typed duration → the end
//! follows. An end at or before the start rolls to the next day. Time already
//! on an invoice gets a warning (amber once the invoice has gone out), and its
//! billable box is locked.

use billable_core::models::{EntryRow, Project, TimeEntryInput, TimeEntryPatch};
use chrono::{DateTime, Duration, Local, LocalResult, NaiveDate, NaiveTime, TimeZone, Timelike, Utc};
use gpui::{
    div, prelude::*, px, App, AppContext, Context, Div, Entity, EventEmitter, FocusHandle, Focusable, Hsla, IntoElement,
    KeyDownEvent, ParentElement, Render, SharedString, StyledText, Styled, Subscription, TextRun, Window,
};
use gpui_component::input::{Input, InputEvent, InputState};

use super::{
    date_input::{DateChanged, DateInput},
    fields::{checkbox, input_box},
    select, toast_error, toast_success, Button, Modal,
};
use crate::{
    fonts::rounded_family,
    format::{duration_short, js_round, parse_time},
    icons::Lucide,
    store,
    style::{dot, icon, label, num_font, r, text},
    theme::{parse_hex, theme},
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EntryEditorMode {
    Add,
    Edit,
}

pub enum EntryEditorEvent {
    Saved,
    Closed,
}

pub struct EntryEditorModal {
    pub mode: EntryEditorMode,
    pub entry: Option<EntryRow>,
    pub projects: Vec<Project>,
    project_id: i64,
    description: Entity<InputState>,
    date: Entity<DateInput>,
    start: Entity<DateInput>,
    end: Entity<DateInput>,
    duration: Entity<InputState>,
    date_v: Option<NaiveDate>,
    start_v: Option<NaiveTime>,
    end_v: Option<NaiveTime>,
    is_billable: bool,
    /// Set once a save went through, so ⌘↩ can't add the same time twice.
    saved: bool,
    focus: FocusHandle,
    _subs: Vec<Subscription>,
}

impl EventEmitter<EntryEditorEvent> for EntryEditorModal {}

/// True when an entry sits on an invoice that has gone out (or been paid).
pub fn is_locked_entry(e: Option<&EntryRow>) -> bool {
    e.is_some_and(|e| matches!(e.billing_state.as_str(), "sent" | "overdue" | "paid" | "invoiced"))
}

/// format.ts's parseDurationInput: "2h 30m", "1.5h", "2:30", "90" (minutes),
/// "45m". `None` when it can't be read.
pub fn parse_duration_input(input: &str) -> Option<i64> {
    let s = input.trim().to_lowercase();
    if s.is_empty() {
        return None;
    }
    let digits = |x: &str| !x.is_empty() && x.bytes().all(|c| c.is_ascii_digit());
    // ^(\d+):(\d{1,2})$
    if let Some((h, m)) = s.split_once(':') {
        if digits(h) && digits(m) && m.len() <= 2 {
            return Some(h.parse::<f64>().ok()? as i64 * 60 + m.parse::<i64>().ok()?);
        }
    }
    // ^\d+(\.\d+)?$
    let decimal = match s.split_once('.') {
        Some((a, b)) => digits(a) && digits(b),
        None => digits(&s),
    };
    if decimal {
        return Some(js_round(s.parse::<f64>().ok()?) as i64);
    }
    let mut total = 0.0;
    let mut matched = false;
    if let Some(h) = unit_amount(&s, 'h') {
        total += h * 60.0;
        matched = true;
    }
    if let Some(m) = unit_amount(&s, 'm') {
        total += m;
        matched = true;
    }
    matched.then(|| js_round(total) as i64)
}

/// The first match of `(\d+(?:\.\d+)?)\s*<unit>`.
fn unit_amount(s: &str, unit: char) -> Option<f64> {
    let c: Vec<char> = s.chars().collect();
    for start in 0..c.len() {
        if !c[start].is_ascii_digit() {
            continue;
        }
        let mut i = start;
        while i < c.len() && c[i].is_ascii_digit() {
            i += 1;
        }
        if i + 1 < c.len() && c[i] == '.' && c[i + 1].is_ascii_digit() {
            i += 1;
            while i < c.len() && c[i].is_ascii_digit() {
                i += 1;
            }
        }
        let number: String = c[start..i].iter().collect();
        let mut j = i;
        while j < c.len() && c[j].is_whitespace() {
            j += 1;
        }
        if j < c.len() && c[j] == unit {
            return number.parse().ok();
        }
    }
    None
}

/// A local wall-clock time as epoch ms, like `new Date("YYYY-MM-DDTHH:MM:00")`
/// (the earlier instant when the clock repeats, moved past a skipped hour).
fn local_ms(date: NaiveDate, time: NaiveTime) -> i64 {
    let at = date.and_time(time);
    match Local.from_local_datetime(&at) {
        LocalResult::Single(t) | LocalResult::Ambiguous(t, _) => t.timestamp_millis(),
        LocalResult::None => Local
            .from_local_datetime(&(at + Duration::hours(1)))
            .earliest()
            .map(|t| t.timestamp_millis())
            .unwrap_or_else(|| at.and_utc().timestamp_millis()),
    }
}

/// Minutes from date+start to date+end; an end at or before start rolls to the next day.
pub fn minutes_between(date: NaiveDate, start: NaiveTime, end: NaiveTime) -> i64 {
    let s = local_ms(date, start);
    let mut e = local_ms(date, end);
    if e <= s {
        e += 86_400_000;
    }
    js_round((e - s) as f64 / 60_000.0) as i64
}

fn end_from_duration(date: NaiveDate, start: NaiveTime, mins: i64) -> NaiveTime {
    hhmm((date.and_time(start) + Duration::minutes(mins)).time())
}

/// Hours and minutes only (the `<input type=time>` value).
fn hhmm<T: Timelike>(t: T) -> NaiveTime {
    NaiveTime::from_hms_opt(t.hour(), t.minute(), 0).unwrap_or_default()
}

fn iso(ms: i64) -> String {
    DateTime::<Utc>::from_timestamp_millis(ms).unwrap_or_default().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

/// A paragraph mixing weights and colours (`<b className="text-fg">` inside text).
fn rich(parts: &[(&str, u16, Hsla)], size: f32, line_height: f32) -> Div {
    let mut s = String::new();
    let mut runs = Vec::new();
    for (p, weight, color) in parts {
        s.push_str(p);
        runs.push(TextRun { len: p.len(), font: gpui::font(rounded_family(*weight)), color: *color, background_color: None, underline: None, strikethrough: None });
    }
    div().text_size(px(size)).line_height(px(line_height)).child(StyledText::new(s).with_runs(runs))
}

impl EntryEditorModal {
    /// `default_date` is a local "YYYY-MM-DD".
    pub fn new(
        mode: EntryEditorMode,
        entry: Option<EntryRow>,
        projects: Vec<Project>,
        default_date: Option<String>,
        default_project_id: Option<i64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let nine = NaiveTime::from_hms_opt(9, 0, 0).unwrap_or_default();
        let ten = NaiveTime::from_hms_opt(10, 0, 0).unwrap_or_default();
        let today = Local::now().date_naive();
        let editing = entry.as_ref().filter(|_| mode == EntryEditorMode::Edit);
        let (project_id, description, date_v, start_v, end_v, duration_text, is_billable) = match editing {
            Some(e) => {
                let s = parse_time(&e.entry.start_time).unwrap_or_else(Local::now);
                let length = Duration::milliseconds((e.entry.duration_minutes * 60_000.0) as i64);
                let end = e.entry.end_time.as_deref().and_then(parse_time).unwrap_or(s + length);
                // Paused time isn't in the duration, so derive the end from the duration
                let spanned = (end - s).num_milliseconds() as f64 / 60_000.0;
                let shown = if (spanned - e.entry.duration_minutes).abs() > 1.0 { s + length } else { end };
                (
                    e.entry.project_id,
                    e.entry.description.clone(),
                    Some(s.date_naive()),
                    Some(hhmm(s)),
                    Some(hhmm(shown)),
                    duration_short(e.entry.duration_minutes),
                    e.entry.is_billable != 0,
                )
            }
            None => (
                default_project_id.filter(|id| *id != 0).or_else(|| projects.first().map(|p| p.id)).unwrap_or(0),
                String::new(),
                Some(default_date.as_deref().and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()).unwrap_or(today)),
                Some(nine),
                Some(ten),
                duration_short(60.0),
                true,
            ),
        };

        let description_state = cx.new(|cx| InputState::new(window, cx).placeholder("What did you work on?").default_value(description));
        let duration = cx.new(|cx| InputState::new(window, cx).placeholder("2h 30m").default_value(duration_text));
        let date = cx.new(|cx| DateInput::date(date_v, cx));
        let start = cx.new(|cx| DateInput::time(start_v, cx));
        let end = cx.new(|cx| DateInput::time(end_v, cx));

        // ⌘↩ from a text field propagates to the key handler in `render`.
        let subs = vec![
            cx.subscribe(&duration, |this, _, e: &InputEvent, cx| {
                if matches!(e, InputEvent::Change) {
                    this.duration_typed(cx);
                }
            }),
            cx.subscribe_in(&date, window, |this, _, e: &DateChanged, window, cx| {
                if let DateChanged::Date(d) = e {
                    this.date_v = *d;
                    this.times_changed(window, cx);
                }
            }),
            cx.subscribe_in(&start, window, |this, _, e: &DateChanged, window, cx| {
                if let DateChanged::Time(t) = e {
                    this.start_v = *t;
                    this.times_changed(window, cx);
                }
            }),
            cx.subscribe_in(&end, window, |this, _, e: &DateChanged, window, cx| {
                if let DateChanged::Time(t) = e {
                    this.end_v = *t;
                    this.times_changed(window, cx);
                }
            }),
        ];

        let focus = cx.focus_handle();
        // autoFocus on the description when adding; otherwise the dialog
        // itself holds focus so Escape and ⌘↩ still reach it.
        if mode == EntryEditorMode::Add {
            description_state.update(cx, |s, cx| s.focus(window, cx));
        } else {
            focus.focus(window, cx);
        }

        EntryEditorModal {
            mode,
            entry,
            projects,
            project_id,
            description: description_state,
            date,
            start,
            end,
            duration,
            date_v,
            start_v,
            end_v,
            is_billable,
            saved: false,
            focus,
            _subs: subs,
        }
    }

    /// The minutes the start and end span, when both (and the date) are set.
    fn minutes(&self) -> Option<i64> {
        Some(minutes_between(self.date_v?, self.start_v?, self.end_v?))
    }

    /// Start, end or date changed: the duration follows.
    fn times_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.minutes().map(|m| duration_short(m as f64)).unwrap_or_default();
        self.duration.update(cx, |s, cx| s.set_value(text, window, cx));
        cx.notify();
    }

    /// A duration was typed: the end follows when it reads as time.
    fn duration_typed(&mut self, cx: &mut Context<Self>) {
        let text = self.duration.read(cx).value().to_string();
        if let (Some(mins), Some(date), Some(start)) = (parse_duration_input(&text), self.date_v, self.start_v) {
            if mins > 0 {
                let end = end_from_duration(date, start, mins);
                self.end_v = Some(end);
                self.end.update(cx, |d, cx| {
                    d.set_time(Some(end));
                    cx.notify();
                });
            }
        }
        cx.notify();
    }

    /// The +15m / +30m / +1h buttons.
    fn bump(&mut self, add: i64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(minutes) = self.minutes() else { return };
        let text = duration_short((minutes + add) as f64);
        self.duration.update(cx, |s, cx| s.set_value(text, window, cx));
        self.duration_typed(cx);
    }

    fn duration_invalid(&self, cx: &App) -> bool {
        parse_duration_input(&self.duration.read(cx).value()).is_none()
    }

    fn invalid(&self, cx: &App) -> bool {
        self.duration_invalid(cx) || self.minutes().is_none_or(|m| m <= 0) || (self.mode == EntryEditorMode::Add && self.project_id == 0)
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        if self.saved || self.invalid(cx) {
            return;
        }
        let (Some(date), Some(start), Some(minutes)) = (self.date_v, self.start_v, self.minutes()) else { return };
        let start_ms = local_ms(date, start);
        let start_time = iso(start_ms);
        let end_time = iso(start_ms + minutes * 60_000);
        let description = self.description.read(cx).value().to_string();
        let is_billable = if self.is_billable { 1 } else { 0 };
        let result = match (self.mode, self.entry.as_ref()) {
            (EntryEditorMode::Edit, Some(entry)) => {
                let id = entry.entry.id;
                let patch = TimeEntryPatch {
                    description: Some(description),
                    start_time: Some(start_time),
                    end_time: Some(Some(end_time)),
                    duration_minutes: Some(minutes as f64),
                    is_billable: Some(is_billable),
                    ..Default::default()
                };
                store::write(cx, |db| db.update_time_entry(id, &patch)).map(|_| "Entry updated")
            }
            _ => {
                let input = TimeEntryInput {
                    project_id: self.project_id,
                    description: Some(description),
                    start_time,
                    end_time: Some(end_time),
                    duration_minutes: Some(minutes as f64),
                    is_billable: Some(is_billable),
                };
                store::write(cx, |db| db.create_time_entry(&input)).map(|_| "Time added")
            }
        };
        match result {
            Ok(message) => {
                self.saved = true;
                toast_success(message, cx);
                cx.emit(EntryEditorEvent::Saved);
            }
            Err(e) => toast_error(format!("Couldn't save: {e}"), cx),
        }
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        cx.emit(EntryEditorEvent::Closed);
    }
}

/// The labelled cell in the date/start/end/duration row.
fn field_cell(grow: f32, label_el: impl IntoElement, control: impl IntoElement) -> Div {
    let mut cell = div().flex_basis(px(0.)).min_w_0().child(label_el).child(control);
    cell.style().flex_grow = Some(grow);
    cell.style().flex_shrink = Some(1.0);
    cell
}

impl Render for EntryEditorModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(cx);
        let mode = self.mode;
        let entry = self.entry.clone();
        let editing = mode == EntryEditorMode::Edit;
        let on_invoice = editing && entry.as_ref().is_some_and(|e| e.entry.invoice_id.is_some_and(|i| i != 0));
        let locked = editing && is_locked_entry(entry.as_ref());
        let duration_invalid = self.duration_invalid(cx);
        let invalid = self.invalid(cx);

        let mut body = div().flex().flex_col().gap(r(4.));

        if on_invoice {
            let e = entry.as_ref();
            let number = e.and_then(|e| e.invoice_number.clone()).unwrap_or_default();
            let message = if locked {
                let state = if e.is_some_and(|e| e.billing_state == "paid") { "paid" } else { "sent" };
                rich(
                    &[
                        ("This time is on ", 500, t.fg2),
                        (&number, 700, t.fg),
                        (&format!(", which has been {state}. Changing it here won't change the invoice."), 500, t.fg2),
                    ],
                    13.0,
                    18.0,
                )
            } else {
                rich(
                    &[("This time is on draft ", 500, t.fg2), (&number, 700, t.fg), (". After saving, open the draft and update its lines.", 500, t.fg2)],
                    13.0,
                    18.0,
                )
            };
            body = body.child(
                div()
                    .flex()
                    .gap(r(2.5))
                    .p(r(3.))
                    .rounded(r(2.))
                    .bg(if locked { t.amber.opacity(0.1) } else { t.fg.opacity(0.04) })
                    .child(icon(Lucide::AlertTriangle).size(r(4.)).mt(px(1.)).text_color(if locked { t.amber } else { t.fg3 }))
                    .child(message.flex_1().min_w_0()),
            );
        }

        if mode == EntryEditorMode::Add {
            let mut options: Vec<(i64, SharedString)> = Vec::new();
            if self.projects.is_empty() {
                options.push((0, "No active projects".into()));
            }
            for p in &self.projects {
                let name = match p.client_name.as_deref() {
                    Some(c) if !c.is_empty() => format!("{} · {}", p.name, c),
                    _ => p.name.clone(),
                };
                options.push((p.id, name.into()));
            }
            let this = cx.entity().downgrade();
            body = body.child(div().child(label(&t, "Project")).child(select(
                "entry-project",
                options,
                &self.project_id,
                move |v, _, cx| {
                    let _ = this.update(cx, |m, cx| {
                        m.project_id = v;
                        cx.notify();
                    });
                },
                cx,
            )));
        } else if let Some(e) = entry.as_ref() {
            let mut line = div()
                .flex()
                .items_center()
                .gap(r(2.))
                .child(dot(parse_hex(e.project_color.as_deref().unwrap_or("#8E8E93")), r(2.)))
                .child(text(e.project_name.clone().unwrap_or_default(), 13.0, 500, t.fg).line_height(px(18.)));
            if let Some(c) = e.client_name.as_deref().filter(|c| !c.is_empty()) {
                line = line.child(text(format!("· {c}"), 13.0, 500, t.fg3).line_height(px(18.)));
            }
            body = body.child(line);
        }

        body = body.child(div().child(label(&t, "Description")).child(super::fields::TextField::new(&self.description)));

        // Duration: `.input.num`, with a red border while it can't be read.
        let duration_focused = self.duration.read(cx).focus_handle(cx).is_focused(window);
        let mut duration_box = input_box(&t, duration_focused, false);
        if duration_invalid {
            duration_box = duration_box.border_color(t.red.opacity(0.6));
        }
        // gpui-component pads single-line inputs itself; `.input`'s px-3 is already on the box
        let duration_box = duration_box.child(div().flex_1().min_w_0().child(Input::new(&self.duration).appearance(false).px_0().font(num_font(500))));

        body = body.child(
            div()
                .flex()
                .gap(r(3.))
                .child(field_cell(1.25, label(&t, "Date"), self.date.clone()))
                .child(field_cell(1.1, label(&t, "Start"), self.start.clone()))
                .child(field_cell(1.1, label(&t, "End"), self.end.clone()))
                .child(field_cell(0.8, label(&t, "Duration"), duration_box)),
        );

        let mut bumps = div().flex().gap(r(1.));
        for m in [15i64, 30, 60] {
            bumps = bumps.child(
                Button::ghost(SharedString::from(format!("entry-bump-{m}")), if m >= 60 { "+1h".to_string() } else { format!("+{m}m") })
                    .small()
                    .on_click(cx.listener(move |this, _, window, cx| this.bump(m, window, cx))),
            );
        }
        body = body.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    text(
                        if duration_invalid { "Try 2h 30m, 1.5h, 2:30, or minutes" } else { "An end time before the start rolls to the next day" },
                        12.0,
                        500,
                        t.fg3,
                    )
                    .line_height(px(16.)),
                )
                .child(bumps),
        );

        let mut billable = div()
            .id("entry-billable")
            .flex()
            .items_center()
            .gap(r(2.5))
            .cursor_pointer()
            .child(checkbox(&t, "entry-billable-box", self.is_billable, false))
            .child(text("Billable", 13.0, 500, t.fg).line_height(px(18.)));
        if on_invoice {
            billable = billable.child(text("· already on an invoice", 12.0, 500, t.fg3).line_height(px(16.)));
        } else {
            billable = billable.on_click(cx.listener(|this, _, _, cx| {
                this.is_billable = !this.is_billable;
                cx.notify();
            }));
        }
        body = body.child(div().flex().child(billable));

        let e1 = cx.entity().downgrade();
        let modal = Modal::new("entry-editor", if editing { "Edit time entry" } else { "Add time" }, move |_, cx| {
            let _ = e1.update(cx, |m, cx| m.close(cx));
        })
        .child(body)
        .footer(Button::secondary("entry-cancel", "Cancel").on_click(cx.listener(|this, _, _, cx| this.close(cx))))
        .footer(
            Button::primary("entry-save", if editing { "Save changes" } else { "Add time" })
                .disabled(invalid)
                .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
        );

        // The dialog's own focus: Escape and ⌘↩ when no field has focus, and
        // ⌘↩ from the date and time fields.
        div()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| {
                let k = &e.keystroke;
                if k.key == "enter" && (k.modifiers.platform || k.modifiers.control) {
                    cx.stop_propagation();
                    this.save(cx);
                } else if k.key == "escape" {
                    cx.stop_propagation();
                    this.close(cx);
                }
            }))
            .child(modal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_read_like_the_app() {
        assert_eq!(parse_duration_input("2h 30m"), Some(150));
        assert_eq!(parse_duration_input("1.5h"), Some(90));
        assert_eq!(parse_duration_input("2:30"), Some(150));
        assert_eq!(parse_duration_input("90"), Some(90));
        assert_eq!(parse_duration_input("45m"), Some(45));
        assert_eq!(parse_duration_input("2.5"), Some(3));
        assert_eq!(parse_duration_input(" 1H5M "), Some(65));
        assert_eq!(parse_duration_input("2 hours 15 min"), Some(135));
        assert_eq!(parse_duration_input("abc"), None);
        assert_eq!(parse_duration_input(""), None);
        assert_eq!(parse_duration_input("1:2:3"), None);
        assert_eq!(parse_duration_input("0"), Some(0));
    }

    #[test]
    fn end_before_start_rolls_over() {
        let d = NaiveDate::from_ymd_opt(2026, 3, 4).unwrap();
        let t = |h, m| NaiveTime::from_hms_opt(h, m, 0).unwrap();
        assert_eq!(minutes_between(d, t(9, 0), t(10, 30)), 90);
        assert_eq!(minutes_between(d, t(23, 0), t(1, 0)), 120);
        assert_eq!(minutes_between(d, t(9, 0), t(9, 0)), 1440);
        assert_eq!(end_from_duration(d, t(23, 30), 60), t(0, 30));
    }
}
