//! Dashboard.tsx: the mascot's take on today, the money pipeline, today's
//! ruler, things that need attention, recent time, this week, and quick
//! timers.

use std::{collections::HashMap, time::Duration};

use billable_core::models::{BillingOverview, DashboardStats, EntryRow, Project, TimerRow};
use chrono::{Datelike, Duration as Days, Local, NaiveDate};
use gpui::{
    div, img, prelude::*, px, relative, Animation, AnimationExt as _, AnyElement, Context, Div, ElementId, Hsla,
    IntoElement, ParentElement, Render, SharedString, Styled, Window,
};

use crate::{
    art::{Art, Gallery, Icon},
    components::{billing_chip, count_badge, money, money_counting, metric_strip, Button, EmptyState, Mascot, Metric, Mood, Motion, Tint},
    format::{self, duration_short, hours_short},
    icons::Lucide,
    motion::{self, after, cubic_bezier, rise_in},
    shell::{
        app::{navigate, Header, Page},
        route::Route,
    },
    store,
    style::{card, dot, figures, group_head, icon, icon_filled, icon_stroke, r, section_title, separator, text},
    theme::{parse_hex, rgba4, shadow, theme, Theme},
    timer,
};

struct Data {
    stats: DashboardStats,
    billing: BillingOverview,
    entries: Vec<EntryRow>,
    projects: Vec<Project>,
}

pub struct DashboardPage {
    rev: u64,
    data: Option<Data>,
}

impl DashboardPage {
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        DashboardPage { rev: 0, data: None }
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        let db = store::db(cx);
        let data = (|| {
            let mut projects: Vec<Project> = db.list_projects(None)?.into_iter().filter(|p| p.status == "active").collect();
            projects.sort_by(|a, b| {
                let ka = a.last_activity.clone().unwrap_or_else(|| a.created_at.clone());
                let kb = b.last_activity.clone().unwrap_or_else(|| b.created_at.clone());
                kb.cmp(&ka)
            });
            anyhow::Ok(Data { stats: db.get_dashboard_stats()?, billing: db.get_billing_overview()?, entries: db.list_time_entries(None)?, projects })
        })();
        self.data = data.ok();
        self.rev = store::rev(cx);
    }
}

impl Page for DashboardPage {
    fn header(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> Header {
        Header::new("Dashboard").subtitle(Local::now().format("%A, %B %-d").to_string())
    }
}

/// `link` with a chevron (`.head-link`).
fn head_link(t: &Theme, id: &str, label: &str, to: Route) -> impl IntoElement {
    let accent = t.accent_text;
    div()
        .id(ElementId::Name(id.to_string().into()))
        .flex()
        .items_center()
        .gap(r(0.5))
        .cursor_pointer()
        .text_color(accent)
        .on_click(move |_, w, cx| navigate(to.clone(), w, cx))
        .child(div().font_family(crate::fonts::rounded_family(600)).text_size(px(13.)).child(label.to_string()))
        .child(icon(Lucide::ChevronRight).size(r(3.5)).text_color(accent))
}

impl Render for DashboardPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.data.is_none() || self.rev != store::rev(cx) {
            self.reload(cx);
        }
        let t = theme(cx);
        let Some(d) = self.data.as_ref() else {
            return div().into_any_element();
        };
        let s = store::store(cx);
        let active = s.timer_entry().cloned();
        let running = s.timer_running();
        let paused = s.timer_paused();
        let now = Local::now();
        let today = now.date_naive();
        let p = &d.billing.pipeline;

        // Today's hours, with a running timer counted in
        let running_today = active
            .as_ref()
            .filter(|e| format::local_day(&e.entry.start_time) == Some(today))
            .map(|e| timer::elapsed_seconds(e) / 3600.0)
            .unwrap_or(0.0);
        let today_hours = d.stats.hours_today + running_today;

