//! recharts' `<BarChart>` as Reports.tsx and TaxOverview.tsx use it, drawn
//! with plain elements: a `CartesianGrid` without vertical lines, a category
//! X axis and a number Y axis (no axis or tick lines, 11px fg-4 labels), the
//! `fill: fg/4%` cursor band, bars with 3px top corners, an optional circle
//! legend, and the custom tooltip that glides after the pointer.
//!
//! The geometry follows recharts 2.15 step by step: the plot offset (margin,
//! 56px Y axis, 30px X axis, measured legend), the band scale with
//! `barCategoryGap="10%"` and `barGap`, `maxBarSize`, the "nice" Y ticks from
//! recharts-scale, `interval="preserveEnd"` tick thinning, the tooltip's
//! `translate` that flips at the plot's edges, and the bars' 400ms `ease`
//! animation (grow from the baseline, or move from the last shape when the
//! data or the width changes).

use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};

use gpui::{
    canvas, div, prelude::*, px, AnyElement, Bounds, Context, Div, Hsla, IntoElement, MouseMoveEvent, ParentElement,
    Pixels, Render, SharedString, Styled, Window,
};

use crate::{
    components::table::measure,
    fonts::rounded_family,
    format::{money, money_compact},
    motion::cubic_bezier,
    style::{dot, num_font, page_width, r, text},
    theme::{theme, Theme},
};

/// One `<Bar>`.
pub struct Series {
    /// `name` (legend and tooltip).
    pub name: SharedString,
    /// `fill`, a theme colour.
    pub color: fn(&Theme) -> Hsla,
    /// The tooltip's dot (Reports' `SERIES_COLOR`, fixed hex values).
    pub dot: Hsla,
    pub max_bar_size: f32,
}

/// The two custom tooltip contents.
pub enum Tip {
    /// Reports: the label, then a dot, name and amount for each bar.
    Series,
    /// Tax overview: "{label} {suffix}" over the first bar's amount.
    Single { suffix: SharedString },
}

#[derive(Clone, Copy, PartialEq, Debug)]
struct Rect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

#[derive(Clone, Copy)]
struct Plot {
    left: f32,
    top: f32,
    width: f32,
    height: f32,
    band: f32,
    n: usize,
}

/// The tooltip's `transition: transform 400ms ease`.
#[derive(Clone, Copy)]
struct Glide {
    from: (f32, f32),
    to: (f32, f32),
    start: Instant,
}

const DURATION: f32 = 400.0;

pub struct BarChart {
    height: f32,
    /// top, right, bottom, left
    margin: [f32; 4],
    bar_gap: f32,
    legend: bool,
    /// The page width minus this is the chart's width (card border + padding).
    inset: f32,
    series: Vec<Series>,
    tip: Tip,
    labels: Vec<SharedString>,
    /// `[category][series]`
    values: Vec<Vec<f64>>,
    target: Option<Vec<Vec<Rect>>>,
    prev: Option<Vec<Vec<Rect>>>,
    anim: Instant,
    /// Active category and the pointer's y, in chart coordinates.
    hover: Option<(usize, f32)>,
    glide: Option<Glide>,
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    plot: Option<Plot>,
}

impl BarChart {
    pub fn new(height: f32, margin: [f32; 4], bar_gap: f32, legend: bool, inset: f32, series: Vec<Series>, tip: Tip) -> Self {
        BarChart {
            height,
            margin,
            bar_gap,
            legend,
            inset,
            series,
            tip,
            labels: vec![],
            values: vec![],
            target: None,
            prev: None,
            anim: Instant::now(),
            hover: None,
            glide: None,
            bounds: Rc::new(Cell::new(None)),
            plot: None,
        }
    }

    /// New data; the bars move to it on the next frame (the owner re-renders).
    pub fn set_data(&mut self, labels: Vec<SharedString>, values: Vec<Vec<f64>>) {
        self.labels = labels;
        self.values = values;
        if self.hover.is_some_and(|(i, _)| i >= self.labels.len()) {
            self.hover = None;
        }
    }

    pub fn set_tip(&mut self, tip: Tip) {
        self.tip = tip;
    }

