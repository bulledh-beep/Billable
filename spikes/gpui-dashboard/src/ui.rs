//! The window: sidebar, toolbar and the Dashboard page. Each piece follows the
//! React component it copies (Sidebar.tsx, App.tsx's toolbar, Dashboard.tsx,
//! Metric.tsx, TodayRuler.tsx ...) with the same sizes from index.css.

use std::{collections::HashMap, rc::Rc, sync::Arc, time::Duration};

use chrono::{Datelike, Duration as Days, Local, NaiveDate};
use gpui::{
    div, img, prelude::*, px, relative, AnyElement, Animation, AnimationExt as _, Context, Div, ElementId, Font,
    FontFeatures, FontStyle, FontWeight, Hsla, Image, MouseButton, ScrollHandle, SharedString, Window, WindowAppearance,
};

use crate::{
    art::{Art, Gallery, Glyph, Icon, Mood},
    data::{self, AttentionKind, BillingState, Dashboard},
    fonts::rounded_family,
    format::{self, duration_short, hours_short, money, money_parts},
    theme::{css_hex, hex, mix, Theme},
};

pub struct BillableWindow {
    data: Result<Rc<Dashboard>, String>,
    gallery: Gallery,
    scroll: ScrollHandle,
}

impl BillableWindow {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.observe_window_appearance(window, |_, _, cx| cx.notify()).detach();
        // The app refreshes on its own events; here, reread every 30 seconds
        // (which also moves the "now" needle, like TodayRuler's interval).
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(Duration::from_secs(30)).await;
            if this.update(cx, |this, cx| {
                this.data = data::load().map(Rc::new).map_err(|e| format!("{e:#}"));
                cx.notify();
            })
            .is_err()
            {
                break;
            }
        })
        .detach();
        BillableWindow { data: data::load().map(Rc::new).map_err(|e| format!("{e:#}")), gallery: Gallery::default(), scroll: ScrollHandle::new() }
    }
}

// ------------------------------------------------------------------ helpers

/// Tailwind spacing units (`px-4`, `gap-3`, `w-9`) are 0.25rem, and Billable
/// sets the root font size to 13px, so one unit is 3.25px, not 4px.
fn r(units: f32) -> gpui::Pixels {
    px(units * 3.25)
}

/// CSS `cubic-bezier()`.
fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32) -> impl Fn(f32) -> f32 + Clone {
    move |x: f32| {
        let bx = |t: f32| 3.0 * (1.0 - t).powi(2) * t * x1 + 3.0 * (1.0 - t) * t * t * x2 + t * t * t;
        let by = |t: f32| 3.0 * (1.0 - t).powi(2) * t * y1 + 3.0 * (1.0 - t) * t * t * y2 + t * t * t;
        let (mut lo, mut hi) = (0.0f32, 1.0f32);
        for _ in 0..24 {
            let mid = (lo + hi) / 2.0;
            if bx(mid) < x { lo = mid } else { hi = mid }
        }
        by((lo + hi) / 2.0)
    }
}

/// BILLABLE_STILL=1 turns off the animations that loop forever (blink, bob,
/// the needle's pulse), to measure what they cost.
fn looping() -> bool {
    std::env::var_os("BILLABLE_STILL").is_none()
}

/// Progress of an animation that waits `delay` ms, then runs `dur` ms.
fn after(delta: f32, delay: f32, dur: f32) -> f32 {
    ((delta * (delay + dur) - delay) / dur).clamp(0.0, 1.0)
}

fn text(s: impl Into<SharedString>, size: f32, weight: u16, color: Hsla) -> Div {
    div().font_family(rounded_family(weight)).text_size(px(size)).text_color(color).child(s.into())
}

/// `.font-figures`: bold rounded numerals, all the same width.
fn figures_font() -> Font {
    Font {
        family: rounded_family(700).into(),
        features: FontFeatures(Arc::new(vec![("tnum".into(), 1)])),
        fallbacks: None,
        weight: FontWeight::BOLD,
        style: FontStyle::Normal,
    }
}

fn figures(s: impl Into<SharedString>, size: f32, color: Hsla) -> Div {
    div().font(figures_font()).text_size(px(size)).text_color(color).child(s.into())
}

fn picture(image: Arc<Image>, size: f32) -> gpui::Img {
    img(image).size(px(size)).flex_none()
}

fn card(t: &Theme) -> Div {
    div().bg(t.panel).border_1().border_color(t.line).rounded(px(16.)).shadow(t.shadow_card())
}

/// `.list-row`: the separator is set in from the left, and the last row has none.
fn separator(t: &Theme, inset: f32) -> Div {
    div().absolute().bottom_0().right_0().left(px(inset)).h(px(1.)).bg(t.line)
}

fn badge_count(t: &Theme, n: usize) -> Div {
    div()
        .flex()
        .items_center()
        .justify_center()
        .min_w(px(20.))
        .h(px(20.))
        .px(r(1.5))
        .rounded_full()
        .bg(t.red)
        .child(text(n.to_string(), 11.5, 700, hex(0xffffff)).line_height(px(20.)))
}

