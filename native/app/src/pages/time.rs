//! TimeTracking.tsx: the running timer, the week at a glance with each
//! day's bar split by project, the unbilled / on-an-invoice filter and
//! search, entries grouped by day with their row menus, and the add/edit
//! editor and delete confirmation.

pub mod row;

use std::{collections::BTreeMap, rc::Rc, time::Duration};

use billable_core::models::{EntryRow, Project};
use chrono::{Datelike, Duration as Days, Local, NaiveDate, Utc};
use gpui::{
    div, prelude::*, px, relative, Animation, AnimationExt as _, AnyElement, App, AppContext, Context, Div, ElementId,
    Entity, FocusHandle, Hsla, IntoElement, ParentElement, Render, SharedString, Styled, Subscription, Window,
};
use gpui_component::{
    input::{InputEvent, InputState},
    tooltip::Tooltip,
};

use self::row::{entry_row, entry_value, num, rich, running_clock, RowHandlers};
use crate::{
    components::{
        entry_editor::{EntryEditorEvent, EntryEditorMode, EntryEditorModal},
        fields::search_field,
        segmented, toast, toast_error, toast_success, Button, EmptyState, Modal, ModalSize, Mood, Segment, ToastIcon, Variant,
    },
    format::{duration_short, local_day, money, parse_time},
    icons::Lucide,
    motion::{self, after, cubic_bezier, rise_in},
    shell::app::{Header, Page},
    store,
    style::{card, r, text},
    theme::{hex, parse_hex, theme, Theme},
    timer,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Filter {
    All,
    Unbilled,
    Billed,
}

pub struct TimePage {
    rev: u64,
    entries: Vec<EntryRow>,
    projects: Vec<Project>,
    search: Entity<InputState>,
    filter: Filter,
    week_start: NaiveDate,
    selected_day: Option<NaiveDate>,
    editor: Option<Entity<EntryEditorModal>>,
    editor_sub: Option<Subscription>,
    deleting: Option<EntryRow>,
    delete_focus: FocusHandle,
    menu: Option<i64>,
    _subs: Vec<Subscription>,
}

/// Monday of the week containing `d`.
fn monday_of(d: NaiveDate) -> NaiveDate {
    d - Days::days(d.weekday().num_days_from_monday() as i64)
}

/// "Today", "Yesterday", or "Monday, Nov 3".
fn day_label(d: NaiveDate, today: NaiveDate) -> String {
    if d == today {
        "Today".into()
    } else if d == today - Days::days(1) {
        "Yesterday".into()
    } else {
        d.format("%A, %b %-d").to_string()
    }
}

/// "Nov 3 – 9, 2026" or "Oct 27 – Nov 2, 2026".
fn week_range_label(a: NaiveDate, b: NaiveDate) -> String {
    let left = a.format("%b %-d").to_string();
    let right = if a.month() == b.month() { b.day().to_string() } else { b.format("%b %-d").to_string() };
    format!("{left} – {right}, {}", b.year())
}

fn is_billed(e: &EntryRow) -> bool {
    e.entry.invoice_id.is_some_and(|i| i != 0) || e.billing_state == "invoiced"
}

/// One project's share of a day's bar.
struct DaySegment {
    project_id: i64,
    name: String,
    color: Hsla,
    minutes: f64,
}

impl TimePage {
    pub fn new(action_new: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search this week"));
        let subs = vec![cx.subscribe(&search, |_, _, e: &InputEvent, cx| {
            if matches!(e, InputEvent::Change) {
                cx.notify();
            }
        })];
        let mut page = TimePage {
            rev: 0,
            entries: vec![],
            projects: vec![],
            search,
            filter: Filter::All,
            week_start: monday_of(Local::now().date_naive()),
            selected_day: None,
            editor: None,
            editor_sub: None,
            deleting: None,
            delete_focus: cx.focus_handle(),
            menu: None,
            _subs: subs,
        };
        page.reload(cx);
        // ⌘T from the menu opens the editor
        if action_new {
            page.open_editor(EntryEditorMode::Add, None, window, cx);
        }
        page
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        let db = store::db(cx);
        self.entries = db.list_time_entries(None).unwrap_or_default();
        self.projects = db.list_projects(None).map(|p| p.into_iter().filter(|p| p.status == "active").collect()).unwrap_or_default();
        self.rev = store::rev(cx);
    }

    fn open_editor(&mut self, mode: EntryEditorMode, entry: Option<EntryRow>, window: &mut Window, cx: &mut Context<Self>) {
        let projects = self.projects.clone();
        let default_date = self.selected_day.map(|d| d.format("%Y-%m-%d").to_string());
        let modal = cx.new(|cx| EntryEditorModal::new(mode, entry, projects, default_date, None, window, cx));
        self.editor_sub = Some(cx.subscribe(&modal, |p, _, _: &EntryEditorEvent, cx| {
            p.editor = None;
            cx.notify();
        }));
        self.editor = Some(modal);
        self.menu = None;
        cx.notify();
    }

    fn set_week(&mut self, start: NaiveDate, cx: &mut Context<Self>) {
        self.week_start = start;
        self.selected_day = None;
        cx.notify();
    }
}

// ---- Actions ----

fn stop_timer(cx: &mut App) {
    let s = store::store(cx);
    let Some(id) = s.timer.active.as_ref().or(s.timer.paused.as_ref()).map(|e| e.entry.id) else { return };
    match store::write(cx, |db| db.stop_timer(id, Some(Utc::now()))) {
        Ok(Some(e)) if e.discarded == Some(true) => toast("Timer ran under a minute, so it was discarded", ToastIcon::None, cx),
        Ok(_) => toast_success("Timer stopped", cx),
        Err(e) => toast_error(format!("Couldn't stop the timer: {e}"), cx),
    }
}

fn restart(entry: &EntryRow, cx: &mut App) {
    timer::start(entry.entry.project_id, Some(entry.entry.description.clone()), cx);
    let started = store::store(cx).timer.active.as_ref().is_some_and(|a| a.entry.project_id == entry.entry.project_id);
    if started {
        toast_success(format!("Timer started for {}", entry.project_name.clone().unwrap_or_default()), cx);
    }
}

fn add_time(entry: &EntryRow, minutes: f64, cx: &mut App) {
    let duration = entry.entry.duration_minutes + minutes;
    let Some(start) = parse_time(&entry.entry.start_time) else { return };
    let end = start + Days::milliseconds((duration * 60_000.0) as i64);
    let patch = billable_core::models::TimeEntryPatch {
        duration_minutes: Some(duration),
        end_time: Some(Some(end.with_timezone(&Utc).format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string())),
        ..Default::default()
    };
    let id = entry.entry.id;
    match store::write(cx, |db| db.update_time_entry(id, &patch)) {
        Ok(_) => toast_success(format!("Added {}", duration_short(minutes)), cx),
        Err(e) => toast_error(e.to_string(), cx),
    }
}

fn set_billable(id: i64, billable: bool, cx: &mut App) {
    match store::write(cx, |db| db.set_entries_billable(&[id], billable)) {
        Ok(_) => toast_success(if billable { "Marked billable" } else { "Marked as don't bill" }, cx),
        Err(e) => toast_error(e.to_string(), cx),
    }
}

impl Page for TimePage {
    fn header(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> Header {
        Header::new("Time").actions(Button::secondary("time-add", "Add time").on_click(cx.listener(|p, _, w, cx| p.open_editor(EntryEditorMode::Add, None, w, cx))))
    }
}

impl Render for TimePage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.rev != store::rev(cx) {
            self.reload(cx);
        }
        let t = theme(cx);
        let today = Local::now().date_naive();
        let week_start = self.week_start;
        let week_days: Vec<NaiveDate> = (0..7).map(|i| week_start + Days::days(i)).collect();
        let week_end = week_days[6];
        let is_current_week = week_start == monday_of(today);

        // ---- Week / day derivations ----
        let completed: Vec<&EntryRow> = self.entries.iter().filter(|e| e.entry.end_time.as_deref().is_some_and(|s| !s.is_empty())).collect();
        let in_week: Vec<(NaiveDate, &EntryRow)> = completed
            .iter()
            .filter_map(|e| local_day(&e.entry.start_time).filter(|d| *d >= week_start && *d <= week_end).map(|d| (d, *e)))
            .collect();
        let mut minutes_by_day: BTreeMap<NaiveDate, f64> = BTreeMap::new();
        let mut segments_by_day: BTreeMap<NaiveDate, Vec<DaySegment>> = BTreeMap::new();
        for (d, e) in &in_week {
            *minutes_by_day.entry(*d).or_default() += e.entry.duration_minutes;
            let day = segments_by_day.entry(*d).or_default();
            match day.iter_mut().find(|s| s.project_id == e.entry.project_id) {
                Some(s) => s.minutes += e.entry.duration_minutes,
                None => day.push(DaySegment {
                    project_id: e.entry.project_id,
                    name: e.project_name.clone().unwrap_or_else(|| "Project".into()),
                    color: parse_hex(e.project_color.as_deref().unwrap_or("#8E8E93")),
                    minutes: e.entry.duration_minutes,
                }),
            }
        }
        for segs in segments_by_day.values_mut() {
            segs.sort_by(|a, b| b.minutes.total_cmp(&a.minutes));
        }
        let max_day_minutes = minutes_by_day.values().copied().fold(60.0, f64::max);
        let week_minutes: f64 = in_week.iter().map(|(_, e)| e.entry.duration_minutes).sum();
        let week_value: f64 = in_week.iter().map(|(_, e)| entry_value(e)).sum();
        let week_unbilled: f64 = in_week.iter().filter(|(_, e)| e.billing_state == "unbilled").map(|(_, e)| entry_value(e)).sum();

        let selected = self.selected_day;
        let on_day = |d: &NaiveDate| selected.is_none_or(|s| s == *d);
        let search_raw = self.search.read(cx).value().to_string();
        let q = search_raw.trim().to_lowercase();
        let filter = self.filter;
        let visible: Vec<(NaiveDate, &EntryRow)> = in_week
            .iter()
            .filter(|(d, e)| {
                on_day(d)
                    && match filter {
                        Filter::All => true,
                        Filter::Unbilled => e.billing_state == "unbilled",
                        Filter::Billed => is_billed(e),
                    }
                    && (q.is_empty()
                        || e.entry.description.to_lowercase().contains(&q)
                        || e.project_name.as_deref().unwrap_or("").to_lowercase().contains(&q)
                        || e.client_name.as_deref().unwrap_or("").to_lowercase().contains(&q))
            })
            .copied()
            .collect();
        let mut grouped: BTreeMap<NaiveDate, Vec<&EntryRow>> = BTreeMap::new();
        for (d, e) in &visible {
            grouped.entry(*d).or_default().push(e);
        }
        for list in grouped.values_mut() {
            list.sort_by(|a, b| b.entry.start_time.cmp(&a.entry.start_time));
        }
        let counts = (
            in_week.iter().filter(|(d, _)| on_day(d)).count(),
            in_week.iter().filter(|(d, e)| on_day(d) && e.billing_state == "unbilled").count(),
            in_week.iter().filter(|(d, e)| on_day(d) && is_billed(e)).count(),
        );

        let mut page = div().w_full().max_w(px(1160.)).mx_auto().px(r(7.)).pt(r(6.)).pb(r(14.)).flex().flex_col();
        let mut block = 0;

        // ---- Running timer ----
        let s = store::store(cx);
        let (running, paused) = (s.timer_running(), s.timer_paused());
        if let (true, Some(active)) = (running || paused, s.timer_entry().cloned()) {
            let toggle = Button::secondary("time-pause", if paused { "Resume" } else { "Pause" })
                .icon(if paused { Lucide::Play } else { Lucide::Pause })
                .filled()
                .on_click(move |_, _, cx| if paused { timer::resume(cx) } else { timer::pause(cx) });
            let stop = Button::primary("time-stop", "Stop").icon(Lucide::Square).filled().icon_size(r(3.).into()).on_click(|_, _, cx| stop_timer(cx));
            let timer_card = card(&t)
                .mb(r(5.))
                .pl(r(3.))
                .pr(r(4.))
                .py(r(3.))
                .flex()
                .items_center()
                .gap(r(4.))
                .children(running_clock(&t, &active, paused))
                .child(div().flex().gap(r(2.)).child(toggle).child(stop));
            page = page.child(rise_in("time-rise-timer", block, timer_card));
            block += 1;
        }

        // ---- Week ----
        page = page.child(rise_in("time-rise-week", block, self.week_card(&t, &week_days, today, is_current_week, &minutes_by_day, &segments_by_day, max_day_minutes, (week_minutes, week_value, week_unbilled), cx)));
        block += 1;

        // ---- Filters ----
        let mut filters = div()
            .flex()
            .items_center()
            .gap(r(3.))
            .mb(r(4.))
            .child(segmented(
                "time-filter",
                &self.filter,
                vec![
                    Segment::new(Filter::All, "All").count(counts.0),
                    Segment::new(Filter::Unbilled, "Unbilled").count(counts.1),
                    Segment::new(Filter::Billed, "On an invoice").count(counts.2),
                ],
                {
                    let this = cx.weak_entity();
                    move |v, _, cx| {
                        let _ = this.update(cx, |p, cx| {
                            p.filter = v;
                            cx.notify();
                        });
                    }
                },
                cx,
            ));
        if let Some(d) = selected {
            filters = filters.child(
                Button::ghost("time-whole-week", format!("Showing {} · show the whole week", day_label(d, today))).small().on_click(cx.listener(|p, _, _, cx| {
                    p.selected_day = None;
                    cx.notify();
                })),
            );
        }
        filters = filters.child(div().ml_auto().w(r(64.)).child(search_field("time-search-clear", &self.search, cx)));
        page = page.child(rise_in("time-rise-filters", block, filters));
        block += 1;

        // ---- Entries by day ----
        if grouped.is_empty() {
            let title = if completed.is_empty() {
                "Nothing on the clock yet".to_string()
            } else if filter != Filter::All || !search_raw.is_empty() {
                "Nothing matches".into()
            } else if let Some(d) = selected {
                let l = day_label(d, today);
                if l == "Today" { "Nothing logged today".into() } else { format!("Nothing logged on {l}") }
            } else {
                "Nothing logged this week".into()
            };
            let description = if completed.is_empty() { "Start a timer from the toolbar or add time by hand." } else { "Try another week or filter, or add time by hand." };
            let empty = card(&t).child(
                EmptyState::new("time-empty", title, description)
                    .mood(Mood::Sleepy)
                    .action("Add time", cx.listener(|p, _, w, cx| p.open_editor(EntryEditorMode::Add, None, w, cx))),
            );
            page = page.child(rise_in("time-rise-empty", block, empty));
        } else {
            let mut list = div().flex().flex_col().gap(r(4.));
            for (day, entries) in grouped.iter().rev() {
                list = list.child(self.day_group(&t, *day, today, entries, cx));
            }
            page = page.child(rise_in("time-rise-list", block, list));
        }

        page = page.children(self.editor.clone());
        if let Some(e) = self.deleting.clone() {
            page = page.child(self.delete_dialog(&t, e, cx));
        }
        page
    }
}