    fn on_move(&mut self, e: &MouseMoveEvent, cx: &mut Context<Self>) {
        let (Some(b), Some(p)) = (self.bounds.get(), self.plot) else { return };
        let x = f32::from(e.position.x - b.origin.x);
        let y = f32::from(e.position.y - b.origin.y);
        let inside = x >= p.left && x <= p.left + p.width && y >= p.top && y <= p.top + p.height;
        let next = if inside && p.n > 0 {
            let centers: Vec<f32> = (0..p.n).map(|i| p.left + p.band * (i as f32 + 0.5)).collect();
            Some((active_index(x, &centers), y))
        } else {
            None
        };
        if next != self.hover {
            self.hover = next;
            cx.notify();
        }
    }
}

/// recharts' `calculateActiveTickIndex` for ticks in one direction.
fn active_index(x: f32, c: &[f32]) -> usize {
    let len = c.len();
    if len <= 1 {
        return 0;
    }
    for i in 0..len {
        let hit = (i == 0 && x <= (c[0] + c[1]) / 2.0)
            || (i > 0 && i < len - 1 && x > (c[i] + c[i - 1]) / 2.0 && x <= (c[i] + c[i + 1]) / 2.0)
            || (i == len - 1 && x > (c[i] + c[i - 1]) / 2.0);
        if hit {
            return i;
        }
    }
    0
}

// ---------- recharts-scale's getNiceTickValues ----------

/// `ceil`, forgiving the float noise Decimal.js doesn't have.
fn ceil_eps(x: f64) -> f64 {
    if (x - x.round()).abs() < 1e-9 {
        x.round()
    } else {
        x.ceil()
    }
}

fn digit_count(v: f64) -> i32 {
    if v == 0.0 {
        return 1;
    }
    let a = v.abs();
    let mut d = a.log10().floor() as i32;
    if 10f64.powi(d) > a {
        d -= 1;
    } else if 10f64.powi(d + 1) <= a {
        d += 1;
    }
    d + 1
}

fn format_step(rough: f64, correction: i32) -> f64 {
    if rough <= 0.0 {
        return 0.0;
    }
    let dc = digit_count(rough);
    let dcv = 10f64.powi(dc);
    // stepRatioScale is 0.05, or 0.1 for one-digit steps
    let per = if dc != 1 { 20.0 } else { 10.0 };
    let q = ceil_eps(rough * per / dcv);
    (q + correction as f64) * dcv / per
}

fn calculate_step(min: f64, max: f64, count: i32, correction: i32) -> (f64, f64, f64) {
    let step = format_step((max - min) / (count - 1) as f64, correction);
    if step <= 0.0 || !step.is_finite() {
        return (0.0, 0.0, 0.0);
    }
    let middle = if min <= 0.0 && max >= 0.0 {
        0.0
    } else {
        let m = (min + max) / 2.0;
        m - m % step
    };
    let mut below = ceil_eps((middle - min) / step) as i32;
    let mut up = ceil_eps((max - middle) / step) as i32;
    let scale_count = below + up + 1;
    if scale_count > count {
        return calculate_step(min, max, count, correction + 1);
    }
    if scale_count < count {
        if max > 0.0 {
            up += count - scale_count;
        } else {
            below += count - scale_count;
        }
    }
    (step, middle - below as f64 * step, middle + up as f64 * step)
}

/// The Y axis ticks for `domain=[0, 'auto']`, `tickCount=5`, decimals allowed.
fn nice_ticks(min: f64, max: f64) -> Vec<f64> {
    let count = 5;
    if min == max {
        // getTickOfSingleValue
        let (middle, step) = if min.fract() != 0.0 {
            if min.abs() < 1.0 {
                let step = 10f64.powi(digit_count(min) - 1);
                ((min / step).floor() * step, step)
            } else {
                (min.floor(), 1.0)
            }
        } else if min == 0.0 {
            (((count - 1) / 2) as f64, 1.0)
        } else {
            (min, 1.0)
        };
        let mid = (count - 1) / 2;
        return (0..count).map(|n| middle + (n - mid) as f64 * step).collect();
    }
    let (step, lo, hi) = calculate_step(min, max, count, 0);
    if step == 0.0 {
        return vec![0.0];
    }
    let end = hi + 0.1 * step;
    let mut out = Vec::new();
    let mut i = 0;
    loop {
        let v = lo + i as f64 * step;
        if v >= end || i > 1000 {
            break;
        }
        out.push(v);
        i += 1;
    }
    out
}