        // What the mascot says: the most useful thing right now
        let (mood, message, action): (Mood, String, Option<(&str, Route, bool)>) = if p.overdue_count > 0 {
            (
                Mood::Worried,
                format!("{} is overdue. A friendly nudge usually does the trick.", format::money(p.overdue_amount)),
                Some(("See overdue", Route::Billing { status: Some("overdue".into()) }, false)),
            )
        } else if let (true, Some(e)) = (running, active.as_ref()) {
            (Mood::Happy, format!("You're on the clock for {}. Nice focus!", e.project_name.clone().unwrap_or_default()), None)
        } else if p.unbilled_amount > 0.0 && d.billing.attention.iter().any(|a| a.kind == "unbilled") {
            (Mood::Wave, format!("{} is ready to bill whenever you are.", format::money(p.unbilled_amount)), Some(("Bill it", Route::Billing { status: None }, true)))
        } else if today_hours == 0.0 {
            (Mood::Idle, "Fresh day, clean slate. Start a timer when you're ready.".into(), None)
        } else {
            (Mood::Happy, format!("Nice work! {} tracked so far today.", hours_short(today_hours)), None)
        };

        let mut talk = card(&t)
            .flex()
            .items_center()
            .gap(r(4.))
            .pl(r(3.))
            .pr(r(4.))
            .py(r(2.5))
            .child(Mascot::new("dash-mascot", mood, 58.0).motion(if mood == Mood::Happy { Motion::Hop } else { Motion::Bob }))
            .child(text(message, 16.0, 600, t.fg).line_height(px(22.)).flex_1().min_w_0());
        if let Some((label, to, primary)) = action {
            let b = if primary { Button::primary("dash-action", label) } else { Button::secondary("dash-action", label) };
            talk = talk.child(b.on_click(move |_, w, cx| navigate(to.clone(), w, cx)));
        }

        let plural = |n: i64| if n == 1 { "" } else { "s" };
        let metrics = metric_strip(vec![
            Metric::new("m-unbilled", "Ready to bill", money_counting("m-unbilled-v", p.unbilled_amount, 22.0, t.fg))
                .icon(Icon::Hourglass)
                .tint(Tint::Orange)
                .sub(if p.unbilled_hours > 0.0 { format!("{} not on an invoice", hours_short(p.unbilled_hours)) } else { "All caught up".into() })
                .on_click(|_, w, cx| navigate(Route::Billing { status: None }, w, cx)),
            Metric::new("m-awaiting", "Awaiting payment", money_counting("m-awaiting-v", p.awaiting_amount + p.draft_amount, 22.0, t.fg))
                .icon(Icon::Send)
                .tint(Tint::Blue)
                .sub(if p.draft_count > 0 {
                    format!("{} sent · {} draft", p.awaiting_count, p.draft_count)
                } else {
                    format!("{} invoice{} sent", p.awaiting_count, plural(p.awaiting_count))
                })
                .on_click(|_, w, cx| navigate(Route::Billing { status: Some("sent".into()) }, w, cx)),
            Metric::new("m-overdue", "Overdue", money_counting("m-overdue-v", p.overdue_amount, 22.0, if p.overdue_amount > 0.0 { t.red } else { t.fg }))
                .icon(Icon::Alert)
                .tint(Tint::Red)
                .sub(if p.overdue_count > 0 { format!("{} past due", p.overdue_count) } else { "Nothing late".into() })
                .on_click(|_, w, cx| navigate(Route::Billing { status: Some("overdue".into()) }, w, cx)),
            Metric::new("m-paid", format!("Paid in {}", today.year()), money_counting("m-paid-v", p.paid_ytd_amount, 22.0, t.fg))
                .icon(Icon::Trophy)
                .tint(Tint::Green)
                .sub(format!("{} invoice{}", p.paid_ytd_count, plural(p.paid_ytd_count)))
                .on_click(|_, w, cx| navigate(Route::Billing { status: Some("paid".into()) }, w, cx)),
        ]);

        let g = cx.global_mut::<Gallery>();
        let left = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(r(7.))
            .child(attention(&t, g, d))
            .child(recent(&t, d, now));
        let right = div()
            .w(px(310.))
            .flex_none()
            .flex()
            .flex_col()
            .gap(r(7.))
            .child(this_week(&t, d, today))
            .child(quick_timers(&t, d, active.as_ref().map(|e| e.entry.project_id), running, paused));

