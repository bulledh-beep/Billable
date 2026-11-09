//! ConfirmDialog.tsx: a small modal asking before something destructive.

use gpui::{div, px, App, IntoElement, ParentElement, SharedString, Styled, Window};

use super::{
    button::{Button, Variant},
    modal::{Modal, ModalSize},
};
use crate::{style::r, theme::theme};

/// `danger` (the default in the app) uses the red #E0383E button.
pub fn confirm_dialog(
    id: &str,
    title: impl Into<SharedString>,
    message: impl IntoElement,
    confirm_text: impl Into<SharedString>,
    danger: bool,
    on_confirm: impl Fn(&mut Window, &mut App) + 'static,
    on_close: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> impl IntoElement {
    let t = theme(cx);
    let on_close = std::rc::Rc::new(on_close);
    let close_a = on_close.clone();
    let close_b = on_close.clone();
    let close_c = on_close.clone();
    let confirm = Button::new(format!("{id}-confirm"), if danger { Variant::Destructive } else { Variant::Primary })
        .label(confirm_text)
        .on_click(move |_, w, cx| {
            on_confirm(w, cx);
            close_c(w, cx);
        });
    Modal::new(SharedString::from(id.to_string()), title, move |w, cx| close_a(w, cx))
        .size(ModalSize::Sm)
        .child(div().font_family(crate::fonts::rounded_family(500)).text_size(px(13.)).text_color(t.fg2).line_height(r(5.)).child(message))
        .footer(Button::secondary(format!("{id}-cancel"), "Cancel").on_click(move |_, w, cx| close_b(w, cx)))
        .footer(confirm)
}
