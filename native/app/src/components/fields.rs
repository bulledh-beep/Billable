//! Form controls with Billable's `.input` look (index.css): 34px tall, 10px
//! corners, a field-grey fill that lightens on hover, and an orange border
//! with a soft 4px ring when focused. Text editing (selection, IME, undo)
//! comes from gpui-component's input; the chrome is ours.

use gpui::{
    div, px, AbsoluteLength, App, Div, ElementId, Entity, Focusable, Hsla, InteractiveElement, IntoElement,
    ParentElement, RenderOnce, SharedString, StatefulInteractiveElement, Styled, Window,
};
use gpui_component::{
    input::{Input, InputState, Textarea, TextareaState},
    Sizable, Size,
};

use crate::{
    fonts::rounded_family,
    icons::Lucide,
    style::{icon, icon_stroke, r},
    theme::{rgba4, shadow, theme, Theme},
};

/// Set every border to the same width (Tailwind's `border-[1.5px]`).
pub trait BorderWidth: Styled + Sized {
    fn border_px(mut self, w: f32) -> Self {
        let w = Some(AbsoluteLength::Pixels(px(w)));
        let edges = &mut self.style().border_widths;
        edges.top = w;
        edges.right = w;
        edges.bottom = w;
        edges.left = w;
        self
    }
}
impl<T: Styled + Sized> BorderWidth for T {}

/// The `.input` box around a control.
pub fn input_box(t: &Theme, focused: bool, disabled: bool) -> Div {
    let hover = t.fg.opacity(0.07);
    let mut b = div()
        .relative()
        .flex()
        .items_center()
        .w_full()
        .h(px(34.))
        .px(r(3.))
        .rounded(px(10.))
        .border_px(1.5)
        .font_family(rounded_family(500))
        .text_size(px(14.))
        .text_color(t.fg);
    if focused {
        let a = t.accent.to_rgb();
        b = b.bg(t.panel).border_color(t.accent).shadow(vec![shadow(0.0, 0.0, 0.0, 4.0, rgba4((a.r * 255.0) as u8, (a.g * 255.0) as u8, (a.b * 255.0) as u8, 0.15))]);
    } else {
        b = b.bg(t.field).border_color(gpui::transparent_black());
        if !disabled {
            b = b.hover(move |s| s.bg(hover));
        }
    }
    if disabled {
        b = b.opacity(0.5);
    }
    b
}

/// A single-line text field. The page owns the `InputState` entity.
#[derive(IntoElement)]
pub struct TextField {
    state: Entity<InputState>,
    leading: Option<Lucide>,
    leading_text: Option<SharedString>,
    trailing: Option<gpui::AnyElement>,
    disabled: bool,
    tabular: bool,
}

impl TextField {
    pub fn new(state: &Entity<InputState>) -> Self {
        TextField { state: state.clone(), leading: None, leading_text: None, trailing: None, disabled: false, tabular: false }
    }
    /// An icon inside the left edge (SearchInput's magnifier; text starts at pl-8).
    pub fn leading_icon(mut self, i: Lucide) -> Self {
        self.leading = Some(i);
        self
    }
    /// Text inside the left edge, like the "$" before a rate (text starts at pl-6).
    pub fn leading_text(mut self, s: impl Into<SharedString>) -> Self {
        self.leading_text = Some(s.into());
        self
    }
    pub fn trailing(mut self, el: impl IntoElement) -> Self {
        self.trailing = Some(el.into_any_element());
        self
    }
    pub fn disabled(mut self, d: bool) -> Self {
        self.disabled = d;
        self
    }
    /// `.num`: tabular figures.
    pub fn tabular(mut self) -> Self {
        self.tabular = true;
        self
    }
}

impl RenderOnce for TextField {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = theme(cx);
        let focused = self.state.focus_handle(cx).is_focused(window);
        let mut b = input_box(&t, focused, self.disabled);
        if let Some(i) = self.leading {
            b = b.pl(r(8.)).child(div().absolute().left(r(2.5)).top_0().bottom_0().flex().items_center().child(icon(i).size(px(14.)).text_color(t.fg4)));
        }
        if let Some(s) = self.leading_text {
            b = b.pl(r(6.)).child(div().absolute().left(r(2.5)).top_0().bottom_0().flex().items_center().text_size(px(13.)).text_color(t.fg3).child(s));
        }
        // gpui-component pads its input; the `.input` box already has px-3.
        let mut field = Input::new(&self.state).appearance(false).disabled(self.disabled).px_0().py_0();
        if self.tabular {
            field = field.font(crate::style::num_font(500));
        }
        b = b.child(div().flex_1().min_w_0().child(field));
        if let Some(tr) = self.trailing {
            b = b.child(tr);
        }
        b
    }
}