fn group_head(left: impl IntoElement, right: impl IntoElement) -> Div {
    div().flex().items_center().justify_between().gap(r(3.)).min_h(px(26.)).mb(r(2.5)).px(r(0.5)).child(left).child(right)
}

fn section_title(t: &Theme, s: &str) -> Div {
    text(s.to_string(), 16.0, 700, t.fg)
}

fn head_link(t: &Theme, g: &mut Gallery, label: &str) -> Div {
    div()
        .flex()
        .items_center()
        .gap(r(0.5))
        .cursor_pointer()
        .child(text(label.to_string(), 13.0, 600, t.accent_text))
        .child(picture(g.get(Art::Glyph(Glyph::ChevronRight, css_hex(t.accent_text)), 11.375), 11.375))
}

/// `<Money>`: dollars first, the cents set back. With `animate` it counts up.
fn money_figure(id: impl Into<ElementId>, amount: f64, size: f32, color: Hsla, animate: bool) -> AnyElement {
    let row = move |value: f64| {
        let (whole, cents) = money_parts(value);
        div()
            .flex()
            .whitespace_nowrap()
            .child(figures(whole, size, color))
            .child(figures(cents, size, color.opacity(0.45)))
    };
    if !animate {
        return row(amount).into_any_element();
    }
    div()
        .with_animation(id, Animation::new(Duration::from_millis(700)), move |d, delta| {
            let eased = 1.0 - (1.0 - delta).powi(3);
            d.child(row(amount * eased as f64))
        })
        .into_any_element()
}

/// `.page > *` rise in one after another.
fn rise_in(id: &'static str, index: usize, el: impl IntoElement) -> AnyElement {
    let delay = (index as f32 * 40.0).min(240.0);
    let ease = cubic_bezier(0.2, 0.8, 0.2, 1.0);
    div()
        .relative()
        .child(el)
        .with_animation(id, Animation::new(Duration::from_millis((delay + 420.0) as u64)), move |d, delta| {
            let p = ease(after(delta, delay, 420.0));
            d.opacity(p).top(px(10.0 * (1.0 - p)))
        })
        .into_any_element()
}

fn primary_button(t: &Theme, g: &mut Gallery, label: &str, icon: Option<Glyph>) -> Div {
    let accent_hover = t.accent_hover;
    let mut b = div()
        .flex()
        .items_center()
        .justify_center()
        .gap(r(1.5))
        .h(px(32.))
        .px(r(4.))
        .rounded_full()
        .bg(t.accent)
        .shadow(t.shadow_primary())
        .cursor_pointer()
        .hover(move |s| s.bg(accent_hover))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
    if let Some(icon) = icon {
        b = b.child(picture(g.get(Art::Glyph(icon, "#ffffff".into()), 15.0), 15.0));
    }
    b.child(text(label.to_string(), 13.0, 600, hex(0xffffff)).whitespace_nowrap())
}

fn secondary_button(t: &Theme, label: &str) -> Div {
    let hover = t.fg.opacity(0.1);
    div()
        .flex()
        .items_center()
        .h(px(32.))
        .px(r(4.))
        .rounded_full()
        .bg(t.fg.opacity(0.06))
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .child(text(label.to_string(), 13.0, 600, t.fg).whitespace_nowrap())
}

/// The mascot, blinking every 4.6s and bobbing (or hopping twice when happy).
fn mascot(g: &mut Gallery, id: &'static str, mood: Mood, size: f32, moving: bool) -> AnyElement {
    let moving = moving && looping();
    let open = g.get(Art::Mascot { mood, blink: false }, size);
    if !looping() {
        return picture(open, size).into_any_element();
    }
    let shut = g.get(Art::Mascot { mood, blink: true }, size);
    let face = div().size(px(size)).flex_none().with_animation(
        SharedString::from(format!("{id}-blink")),
        Animation::new(Duration::from_millis(4600)).repeat(),
        move |d, delta| d.child(picture(if (0.925..0.975).contains(&delta) { shut.clone() } else { open.clone() }, size)),
    );
    if !moving {
        return face.into_any_element();
    }
    let holder = div().relative().flex_none().child(face);
    if mood == Mood::Happy {
        // mascot-hop, twice
        holder
            .with_animation(SharedString::from(format!("{id}-hop")), Animation::new(Duration::from_millis(1800)), |d, delta| {
                let p = (delta * 2.0).fract();
                let y = if p < 0.3 { -10.0 * p / 0.3 } else if p < 0.55 { -10.0 + 10.0 * (p - 0.3) / 0.25 } else if p < 0.7 { -3.0 * (p - 0.55) / 0.15 } else { -3.0 + 3.0 * (p - 0.7) / 0.3 };
                d.top(px(if delta >= 1.0 { 0.0 } else { y }))
            })
            .into_any_element()
    } else {
        // mascot-bob: up 3px and back, ease-in-out, every 3.2s
        holder
            .with_animation(
                SharedString::from(format!("{id}-bob")),
                Animation::new(Duration::from_millis(3200)).repeat(),
                |d, delta| d.top(px(-3.0 * (0.5 - 0.5 * (delta * std::f32::consts::TAU).cos()))),
            )
            .into_any_element()
    }
}