        div()
            .w_full()
            .max_w(px(1160.))
            .mx_auto()
            .px(r(7.))
            .pt(r(6.))
            .pb(r(14.))
            .flex()
            .flex_col()
            .child(rise_in("dash-rise-0", 0, talk.mb(r(5.))))
            .child(rise_in("dash-rise-1", 1, metrics.mb(r(6.))))
            .child(rise_in("dash-rise-2", 2, today_ruler(&t, d, active.as_ref(), now, window, cx).mb(r(7.))))
            .child(rise_in("dash-rise-3", 3, div().flex().gap(r(6.)).items_start().child(left).child(right)))
            .into_any_element()
    }
}

fn today_ruler(t: &Theme, d: &Data, active: Option<&TimerRow>, now: chrono::DateTime<Local>, window: &Window, cx: &mut gpui::App) -> Div {
    let today = now.date_naive();
    let midnight = format::local_midnight(today);
    let hours_at = |s: &str| format::parse_time(s).map(|x| ((x - midnight).num_milliseconds() as f64 / 3_600_000.0).clamp(0.0, 24.0)).unwrap_or(0.0);
    let now_h = ((now - midnight).num_milliseconds() as f64 / 3_600_000.0).clamp(0.0, 24.0);

    struct Block {
        start: f64,
        end: f64,
        color: Hsla,
        running: bool,
    }
    let mut blocks = Vec::new();
    let (mut minutes, mut earned) = (0.0, 0.0);
    for e in &d.entries {
        let Some(end) = e.entry.end_time.as_deref() else { continue };
        if format::local_day(&e.entry.start_time) != Some(today) {
            continue;
        }
        let start = hours_at(&e.entry.start_time);
        let stop = hours_at(end);
        blocks.push(Block {
            start,
            end: if stop > start { stop } else { (start + e.entry.duration_minutes / 60.0).min(24.0) },
            color: parse_hex(e.project_color.as_deref().unwrap_or("#8E8E93")),
            running: false,
        });
        minutes += e.entry.duration_minutes;
        if e.entry.is_billable != 0 {
            earned += e.entry.duration_minutes / 60.0 * e.rate.unwrap_or(0.0);
        }
    }
    if let Some(a) = active.filter(|a| format::local_day(&a.entry.start_time) == Some(today)) {
        let start = hours_at(&a.entry.start_time);
        let until = a.entry.paused_at.as_deref().map(hours_at).unwrap_or(now_h);
        let secs = timer::elapsed_seconds(a);
        blocks.push(Block {
            start,
            end: until.max(start + 1.0 / 60.0),
            color: parse_hex(a.project_color.as_deref().unwrap_or("#8E8E93")),
            running: a.entry.paused_at.is_none(),
        });
        minutes += secs / 60.0;
        if a.entry.is_billable != 0 {
            earned += secs / 3600.0 * a.rate.unwrap_or(0.0);
        }
    }

    let first = blocks.iter().map(|b| b.start.floor() as i64).fold(8, i64::min);
    let last = blocks.iter().map(|b| b.end.ceil() as i64).fold(20.max((now_h + 0.25).ceil() as i64), i64::max);
    let span = (last - first) as f64;
    let pct = |h: f64| relative(((h - first as f64) / span) as f32);

    let summary: AnyElement = if minutes > 0.0 {
        let mut row = div().flex().items_baseline().child(figures(duration_short(minutes), 15.0, t.fg));
        if earned > 0.0 {
            row = row
                .child(text(" · ", 13.0, 700, t.fg3))
                .child(figures(format!("+{}", format::money(earned)), 15.0, t.green))
                .child(text(" earned", 13.0, 700, t.fg3));
        }
        row.into_any_element()
    } else {
        text("Nothing on the clock yet", 13.0, 700, t.fg3).into_any_element()
    };

    let grow = cubic_bezier(0.2, 0.9, 0.2, 1.0);
    let mut track = div().relative().h(px(30.)).rounded(px(11.)).bg(t.line.opacity(0.7));
    for (i, b) in blocks.iter().enumerate() {
        let width = ((b.end - b.start) / span) as f32;
        let block = div()
            .absolute()
            .top(px(4.))
            .bottom(px(4.))
            .left(pct(b.start))
            .min_w(px(3.))
            .rounded(px(8.))
            .bg(b.color.opacity(if b.running { 0.85 } else { 1.0 }));
        let delay = 200.0 + i as f32 * 90.0;
        let g2 = grow.clone();
        track = track.child(if motion::reduced() {
            block.w(relative(width)).into_any_element()
        } else {
            block
                .with_animation(("ruler-block", i), Animation::new(Duration::from_millis((delay + 700.0) as u64)), move |el, delta| {
                    el.w(relative(width * g2(after(delta, delay, 700.0))))
                })
                .into_any_element()
        });
    }
    let (accent, panel) = (t.accent, t.panel);
    let mut needle = div().absolute().top(px(-6.)).bottom(px(-6.)).left(pct(now_h)).w(px(0.)).child(
        div().absolute().top_0().bottom_0().left(px(-1.5)).w(px(3.)).rounded_full().bg(accent),
    );
    if !motion::reduced() {
        // pulse-ring: grows from the dot and fades, every 2.2s
        // Drawn from the clock at 30fps rather than on every display frame
        // (a repeating animation redraws the whole window each frame).
        // It only pulses while Billable is the active window; in the
        // background it rests at its end state (faded out).
        let delta = if window.is_window_active() {
            crate::motion::redraw_in(1.0 / 30.0, window, cx);
            (crate::motion::clock() % 2.2) / 2.2
        } else {
            1.0
        };
        let p = 1.0 - (1.0 - delta).powi(2);
        let size = 11.0 * (1.0 + 1.6 * p);
        needle = needle.child(div().absolute().left(px(-size / 2.0)).top(px(0.5 - size / 2.0)).size(px(size)).rounded_full().bg(accent.opacity(0.6 * (1.0 - p))));
    }
    needle = needle.child(div().absolute().top(px(-5.)).left(px(-5.5)).size(px(11.)).rounded_full().bg(accent).border_2().border_color(panel));
    track = track.child(needle);

    let mut ruler = div().relative().h(px(20.)).mt(r(1.5));
    for h in first..=last {
        ruler = ruler.child(
            div()
                .absolute()
                .top_0()
                .left(pct(h as f64))
                .w(px(1.))
                .h(px(if h % 3 == 0 { 6.0 } else { 3.0 }))
                .bg(t.fg.opacity(if h % 3 == 0 { 0.3 } else { 0.2 })),
        );
        if h < last {
            ruler = ruler.child(div().absolute().top_0().left(pct(h as f64 + 0.5)).w(px(1.)).h(px(2.)).bg(t.fg.opacity(0.12)));
        }
        if h % 3 == 0 {
            let label = text(format::hour_label(h), 11.0, 700, t.fg4).line_height(px(12.)).w(px(60.)).whitespace_nowrap().absolute();
            let label = if h == first { label.left_0().text_left() } else if h == last { label.left(px(-60.)).text_right() } else { label.left(px(-30.)).text_center() };
            ruler = ruler.child(div().absolute().top(px(8.)).left(pct(h as f64)).w(px(0.)).child(label));
        }
    }

    div()
        .flex()
        .flex_col()
        .child(group_head(section_title(t, "Today"), summary))
        .child(card(t).px(r(4.)).pt(r(3.5)).pb(r(2.)).child(track).child(ruler))
}