/// SearchInput.tsx: a search field with a magnifier and a clear button.
pub fn search_field(id: impl Into<ElementId>, state: &Entity<InputState>, cx: &App) -> TextField {
    let t = theme(cx);
    let has_text = !state.read(cx).value().is_empty();
    let st = state.clone();
    let mut f = TextField::new(state).leading_icon(Lucide::Search);
    if has_text {
        let fg4 = t.fg4;
        let fg = t.fg;
        f = f.trailing(
            div()
                .id(id)
                .p(r(1.))
                .mr(px(-4.))
                .rounded(px(4.))
                .cursor_pointer()
                .group("search-clear")
                .on_click(move |_, window, cx| st.update(cx, |s, cx| s.set_value("", window, cx)))
                .child(icon(Lucide::X).size(r(3.)).text_color(fg4).group_hover("search-clear", move |s| s.text_color(fg))),
        );
    }
    f
}

/// A multi-line text box (`textarea.input`: py-2, `leading-5` lines of
/// 16.25px, grows with `rows`).
#[derive(IntoElement)]
pub struct TextArea {
    state: Entity<TextareaState>,
    rows: usize,
}

impl TextArea {
    pub fn new(state: &Entity<TextareaState>, rows: usize) -> Self {
        TextArea { state: state.clone(), rows }
    }
}

impl RenderOnce for TextArea {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = theme(cx);
        let focused = self.state.focus_handle(cx).is_focused(window);
        const LINE: f32 = 16.25;
        // gpui-component insets a multi-line editor's text by its size's
        // padding; XSmall's is the least (4px left/right, 0 top/bottom), so
        // the box's px-3 gives those 4px back.
        const EDITOR_PX: f32 = 4.0;
        // py-2 (6.5px) and the lines, plus the 1.5px borders
        let height = 6.5 * 2.0 + LINE * self.rows as f32 + 3.0;
        input_box(&t, focused, false)
            .h(px(height))
            .items_start()
            .py(r(2.))
            .px(r(3.) - px(EDITOR_PX))
            .line_height(px(LINE))
            .child(
                div().flex_1().min_w_0().h_full().child(
                    Textarea::new(&self.state)
                        .appearance(false)
                        .with_size(Size::XSmall)
                        .h_full()
                        .px_0()
                        .text_size(px(14.))
                        .line_height(px(LINE)),
                ),
            )
    }
}

/// `select.input`: the closed control, with the chevron drawn 10px from the
/// right. Opening it shows the Mac's own pop-up menu (see `select_menu`), as
/// Chromium does.
pub fn select_box(t: &Theme, id: impl Into<ElementId>, label: impl Into<SharedString>, placeholder: bool) -> gpui::Stateful<Div> {
    input_box(t, false, false)
        .id(id)
        .pr(r(8.))
        .cursor_pointer()
        .child(div().flex_1().min_w_0().truncate().text_color(if placeholder { t.fg4 } else { t.fg }).child(label.into()))
        .child(
            div()
                .absolute()
                .right(px(10.))
                .top_0()
                .bottom_0()
                .flex()
                .items_center()
                .child(icon_stroke(Lucide::ChevronDown, 2.6).size(px(14.)).text_color(crate::theme::hex(0x999999))),
        )
}

/// Billable's checkbox (index.css `input[type='checkbox']`): 17px, 6px
/// corners, orange with a white tick when checked.
pub fn checkbox(t: &Theme, id: impl Into<ElementId>, checked: bool, indeterminate: bool) -> gpui::Stateful<Div> {
    let on = checked || indeterminate;
    let mut b = div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(17.))
        .rounded(px(6.))
        .border_px(1.5)
        .cursor_pointer()
        .bg(if on { t.accent } else { t.panel })
        .border_color(if on { t.accent } else { t.line_strong });
    if indeterminate {
        b = b.child(div().w(px(9.)).h(px(2.5)).bg(crate::theme::hex(0xffffff)));
    } else if checked {
        b = b.child(icon_stroke(Lucide::Check, 3.6).size(px(11.)).text_color(crate::theme::hex(0xffffff)));
    }
    b
}

/// A colour for text inside fields that are read-only or placeholders.
pub fn placeholder_color(t: &Theme) -> Hsla {
    t.fg4
}