fn parse_color(s: &str) -> Hsla {
    u32::from_str_radix(s.trim_start_matches('#'), 16).map(hex).unwrap_or(hex(0xAFAFAF))
}

// ------------------------------------------------------------------ render

impl Render for BillableWindow {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let t = match window.appearance() {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => Theme::dark(),
            _ => Theme::light(),
        };
        let root = div()
            .size_full()
            .flex()
            .bg(t.bg)
            .text_color(t.fg)
            .font_family(rounded_family(500))
            .text_size(px(13.))
            .line_height(relative(1.5));

        let data = match &self.data {
            Ok(d) => Rc::clone(d),
            Err(e) => {
                return root
                    .items_center()
                    .justify_center()
                    .child(text(format!("Couldn't read Billable's data: {e}"), 14.0, 600, t.fg3));
            }
        };
        let g = &mut self.gallery;
        let scrollbar = scrollbar(&t, &self.scroll);
        root.child(sidebar(&t, g, &data)).child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .bg(t.bg)
                .child(toolbar(&t, g))
                .child(
                    div()
                        .relative()
                        .flex_1()
                        .min_h_0()
                        .child(
                            // The page gives up 11px on the right to its scrollbar, as in Chromium.
                            div()
                                .id("page-scroll")
                                .absolute()
                                .top_0()
                                .bottom_0()
                                .left_0()
                                .right(px(11.))
                                .overflow_y_scroll()
                                .track_scroll(&self.scroll)
                                .child(dashboard(&t, g, &data)),
                        )
                        .child(scrollbar),
                ),
        )
    }
}

/// index.css's ::-webkit-scrollbar: an 11px gutter with a 5px pill
/// (an 11px thumb inside a 3px transparent border), fg at 14%.
fn scrollbar(t: &Theme, scroll: &ScrollHandle) -> Div {
    let track = div().absolute().top_0().bottom_0().right_0().w(px(11.));
    let view = scroll.bounds().size.height;
    let max = scroll.max_offset().height;
    if max <= px(0.) || view <= px(0.) {
        return track;
    }
    let content = view + max;
    let thumb = (view * (view / content)).max(px(20.));
    let top = (view - thumb) * (-scroll.offset().y / max).clamp(0.0, 1.0);
    track.child(div().absolute().left(px(3.)).w(px(5.)).top(top + px(3.)).h(thumb - px(6.)).rounded_full().bg(t.fg.opacity(0.14)))
}