fn attention(t: &Theme, g: &mut Gallery, d: &Data) -> Div {
    let all = &d.billing.attention;
    let total = all.len();
    let mut title = div().flex().items_center().gap(r(2.)).child(section_title(t, "Needs attention"));
    if total > 0 {
        title = title.child(count_badge(t, total));
    }
    let right: AnyElement = if total > 0 { head_link(t, "dash-open-billing", "Open billing", Route::Billing { status: None }).into_any_element() } else { div().into_any_element() };
    let mut body = card(t).overflow_hidden().flex().flex_col();
    let shown: Vec<_> = all.iter().take(4).collect();
    if shown.is_empty() {
        body = body.child(
            div()
                .flex()
                .items_center()
                .gap(r(4.))
                .px(r(4.))
                .py(r(3.))
                .child(Mascot::new("dash-square", Mood::Happy, 52.0))
                .child(text("All square! Nothing is late or waiting to be billed.", 15.0, 700, t.fg)),
        );
    }
    let hover = t.fg.opacity(0.03);
    let (fg4, fg3) = (t.fg4, t.fg3);
    for (i, item) in shown.iter().enumerate() {
        let art = match item.kind.as_str() {
            "unbilled" => Icon::Hourglass,
            "overdue" => Icon::Alert,
            "draft" => Icon::Receipt,
            "accidental_timer" => Icon::Time,
            _ => Icon::Clients,
        };
        let to = match (item.invoice_id, item.project_id) {
            (Some(inv), _) => Route::Invoice(inv),
            (None, Some(p)) => Route::Project(p),
            _ => Route::Billing { status: None },
        };
        let group: SharedString = format!("att-{i}").into();
        let mut row = div()
            .id(ElementId::Name(format!("dash-att-{i}").into()))
            .group(group.clone())
            .relative()
            .flex()
            .items_center()
            .gap(r(3.5))
            .px(r(4.))
            .h(px(62.))
            .cursor_pointer()
            .hover(move |s| s.bg(hover))
            .on_click(move |_, w, cx| navigate(to.clone(), w, cx))
            .child(img(g.get(Art::Icon(art), 29.25)).size(r(9.)).flex_none())
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .child(text(item.title.clone(), 15.0, 700, t.fg).truncate().line_height(px(22.5)))
                    .child(text(item.detail.clone(), 12.5, 600, t.fg3).truncate().line_height(px(18.75))),
            );
        if let Some(amount) = item.amount {
            row = row.child(figures(format::money(amount), 15.0, t.fg));
        }
        row = row.child(
            div()
                .relative()
                .group_hover(group.clone(), move |s| s.left(r(1.)))
                .child(icon_stroke(Lucide::ChevronRight, 3.0).size(r(5.)).text_color(fg4).group_hover(group, move |s| s.text_color(fg3))),
        );
        if i + 1 < shown.len() || total > shown.len() {
            row = row.child(separator(t, 64.0));
        }
        body = body.child(row);
    }
    if total > shown.len() {
        body = body.child(
            div()
                .id("dash-att-more")
                .h(px(44.))
                .px(r(4.))
                .flex()
                .items_center()
                .cursor_pointer()
                .hover(move |s| s.bg(hover))
                .on_click(|_, w, cx| navigate(Route::Billing { status: None }, w, cx))
                .child(text(format!("{} more in Billing", total - shown.len()), 13.0, 600, t.accent_text)),
        );
    }
    div().flex().flex_col().child(group_head(title, right)).child(body)
}