/// `getTicksEnd` (interval "preserveEnd"): which ticks fit, and where.
/// `ticks` are (coordinate, size); returns the drawn coordinate of each shown tick.
fn ticks_end(sign: f32, start: f32, end: f32, ticks: &[(f32, f32)], min_gap: f32) -> Vec<Option<f32>> {
    let visible = |pos: f32, size: f32, start: f32, end: f32| {
        if sign * pos < sign * start || sign * pos > sign * end {
            return false;
        }
        sign * (pos - sign * size / 2.0 - start) >= 0.0 && sign * (pos + sign * size / 2.0 - end) <= 0.0
    };
    let mut out = vec![None; ticks.len()];
    let mut end = end;
    for i in (0..ticks.len()).rev() {
        let (coord, size) = ticks[i];
        let tick = if i == ticks.len() - 1 {
            let gap = sign * (coord + sign * size / 2.0 - end);
            if gap > 0.0 { coord - gap * sign } else { coord }
        } else {
            coord
        };
        if visible(tick, size, start, end) {
            end = tick - sign * (size / 2.0 + min_gap);
            out[i] = Some(tick);
        }
    }
    out
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn same(a: &[Vec<Rect>], b: &[Vec<Rect>]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| {
            x.len() == y.len()
                && x.iter().zip(y).all(|(p, q)| (p.x - q.x).abs() < 0.01 && (p.y - q.y).abs() < 0.01 && (p.w - q.w).abs() < 0.01 && (p.h - q.h).abs() < 0.01)
        })
}

impl Render for BarChart {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(cx);
        let now = Instant::now();
        let ease = cubic_bezier(0.25, 0.1, 0.25, 1.0);
        let width = (page_width(window) - self.inset).max(0.0);
        let height = self.height;
        let [m_top, m_right, m_bottom, m_left] = self.margin;

        // Font metrics: SVG text sits on its baseline; GPUI places a line box.
        let ts = window.text_system();
        let fid = ts.resolve_font(&gpui::font(rounded_family(500)));
        // Where a line box puts its baseline, as `paint_line` does. (GPUI's
        // `baseline_offset` adds the font's descent with the wrong sign: the
        // font metrics store it negative.)
        let baseline = |size: f32, lh: f32| {
            let ascent = f32::from(ts.ascent(fid, px(size)));
            let descent = f32::from(ts.descent(fid, px(size))).abs();
            (lh - ascent - descent) / 2.0 + ascent
        };
        let tick_lh = 16.5;
        let tick_base = baseline(11.0, tick_lh);
        let tick_height = f32::from(ts.ascent(fid, px(11.))).round() + f32::from(ts.descent(fid, px(11.))).abs().round();
        let legend_lh = 18.0;
        let legend_base = baseline(12.0, legend_lh);
        let x_height = f32::from(ts.x_height(fid, px(12.)));

        // The plot area: margin, the 56px Y axis, the 30px X axis, the legend
        let legend_h = if self.legend { legend_lh } else { 0.0 };
        let left = m_left + 56.0;
        let top = m_top;
        let plot_w = (width - left - m_right).max(0.0);
        let plot_h = (height - top - (m_bottom + 30.0 + legend_h)).max(0.0);
        let n = self.labels.len();
        let band = if n > 0 { plot_w / n as f32 } else { 0.0 };
        let plot = Plot { left, top, width: plot_w, height: plot_h, band, n };
        self.plot = Some(plot);

        // Y scale: [0, auto] made nice
        let max = self.values.iter().flatten().copied().fold(0.0, f64::max);
        let ticks = nice_ticks(0.0, max);
        let (t_lo, t_hi) = (ticks.first().copied().unwrap_or(0.0), ticks.last().copied().unwrap_or(1.0));
        let span = if t_hi - t_lo == 0.0 { 1.0 } else { t_hi - t_lo };
        let y_of = |v: f64| top + plot_h - ((v - t_lo) / span) as f32 * plot_h;

