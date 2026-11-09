//! Small pieces of the Commissions screen: the job type tag, the status
//! pill, button widths for table sizing, and a `.tbl` that (unlike
//! `components::table`) takes custom header cells, double-clicks and quiet
//! row hovers.

use gpui::{
    div, px, AnyElement, App, ClickEvent, Div, Hsla, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, Window,
};

use crate::{
    components::table::{layout, measure, Align, Column, Row},
    fonts::rounded_family,
    icons::Lucide,
    style::{icon, r, text},
    theme::Theme,
};

/// JobTypeTag: a sun or a house, then "Solar"/"Roofing".
pub fn job_type_tag(t: &Theme, job_type: &str) -> Div {
    let solar = job_type == "solar";
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(r(1.5))
        .child(icon(if solar { Lucide::Sun } else { Lucide::Home }).size(r(3.5)).text_color(if solar { t.amber } else { t.blue }))
        .child(text(if solar { "Solar" } else { "Roofing" }, 13.0, 500, t.fg2).line_height(px(18.)).whitespace_nowrap())
}

pub fn job_type_tag_width(window: &Window, job_type: &str) -> f32 {
    f32::from(r(3.5)) + f32::from(r(1.5)) + measure(window, if job_type == "solar" { "Solar" } else { "Roofing" }, 13.0, 500, false)
}

/// `.badge text-*`: the colour's text on a 13% wash of itself.
pub fn pill(color: Hsla, label: impl Into<SharedString>) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .h(px(22.))
        .px(r(2.5))
        .rounded_full()
        .bg(color.opacity(0.13))
        .child(text(label, 12.0, 600, color).whitespace_nowrap().line_height(px(16.)))
}

pub fn pill_width(window: &Window, label: &str) -> f32 {
    measure(window, label, 12.0, 600, false) + 2.0 * f32::from(r(2.5))
}

/// The width of a `.btn-sm` with this label (and an icon).
pub fn small_button_width(window: &Window, label: &str, with_icon: bool) -> f32 {
    let mut w = measure(window, label, 12.0, 600, false) + 2.0 * f32::from(r(3.));
    if with_icon {
        w += 13.0 + f32::from(r(1.5));
    }
    w
}

type Handler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// How a row looks and reacts beyond its cells.
#[derive(Default)]
pub struct RowLook {
    pub background: Option<Hsla>,
    pub hover: Option<Hsla>,
    pub pointer: bool,
    pub group: Option<SharedString>,
    pub on_click: Option<Handler>,
}

/// A header cell: custom content and its measured width.
pub fn header_text(t: &Theme, window: &Window, s: &str) -> (AnyElement, f32) {
    (
        div().font_family(rounded_family(600)).text_size(px(12.)).text_color(t.fg3).whitespace_nowrap().child(s.to_string()).into_any_element(),
        measure(window, s, 12.0, 600, false),
    )
}

/// Cell padding: px-3, with pl-4 on the first column and pr-4 on the last.
fn pads(i: usize, n: usize) -> (f32, f32) {
    let x = 9.75;
    (if i == 0 { 13.0 } else { x }, if i + 1 == n { 13.0 } else { x })
}

/// A `.tbl` filling `total` pixels, with Chromium's automatic column widths.
pub fn grid(t: &Theme, columns: Vec<Column>, headers: Vec<(AnyElement, f32)>, rows: Vec<(Row, RowLook)>, total: f32) -> Div {
    let header_widths: Vec<f32> = headers.iter().map(|h| h.1).collect();
    let (rows, looks): (Vec<Row>, Vec<RowLook>) = rows.into_iter().unzip();
    let widths = layout(&columns, &header_widths, &rows, total);
    let n = columns.len();
    let cell = |i: usize, align: Align, height: f32| {
        let (l, rr) = pads(i, n);
        let d = div().w(px(widths[i])).flex_none().h(px(height)).pl(px(l)).pr(px(rr)).flex().items_center().min_w_0().overflow_hidden();
        match align {
            Align::Left => d,
            Align::Right => d.justify_end(),
            Align::Center => d.justify_center(),
        }
    };
    let mut head = div().flex().border_b_1().border_color(t.line);
    for (i, (h, _)) in headers.into_iter().enumerate() {
        head = head.child(cell(i, columns[i].align, 35.0).child(h));
    }
    let mut body = div().flex().flex_col();
    let count = rows.len();
    for (ri, (row, look)) in rows.into_iter().zip(looks).enumerate() {
        let last = ri + 1 == count;
        let height = if last { row.height } else { row.height - 1.0 };
        let mut el = div().id(row.id).flex();
        if !last {
            el = el.border_b_1().border_color(t.line);
        }
        if let Some(bg) = look.background {
            el = el.bg(bg);
        }
        if let Some(h) = look.hover {
            el = el.hover(move |s| s.bg(h));
        }
        if look.pointer {
            el = el.cursor_pointer();
        }
        if let Some(g) = look.group {
            el = el.group(g);
        }
        if let Some(f) = look.on_click {
            el = el.on_click(f);
        }
        for (i, c) in row.cells.into_iter().enumerate().take(n) {
            el = el.child(cell(i, columns[i].align, height).child(c.content));
        }
        body = body.child(el);
    }
    div().w(px(total)).text_size(px(14.)).child(head).child(body)
}
