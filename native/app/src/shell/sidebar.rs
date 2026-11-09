//! Sidebar.tsx: the app switch beside the window buttons, the wordmark,
//! the navigation with its sliding pill, Settings and the profile switcher.

use std::time::Duration;

use gpui::{
    div, img, px, Animation, AnimationExt as _, AnyElement, App, Div, ElementId, InteractiveElement, IntoElement,
    MouseButton, ParentElement, StatefulInteractiveElement, Styled, Window,
};

use super::{
    app::navigate,
    route::{Route, Section},
};
use crate::{
    art::{Art, Gallery, Icon},
    components::count_badge,
    motion,
    store,
    style::{kbd, r, text},
    theme::{rgba4, shadow, theme, Theme},
};

struct NavItem {
    section: Section,
    route: Route,
    icon: Icon,
    label: &'static str,
}

fn top_items() -> Vec<NavItem> {
    vec![
        NavItem { section: Section::Dashboard, route: Route::Dashboard, icon: Icon::Home, label: "Dashboard" },
        NavItem { section: Section::Clients, route: Route::Clients { action_new: false }, icon: Icon::Clients, label: "Clients" },
        NavItem { section: Section::Projects, route: Route::Projects { action_new: false }, icon: Icon::Projects, label: "Projects" },
        NavItem { section: Section::Time, route: Route::Time { action_new: false }, icon: Icon::Time, label: "Time" },
        NavItem { section: Section::Billing, route: Route::Billing { status: None }, icon: Icon::Billing, label: "Billing" },
        NavItem { section: Section::Reports, route: Route::Reports, icon: Icon::Reports, label: "Reports" },
    ]
}

fn business_items() -> Vec<NavItem> {
    vec![
        NavItem { section: Section::Commissions, route: Route::Commissions, icon: Icon::Commissions, label: "Commissions" },
        NavItem { section: Section::TaxOverview, route: Route::TaxOverview, icon: Icon::Tax, label: "Tax overview" },
        NavItem { section: Section::TaxSettings, route: Route::TaxSettings, icon: Icon::Receipt, label: "Tax settings" },
    ]
}

const ROW: f32 = 38.0;
/// space-y-0.5 between rows
const GAP: f32 = 1.625;
/// The "Business" heading: mt-5 (16.25) + 18px line + mb-1 (3.25)
const HEADING: f32 = 16.25 + 18.0 + 3.25;

/// The pill's top in the nav for a section (`None` if it isn't in the nav).
fn pill_top(section: Section) -> Option<f32> {
    if let Some(i) = top_items().iter().position(|n| n.section == section) {
        return Some(i as f32 * (ROW + GAP));
    }
    let base = 6.0 * ROW + 5.0 * GAP + HEADING;
    business_items().iter().position(|n| n.section == section).map(|i| base + i as f32 * (ROW + GAP))
}

thread_local! {
    static LAST_PILL: std::cell::Cell<Option<(Section, Section)>> = const { std::cell::Cell::new(None) };
}

fn nav_row(t: &Theme, g: &mut Gallery, item: &NavItem, active: bool, badge: usize) -> gpui::Stateful<Div> {
    let hover = t.fg.opacity(0.05);
    let route = item.route.clone();
    let mut row = div()
        .id(ElementId::Name(format!("nav-{}", item.label).into()))
        .relative()
        .flex()
        .items_center()
        .gap(r(2.5))
        .h(px(ROW))
        .px(r(2.5))
        .rounded(px(11.))
        .cursor_pointer()
        .on_click(move |_, w, cx| navigate(route.clone(), w, cx))
        .child(img(g.get(Art::Icon(item.icon), 22.0)).size(px(22.)).flex_none())
        .child(text(item.label, 14.0, if active { 600 } else { 500 }, if active { t.fg } else { t.fg2 }).flex_1().truncate());
    if !active {
        row = row.hover(move |s| s.bg(hover));
    }
    if badge > 0 {
        row = row.child(count_badge(t, badge));
    }
    row
}

/// The orange pill under the active row, sliding between rows with
/// framer-motion's spring (stiffness 500, damping 38).
fn pill(t: &Theme, section: Section) -> Option<AnyElement> {
    let to = pill_top(section)?;
    let prev = LAST_PILL.with(|l| l.get());
    let from_section = match prev {
        Some((from, to_s)) if to_s == section => Some(from),
        Some((_, last)) => {
            LAST_PILL.with(|l| l.set(Some((last, section))));
            Some(last)
        }
        None => {
            LAST_PILL.with(|l| l.set(Some((section, section))));
            None
        }
    };
    let from = from_section.and_then(pill_top).unwrap_or(to);
    let base = div().absolute().left_0().right_0().h(px(ROW)).rounded(px(11.)).bg(t.accent.opacity(0.12));
    if from == to || motion::reduced() {
        return Some(base.top(px(to)).into_any_element());
    }
    Some(
        base.with_animation(
            ElementId::Name(format!("nav-pill-{from}-{to}").into()),
            Animation::new(Duration::from_millis(500)),
            move |p, d| {
                let s = motion::spring(d * 0.5, 500.0, 38.0, 1.0);
                p.top(px(from + (to - from) * s))
            },
        )
        .into_any_element(),
    )
}