impl TimePage {
    #[allow(clippy::too_many_arguments)]
    fn week_card(
        &self,
        t: &Theme,
        week_days: &[NaiveDate],
        today: NaiveDate,
        is_current_week: bool,
        minutes_by_day: &BTreeMap<NaiveDate, f64>,
        segments_by_day: &BTreeMap<NaiveDate, Vec<DaySegment>>,
        max_day_minutes: f64,
        (week_minutes, week_value, week_unbilled): (f64, f64, f64),
        cx: &mut Context<Self>,
    ) -> Div {
        let week_start = self.week_start;
        let mut left = div()
            .flex()
            .items_center()
            .gap(r(1.))
            .child(
                Button::icon_button("time-prev-week", Lucide::ChevronLeft)
                    .small()
                    .tooltip("Previous week")
                    .on_click(cx.listener(move |p, _, _, cx| p.set_week(week_start - Days::days(7), cx))),
            )
            .child(
                Button::icon_button("time-next-week", Lucide::ChevronRight)
                    .small()
                    .tooltip("Next week")
                    .on_click(cx.listener(move |p, _, _, cx| p.set_week(week_start + Days::days(7), cx))),
            )
            .child(text(week_range_label(week_days[0], week_days[6]), 13.0, 600, t.fg).ml(r(1.5)));
        if !is_current_week {
            left = left.child(
                div().ml(r(1.)).child(Button::ghost("time-this-week", "This week").small().on_click(cx.listener(move |p, _, _, cx| p.set_week(monday_of(today), cx)))),
            );
        }

        let stat = |label: &str, value: String, weight: u16, color: Hsla| {
            div().flex().items_baseline().child(text(format!("{label} "), 12.0, 500, t.fg3).line_height(px(16.))).child(num(value, 13.0, weight, color).line_height(px(16.)))
        };
        let mut right = div().flex().items_center().gap(r(4.)).pr(r(1.));
        if week_unbilled > 0.0 {
            right = right.child(stat("Unbilled", money(week_unbilled), 500, t.amber));
        }
        right = right.child(stat("Value", money(week_value), 500, t.fg)).child(stat("Total", duration_short(week_minutes), 600, t.fg));

        let head = div().flex().items_center().justify_between().px(r(3.)).h(px(42.)).border_b_1().border_color(t.line).child(left).child(right);

        let mut grid = div().flex().gap(r(1.)).p(r(1.5));
        let grow = cubic_bezier(0.25, 1.3, 0.5, 1.0);
        for (i, d) in week_days.iter().enumerate() {
            let d = *d;
            let mins = minutes_by_day.get(&d).copied().unwrap_or(0.0);
            let segments = segments_by_day.get(&d).map(|v| v.as_slice()).unwrap_or(&[]);
            let is_today = d == today;
            let is_selected = self.selected_day == Some(d);

            let date_number: AnyElement = if is_today {
                div()
                    .min_w(px(20.))
                    .h(r(5.))
                    .px(r(1.))
                    .rounded_full()
                    .bg(t.accent)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(num(d.day().to_string(), 12.0, 600, hex(0xffffff)).line_height(px(16.)))
                    .into_any_element()
            } else {
                num(d.day().to_string(), 13.0, 600, t.fg).into_any_element()
            };

            let bar: AnyElement = if mins > 0.0 {
                let height = ((mins / max_day_minutes) * 100.0).max(16.0) as f32 / 100.0;
                let n = segments.len();
                // flex-col-reverse: the first (largest) project sits at the bottom
                let mut stack = div().w_full().h_full().flex().flex_col_reverse().gap(px(1.));
                for (k, sg) in segments.iter().enumerate() {
                    let mut seg = div().flex_basis(px(0.)).bg(sg.color);
                    seg.style().flex_grow = Some(sg.minutes as f32);
                    // rounded-[6px] overflow-hidden: GPUI clips to rectangles, so round the ends themselves
                    if k == 0 {
                        seg = seg.rounded_b(px(6.));
                    }
                    if k + 1 == n {
                        seg = seg.rounded_t(px(6.));
                    }
                    stack = stack.child(seg);
                }
                let holder = div().w_full().flex().items_end().child(stack);
                if motion::reduced() {
                    holder.h(relative(height)).into_any_element()
                } else {
                    let g = grow.clone();
                    let delay = 120.0 + i as f32 * 50.0;
                    holder
                        .with_animation(ElementId::Name(format!("time-bar-{d}").into()), Animation::new(Duration::from_millis((delay + 620.0) as u64)), move |el, delta| {
                            el.h(relative((height * g(after(delta, delay, 620.0))).max(0.0)))
                        })
                        .into_any_element()
                }
            } else {
                div().w_full().h(px(2.)).rounded_full().bg(t.fg.opacity(0.07)).into_any_element()
            };

            let (sel_bg, hover_bg) = (t.fg.opacity(0.07), t.fg.opacity(0.035));
            let mut button = div()
                .id(ElementId::Name(format!("time-day-{d}").into()))
                .flex_1()
                .min_w_0()
                .rounded(px(7.))
                .px(r(2.5))
                .pt(r(2.))
                .pb(r(2.))
                .cursor_pointer()
                .on_click(cx.listener(move |p, _, _, cx| {
                    p.selected_day = if p.selected_day == Some(d) { None } else { Some(d) };
                    cx.notify();
                }))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .h(r(5.))
                        .child(text(d.format("%a").to_string(), 11.0, 500, if is_today { t.accent_text } else { t.fg3 }).line_height(px(14.)))
                        .child(date_number),
                )
                .child(div().h(r(10.)).mt(r(2.)).flex().items_end().child(bar))
                .child(
                    num(if mins > 0.0 { duration_short(mins) } else { "—".into() }, 12.0, 500, if mins > 0.0 { t.fg2 } else { t.fg4 })
                        .mt(r(1.5))
                        .line_height(px(16.)),
                );
            button = if is_selected { button.bg(sel_bg) } else { button.hover(move |s| s.bg(hover_bg)) };
            if !segments.is_empty() {
                let tip: SharedString = segments.iter().map(|sg| format!("{} {}", sg.name, duration_short(sg.minutes))).collect::<Vec<_>>().join("\n").into();
                button = button.tooltip(move |w, cx| Tooltip::new(tip.clone()).build(w, cx));
            }
            grid = grid.child(button);
        }