fn sidebar(t: &Theme, g: &mut Gallery, data: &Dashboard) -> Div {
    let billing_badge = data.attention.iter().filter(|a| a.kind != AttentionKind::DuplicateClients).count();
    let link = |g: &mut Gallery, icon: Icon, label: &str, active: bool, badge: usize| {
        let hover = t.fg.opacity(0.05);
        let mut row = div()
            .relative()
            .flex()
            .items_center()
            .gap(r(2.5))
            .h(px(38.))
            .px(r(2.5))
            .rounded(px(11.))
            .cursor_pointer()
            .child(picture(g.get(Art::Icon(icon), 22.0), 22.0))
            .child(text(label.to_string(), 14.0, if active { 600 } else { 500 }, if active { t.fg } else { t.fg2 }).flex_1().truncate());
        row = if active { row.bg(t.accent.opacity(0.12)) } else { row.hover(move |s| s.bg(hover)) };
        if badge > 0 {
            row = row.child(badge_count(t, badge));
        }
        row
    };

    let switch = {
        let thumb_shadow = if t.dark {
            vec![crate::theme::shadow(0.0, 1.0, 3.0, 0.0, gpui::Rgba { r: 0.0, g: 0.0, b: 0.0, a: 0.45 })]
        } else {
            vec![
                crate::theme::shadow(0.0, 1.0, 2.0, 0.0, gpui::Rgba { r: 20. / 255., g: 20. / 255., b: 40. / 255., a: 0.14 }),
                crate::theme::shadow(0.0, 3.0, 10.0, -4.0, gpui::Rgba { r: 20. / 255., g: 20. / 255., b: 40. / 255., a: 0.2 }),
            ]
        };
        div()
            .flex()
            .items_center()
            .p(px(3.))
            .rounded_full()
            .bg(t.fg.opacity(0.055))
            .border_1()
            .border_color(t.fg.opacity(0.04))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(r(1.5))
                    .h(px(26.))
                    .pl(px(4.))
                    .pr(px(10.))
                    .rounded_full()
                    .bg(if t.dark { t.panel2 } else { t.panel })
                    .shadow(thumb_shadow)
                    .child(picture(g.get(Art::BillableMark, 18.0), 18.0))
                    .child(text("Billable", 12.0, 600, t.fg).line_height(px(16.))),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(px(26.))
                    .px(px(7.))
                    .opacity(if t.dark { 0.7 } else { 0.5 })
                    .hover(|s| s.opacity(1.0))
                    .cursor_pointer()
                    .child(picture(g.get(Art::ContentMark, 18.0), 18.0)),
            )
    };

    let profile_color = parse_color(&data.profile.color);
    let initial = data.profile.name.trim().chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or("?".into());
    let hover = t.fg.opacity(0.05);

    div()
        .w(px(228.))
        .flex_none()
        .flex()
        .flex_col()
        .h_full()
        .bg(t.sidebar)
        .border_r_1()
        .border_color(t.line)
        .child(
            div()
                .h(px(48.))
                .flex_none()
                .flex()
                .items_center()
                .pl(px(78.))
                .on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move())
                .child(switch),
        )
        .child(
            div().px(r(5.)).pb(r(3.)).flex_none().child(
                // The wordmark is set with -0.03em tracking in the app; GPUI has no letter-spacing.
                text("billable", 24.0, 700, t.accent).line_height(px(24.)),
            ),
        )
        .child(
            div()
                .flex_1()
                .px(r(3.))
                .pb(r(2.))
                .flex()
                .flex_col()
                .gap(r(0.5))
                .child(link(g, Icon::Home, "Dashboard", true, 0))
                .child(link(g, Icon::Clients, "Clients", false, 0))
                .child(link(g, Icon::Projects, "Projects", false, 0))
                .child(link(g, Icon::Time, "Time", false, 0))
                .child(link(g, Icon::Billing, "Billing", false, billing_badge))
                .child(link(g, Icon::Reports, "Reports", false, 0))
                .child(text("Business", 12.0, 600, t.fg4).px(r(2.5)).mt(r(5.)).mb(r(1.)).line_height(px(18.)))
                .child(link(g, Icon::Commissions, "Commissions", false, 0))
                .child(link(g, Icon::Tax, "Tax overview", false, 0))
                .child(link(g, Icon::Receipt, "Tax settings", false, 0)),
        )
        .child(
            div().px(r(3.)).pt(r(1.)).flex_none().child(
                link(g, Icon::Settings, "Settings", false, 0).child(
                    div()
                        .flex()
                        .items_center()
                        .h(px(18.))
                        .px(r(1.5))
                        .rounded(px(6.))
                        .bg(t.fg.opacity(0.07))
                        .child(text("⌘,", 11.0, 600, t.fg3).line_height(px(18.))),
                ),
            ),
        )
        .child(
            div().pt(r(1.)).pb(r(3.)).px(r(2.5)).flex_none().child(
                div()
                    .flex()
                    .items_center()
                    .gap(r(2.))
                    .h(px(30.))
                    .px(r(2.))
                    .rounded(px(6.))
                    .cursor_pointer()
                    .hover(move |s| s.bg(hover))
                    .child(
                        div()
                            .size(px(18.))
                            .flex_none()
                            .rounded_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(profile_color)
                            .child(text(initial, 9.0, 600, hex(0x1a1a1a)).line_height(px(18.))),
                    )
                    .child(text(data.profile.name.clone(), 13.0, 500, t.fg.opacity(0.85)).flex_1().truncate())
                    .child(picture(g.get(Art::Glyph(Glyph::ChevronsUpDown, css_hex(t.fg4)), 11.375), 11.375)),
            ),
        )
}

fn toolbar(t: &Theme, g: &mut Gallery) -> Div {
    let date_line = Local::now().format("%A, %B %-d").to_string();
    div()
        .h(px(58.))
        .flex_none()
        .flex()
        .items_center()
        .gap(r(3.))
        .pl(r(7.))
        .pr(r(4.))
        .border_b_1()
        .border_color(t.line)
        .bg(t.bg)
        .on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move())
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                // Title: 20/24 bold, -0.015em tracking in the app
                .child(text("Dashboard", 20.0, 700, t.fg).line_height(px(24.)))
                .child(text(date_line, 12.0, 600, t.fg3).line_height(px(18.)).truncate()),
        )
        .child(primary_button(t, g, "Start timer", Some(Glyph::Play)))
}

struct Segment {
    color: Hsla,
    minutes: f64,
}

