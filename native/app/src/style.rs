//! Small building blocks shared by every screen: Billable's spacing unit,
//! text in the rounded face, figures, icons and the common surfaces.

use std::sync::Arc;

use gpui::{
    div, px, relative, svg, Div, Font, FontFeatures, FontStyle, FontWeight, Hsla, IntoElement, ParentElement, Pixels,
    SharedString, Styled, Svg,
};

use crate::{fonts::rounded_family, icons::Lucide, theme::Theme};

/// Tailwind spacing units (`px-4`, `gap-3`, `w-9`) are 0.25rem, and Billable
/// sets the root font size to 13px, so one unit is 3.25px, not 4px.
pub fn r(units: f32) -> Pixels {
    px(units * 3.25)
}

/// Text in rounded SF at a CSS weight (500 body, 600 semibold, 700 bold).
pub fn text(s: impl Into<SharedString>, size: f32, weight: u16, color: Hsla) -> Div {
    div().font_family(rounded_family(weight)).text_size(px(size)).text_color(color).child(s.into())
}

/// Apply rounded SF at a weight to an element's text.
pub trait Weighted: Styled + Sized {
    fn weight(self, weight: u16) -> Self {
        self.font_family(rounded_family(weight))
    }
    /// `leading-[x]` / line height in px.
    fn leading(self, line_height: f32) -> Self {
        self.line_height(px(line_height))
    }
}
impl<T: Styled + Sized> Weighted for T {}

/// `.font-figures` / `.font-display`: bold rounded numerals, all one width.
pub fn figures_font() -> Font {
    Font {
        family: crate::fonts::tracked_family(700, -0.015).into(),
        features: FontFeatures(Arc::new(vec![("tnum".into(), 1)])),
        fallbacks: None,
        weight: FontWeight::BOLD,
        style: FontStyle::Normal,
    }
}

/// Tabular numerals at another weight (`.num`).
pub fn num_font(weight: u16) -> Font {
    Font { family: rounded_family(weight).into(), weight: FontWeight(weight as f32), ..figures_font() }
}

/// `.font-figures` text: bold tabular numerals with the -0.015em tracking.
pub fn figures(s: impl Into<SharedString>, size: f32, color: Hsla) -> Div {
    div().font(figures_font()).text_size(px(size)).text_color(color).child(s.into())
}

/// Text with CSS `letter-spacing` (in em) at a weight: uses a font copy with
/// the spacing built into its advances (see `fonts::TRACKED`).
pub fn tracked(s: impl Into<SharedString>, size: f32, em: f32) -> Div {
    div().font_family(crate::fonts::tracked_family(700, em)).text_size(px(size)).whitespace_nowrap().child(s.into())
}

/// `text()` with letter-spacing.
pub fn text_tracked(s: impl Into<SharedString>, size: f32, weight: u16, color: Hsla, em: f32) -> Div {
    div().font_family(crate::fonts::tracked_family(weight, em)).text_size(px(size)).text_color(color).child(s.into())
}

/// A Lucide icon, tinted by `text_color`. Lucide's default stroke is 2.
pub fn icon(i: Lucide) -> Svg {
    svg().path(format!("billable/{}.svg", i.name())).flex_none()
}

/// A Lucide icon with another stroke width (`strokeWidth={3}`).
pub fn icon_stroke(i: Lucide, stroke: f32) -> Svg {
    svg().path(format!("billable/{}@{}.svg", i.name(), stroke)).flex_none()
}

/// A Lucide icon filled with its colour (`fill-current`).
pub fn icon_filled(i: Lucide) -> Svg {
    svg().path(format!("billable/{}@fill.svg", i.name())).flex_none()
}

/// `.card`: panel, hairline border, 16px corners, soft shadow.
pub fn card(t: &Theme) -> Div {
    div().bg(t.panel).border_1().border_color(t.line).rounded(px(16.)).shadow(t.shadow_card())
}

/// `.list-row::after`: a separator set in from the left (put it in a
/// `relative` row; leave it off the last row).
pub fn separator(t: &Theme, inset: f32) -> Div {
    div().absolute().bottom_0().right_0().left(px(inset)).h(px(1.)).bg(t.line)
}

/// `.section-title` (tracking -0.01em)
pub fn section_title(t: &Theme, s: impl Into<SharedString>) -> Div {
    text_tracked(s, 16.0, 700, t.fg, -0.01)
}

/// `.group-head`: a section's title row.
pub fn group_head(left: impl IntoElement, right: impl IntoElement) -> Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(r(3.))
        .min_h(px(26.))
        .mb(r(2.5))
        .px(r(0.5))
        .child(left)
        .child(right)
}

/// `.label`: a form label.
pub fn label(t: &Theme, s: impl Into<SharedString>) -> Div {
    text(s, 13.0, 600, t.fg2).mb(r(1.5)).line_height(px(19.5))
}

/// `.hint`: small grey help text under a field.
pub fn hint(t: &Theme, s: impl Into<SharedString>) -> Div {
    text(s, 12.0, 500, t.fg3).mt(r(1.5)).line_height(px(16.))
}

/// `.kbd`
pub fn kbd(t: &Theme, s: impl Into<SharedString>) -> Div {
    div()
        .flex()
        .items_center()
        .h(px(18.))
        .px(r(1.5))
        .rounded(px(6.))
        .bg(t.fg.opacity(0.07))
        .child(text(s, 11.0, 600, t.fg3).line_height(px(18.)))
}

/// The root text style: 13px rounded SF, medium, 1.5 line height.
pub fn root_text(d: Div, t: &Theme) -> Div {
    d.font_family(rounded_family(500)).text_size(px(13.)).line_height(relative(1.5)).text_color(t.fg)
}

/// A small coloured dot (project colours, status dots).
pub fn dot(color: Hsla, size: Pixels) -> Div {
    div().size(size).flex_none().rounded_full().bg(color)
}

thread_local! {
    /// Whether the page currently reserves its scrollbar's 11px (set by the shell).
    pub static PAGE_GUTTER: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
}

/// The width of a page's content column (`.page`: max 1160px, px-7), from the
/// window width: the sidebar is 228px and the page's scrollbar 11px.
pub fn page_width(window: &gpui::Window) -> f32 {
    let gutter = if PAGE_GUTTER.with(|g| g.get()) { 11.0 } else { 0.0 };
    let main = f32::from(window.viewport_size().width) - 228.0 - gutter;
    main.min(1160.0) - 2.0 * 22.75
}