        card(t).mb(r(5.)).child(head).child(grid)
    }

    fn day_group(&self, t: &Theme, day: NaiveDate, today: NaiveDate, entries: &[&EntryRow], cx: &mut Context<Self>) -> Div {
        let minutes: f64 = entries.iter().map(|e| e.entry.duration_minutes).sum();
        let amount: f64 = entries.iter().map(|e| entry_value(e)).sum();
        let n = entries.len();
        // Same trailing columns as the rows below: duration, value, actions
        let head = div()
            .flex()
            .items_center()
            .gap(r(3.))
            .px(r(4.))
            .h(px(34.))
            .border_b_1()
            .border_color(t.line)
            .bg(t.fg.opacity(0.018))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_baseline()
                    .gap(r(2.))
                    .child(text(day_label(day, today), 13.0, 600, t.fg))
                    .child(text(format!("{n} {}", if n == 1 { "entry" } else { "entries" }), 12.0, 500, t.fg3).line_height(px(16.))),
            )
            .child(num(duration_short(minutes), 13.0, 600, t.fg).w(px(64.)).flex_none().text_right().whitespace_nowrap())
            .child(num(if amount > 0.0 { money(amount) } else { String::new() }, 13.0, 500, t.fg3).w(px(84.)).flex_none().text_right().whitespace_nowrap())
            .child(div().w(px(60.)).flex_none());

        let mut body = div().flex().flex_col();
        for (i, e) in entries.iter().enumerate() {
            body = body.child(entry_row(e, true, i + 1 == n, self.menu == Some(e.entry.id), self.row_handlers(e, cx), cx));
        }
        card(t).overflow_hidden().child(head).child(body)
    }

    fn row_handlers(&self, e: &EntryRow, cx: &mut Context<Self>) -> RowHandlers {
        let this = cx.weak_entity();
        let id = e.entry.id;
        let (w_edit, w_del, w_toggle, w_close) = (this.clone(), this.clone(), this.clone(), this.clone());
        let (edit_entry, del_entry, restart_entry, add_entry) = (e.clone(), e.clone(), e.clone(), e.clone());
        RowHandlers {
            edit: Rc::new(move |w, cx| {
                let entry = edit_entry.clone();
                let _ = w_edit.update(cx, |p, cx| p.open_editor(EntryEditorMode::Edit, Some(entry), w, cx));
            }),
            delete: Rc::new(move |_, cx| {
                let entry = del_entry.clone();
                let _ = w_del.update(cx, |p, cx| {
                    p.deleting = Some(entry);
                    cx.notify();
                });
            }),
            restart: Some(Rc::new(move |_, cx| restart(&restart_entry, cx))),
            add_time: Some(Rc::new(move |m, _, cx| add_time(&add_entry, m, cx))),
            set_billable: Some(Rc::new(move |b, _, cx| set_billable(id, b, cx))),
            toggle_menu: Rc::new(move |_, cx| {
                let _ = w_toggle.update(cx, |p, cx| {
                    p.menu = if p.menu == Some(id) { None } else { Some(id) };
                    cx.notify();
                });
            }),
            close_menu: Rc::new(move |_, cx| {
                let _ = w_close.update(cx, |p, cx| {
                    p.menu = None;
                    cx.notify();
                });
            }),
        }
    }

    /// ConfirmDialog, with the invoice number in bold when the entry is on one.
    fn delete_dialog(&self, t: &Theme, entry: EntryRow, cx: &mut Context<Self>) -> impl IntoElement {
        let message = match (entry.entry.invoice_id.filter(|i| *i != 0), entry.invoice_number.as_deref()) {
            (Some(_), number) => rich(
                &[
                    ("It's on ", 500, t.fg2),
                    (number.unwrap_or(""), 700, t.fg),
                    (". The invoice keeps its lines, but this time will no longer be linked to it.", 500, t.fg2),
                ],
                13.0,
                20.0,
            ),
            _ => text("This removes the entry for good.", 13.0, 500, t.fg2).line_height(px(20.)),
        };
        let id = entry.entry.id;
        let close = {
            let this = cx.weak_entity();
            Rc::new(move |cx: &mut App| {
                let _ = this.update(cx, |p, cx| {
                    p.deleting = None;
                    cx.notify();
                });
            })
        };
        let (c1, c2, c3) = (close.clone(), close.clone(), close);
        Modal::new("time-delete", "Delete this time entry?", move |_, cx| c1(cx))
            .size(ModalSize::Sm)
            .focus(&self.delete_focus)
            .child(message)
            .footer(Button::secondary("time-delete-cancel", "Cancel").on_click(move |_, _, cx| c2(cx)))
            .footer(Button::new("time-delete-confirm", Variant::Destructive).label("Delete").on_click(move |_, _, cx| {
                match store::write(cx, |db| db.delete_time_entry(id)) {
                    Ok(_) => toast_success("Entry deleted", cx),
                    Err(e) => toast_error(e.to_string(), cx),
                }
                c3(cx);
            }))
    }
}
