//! Segmented.tsx (`.seg`): a soft pill track with a raised thumb under the
//! chosen option, for filters and view modes.

use gpui::{
    div, px, AnyElement, App, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce, SharedString,
    StatefulInteractiveElement, Styled, Window,
};

use super::table::measure;
use crate::{
    style::{num_font, r},
    theme::{rgba4, shadow, theme},
};

pub struct Segment<T> {
    pub value: T,
    pub label: SharedString,
    pub count: Option<usize>,
}

impl<T> Segment<T> {
    pub fn new(value: T, label: impl Into<SharedString>) -> Self {
        Segment { value, label: label.into(), count: None }
    }
    pub fn count(mut self, n: usize) -> Self {
        self.count = Some(n);
        self
    }
}

pub fn segmented<T: Clone + PartialEq + 'static>(
    id: &str,
    value: &T,
    options: Vec<Segment<T>>,
    on_change: impl Fn(T, &mut Window, &mut App) + 'static,
    _cx: &App,
) -> AnyElement {
    Segmented { id: id.into(), value: value.clone(), options, on_change: std::rc::Rc::new(on_change) }.into_any_element()
}

/// Built at render time so the labels can be measured: GPUI rounds a text
/// element's width up to a whole pixel, and Chromium doesn't, so each label
/// and count gets its exact width.
#[derive(IntoElement)]
struct Segmented<T: Clone + PartialEq + 'static> {
    id: SharedString,
    value: T,
    options: Vec<Segment<T>>,
    on_change: std::rc::Rc<dyn Fn(T, &mut Window, &mut App)>,
}

impl<T: Clone + PartialEq + 'static> RenderOnce for Segmented<T> {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = theme(cx);
        let id = self.id;
        let value = &self.value;
        let on_change = self.on_change;
        let mut track = div().flex().flex_none().items_center().p(px(3.)).rounded_full().bg(t.fg.opacity(0.06));
        for (i, o) in self.options.into_iter().enumerate() {
            let active = &o.value == value;
            let (fg, hover) = (if active { t.fg } else { t.fg3 }, t.fg);
            let mut item = div()
                .id(ElementId::Name(format!("{id}-{i}").into()))
                .flex()
                .items_center()
                .gap(r(1.5))
                .h(px(27.))
                .px(r(3.5))
                .rounded_full()
                .cursor_pointer()
                .whitespace_nowrap()
                .text_color(fg)
                .hover(move |s| s.text_color(hover));
            if active {
                item = item.bg(if t.dark { t.fg.opacity(0.14) } else { t.panel }).shadow(vec![
                    shadow(0.0, 1.0, 2.0, 0.0, rgba4(0, 0, 0, 0.1)),
                    shadow(0.0, 2.0, 8.0, -2.0, rgba4(0, 0, 0, 0.1)),
                ]);
            }
            let v = o.value.clone();
            let f = on_change.clone();
            let label_w = measure(window, &o.label, 13.0, 600, false);
            item = item.on_click(move |_, w, cx| f(v.clone(), w, cx)).child(
                div().flex_none().w(px(label_w)).font_family(crate::fonts::rounded_family(600)).text_size(px(13.)).child(o.label),
            );
            if let Some(n) = o.count {
                let n = n.to_string();
                let count_w = measure(window, &n, 11.0, 600, true);
                item = item.child(
                    div()
                        .flex_none()
                        .w(px(count_w))
                        .font(num_font(600))
                        .text_size(px(11.))
                        .line_height(px(14.))
                        .text_color(if active { t.fg3 } else { t.fg4 })
                        .child(n),
                );
            }
            track = track.child(item);
        }
        track
    }
}