fn dashboard(t: &Theme, g: &mut Gallery, d: &Dashboard) -> Div {
    let now = Local::now();
    let today = now.date_naive();
    let p = &d.pipeline;

    // Today's hours, with a running timer counted in
    let running = d.active.as_ref().filter(|e| e.paused_at.is_none());
    let running_today = d
        .active
        .as_ref()
        .filter(|e| format::local_day(&e.start_time) == Some(today))
        .map(|e| data::elapsed_seconds(e) / 3600.0)
        .unwrap_or(0.0);
    let today_hours = d.hours_today + running_today;

    // What the mascot says
    let (mood, message, action): (Mood, String, Option<(&str, bool)>) = if p.overdue_count > 0 {
        (Mood::Worried, format!("{} is overdue. A friendly nudge usually does the trick.", money(p.overdue_amount)), Some(("See overdue", false)))
    } else if let Some(e) = running {
        (Mood::Happy, format!("You're on the clock for {}. Nice focus!", e.project_name), None)
    } else if p.unbilled_amount > 0.0 && d.attention.iter().any(|a| a.kind == AttentionKind::Unbilled) {
        (Mood::Wave, format!("{} is ready to bill whenever you are.", money(p.unbilled_amount)), Some(("Bill it", true)))
    } else if today_hours == 0.0 {
        (Mood::Idle, "Fresh day, clean slate. Start a timer when you're ready.".into(), None)
    } else {
        (Mood::Happy, format!("Nice work! {} tracked so far today.", hours_short(today_hours)), None)
    };

    let mut talk = card(t)
        .flex()
        .items_center()
        .gap(r(4.))
        .pl(r(3.))
        .pr(r(4.))
        .py(r(2.5))
        .child(mascot(g, "mascot-main", mood, 58.0, true))
        .child(text(message, 16.0, 600, t.fg).line_height(px(22.)).flex_1().min_w_0());
    if let Some((label, primary)) = action {
        talk = talk.child(if primary { primary_button(t, g, label, None) } else { secondary_button(t, label) });
    }

    // ---- Stat cards ----
    let metric = |g: &mut Gallery, id: &'static str, label: String, icon: Icon, tint: u32, amount: f64, sub: String, value_color: Hsla| {
        let tint = hex(tint);
        div()
            .relative()
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .gap(r(3.))
            .px(r(4.))
            .py(r(3.5))
            .rounded(px(16.))
            .border_1()
            .bg(mix(tint, t.panel, 0.09))
            .border_color(mix(tint, t.line, 0.22))
            .shadow(t.shadow_card())
            .cursor_pointer()
            .hover(|s| s.top(px(-2.)))
            .child(picture(g.get(Art::Icon(icon), 29.25), 29.25))
            .child(
                div()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(div().h(px(27.)).flex().items_center().child(money_figure(id, amount, 22.0, value_color, true)))
                    .child(text(label, 13.0, 600, t.fg2).truncate())
                    .child(text(sub, 12.0, 500, t.fg3).truncate()),
            )
    };
    let plural = |n: usize| if n == 1 { "" } else { "s" };
    let metrics = div()
        .flex()
        .gap(r(3.))
        .child(metric(
            g,
            "m-unbilled",
            "Ready to bill".into(),
            Icon::Hourglass,
            0xFF9600,
            p.unbilled_amount,
            if p.unbilled_hours > 0.0 { format!("{} not on an invoice", hours_short(p.unbilled_hours)) } else { "All caught up".into() },
            t.fg,
        ))
        .child(metric(
            g,
            "m-awaiting",
            "Awaiting payment".into(),
            Icon::Send,
            0x1CB0F6,
            p.awaiting_amount + p.draft_amount,
            if p.draft_count > 0 {
                format!("{} sent · {} draft", p.awaiting_count, p.draft_count)
            } else {
                format!("{} invoice{} sent", p.awaiting_count, plural(p.awaiting_count))
            },
            t.fg,
        ))
        .child(metric(
            g,
            "m-overdue",
            "Overdue".into(),
            Icon::Alert,
            0xFF4B4B,
            p.overdue_amount,
            if p.overdue_count > 0 { format!("{} past due", p.overdue_count) } else { "Nothing late".into() },
            if p.overdue_amount > 0.0 { t.red } else { t.fg },
        ))
        .child(metric(
            g,
            "m-paid",
            format!("Paid in {}", today.year()),
            Icon::Trophy,
            0x58CC02,
            p.paid_ytd_amount,
            format!("{} invoice{}", p.paid_ytd_count, plural(p.paid_ytd_count)),
            t.fg,
        ));

    div()
        .w_full()
        .max_w(px(1160.))
        .mx_auto()
        .px(r(7.))
        .pt(r(6.))
        .pb(r(14.))
        .flex()
        .flex_col()
        .child(rise_in("rise-0", 0, talk.mb(r(5.))))
        .child(rise_in("rise-1", 1, metrics.mb(r(6.))))
        .child(rise_in("rise-2", 2, today_ruler(t, d, now).mb(r(7.))))
        .child(rise_in(
            "rise-3",
            3,
            div()
                .flex()
                .gap(r(6.))
                .items_start()
                .child(div().flex_1().min_w_0().flex().flex_col().gap(r(7.)).child(attention(t, g, d)).child(recent(t, g, d, now)))
                .child(div().w(px(310.)).flex_none().flex().flex_col().gap(r(7.)).child(this_week(t, d, today)).child(quick_timers(t, g, d))),
        ))
}