fn recent(t: &Theme, d: &Data, now: chrono::DateTime<Local>) -> Div {
    let entries: Vec<&EntryRow> = d.entries.iter().filter(|e| e.entry.end_time.is_some()).take(6).collect();
    let mut body = card(t).overflow_hidden().flex().flex_col();
    if entries.is_empty() {
        body = body.child(EmptyState::new("dash-recent-empty", "Nothing on the clock yet", "Start a timer from the toolbar, and your time shows up here.").mood(Mood::Sleepy).compact());
    }
    for (i, e) in entries.iter().enumerate() {
        let project = e.project_name.clone().unwrap_or_default();
        let client = e.client_name.clone().unwrap_or_default();
        let (title, sub) = if e.entry.description.is_empty() { (project, client) } else { (e.entry.description.clone(), format!("{project} · {client}")) };
        let mut row = div()
            .relative()
            .flex()
            .items_center()
            .gap(r(3.))
            .px(r(4.))
            .h(px(54.))
            .child(dot(parse_hex(e.project_color.as_deref().unwrap_or("#AFAFAF")), r(3.)))
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .child(text(title, 14.5, 700, t.fg).truncate().line_height(px(21.75)))
                    .child(text(sub, 12.5, 600, t.fg3).truncate().line_height(px(18.75))),
            );
        if let Some(chip) = billing_chip(t, &e.billing_state, e.invoice_number.as_deref()) {
            row = row.child(chip);
        }
        row = row.child(
            div()
                .w(px(76.))
                .flex_none()
                .flex()
                .flex_col()
                .items_end()
                .child(figures(duration_short(e.entry.duration_minutes), 15.0, t.fg).line_height(px(22.5)))
                .child(text(format::relative(&e.entry.start_time, now), 11.5, 600, t.fg4).line_height(px(17.25))),
        );
        if i + 1 < entries.len() {
            row = row.child(separator(t, 40.0));
        }
        body = body.child(row);
    }
    div().flex().flex_col().child(group_head(section_title(t, "Recent time"), head_link(t, "dash-view-all", "View all", Route::Time { action_new: false }))).child(body)
}