        // Bars: barCategoryGap 10%, barGap, maxBarSize
        let len = self.series.len().max(1) as f32;
        let offset = band * 0.1;
        let mut gap = self.bar_gap.min(band);
        if band - 2.0 * offset - (len - 1.0) * gap <= 0.0 {
            gap = 0.0;
        }
        let mut original = (band - 2.0 * offset - (len - 1.0) * gap) / len;
        if original > 1.0 {
            original = original.trunc();
        }
        let base = y_of(0.0f64.clamp(t_lo, t_hi));
        let rects: Vec<Vec<Rect>> = (0..n)
            .map(|i| {
                self.series
                    .iter()
                    .enumerate()
                    .map(|(s, series)| {
                        let size = original.min(series.max_bar_size);
                        let x = left + band * i as f32 + offset + (original + gap) * s as f32 + (original - size) / 2.0;
                        let v = self.values.get(i).and_then(|r| r.get(s)).copied().unwrap_or(0.0);
                        let y = y_of(v);
                        Rect { x, y, w: size, h: base - y }
                    })
                    .collect()
            })
            .collect();
        match &self.target {
            None => {
                self.prev = None;
                self.anim = now;
            }
            Some(old) if !same(old, &rects) => {
                self.prev = Some(old.clone());
                self.anim = now;
            }
            _ => {}
        }
        self.target = Some(rects.clone());
        let progress = ((now - self.anim).as_secs_f32() * 1000.0 / DURATION).min(1.0);
        let p = if progress >= 1.0 { 1.0 } else { ease(progress) };
        if progress < 1.0 {
            window.request_animation_frame();
        }

        let mut chart = div().id("bar-chart").relative().w(px(width)).h(px(height)).flex_none();
        let cell = self.bounds.clone();
        chart = chart.child(canvas(move |b, _, _| cell.set(Some(b)), |_, _, _, _| {}).absolute().top_0().left_0().size_full());

        // CartesianGrid: a line at every Y tick, plus the plot's top and bottom
        let mut grid_ys: Vec<f32> = ticks.iter().map(|v| y_of(*v)).collect();
        if !grid_ys.contains(&top) {
            grid_ys.push(top);
        }
        if !grid_ys.contains(&(top + plot_h)) {
            grid_ys.push(top + plot_h);
        }
        for y in grid_ys {
            chart = chart.child(div().absolute().left(px(left)).top(px(y - 0.5)).w(px(plot_w)).h(px(1.)).bg(t.line));
        }

        // X axis labels at band centres, thinned by preserveEnd
        let label = |s: SharedString| text(s, 11.0, 500, t.fg4).line_height(px(tick_lh)).whitespace_nowrap();
        let x_sizes: Vec<(f32, f32)> =
            self.labels.iter().enumerate().map(|(i, l)| (left + band * (i as f32 + 0.5), measure(window, l, 11.0, 500, false))).collect();
        let x_shown = ticks_end(1.0, 0.0, width, &x_sizes, 5.0);
        let x_base = top + plot_h + 6.0 + 2.0 + 0.71 * 11.0;
        for (i, at) in x_shown.iter().enumerate() {
            let Some(at) = at else { continue };
            // text-anchor: middle
            chart = chart.child(
                div().absolute().left(px(at - 100.0)).top(px(x_base - tick_base)).w(px(200.)).flex().justify_center().child(label(self.labels[i].clone())),
            );
        }

        // Y axis labels: text-anchor end, 8px left of the plot (tickSize 6 + tickMargin 2)
        let y_sizes: Vec<(f32, f32)> = ticks.iter().map(|v| (y_of(*v), tick_height)).collect();
        let sign = if y_sizes.len() >= 2 && y_sizes[1].0 < y_sizes[0].0 { -1.0 } else { 1.0 };
        let (start, end) = if sign > 0.0 { (0.0, height) } else { (height, 0.0) };
        let y_shown = ticks_end(sign, start, end, &y_sizes, 5.0);
        for (i, at) in y_shown.iter().enumerate() {
            let Some(at) = at else { continue };
            chart = chart.child(
                div()
                    .absolute()
                    .right(px(width - (left - 8.0)))
                    .top(px(at + 0.355 * 11.0 - tick_base))
                    .flex()
                    .justify_end()
                    .child(label(money_compact(ticks[i]).into())),
            );
        }

