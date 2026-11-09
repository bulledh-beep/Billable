//! `.btn-*` from index.css: friendly pills (primary, secondary, ghost,
//! danger), the red confirm button, and round icon buttons.

use std::rc::Rc;

use gpui::{
    div, px, App, ClickEvent, ElementId, Hsla, InteractiveElement, IntoElement, MouseButton, ParentElement, RenderOnce,
    SharedString, StatefulInteractiveElement, Styled, Window,
};
use gpui_component::tooltip::Tooltip;

use crate::{
    icons::Lucide,
    style::{icon, icon_filled, r},
    theme::{brighten, hex, inset, rgba4, shadow, theme},
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Variant {
    /// `.btn-primary`: orange pill with a soft glow.
    Primary,
    /// `.btn-secondary`: quiet grey pill.
    Secondary,
    /// `.btn-ghost`: orange text, tinted on hover.
    Ghost,
    /// `.btn-danger`: red pill.
    Danger,
    /// ConfirmDialog's delete button (#E0383E).
    Destructive,
    /// `.btn-icon`: a round icon-only button.
    Icon,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Size {
    /// 32px tall (`.btn`), icon 15px.
    Md,
    /// 27px tall (`.btn-sm`, `.btn-icon-sm`), icon 13px (15px for icon-only).
    Sm,
}

type Handler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct Button {
    id: ElementId,
    variant: Variant,
    size: Size,
    label: Option<SharedString>,
    icon: Option<Lucide>,
    icon_size: Option<f32>,
    icon_filled: bool,
    icon_color: Option<Hsla>,
    text_color: Option<Hsla>,
    hover_text: Option<Hsla>,
    disabled: bool,
    selected: bool,
    full_width: bool,
    tooltip: Option<SharedString>,
    on_click: Option<Handler>,
}

impl Button {
    pub fn new(id: impl Into<ElementId>, variant: Variant) -> Self {
        Button {
            id: id.into(),
            variant,
            size: Size::Md,
            label: None,
            icon: None,
            icon_size: None,
            icon_filled: false,
            icon_color: None,
            text_color: None,
            hover_text: None,
            disabled: false,
            selected: false,
            full_width: false,
            tooltip: None,
            on_click: None,
        }
    }
    pub fn primary(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self::new(id, Variant::Primary).label(label)
    }
    pub fn secondary(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self::new(id, Variant::Secondary).label(label)
    }
    pub fn ghost(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self::new(id, Variant::Ghost).label(label)
    }
    pub fn danger(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self::new(id, Variant::Danger).label(label)
    }
    pub fn icon_button(id: impl Into<ElementId>, i: Lucide) -> Self {
        Self::new(id, Variant::Icon).icon(i)
    }
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
    pub fn icon(mut self, i: Lucide) -> Self {
        self.icon = Some(i);
        self
    }
    /// Draw the icon filled (`fill-current`).
    pub fn filled(mut self) -> Self {
        self.icon_filled = true;
        self
    }
    pub fn icon_size(mut self, px: f32) -> Self {
        self.icon_size = Some(px);
        self
    }
    pub fn icon_color(mut self, c: Hsla) -> Self {
        self.icon_color = Some(c);
        self
    }
    /// Override the text colour (e.g. `text-accent-text` on an icon button).
    pub fn text_color(mut self, c: Hsla) -> Self {
        self.text_color = Some(c);
        self
    }
    /// Text colour on hover (e.g. `hover:!text-red`).
    pub fn hover_text(mut self, c: Hsla) -> Self {
        self.hover_text = Some(c);
        self
    }
    pub fn small(mut self) -> Self {
        self.size = Size::Sm;
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    /// Shows the pressed/open look (Menu's trigger while open).
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
    pub fn full_width(mut self) -> Self {
        self.full_width = true;
        self
    }
    pub fn tooltip(mut self, s: impl Into<SharedString>) -> Self {
        self.tooltip = Some(s.into());
        self
    }
    pub fn on_click(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }
}

impl RenderOnce for Button {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = theme(cx);
        let sm = self.size == Size::Sm;
        let icon_only = self.variant == Variant::Icon;
        let height = if sm { 27.0 } else { 32.0 };
        let (mut fg, bg, hover_bg, hover_fg) = match self.variant {
            Variant::Primary => (hex(0xffffff), t.accent, Some(t.accent_hover), None),
            Variant::Secondary => (t.fg, t.fg.opacity(0.06), Some(t.fg.opacity(0.1)), None),
            Variant::Ghost => (t.accent_text, t.accent.opacity(0.0), Some(t.accent.opacity(0.1)), None),
            Variant::Danger => (hex(0xffffff), t.red, Some(brighten(t.red, 1.05)), None),
            Variant::Destructive => (hex(0xffffff), hex(0xE0383E), Some(hex(0xCC2F35)), None),
            Variant::Icon => (t.fg3, t.fg.opacity(0.0), Some(t.fg.opacity(0.07)), Some(t.fg)),
        };
        let mut bg = bg;
        let mut shadows = match self.variant {
            Variant::Primary => t.shadow_primary(),
            Variant::Destructive => vec![inset(0.0, 0.5, 0.0, 0.0, rgba4(255, 255, 255, 0.3)), shadow(0.0, 0.5, 1.0, 0.0, rgba4(0, 0, 0, 0.2))],
            _ => vec![],
        };
        let mut opacity = 1.0;
        if self.disabled {
            match self.variant {
                Variant::Primary => {
                    bg = t.fg.opacity(0.08);
                    fg = t.fg4;
                    shadows.clear();
                }
                Variant::Secondary => fg = t.fg4,
                _ => opacity = 0.4,
            }
        }
        if self.selected && icon_only {
            bg = t.fg.opacity(0.08);
            fg = t.fg;
        }
        if let Some(c) = self.text_color {
            fg = c;
        }
        let hover_fg = self.hover_text.or(hover_fg);

        let icon_px = self.icon_size.unwrap_or(if icon_only || !sm { 15.0 } else { 13.0 });
        let mut el = div()
            .id(self.id)
            .group("btn")
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .gap(r(1.5))
            .h(px(height))
            .rounded_full()
            .bg(bg)
            .text_color(fg)
            .shadow(shadows)
            .opacity(opacity)
            .whitespace_nowrap();
        el = if icon_only { el.w(px(height)) } else { el.px(r(if sm { 3.0 } else { 4.0 })) };
        if self.full_width {
            el = el.w_full();
        }
        if !self.disabled {
            el = el.cursor_pointer().hover(move |s| {
                let s = match hover_bg {
                    Some(b) => s.bg(b),
                    None => s,
                };
                match hover_fg {
                    Some(c) => s.text_color(c),
                    None => s,
                }
            });
            if let Some(f) = self.on_click.clone() {
                el = el.on_click(move |e, w, cx| f(e, w, cx));
            }
        }
        // Clicking a button shouldn't start a window drag from the toolbar.
        el = el.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        if let Some(tip) = self.tooltip {
            el = el.tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx));
        }
        if let Some(i) = self.icon {
            let svg = if self.icon_filled { icon_filled(i) } else { icon(i) };
            // GPUI's svg doesn't inherit the text colour, so give it one (and
            // follow the button's hover colour, like CSS currentColor).
            let mut svg = svg.size(px(icon_px)).text_color(self.icon_color.unwrap_or(fg));
            if let (None, Some(h), false) = (self.icon_color, hover_fg, self.disabled) {
                svg = svg.group_hover("btn", move |s| s.text_color(h));
            }
            el = el.child(svg);
        }
        if let Some(label) = self.label {
            el = el.child(div().font_family(crate::fonts::rounded_family(600)).text_size(px(if sm { 12.0 } else { 13.0 })).child(label));
        }
        el
    }
}