fn this_week(t: &Theme, d: &Data, today: NaiveDate) -> Div {
    let monday = today - Days::days(today.weekday().num_days_from_monday() as i64);
    let days: Vec<NaiveDate> = (0..7).map(|i| monday + Days::days(i)).collect();
    let mut by_day: Vec<HashMap<i64, (Hsla, f64)>> = (0..7).map(|_| HashMap::new()).collect();
    for e in &d.entries {
        if e.entry.end_time.is_none() {
            continue;
        }
        let Some(day) = format::local_day(&e.entry.start_time) else { continue };
        let Some(i) = days.iter().position(|x| *x == day) else { continue };
        by_day[i].entry(e.entry.project_id).or_insert((parse_hex(e.project_color.as_deref().unwrap_or("#AFAFAF")), 0.0)).1 += e.entry.duration_minutes;
    }
    let segments: Vec<Vec<(Hsla, f64)>> = by_day
        .into_iter()
        .map(|m| {
            let mut v: Vec<(Hsla, f64)> = m.into_values().collect();
            v.sort_by(|a, b| b.1.total_cmp(&a.1));
            v
        })
        .collect();
    let totals: Vec<f64> = segments.iter().map(|s| s.iter().map(|x| x.1).sum()).collect();
    let max = totals.iter().copied().fold(60.0, f64::max);
    let week_total: f64 = totals.iter().sum();

    let grow = cubic_bezier(0.25, 1.3, 0.5, 1.0);
    let mut bars = div().flex().gap(r(2.)).h(px(108.)).items_end();
    for (i, segs) in segments.iter().enumerate() {
        let total = totals[i];
        let column = div().flex_1().h_full().flex().items_end().justify_center();
        let bar: AnyElement = if total > 0.0 {
            let height = (total / max).max(0.09) as f32;
            let mut stack = div().w_full().h_full().flex().flex_col_reverse().gap(px(3.)).rounded(px(9.)).overflow_hidden();
            for (color, minutes) in segs {
                let mut seg = div().flex_basis(px(0.)).bg(*color);
                seg.style().flex_grow = Some(*minutes as f32);
                stack = stack.child(seg);
            }
            let holder = div().w_full().max_w(px(28.)).flex().items_end().child(stack);
            if motion::reduced() {
                holder.h(relative(height)).into_any_element()
            } else {
                let g2 = grow.clone();
                let delay = 150.0 + i as f32 * 55.0;
                holder
                    .with_animation(("dash-bar", i), Animation::new(Duration::from_millis((delay + 620.0) as u64)), move |el, delta| {
                        el.h(relative(height * g2(after(delta, delay, 620.0)).max(0.0)))
                    })
                    .into_any_element()
            }
        } else {
            div().w_full().max_w(px(28.)).h(px(8.)).rounded_full().bg(t.line).into_any_element()
        };
        bars = bars.child(column.child(bar));
    }
    let mut labels = div().flex().gap(r(2.)).mt(r(2.));
    for day in &days {
        let letter = day.format("%a").to_string().chars().next().unwrap_or(' ').to_string();
        labels = labels.child(text(letter, 12.0, 700, if *day == today { t.accent } else { t.fg4 }).flex_1().text_center().line_height(px(18.)));
    }
    let best = if week_total > 0.0 {
        let (i, m) = totals.iter().enumerate().fold((0, 0.0), |acc, (i, &m)| if m > acc.1 { (i, m) } else { acc });
        format!("{} · {}", days[i].format("%a"), duration_short(m))
    } else {
        "—".into()
    };
    let stat = |label: String, value: String| {
        div().flex_1().flex().flex_col().child(text(label, 12.0, 600, t.fg3).line_height(px(18.))).child(text(value, 14.0, 700, t.fg).line_height(px(21.)))
    };
    div()
        .flex()
        .flex_col()
        .child(group_head(section_title(t, "This week"), figures(duration_short(week_total), 16.0, t.fg3)))
        .child(
            card(t).px(r(4.)).pt(r(4.)).pb(r(3.)).child(bars).child(labels).child(
                div()
                    .flex()
                    .gap(r(3.))
                    .mt(r(3.))
                    .pt(r(3.))
                    .border_t_2()
                    .border_color(t.line)
                    .child(stat("Best day".into(), best))
                    .child(stat(today.format("%B").to_string(), hours_short(d.stats.hours_this_month))),
            ),
        )
}