        // The cursor band under the active category
        if let Some((i, _)) = self.hover.filter(|(i, _)| *i < n) {
            let cx_ = left + band * (i as f32 + 0.5);
            chart = chart.child(div().absolute().left(px(cx_ - band / 2.0)).top(px(top + 0.5)).w(px(band)).h(px((plot_h - 1.0).max(0.0))).bg(t.fg.opacity(0.04)));
        }

        // Bars
        for (s, series) in self.series.iter().enumerate() {
            let color = (series.color)(&t);
            for (i, row) in rects.iter().enumerate() {
                let target = row[s];
                let prev = self.prev.as_ref().and_then(|p| p.get(i)).and_then(|r| r.get(s)).copied();
                let r_ = match prev {
                    Some(q) if p < 1.0 => Rect { x: lerp(q.x, target.x, p), y: lerp(q.y, target.y, p), w: lerp(q.w, target.w, p), h: lerp(q.h, target.h, p) },
                    None if p < 1.0 => {
                        let h = target.h * p;
                        Rect { y: target.y + target.h - h, h, ..target }
                    }
                    _ => target,
                };
                if r_.w <= 0.0 || r_.h <= 0.0 {
                    continue;
                }
                let radius = 3.0f32.min(r_.w / 2.0).min(r_.h / 2.0);
                chart = chart.child(
                    div().absolute().left(px(r_.x)).top(px(r_.y)).w(px(r_.w)).h(px(r_.h)).bg(color).rounded_tl(px(radius)).rounded_tr(px(radius)),
                );
            }
        }

        // Legend: circles and names in the bars' colours, centred under the plot
        if self.legend {
            let items: Vec<(Hsla, SharedString, f32)> =
                self.series.iter().map(|s| ((s.color)(&t), s.name.clone(), measure(window, &s.name, 12.0, 500, false))).collect();
            let total: f32 = items.iter().map(|(_, _, w)| 8.0 + 4.0 + w + 10.0).sum();
            let legend_w = width - m_left - m_right;
            let mut row = div()
                .absolute()
                .left(px(m_left + ((legend_w - total) / 2.0).max(0.0)))
                .top(px(height - m_bottom - legend_lh))
                .h(px(legend_lh))
                .flex()
                .items_start();
            let circle_top = legend_base - x_height / 2.0 - 4.0;
            for (color, name, _) in items {
                row = row
                    .child(div().mt(px(circle_top)).mr(px(4.)).child(dot(color, px(8.))))
                    .child(text(name, 12.0, 500, color).line_height(px(legend_lh)).whitespace_nowrap().flex_none().mr(px(10.)));
            }
            chart = chart.child(row);
        }

        // Tooltip
        if let Some((i, mouse_y)) = self.hover.filter(|(i, _)| *i < n) {
            let (content, w, h) = self.tooltip(window, &t, i);
            let coord = (left + band * (i as f32 + 0.5), mouse_y);
            let place = |c: f32, dim: f32, lo: f32, extent: f32| {
                let positive = c + 10.0;
                let negative = c - dim - 10.0;
                if positive + dim > lo + extent { negative.max(lo) } else { positive.max(lo) }
            };
            let target = (place(coord.0, w, left, plot_w), place(coord.1, h, top, plot_h));
            let glide = match self.glide {
                // The first time, it slides in from the chart's corner, as recharts' does
                None => Glide { from: (0.0, 0.0), to: target, start: now },
                Some(g) if g.to != target => Glide { from: glide_at(&g, now, &ease), to: target, start: now },
                Some(g) => g,
            };
            self.glide = Some(glide);
            let (x, y) = glide_at(&glide, now, &ease);
            if (now - glide.start) < Duration::from_millis(DURATION as u64) {
                window.request_animation_frame();
            }
            chart = chart.child(div().absolute().left(px(x)).top(px(y)).w(px(w)).child(content));
        }

