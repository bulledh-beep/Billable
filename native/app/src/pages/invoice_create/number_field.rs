//! The builder's text fields: NumberField.tsx (a tidy fixed-decimal value
//! until you focus it, then free typing), `<input type="number">` with
//! `value={n || ''}` (the tax rates), and the small `h-7` field boxes the
//! invoice lines use (plain, or borderless until hovered).

use std::{cell::Cell, rc::Rc};

use billable_core::js::{js_number_to_string, js_parse_float};
use gpui::{
    div, px, AppContext, Context, Div, Entity, FocusHandle, Focusable, InteractiveElement, IntoElement, MouseButton, ParentElement,
    Styled, Subscription, Window,
};
use gpui_component::input::{Input, InputEvent, InputState};

use super::lines::to_fixed;
use crate::{
    components::fields::{input_box, BorderWidth},
    fonts::rounded_family,
    style::{num_font, r},
    theme::{rgba4, shadow, Theme},
};

/// NumberField.tsx. The page owns the number; this keeps the text in step.
pub struct NumberField {
    pub state: Entity<InputState>,
    value: Rc<Cell<f64>>,
    decimals: usize,
    _sub: Subscription,
}

impl NumberField {
    /// `on_change` gets each typed value (`parseFloat`, or 0).
    pub fn new<V: 'static>(
        value: f64,
        decimals: usize,
        blur_to: FocusHandle,
        window: &mut Window,
        cx: &mut Context<V>,
        on_change: impl Fn(&mut V, f64, &mut Window, &mut Context<V>) + 'static,
    ) -> Self {
        let state = cx.new(|cx| InputState::new(window, cx).default_value(to_fixed(value, decimals)));
        let cell = Rc::new(Cell::new(value));
        let c = cell.clone();
        let sub = cx.subscribe_in(&state, window, move |this, state, e: &InputEvent, window, cx| match e {
            // Focus shows the raw number and selects it, ready to retype.
            InputEvent::Focus => {
                let v = c.get();
                state.update(cx, |s, cx| {
                    s.set_value(js_number_to_string(v), window, cx);
                    s.select_all(window, cx);
                });
            }
            InputEvent::Blur => {
                let v = c.get();
                state.update(cx, |s, cx| s.set_value(to_fixed(v, decimals), window, cx));
            }
            InputEvent::Change => {
                let text = state.read(cx).value().to_string();
                let raw: String = text.chars().filter(|ch| ch.is_ascii_digit() || *ch == '.' || *ch == '-').collect();
                if raw != text {
                    state.update(cx, |s, cx| s.set_value(raw.clone(), window, cx));
                }
                let n = js_parse_float(&raw);
                let n = if n.is_finite() { n } else { 0.0 };
                c.set(n);
                on_change(this, n, window, cx);
                cx.notify();
            }
            // Enter commits: the field blurs and tidies itself.
            InputEvent::PressEnter { .. } => blur_to.focus(window, cx),
        });
        NumberField { state, value: cell, decimals, _sub: sub }
    }

    /// Follow the page's number while the field isn't being typed in.
    pub fn sync(&self, value: f64, window: &mut Window, cx: &mut gpui::App) {
        self.value.set(value);
        if self.state.focus_handle(cx).is_focused(window) {
            return;
        }
        let shown = to_fixed(value, self.decimals);
        if self.state.read(cx).value().as_ref() != shown {
            self.state.update(cx, |s, cx| s.set_value(shown, window, cx));
        }
    }
}

/// HTML's "valid floating-point number" (what `<input type=number>` accepts as a value).
fn valid_number(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = 0;
    if b.first() == Some(&b'-') {
        i += 1;
    }
    let int_start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    let int_digits = i - int_start;
    let mut frac_digits = 0;
    if i < b.len() && b[i] == b'.' {
        i += 1;
        let f = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        frac_digits = i - f;
        if frac_digits == 0 {
            return false;
        }
    }
    if int_digits + frac_digits == 0 {
        return false;
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        i += 1;
        if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
            i += 1;
        }
        let e = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == e {
            return false;
        }
    }
    i == b.len()
}

/// `<input type="number" step="0.01" value={n || ''} onChange={parseFloat || 0}>`.
pub struct RateInput {
    pub state: Entity<InputState>,
    value: Rc<Cell<f64>>,
    _sub: Subscription,
}