fn quick_timers(t: &Theme, d: &Data, current_project: Option<i64>, running: bool, paused: bool) -> Div {
    let mut body = card(t).overflow_hidden().flex().flex_col();
    let projects: Vec<&Project> = d.projects.iter().take(5).collect();
    if projects.is_empty() {
        body = body.child(
            div()
                .flex()
                .items_center()
                .gap(r(1.))
                .px(r(4.))
                .py(r(3.))
                .child(text("No active projects.", 14.0, 600, t.fg3))
                .child(
                    div()
                        .id("dash-create-project")
                        .cursor_pointer()
                        .on_click(|_, w, cx| navigate(Route::Projects { action_new: true }, w, cx))
                        .child(text("Create one", 14.0, 600, t.accent_text)),
                ),
        );
    }
    for (i, p) in projects.iter().enumerate() {
        let is_current = (running || paused) && current_project == Some(p.id);
        let pid = p.id;
        let (glyph, tip) = if is_current {
            if paused { (Lucide::Play, "Resume".to_string()) } else { (Lucide::Pause, "Pause".to_string()) }
        } else {
            (Lucide::Play, format!("Start a timer for {}", p.name))
        };
        let accent_hover = t.accent_hover;
        let mut row = div()
            .relative()
            .flex()
            .items_center()
            .gap(r(3.))
            .pl(r(4.))
            .pr(r(3.))
            .h(px(56.))
            .child(dot(parse_hex(&p.color), r(3.)))
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .child(text(p.name.clone(), 14.0, 700, t.fg).truncate().line_height(px(21.)))
                    .child(text(p.client_name.clone().unwrap_or_default(), 12.0, 600, t.fg3).truncate().line_height(px(18.))),
            )
            .child(
                div()
                    .id(ElementId::Name(format!("dash-quick-{pid}").into()))
                    .size(r(9.))
                    .flex_none()
                    .rounded_full()
                    .bg(t.accent)
                    .text_color(crate::theme::hex(0xffffff))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .shadow(vec![shadow(0.0, 3.0, 10.0, -3.0, rgba4(255, 140, 10, 0.6))])
                    // hover:scale-110 can't be drawn (no transforms on elements): a lighter fill instead
                    .hover(move |s| s.bg(accent_hover))
                    .tooltip(move |w, cx| gpui_component::tooltip::Tooltip::new(tip.clone()).build(w, cx))
                    .on_click(move |_, _, cx| {
                        if is_current {
                            if paused { timer::resume(cx) } else { timer::pause(cx) }
                        } else {
                            timer::start(pid, None, cx)
                        }
                    })
                    .child(div().pl(px(if glyph == Lucide::Play { 1.625 } else { 0.0 })).child(icon_filled(glyph).size(r(4.)).text_color(crate::theme::hex(0xffffff)))),
            );
        if is_current {
            row = row.bg(t.accent.opacity(0.1));
        }
        if i + 1 < projects.len() {
            row = row.child(separator(t, 40.0));
        }
        body = body.child(row);
    }
    div().flex().flex_col().child(group_head(section_title(t, "Start a timer"), head_link(t, "dash-projects", "Projects", Route::Projects { action_new: false }))).child(body)
}

#[allow(unused)]
fn unused(_: Option<Button>) -> Option<AnyElement> {
    let _ = money(0.0, 1.0, crate::theme::hex(0));
    None
}
