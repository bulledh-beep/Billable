//! The window toolbar (App.tsx's header + PageHeader.tsx): the page title
//! with an optional back button and subtitle, the page's actions, and the
//! timer.

use gpui::{div, px, App, InteractiveElement, IntoElement, MouseButton, ParentElement, Styled, Window};

use super::{
    app::{navigate, Header},
    timer_control::TimerControl,
};
use crate::{
    components::Button,
    icons::Lucide,
    style::{r, text},
    theme::theme,
};

pub fn render(header: Header, timer: gpui::Entity<TimerControl>, _window: &mut Window, cx: &mut App) -> impl IntoElement {
    let t = theme(cx);
    let mut title_row = div().flex().items_center().gap(r(1.5)).min_w_0();
    let subtitle = header.subtitle.clone().or_else(|| header.back.as_ref().map(|b| b.0.clone()));
    if let Some((label, to)) = header.back.clone() {
        title_row = title_row.child(
            div().ml(-r(2.)).child(
                Button::icon_button("toolbar-back", Lucide::ChevronLeft)
                    .icon_size(18.0)
                    .tooltip(format!("Back to {label}"))
                    .on_click(move |_, w, cx| navigate(to.clone(), w, cx)),
            ),
        );
    }
    // h1: 20/24 bold, tracking -0.015em
    let mut titles = div().min_w_0().child(crate::style::text_tracked(header.title.clone(), 20.0, 700, t.fg, -0.015).line_height(px(24.)));
    if let Some(s) = subtitle {
        titles = titles.child(text(s, 12.0, 600, t.fg3).line_height(px(18.)).truncate());
    }
    title_row = title_row.child(titles);
    if let Some(meta) = header.meta {
        title_row = title_row.child(div().flex().items_center().gap(r(2.)).ml(r(2.)).flex_none().child(meta));
    }

    let mut bar = div()
        .id("toolbar")
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
        .child(div().flex_1().min_w_0().flex().items_center().child(title_row));
    if let Some(actions) = header.actions {
        bar = bar
            .child(div().flex().items_center().gap(r(2.)).child(actions))
            // .toolbar-divider (hidden when a page has no actions)
            .child(div().w(px(1.)).h(r(4.)).bg(t.line_strong));
    }
    bar.child(timer)
}