impl RateInput {
    pub fn new<V: 'static>(
        value: f64,
        placeholder: &'static str,
        window: &mut Window,
        cx: &mut Context<V>,
        on_change: impl Fn(&mut V, f64, &mut Window, &mut Context<V>) + 'static,
    ) -> Self {
        let shown = if value != 0.0 { js_number_to_string(value) } else { String::new() };
        let state = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder).default_value(shown));
        let cell = Rc::new(Cell::new(value));
        let c = cell.clone();
        let sub = cx.subscribe_in(&state, window, move |this, state, e: &InputEvent, window, cx| {
            if !matches!(e, InputEvent::Change) {
                return;
            }
            let text = state.read(cx).value().to_string();
            // A number field only takes the characters of a number.
            let typed: String = text.chars().filter(|ch| ch.is_ascii_digit() || ".,+-eE".contains(*ch)).collect();
            if typed != text {
                state.update(cx, |s, cx| s.set_value(typed.clone(), window, cx));
            }
            // An unfinished number ("5.") reads as empty; React then writes
            // `0 || ''` back, which clears a typed zero.
            let n = if valid_number(&typed) { js_parse_float(&typed) } else { f64::NAN };
            let n = if n.is_finite() { n } else { 0.0 };
            if n == 0.0 && valid_number(&typed) {
                state.update(cx, |s, cx| s.set_value("", window, cx));
            }
            c.set(n);
            on_change(this, n, window, cx);
            cx.notify();
        });
        RateInput { state, value: cell, _sub: sub }
    }

    /// The arrow keys step by 0.01 like Chromium's number field.
    pub fn step(&self, up: bool, window: &mut Window, cx: &mut gpui::App) -> f64 {
        let v = self.value.get();
        let scaled = v * 100.0;
        let n = if (scaled - scaled.round()).abs() < 1e-7 {
            scaled.round() + if up { 1.0 } else { -1.0 }
        } else if up {
            scaled.ceil()
        } else {
            scaled.floor()
        };
        let next = n / 100.0;
        self.value.set(next);
        let shown = if next != 0.0 { js_number_to_string(next) } else { String::new() };
        self.state.update(cx, |s, cx| s.set_value(shown, window, cx));
        next
    }
}

/// How a small line field looks at rest.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Look {
    /// `.input h-7`
    Plain,
    /// `.input h-7 border-transparent hover:border-line bg-transparent focus:bg-field`
    Ghost,
}

/// An `h-7` field box around an `Input`. Clicks inside keep the page from
/// taking focus back.
pub fn line_field(t: &Theme, state: &Entity<InputState>, look: Look, right: bool, tabular: bool, window: &Window, cx: &gpui::App) -> Div {
    let focused = state.focus_handle(cx).is_focused(window);
    let b = match look {
        Look::Plain => input_box(t, focused, false).h(r(7.)),
        Look::Ghost => {
            let (line, hover_bg) = (t.line, t.fg.opacity(0.07));
            let b = div()
                .relative()
                .flex()
                .items_center()
                .w_full()
                .h(r(7.))
                .px(r(3.))
                .rounded(px(10.))
                .border_px(1.5)
                .font_family(rounded_family(500))
                .text_size(px(14.))
                .text_color(t.fg);
            if focused {
                let a = t.accent.to_rgb();
                b.bg(t.field)
                    .border_color(t.accent)
                    .shadow(vec![shadow(0.0, 0.0, 0.0, 4.0, rgba4((a.r * 255.0) as u8, (a.g * 255.0) as u8, (a.b * 255.0) as u8, 0.15))])
                    // hover:border-line still wins over the focus border
                    .hover(move |s| s.border_color(line))
            } else {
                b.bg(gpui::transparent_black()).border_color(gpui::transparent_black()).hover(move |s| s.bg(hover_bg).border_color(line))
            }
        }
    };
    b.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()).child(div().flex_1().min_w_0().h_full().child(input(state, right, tabular)))
}

/// gpui-component's `Input` with no chrome or padding of its own: the box
/// around it supplies `.input`'s padding, so text sits where Chromium puts it.
pub fn input(state: &Entity<InputState>, right: bool, tabular: bool) -> impl IntoElement {
    // `Styled::h_full`: Input's own `h_full` only sizes multi-line inputs.
    let mut field = Styled::h_full(Input::new(state).appearance(false).px_0().py_0());
    if right {
        field = field.text_right();
    }
    if tabular {
        field = field.font(num_font(500));
    }
    field
}
