//! `.tbl` (index.css): Billable's tables. GPUI has no tables, so this lays
//! columns out the way Chromium's automatic table layout does for a 100%-wide
//! table: each column starts at its widest content (max-content) and spare
//! width is shared out in proportion to those widths; fixed-width columns keep
//! their width. Pages pass each cell's content width, measured with
//! [`measure`].

use gpui::{
    div, px, AnyElement, App, Div, ElementId, Font, Hsla, InteractiveElement, IntoElement, ParentElement, Pixels,
    SharedString, StatefulInteractiveElement, Styled, TextRun, Window,
};

use crate::{
    fonts::rounded_family,
    style::{num_font, r},
    theme::{theme, Theme},
};

/// The width of a line of text in the rounded face.
pub fn measure(window: &Window, s: &str, size: f32, weight: u16, tabular: bool) -> f32 {
    if s.is_empty() {
        return 0.0;
    }
    let font: Font = if tabular { num_font(weight) } else { gpui::font(rounded_family(weight)) };
    let run = TextRun { len: s.len(), font, color: gpui::black(), background_color: None, underline: None, strikethrough: None };
    let line = window.text_system().shape_line(SharedString::from(s.to_string()), px(size), &[run], None);
    f32::from(line.width)
}

#[derive(Clone, Copy, PartialEq)]
pub enum Align {
    Left,
    Right,
    Center,
}

pub struct Column {
    pub header: SharedString,
    pub align: Align,
    /// A fixed width (`w-10`), or `None` to size from content.
    pub fixed: Option<f32>,
}

impl Column {
    pub fn new(header: impl Into<SharedString>) -> Self {
        Column { header: header.into(), align: Align::Left, fixed: None }
    }
    pub fn right(mut self) -> Self {
        self.align = Align::Right;
        self
    }
    pub fn center(mut self) -> Self {
        self.align = Align::Center;
        self
    }
    pub fn fixed(mut self, w: f32) -> Self {
        self.fixed = Some(w);
        self
    }
}

pub struct Cell {
    pub content: AnyElement,
    /// Max-content width of the content (without the cell's padding).
    pub width: f32,
}

impl Cell {
    pub fn new(content: impl IntoElement, width: f32) -> Self {
        Cell { content: content.into_any_element(), width }
    }
}

pub struct Row {
    pub id: ElementId,
    pub cells: Vec<Cell>,
    pub on_click: Option<Box<dyn Fn(&gpui::ClickEvent, &mut Window, &mut App)>>,
    pub height: f32,
    pub background: Option<Hsla>,
    /// A group name for `group_hover` styles inside the row.
    pub group: Option<SharedString>,
}

impl Row {
    pub fn new(id: impl Into<ElementId>, cells: Vec<Cell>) -> Self {
        Row { id: id.into(), cells, on_click: None, height: 50.0, background: None, group: None }
    }
    pub fn on_click(mut self, f: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Box::new(f));
        self
    }
    /// `.tbl-compact` rows are 40px.
    pub fn height(mut self, h: f32) -> Self {
        self.height = h;
        self
    }
    pub fn background(mut self, c: Hsla) -> Self {
        self.background = Some(c);
        self
    }
    pub fn group(mut self, g: impl Into<SharedString>) -> Self {
        self.group = Some(g.into());
        self
    }
}

/// Cell padding: px-3, with pl-4 on the first column and pr-4 on the last.
fn pads(i: usize, n: usize) -> (f32, f32) {
    let x = 9.75;
    (if i == 0 { 13.0 } else { x }, if i + 1 == n { 13.0 } else { x })
}

/// Column widths for a table `total` wide (CSS automatic table layout).
pub fn layout(columns: &[Column], header_widths: &[f32], rows: &[Row], total: f32) -> Vec<f32> {
    let n = columns.len();
    let mut max = vec![0.0f32; n];
    for (i, c) in columns.iter().enumerate() {
        let (l, rr) = pads(i, n);
        max[i] = header_widths.get(i).copied().unwrap_or(0.0) + l + rr;
        if let Some(f) = c.fixed {
            max[i] = max[i].max(f);
        }
    }
    for row in rows {
        for (i, cell) in row.cells.iter().enumerate().take(n) {
            let (l, rr) = pads(i, n);
            max[i] = max[i].max(cell.width + l + rr);
        }
    }
    let fixed: f32 = columns.iter().enumerate().filter(|(_, c)| c.fixed.is_some()).map(|(i, _)| max[i]).sum();
    let auto: f32 = columns.iter().enumerate().filter(|(_, c)| c.fixed.is_none()).map(|(i, _)| max[i]).sum();
    let spare = total - fixed - auto;
    columns
        .iter()
        .enumerate()
        .map(|(i, c)| {
            if c.fixed.is_some() || auto <= 0.0 {
                max[i]
            } else if spare > 0.0 {
                max[i] + spare * max[i] / auto
            } else {
                // Narrower than the content: shrink in proportion (cells truncate).
                (max[i] * (total - fixed) / auto).max(0.0)
            }
        })
        .collect()
}

/// Draw a table filling `total` pixels of width.
pub fn table(t: &Theme, window: &Window, id: &str, columns: Vec<Column>, rows: Vec<Row>, total: f32, empty: Option<AnyElement>) -> Div {
    let header_widths: Vec<f32> = columns.iter().map(|c| measure(window, &c.header, 12.0, 600, false)).collect();
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
    for (i, c) in columns.iter().enumerate() {
        head = head.child(
            cell(i, c.align, 35.5).child(div().font_family(rounded_family(600)).text_size(px(12.)).text_color(t.fg3).whitespace_nowrap().child(c.header.clone())),
        );
    }
    let mut body = div().flex().flex_col();
    let count = rows.len();
    let hover = t.fg.opacity(0.03);
    for (ri, row) in rows.into_iter().enumerate() {
        let last = ri + 1 == count;
        let height = if last { row.height } else { row.height - 1.0 };
        let mut el = div().id(row.id).flex();
        if !last {
            el = el.border_b_1().border_color(t.line);
        }
        if let Some(bg) = row.background {
            el = el.bg(bg);
        }
        if let Some(g) = row.group {
            el = el.group(g);
        }
        if let Some(f) = row.on_click {
            el = el.cursor_pointer().hover(move |s| s.bg(hover)).on_click(f);
        }
        for (i, c) in row.cells.into_iter().enumerate().take(n) {
            el = el.child(cell(i, columns[i].align, height).child(c.content));
        }
        body = body.child(el);
    }
    if count == 0 {
        if let Some(e) = empty {
            body = body.child(e);
        }
    }
    div().w(px(total)).text_size(px(14.)).child(head).child(body)
}

/// A cell's text: `size`/`weight`/colour, truncating, and its measured width.
pub fn text_cell(window: &Window, s: impl Into<SharedString>, size: f32, weight: u16, color: Hsla, tabular: bool) -> Cell {
    let s: SharedString = s.into();
    let w = measure(window, &s, size, weight, tabular);
    let mut d = div().text_size(px(size)).text_color(color).whitespace_nowrap().truncate();
    d = if tabular { d.font(num_font(weight)) } else { d.font_family(rounded_family(weight)) };
    Cell::new(d.child(s), w)
}

/// The "—" shown for empty money columns.
pub fn dash_cell(window: &Window, t: &Theme) -> Cell {
    text_cell(window, "—", 14.0, 500, t.fg4, true)
}

#[allow(unused)]
fn _unused(cx: &App) {
    let _ = (theme(cx), r(1.));
}
