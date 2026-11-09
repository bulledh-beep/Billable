//! CloseOutModal.tsx: shown when a project with unbilled time is marked
//! complete or archived, so leftover hours don't get forgotten.

use std::rc::Rc;

use billable_core::models::Project;
use gpui::{div, prelude::*, px, App, ElementId, IntoElement, ParentElement, Styled, Window};

use crate::{
    components::{Modal, ModalSize},
    format::{format_day, hours_short, money},
    icons::Lucide,
    style::{icon, num_font, r, text},
    theme::theme,
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CloseOutChoice {
    Invoice,
    DontBill,
    Keep,
}

pub fn close_out_modal(
    project: &Project,
    next_status: &str,
    on_choose: impl Fn(CloseOutChoice, &mut Window, &mut App) + 'static,
    on_close: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> impl IntoElement {
    let t = theme(cx);
    let on_choose = Rc::new(on_choose);
    let options: [(CloseOutChoice, Lucide, &str, String, bool); 3] = [
        (
            CloseOutChoice::Invoice,
            Lucide::FileText,
            "Create an invoice for it",
            format!("Marks the project {next_status} and opens a new invoice with this time."),
            true,
        ),
        (CloseOutChoice::Keep, Lucide::Clock, "Keep it unbilled for now", "It stays on your Billing page until you invoice it.".into(), false),
        (CloseOutChoice::DontBill, Lucide::Ban, "Don't bill it", "Marks the time non-billable so it stops showing as money owed.".into(), false),
    ];

    let mut list = div().flex().flex_col().gap(r(2.));
    for (i, (choice, glyph, title, body, primary)) in options.into_iter().enumerate() {
        let (border, bg, hover_bg, hover_border) = if primary {
            (t.accent.opacity(0.5), t.accent.opacity(0.06), t.accent.opacity(0.1), t.accent.opacity(0.5))
        } else {
            (t.line, t.fg.opacity(0.0), t.fg.opacity(0.03), t.line_strong)
        };
        let f = on_choose.clone();
        list = list.child(
            div()
                .id(ElementId::Name(format!("close-out-{i}").into()))
                .w_full()
                .flex()
                .items_start()
                .gap(r(3.))
                .p(r(3.))
                .rounded(r(2.))
                .border_1()
                .border_color(border)
                .bg(bg)
                .cursor_pointer()
                .hover(move |s| s.bg(hover_bg).border_color(hover_border))
                .on_click(move |_, w, cx| f(choice, w, cx))
                .child(icon(glyph).size(r(4.)).mt(r(0.5)).text_color(if primary { t.accent_text } else { t.fg3 }))
                .child(
                    div()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(text(title.to_string(), 13.0, 500, t.fg).line_height(px(18.)))
                        .child(text(body, 12.0, 500, t.fg3).mt(r(0.5)).line_height(px(16.))),
                ),
        );
    }

    let going_back = project.oldest_unbilled.as_deref().filter(|s| !s.is_empty()).map(|d| format!(", going back to {}", format_day(d))).unwrap_or_default();
    let description = div()
        .flex()
        .flex_wrap()
        .child(div().font(num_font(500)).text_color(t.fg).child(money(project.unbilled_amount)))
        .child(format!(" across {}{going_back}.", hours_short(project.unbilled_hours)));

    Modal::new("close-out", format!("{} has unbilled time", project.name), on_close).size(ModalSize::Sm).description(description).child(list)
}