fn suite_switch(t: &Theme, g: &mut Gallery, content_icon: AnyElement) -> impl IntoElement {
    let thumb_shadow = if t.dark {
        vec![crate::theme::inset(0.0, 0.5, 0.0, 0.0, rgba4(255, 255, 255, 0.1)), shadow(0.0, 1.0, 3.0, 0.0, rgba4(0, 0, 0, 0.45))]
    } else {
        vec![
            shadow(0.0, 1.0, 2.0, 0.0, rgba4(20, 20, 40, 0.14)),
            shadow(0.0, 3.0, 10.0, -4.0, rgba4(20, 20, 40, 0.2)),
            shadow(0.0, 0.0, 0.0, 0.5, gpui::Rgba { a: 0.08, ..t.fg.to_rgb() }),
        ]
    };
    div()
        .flex()
        .items_center()
        .p(px(3.))
        .rounded_full()
        .bg(t.fg.opacity(0.055))
        .shadow(vec![crate::theme::inset(0.0, 0.0, 0.0, 0.5, gpui::Rgba { a: 0.08, ..t.fg.to_rgb() })])
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
                .child(img(g.get(Art::BillableMark, 18.0)).size(px(18.)).flex_none())
                .child(text("Billable", 12.0, 600, t.fg).line_height(px(16.))),
        )
        .child(
            div()
                .id("suite-content")
                .flex()
                .items_center()
                .h(px(26.))
                .px(px(7.))
                .opacity(if t.dark { 0.7 } else { 0.5 })
                .hover(|s| s.opacity(1.0))
                .cursor_pointer()
                .on_click(|_, w, cx| navigate(Route::Content, w, cx))
                .child(content_icon),
        )
}

pub fn render(section: Section, profile_view: AnyElement, window: &mut Window, cx: &mut App) -> impl IntoElement {
    let t = theme(cx);
    let attention = store::attention_count(cx);
    let _ = window;
    let update_banner = crate::components::update_banner::view(cx);
    let content_icon = super::app::glide::icon(super::app::glide::SuiteApp::Content, 18.0, cx).into_any_element();
    let g = cx.global_mut::<Gallery>();

    // Two `space-y-0.5` groups with the "Business" heading between them (no
    // gaps around the heading), and the active pill sliding over both.
    let mut nav = div().relative().flex().flex_col();
    if let Some(p) = pill(&t, section) {
        nav = nav.child(p);
    }
    let mut top = div().flex().flex_col().gap(px(GAP));
    for item in top_items() {
        let badge = if item.section == Section::Billing { attention } else { 0 };
        top = top.child(nav_row(&t, g, &item, item.section == section, badge));
    }
    let mut business = div().flex().flex_col().gap(px(GAP));
    for item in business_items() {
        business = business.child(nav_row(&t, g, &item, item.section == section, 0));
    }
    nav = nav
        .child(top)
        .child(text("Business", 12.0, 600, t.fg4).px(r(2.5)).mt(r(5.)).mb(r(1.)).line_height(px(18.)))
        .child(business);

    let settings = NavItem { section: Section::Settings, route: Route::Settings, icon: Icon::Settings, label: "Settings" };
    let settings_active = section == Section::Settings;
    let mut settings_row = div().relative();
    if settings_active {
        settings_row = settings_row.child(div().absolute().inset_0().rounded(px(11.)).bg(t.accent.opacity(0.12)));
    }
    settings_row = settings_row.child(nav_row(&t, g, &settings, settings_active, 0).child(kbd(&t, "⌘,")));

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
                .id("sidebar-drag")
                .h(px(48.))
                .flex_none()
                .flex()
                .items_center()
                .pl(px(78.))
                .on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move())
                .child(suite_switch(&t, g, content_icon)),
        )
        .child(
            div()
                .id("wordmark")
                .px(r(5.))
                .pb(r(3.))
                .flex_none()
                .cursor_pointer()
                .on_click(|_, w, cx| navigate(Route::Dashboard, w, cx))
                // An inline-flex link on a 19.5px-line strut: its line box is
                // ~26px, not the 24px of the text (leading-none).
                .child(div().h(px(26.)).flex().items_start().child(crate::style::text_tracked("billable", 24.0, 700, t.accent, -0.03).line_height(px(24.)))),
        )
        .child(div().id("nav").flex_1().overflow_y_scroll().px(r(3.)).pb(r(2.)).child(nav))
        .child(update_banner)
        .child(div().px(r(3.)).pt(r(1.)).flex_none().child(settings_row))
        .child(div().pt(r(1.)).pb(r(3.)).flex_none().child(profile_view))
}