fn today_ruler(t: &Theme, d: &Dashboard, now: chrono::DateTime<Local>) -> Div {
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
        let Some(end) = e.end_time.as_deref() else { continue };
        if format::local_day(&e.start_time) != Some(today) {
            continue;
        }
        let start = hours_at(&e.start_time);
        let stop = hours_at(end);
        blocks.push(Block { start, end: if stop > start { stop } else { (start + e.duration_minutes / 60.0).min(24.0) }, color: parse_color(&e.project_color), running: false });
        minutes += e.duration_minutes;
        if e.is_billable {
            earned += e.duration_minutes / 60.0 * e.rate;
        }
    }
    if let Some(a) = d.active.as_ref().filter(|a| format::local_day(&a.start_time) == Some(today)) {
        let start = hours_at(&a.start_time);
        let until = a.paused_at.as_deref().map(hours_at).unwrap_or(now_h);
        let secs = data::elapsed_seconds(a);
        blocks.push(Block { start, end: until.max(start + 1.0 / 60.0), color: parse_color(&a.project_color), running: a.paused_at.is_none() });
        minutes += secs / 60.0;
        if a.is_billable {
            earned += secs / 3600.0 * a.rate;
        }
    }

    let first = blocks.iter().map(|b| b.start.floor() as i64).fold(8, i64::min);
    let last = blocks.iter().map(|b| b.end.ceil() as i64).fold(20.max((now_h + 0.25).ceil() as i64), i64::max);
    let span = (last - first) as f64;
    let pct = |h: f64| relative(((h - first as f64) / span) as f32);

    let summary: AnyElement = if minutes > 0.0 {
        let mut row = div().flex().items_baseline().gap(px(4.)).child(figures(duration_short(minutes), 15.0, t.fg));
        if earned > 0.0 {
            row = row
                .child(text("·", 13.0, 700, t.fg3))
                .child(figures(format!("+{}", money(earned)), 15.0, t.green))
                .child(text("earned", 13.0, 700, t.fg3));
        }
        row.into_any_element()
    } else {
        text("Nothing on the clock yet", 13.0, 700, t.fg3).into_any_element()
    };

    let mut track = div().relative().h(px(30.)).rounded(px(11.)).bg(t.line.opacity(0.7));
    for b in &blocks {
        track = track.child(
            div()
                .absolute()
                .top(px(4.))
                .bottom(px(4.))
                .left(pct(b.start))
                .w(relative(((b.end - b.start) / span) as f32))
                .min_w(px(3.))
                .rounded(px(8.))
                .bg(b.color.opacity(if b.running { 0.85 } else { 1.0 })),
        );
    }
    let (accent, panel) = (t.accent, t.panel);
    track = track.child(
        div()
            .absolute()
            .top(px(-6.))
            .bottom(px(-6.))
            .left(pct(now_h))
            .w(px(0.))
            .child(div().absolute().top_0().bottom_0().left(px(-1.5)).w(px(3.)).rounded_full().bg(accent))
            .children(looping().then(|| {
                // pulse-ring: grows from the dot and fades, every 2.2s
                div().absolute().with_animation("needle-pulse", Animation::new(Duration::from_millis(2200)).repeat(), move |el, delta| {
                    let p = 1.0 - (1.0 - delta).powi(2);
                    let size = 11.0 * (1.0 + 1.6 * p);
                    el.left(px(-size / 2.0)).top(px(0.5 - size / 2.0)).size(px(size)).rounded_full().bg(accent.opacity(0.6 * (1.0 - p)))
                })
            }))
            .child(div().absolute().top(px(-5.)).left(px(-5.5)).size(px(11.)).rounded_full().bg(accent).border_2().border_color(panel)),
    );

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
            let label = text(format::hour_label(h), 11.0, 700, t.fg4).line_height(px(12.)).w(px(60.)).whitespace_nowrap();
            let label = if h == first { label.left_0().text_left() } else if h == last { label.left(px(-60.)).text_right() } else { label.left(px(-30.)).text_center() };
            ruler = ruler.child(div().absolute().top(px(8.)).left(pct(h as f64)).w(px(0.)).child(label.absolute()));
        }
    }

    div()
        .flex()
        .flex_col()
        .child(group_head(section_title(t, "Today"), summary))
        .child(card(t).px(r(4.)).pt(r(3.5)).pb(r(2.)).child(track).child(ruler))
}

fn attention(t: &Theme, g: &mut Gallery, d: &Dashboard) -> Div {
    let total = d.attention.len();
    let mut title = div().flex().items_center().gap(r(2.)).child(section_title(t, "Needs attention"));
    if total > 0 {
        title = title.child(badge_count(t, total));
    }
    let mut head_right = div();
    if total > 0 {
        head_right = head_link(t, g, "Open billing");
    }
    let mut body = card(t).overflow_hidden().flex().flex_col();
    let shown: Vec<_> = d.attention.iter().take(4).collect();
    if shown.is_empty() {
        body = body.child(
            div()
                .flex()
                .items_center()
                .gap(r(4.))
                .px(r(4.))
                .py(r(3.))
                .child(mascot(g, "mascot-square", Mood::Happy, 52.0, false))
                .child(text("All square! Nothing is late or waiting to be billed.", 15.0, 700, t.fg)),
        );
    }
    let hover = t.fg.opacity(0.03);
    let chevron = g.get(Art::Glyph(Glyph::ChevronRightBold, css_hex(t.fg4)), 16.25);
    for (i, item) in shown.iter().enumerate() {
        let icon = match item.kind {
            AttentionKind::Unbilled => Icon::Hourglass,
            AttentionKind::Overdue => Icon::Alert,
            AttentionKind::Draft => Icon::Receipt,
            AttentionKind::AccidentalTimer => Icon::Time,
            AttentionKind::DuplicateClients => Icon::Clients,
        };
        let group = SharedString::from(format!("att-{i}"));
        let mut row = div()
            .group(group.clone())
            .relative()
            .flex()
            .items_center()
            .gap(r(3.5))
            .px(r(4.))
            .h(px(62.))
            .cursor_pointer()
            .hover(move |s| s.bg(hover))
            .child(picture(g.get(Art::Icon(icon), 29.25), 29.25))
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .child(text(item.title.clone(), 15.0, 700, t.fg).truncate())
                    .child(text(item.detail.clone(), 12.5, 600, t.fg3).truncate()),
            );
        if let Some(amount) = item.amount {
            row = row.child(figures(money(amount), 15.0, t.fg));
        }
        row = row.child(div().relative().group_hover(group, |s| s.left(r(1.))).child(picture(chevron.clone(), 16.25)));
        if i + 1 < shown.len() || total > shown.len() {
            row = row.child(separator(t, 64.0));
        }
        body = body.child(row);
    }
    if total > shown.len() {
        body = body.child(
            div()
                .h(px(44.))
                .px(r(4.))
                .flex()
                .items_center()
                .hover(move |s| s.bg(hover))
                .child(text(format!("{} more in Billing", total - shown.len()), 13.0, 600, t.accent_text)),
        );
    }
    div().flex().flex_col().child(group_head(title, head_right)).child(body)
}

