//! Modal.tsx: a dimmed backdrop and a rounded panel with a title row, a
//! scrolling body and an optional footer of right-aligned buttons. Escape or
//! a click on the backdrop closes it.
//!
//! The panel springs up and fades in like framer-motion's; GPUI can't scale
//! elements, so the 94%→100% grow is left out.

use std::{rc::Rc, time::Duration};

use gpui::{
    anchored, deferred, div, point, px, Animation, AnimationExt as _, AnyElement, App, ElementId, FocusHandle,
    InteractiveElement, IntoElement, KeyDownEvent, ParentElement, RenderOnce, SharedString, StatefulInteractiveElement,
    Styled, Window,
};

use super::button::Button;
use crate::{
    icons::Lucide,
    motion,
    style::{r, text},
    theme::theme,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ModalSize {
    Sm,
    Md,
    Lg,
    Xl,
}

impl ModalSize {
    fn max_width(self) -> f32 {
        match self {
            ModalSize::Sm => 420.,
            ModalSize::Md => 520.,
            ModalSize::Lg => 680.,
            ModalSize::Xl => 880.,
        }
    }
}

type Close = Rc<dyn Fn(&mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct Modal {
    id: ElementId,
    title: SharedString,
    description: Option<AnyElement>,
    body: Vec<AnyElement>,
    footer: Vec<AnyElement>,
    size: ModalSize,
    focus: Option<FocusHandle>,
    on_close: Close,
}

impl Modal {
    pub fn new(id: impl Into<ElementId>, title: impl Into<SharedString>, on_close: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        Modal { id: id.into(), title: title.into(), description: None, body: vec![], footer: vec![], size: ModalSize::Md, focus: None, on_close: Rc::new(on_close) }
    }
    pub fn size(mut self, s: ModalSize) -> Self {
        self.size = s;
        self
    }
    pub fn description(mut self, d: impl IntoElement) -> Self {
        self.description = Some(d.into_any_element());
        self
    }
    pub fn child(mut self, c: impl IntoElement) -> Self {
        self.body.push(c.into_any_element());
        self
    }
    pub fn footer(mut self, c: impl IntoElement) -> Self {
        self.footer.push(c.into_any_element());
        self
    }
    /// Keyboard focus to hold while open (so Escape reaches the modal even
    /// when no field inside is focused).
    pub fn focus(mut self, f: &FocusHandle) -> Self {
        self.focus = Some(f.clone());
        self
    }
}

impl RenderOnce for Modal {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = theme(cx);
        let viewport = window.viewport_size();
        let close = self.on_close.clone();
        let close_esc = self.on_close.clone();
        let close_btn = self.on_close.clone();
        let backdrop_alpha = if t.dark { 0.5 } else { 0.25 };
        let id = self.id.clone();

        let mut header_text = div().min_w_0().child(text(self.title, 18.0, 700, t.fg).line_height(r(6.)));
        if let Some(d) = self.description {
            header_text = header_text.child(div().text_size(px(12.)).text_color(t.fg3).mt(r(1.)).line_height(px(17.)).child(d));
        }
        let mut panel = div()
            .id(ElementId::Name(format!("{}-panel", id).into()))
            .relative()
            .w_full()
            .max_w(px(self.size.max_width()))
            .max_h(viewport.height - px(48.))
            .flex()
            .flex_col()
            .bg(t.panel)
            .rounded(px(18.))
            .shadow(t.shadow_pop())
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap(r(4.))
                    .px(r(6.))
                    .pt(r(5.))
                    .pb(r(3.))
                    .child(header_text)
                    .child(
                        div().mr(-r(1.5)).mt(-r(0.5)).child(
                            Button::icon_button(ElementId::Name(format!("{}-close", id).into()), Lucide::X).small().on_click(move |_, w, cx| close_btn(w, cx)),
                        ),
                    ),
            )
            .child(
                div()
                    .id(ElementId::Name(format!("{}-body", id).into()))
                    .px(r(6.))
                    .pb(r(6.))
                    .min_h_0()
                    .overflow_y_scroll()
                    .children(self.body),
            );
        if !self.footer.is_empty() {
            panel = panel.child(
                div().flex().items_center().justify_end().gap(r(2.)).px(r(6.)).pt(r(4.)).pb(r(5.)).border_t_1().border_color(t.line).children(self.footer),
            );
        }
        let panel: AnyElement = if motion::reduced() {
            panel.into_any_element()
        } else {
            panel
                .with_animation(ElementId::Name(format!("{}-in", id).into()), Animation::new(Duration::from_millis(450)), |p, delta| {
                    let s = motion::spring(delta * 0.45, 460.0, 30.0, 0.8);
                    p.opacity(s.min(1.0)).top(px(10.0 * (1.0 - s)))
                })
                .into_any_element()
        };

        let mut layer = div()
            .id(ElementId::Name(format!("{}-layer", id).into()))
            .w(viewport.width)
            .h(viewport.height)
            .flex()
            .items_center()
            .justify_center()
            .p(r(6.))
            .on_key_down(move |e: &KeyDownEvent, w, cx| {
                if e.keystroke.key == "escape" {
                    cx.stop_propagation();
                    close_esc(w, cx);
                }
            })
            .child(
                div()
                    .id(ElementId::Name(format!("{}-backdrop", id).into()))
                    .absolute()
                    .inset_0()
                    .bg(gpui::black().opacity(backdrop_alpha))
                    .on_click(move |_, w, cx| close(w, cx)),
            )
            .child(panel);
        if let Some(f) = &self.focus {
            layer = layer.track_focus(f);
            if !f.contains_focused(window, cx) {
                window.focus(f, cx);
            }
        }
        deferred(anchored().position(point(px(0.), px(0.))).child(layer)).with_priority(10)
    }
}
