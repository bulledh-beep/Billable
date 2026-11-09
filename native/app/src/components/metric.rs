//! Metric.tsx: a friendly stat card (illustration, big number, label).

use gpui::{
    div, px, AnyElement, App, ClickEvent, ElementId, Hsla, InteractiveElement, IntoElement, ParentElement, RenderOnce,
    SharedString, StatefulInteractiveElement, Styled, Window,
};

use crate::{
    art::{Art, Gallery, Icon},
    style::{r, text},
    theme::{hex, mix, theme},
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tint {
    Orange,
    Blue,
    Red,
    Green,
    Purple,
    Yellow,
}

impl Tint {
    pub fn color(self) -> Hsla {
        hex(match self {
            Tint::Orange => 0xFF9600,
            Tint::Blue => 0x1CB0F6,
            Tint::Red => 0xFF4B4B,
            Tint::Green => 0x58CC02,
            Tint::Purple => 0xCE82FF,
            Tint::Yellow => 0xFFC800,
        })
    }
}

type Handler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct Metric {
    id: ElementId,
    label: SharedString,
    value: AnyElement,
    sub: Option<SharedString>,
    icon: Option<Icon>,
    tint: Option<Tint>,
    active: Option<bool>,
    on_click: Option<Handler>,
}

impl Metric {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>, value: impl IntoElement) -> Self {
        Metric { id: id.into(), label: label.into(), value: value.into_any_element(), sub: None, icon: None, tint: None, active: None, on_click: None }
    }
    pub fn sub(mut self, s: impl Into<SharedString>) -> Self {
        self.sub = Some(s.into());
        self
    }
    pub fn icon(mut self, i: Icon) -> Self {
        self.icon = Some(i);
        self
    }
    pub fn tint(mut self, t: Tint) -> Self {
        self.tint = Some(t);
        self
    }
    /// Marks the metric that is filtering the list below.
    pub fn active(mut self, a: bool) -> Self {
        self.active = Some(a);
        self
    }
    pub fn on_click(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Box::new(f));
        self
    }
}

impl RenderOnce for Metric {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = theme(cx);
        let (bg, border) = match self.tint {
            Some(tint) => (mix(tint.color(), t.panel, 0.09), mix(tint.color(), t.line, 0.22)),
            None => (t.panel, t.line),
        };
        let active = self.active == Some(true);
        let mut card = div()
            .id(self.id)
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
            .bg(bg)
            .border_color(border)
            .shadow(t.shadow_card());
        if active {
            // ring-2 ring-accent
            card = card.border_2().border_color(t.accent).px(r(4.) - px(1.)).py(r(3.5) - px(1.));
        }
        if let Some(f) = self.on_click {
            card = card.cursor_pointer().hover(|s| s.top(px(-2.))).active(|s| s.top(px(0.))).on_click(f);
        }
        if let Some(icon) = self.icon {
            let image = cx.global_mut::<Gallery>().get(Art::Icon(icon), 29.25);
            card = card.child(gpui::img(image).size(r(9.)).flex_none());
        }
        let mut body = div()
            .min_w_0()
            .flex()
            .flex_col()
            .child(div().h(px(27.)).line_height(px(27.)).flex().items_center().overflow_hidden().child(self.value))
            // Heights pinned to the CSS line boxes (13px × 1.5, 12px × 1.5):
            // GPUI rounds a text line's height to whole pixels.
            .child(text(self.label, 13.0, 600, if active { t.accent_text } else { t.fg2 }).truncate().line_height(px(19.5)).h(px(19.5)));
        if let Some(sub) = self.sub {
            body = body.child(text(sub, 12.0, 500, t.fg3).truncate().line_height(px(18.)).h(px(18.)));
        }
        card.child(body)
    }
}

/// MetricStrip: equal-width stat cards side by side.
pub fn metric_strip(children: Vec<Metric>) -> gpui::Div {
    div().flex().gap(r(3.)).children(children)
}