fn billing_chip(t: &Theme, state: BillingState, invoice: Option<&str>) -> Option<Div> {
    let (label, color) = match state {
        BillingState::Unbilled => return None,
        BillingState::NonBillable => return Some(text("Not billable", 12.0, 500, t.fg4).whitespace_nowrap()),
        BillingState::Draft => ("Draft", t.fg3),
        BillingState::Sent => ("Sent", t.blue),
        BillingState::Overdue => ("Overdue", t.red),
        BillingState::Paid => ("Paid", t.green),
        BillingState::Invoiced => ("Invoiced", t.blue),
    };
    let mut chip = div().flex().whitespace_nowrap().child(text(label, 12.0, 500, color));
    if let Some(n) = invoice {
        chip = chip.child(text(format!(" · {n}"), 12.0, 500, t.fg3));
    }
    Some(chip)
}

fn recent(t: &Theme, g: &mut Gallery, d: &Dashboard, now: chrono::DateTime<Local>) -> Div {
    let entries: Vec<_> = d.entries.iter().filter(|e| e.end_time.is_some()).take(6).collect();
    let mut body = card(t).overflow_hidden().flex().flex_col();
    if entries.is_empty() {
        body = body.child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .py(px(20.))
                .child(mascot(g, "mascot-sleepy", Mood::Sleepy, 64.0, false))
                .child(text("Nothing on the clock yet", 15.0, 700, t.fg).mt(px(6.)))
                .child(text("Start a timer from the toolbar, and your time shows up here.", 13.0, 500, t.fg3)),
        );
    }
    for (i, e) in entries.iter().enumerate() {
        let (title, sub) = if e.description.is_empty() {
            (e.project_name.clone(), e.client_name.clone())
        } else {
            (e.description.clone(), format!("{} · {}", e.project_name, e.client_name))
        };
        let mut row = div()
            .relative()
            .flex()
            .items_center()
            .gap(r(3.))
            .px(r(4.))
            .h(px(54.))
            .child(div().size(r(3.)).flex_none().rounded_full().bg(parse_color(&e.project_color)))
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .child(text(title, 14.5, 700, t.fg).truncate())
                    .child(text(sub, 12.5, 600, t.fg3).truncate()),
            );
        if let Some(chip) = billing_chip(t, e.billing_state, e.invoice_number.as_deref()) {
            row = row.child(chip);
        }
        row = row.child(
            div()
                .w(px(76.))
                .flex_none()
                .flex()
                .flex_col()
                .items_end()
                .child(figures(duration_short(e.duration_minutes), 15.0, t.fg))
                .child(text(format::relative(&e.start_time, now), 11.5, 600, t.fg4)),
        );
        if i + 1 < entries.len() {
            row = row.child(separator(t, 40.0));
        }
        body = body.child(row);
    }
    div().flex().flex_col().child(group_head(section_title(t, "Recent time"), head_link(t, g, "View all"))).child(body)
}