        chart
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| this.on_move(e, cx)))
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                if !*hovered && this.hover.is_some() {
                    this.hover = None;
                    cx.notify();
                }
            }))
    }
}

fn glide_at(g: &Glide, now: Instant, ease: &impl Fn(f32) -> f32) -> (f32, f32) {
    let k = ((now - g.start).as_secs_f32() * 1000.0 / DURATION).min(1.0);
    let e = if k >= 1.0 { 1.0 } else { ease(k) };
    (lerp(g.from.0, g.to.0, e), lerp(g.from.1, g.to.1, e))
}

impl BarChart {
    /// The tooltip's content and its size (shrink-to-fit, like the HTML).
    fn tooltip(&self, window: &Window, t: &Theme, i: usize) -> (AnyElement, f32, f32) {
        let label = self.labels[i].clone();
        let values = self.values.get(i).cloned().unwrap_or_default();
        match &self.tip {
            Tip::Series => {
                // rounded-[8px] bg-panel shadow-pop px-3 py-2 text-xs
                let mut inner = measure(window, &label, 12.0, 500, false);
                let mut body: Div = div()
                    .rounded(px(8.))
                    .bg(t.panel)
                    .shadow(t.shadow_pop())
                    .px(r(3.))
                    .py(r(2.))
                    .flex()
                    .flex_col()
                    .child(text(label, 12.0, 500, t.fg3).line_height(px(16.)).mb(r(1.)).whitespace_nowrap());
                for (s, series) in self.series.iter().enumerate() {
                    let amount = money(values.get(s).copied().unwrap_or(0.0));
                    let row_w = 8.0 + 6.5 + measure(window, &series.name, 12.0, 500, false) + 6.5 + 9.75 + measure(window, &amount, 12.0, 500, true);
                    inner = inner.max(row_w);
                    body = body.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(r(2.))
                            .h(px(16.))
                            .child(dot(series.dot, r(2.)))
                            .child(text(series.name.clone(), 12.0, 500, t.fg2).line_height(px(16.)).whitespace_nowrap())
                            .child(div().ml_auto().pl(r(3.)).font(num_font(500)).text_size(px(12.)).line_height(px(16.)).text_color(t.fg).whitespace_nowrap().child(amount)),
                    );
                }
                let w = inner + 19.5;
                let h = 13.0 + 16.0 + 3.25 + 16.0 * self.series.len() as f32;
                (body.w(px(w)).into_any_element(), w, h)
            }
            Tip::Single { suffix } => {
                // rounded-md border border-line bg-panel shadow-pop px-3 py-2 text-xs
                let head: SharedString = format!("{label} {suffix}").into();
                let amount = money(values.first().copied().unwrap_or(0.0));
                let inner = measure(window, &head, 12.0, 500, false).max(measure(window, &amount, 12.0, 500, true));
                let w = inner + 19.5 + 2.0;
                let h = 2.0 + 13.0 + 16.0 + 1.625 + 16.0;
                let body = div()
                    .w(px(w))
                    .rounded(r(1.5))
                    .border_1()
                    .border_color(t.line)
                    .bg(t.panel)
                    .shadow(t.shadow_pop())
                    .px(r(3.))
                    .py(r(2.))
                    .flex()
                    .flex_col()
                    .child(text(head, 12.0, 500, t.fg3).line_height(px(16.)).mb(r(0.5)).whitespace_nowrap())
                    .child(div().font(num_font(500)).text_size(px(12.)).line_height(px(16.)).text_color(t.fg).whitespace_nowrap().child(amount));
                (body.into_any_element(), w, h)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nice_ticks_match_recharts() {
        assert_eq!(nice_ticks(0.0, 4321.5), vec![0.0, 1500.0, 3000.0, 4500.0, 6000.0]);
        assert_eq!(nice_ticks(0.0, 0.0), vec![0.0, 1.0, 2.0, 3.0, 4.0]);
        assert_eq!(nice_ticks(0.0, 100.0), vec![0.0, 25.0, 50.0, 75.0, 100.0]);
        assert_eq!(nice_ticks(0.0, 8000.0), vec![0.0, 2000.0, 4000.0, 6000.0, 8000.0]);
    }
}