fn this_week(t: &Theme, d: &Dashboard, today: NaiveDate) -> Div {
    let monday = today - Days::days(today.weekday().num_days_from_monday() as i64);
    let days: Vec<NaiveDate> = (0..7).map(|i| monday + Days::days(i)).collect();
    let mut by_day: Vec<HashMap<i64, Segment>> = (0..7).map(|_| HashMap::new()).collect();
    for e in &d.entries {
        if e.end_time.is_none() {
            continue;
        }
        let Some(day) = format::local_day(&e.start_time) else { continue };
        let Some(i) = days.iter().position(|d| *d == day) else { continue };
        by_day[i].entry(e.project_id).or_insert(Segment { color: parse_color(&e.project_color), minutes: 0.0 }).minutes += e.duration_minutes;
    }
    let segments: Vec<Vec<Segment>> = by_day
        .into_iter()
        .map(|m| {
            let mut v: Vec<Segment> = m.into_values().collect();
            v.sort_by(|a, b| b.minutes.total_cmp(&a.minutes));
            v
        })
        .collect();
    let totals: Vec<f64> = segments.iter().map(|s| s.iter().map(|x| x.minutes).sum()).collect();
    let max = totals.iter().copied().fold(60.0, f64::max);
    let week_total: f64 = totals.iter().sum();

    let grow = cubic_bezier(0.25, 1.3, 0.5, 1.0);
    let mut bars = div().flex().gap(r(2.)).h(px(108.)).items_end();
    for (i, segs) in segments.iter().enumerate() {
        let total = totals[i];
        let column = div().flex_1().h_full().flex().items_end().justify_center();
        let bar: AnyElement = if total > 0.0 {
            let height = (total / max).max(0.09) as f32;
            let mut stack = div().w_full().max_w(px(28.)).h_full().flex().flex_col_reverse().gap(px(3.)).rounded(px(9.)).overflow_hidden();
            for s in segs {
                let mut seg = div().flex_basis(px(0.)).bg(s.color);
                seg.style().flex_grow = Some(s.minutes as f32);
                stack = stack.child(seg);
            }
            let grow = grow.clone();
            let delay = 150.0 + i as f32 * 55.0;
            div()
                .w_full()
                .max_w(px(28.))
                .flex()
                .items_end()
                .child(stack)
                .with_animation(("bar", i), Animation::new(Duration::from_millis((delay + 620.0) as u64)), move |el, delta| {
                    el.h(relative(height * grow(after(delta, delay, 620.0)).max(0.0)))
                })
                .into_any_element()
        } else {
            div().w_full().max_w(px(28.)).h(px(8.)).rounded_full().bg(t.line).into_any_element()
        };
        bars = bars.child(column.child(bar));
    }

    let mut labels = div().flex().gap(r(2.)).mt(r(2.));
    for day in &days {
        let letter = day.format("%a").to_string().chars().next().unwrap_or(' ').to_string();
        labels = labels.child(text(letter, 12.0, 700, if *day == today { t.accent } else { t.fg4 }).flex_1().text_center());
    }

    let best = if week_total > 0.0 {
        let (i, m) = totals.iter().enumerate().fold((0, 0.0), |acc, (i, &m)| if m > acc.1 { (i, m) } else { acc });
        format!("{} · {}", days[i].format("%a"), duration_short(m))
    } else {
        "—".into()
    };
    let stat = |label: String, value: String| div().flex_1().flex().flex_col().child(text(label, 12.0, 600, t.fg3)).child(text(value, 14.0, 700, t.fg));

    div()
        .flex()
        .flex_col()
        .child(group_head(section_title(t, "This week"), figures(duration_short(week_total), 16.0, t.fg3)))
        .child(
            card(t)
                .px(r(4.))
                .pt(r(4.))
                .pb(r(3.))
                .child(bars)
                .child(labels)
                .child(
                    div()
                        .flex()
                        .gap(r(3.))
                        .mt(r(3.))
                        .pt(r(3.))
                        .border_t_2()
                        .border_color(t.line)
                        .child(stat("Best day".into(), best))
                        .child(stat(today.format("%B").to_string(), hours_short(d.hours_this_month))),
                ),
        )
}

fn quick_timers(t: &Theme, g: &mut Gallery, d: &Dashboard) -> Div {
    let mut body = card(t).overflow_hidden().flex().flex_col();
    let projects: Vec<_> = d.projects.iter().take(5).collect();
    if projects.is_empty() {
        body = body.child(text("No active projects.", 14.0, 600, t.fg3).px(r(4.)).py(r(3.)));
    }
    for (i, p) in projects.iter().enumerate() {
        let current = d.active.as_ref().is_some_and(|a| a.project_id == p.id);
        let paused = d.active.as_ref().is_some_and(|a| a.paused_at.is_some());
        let icon = if current && !paused { Glyph::Pause } else { Glyph::Play };
        let mut row = div()
            .relative()
            .flex()
            .items_center()
            .gap(r(3.))
            .pl(r(4.))
            .pr(r(3.))
            .h(px(56.))
            .child(div().size(r(3.)).flex_none().rounded_full().bg(parse_color(&p.color)))
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .child(text(p.name.clone(), 14.0, 700, t.fg).truncate())
                    .child(text(p.client_name.clone(), 12.0, 600, t.fg3).truncate()),
            )
            .child(
                div()
                    .size(r(9.))
                    .flex_none()
                    .rounded_full()
                    .bg(t.accent)
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .shadow(vec![crate::theme::shadow(0.0, 3.0, 10.0, -3.0, gpui::Rgba { r: 1.0, g: 140. / 255., b: 10. / 255., a: 0.6 })])
                    // hover:scale-110 can't be done (no transforms on divs): a lighter fill instead
                    .hover({
                        let c = t.accent_hover;
                        move |s| s.bg(c)
                    })
                    .child(div().pl(px(if icon == Glyph::Play { 2.0 } else { 0.0 })).child(picture(g.get(Art::Glyph(icon, "#ffffff".into()), 13.0), 13.0))),
            );
        if current {
            row = row.bg(t.accent.opacity(0.1));
        }
        if i + 1 < projects.len() {
            row = row.child(separator(t, 40.0));
        }
        body = body.child(row);
    }
    div().flex().flex_col().child(group_head(section_title(t, "Start a timer"), head_link(t, g, "Projects"))).child(body)
}
