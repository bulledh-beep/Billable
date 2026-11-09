//! Reports.tsx: time by project or client with its billable value, and
//! income by month (invoiced vs paid), for a preset or custom date range,
//! with CSV export.

pub(crate) mod bar_chart;

use billable_core::models::MonthEarnings;
use chrono::{Datelike, Duration as Days, Local, NaiveDate};
use gpui::{
    div, px, relative, AppContext, Context, ElementId, Entity, IntoElement, ParentElement, Render, SharedString,
    Styled, Subscription, Window,
};

use self::bar_chart::{BarChart, Series, Tip};
use crate::{
    components::{
        metric_strip, money, segmented,
        table::{measure, table, text_cell, Cell, Column, Row},
        toast_error, Button, DateChanged, DateInput, EmptyState, Metric, Segment,
    },
    format::{self, hours_short},
    motion::rise_in,
    shell::app::{Header, Page},
    store,
    style::{card, dot, figures, num_font, page_width, r, text},
    theme::{hex, parse_hex, theme},
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Preset {
    Week,
    Month,
    LastMonth,
    Quarter,
    Year,
    Custom,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Tab {
    Projects,
    Clients,
    Income,
}

const FALLBACK_COLORS: [&str; 8] = ["#F5A623", "#3E8BF7", "#30A46C", "#E5484D", "#8E4EC6", "#12A594", "#F76B15", "#D6409F"];
const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

fn iso(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

fn ymd(y: i32, m0: i32, d: i64) -> NaiveDate {
    // new Date(y, m, d): months and days roll over
    let y = y + m0.div_euclid(12);
    let m = m0.rem_euclid(12) as u32 + 1;
    NaiveDate::from_ymd_opt(y, m, 1).unwrap_or_default() + Days::days(d - 1)
}

/// `rangeFor(preset)`
fn range_for(preset: Preset) -> (String, String) {
    let today = Local::now().date_naive();
    let (y, m) = (today.year(), today.month0() as i32);
    match preset {
        Preset::Week => {
            let start = today - Days::days(today.weekday().num_days_from_monday() as i64);
            (iso(start), iso(start + Days::days(6)))
        }
        Preset::Month => (iso(ymd(y, m, 1)), iso(ymd(y, m + 1, 0))),
        Preset::LastMonth => (iso(ymd(y, m - 1, 1)), iso(ymd(y, m, 0))),
        Preset::Quarter => {
            let q = m / 3 * 3;
            (iso(ymd(y, q, 1)), iso(ymd(y, q + 3, 0)))
        }
        _ => (format!("{y}-01-01"), format!("{y}-12-31")),
    }
}

/// `Number.prototype.toFixed(2)`: exact ties round up, like JavaScript.
fn to_fixed2(v: f64) -> String {
    let a = v.abs();
    let thousandths = a * 1000.0;
    let tie = thousandths < 1e15 && thousandths.fract() == 0.0 && [125, 375, 625, 875].contains(&((thousandths as u64) % 1000));
    let s = if tie {
        let n = (thousandths as u64 + 5) / 10;
        format!("{}.{:02}", n / 100, n % 100)
    } else {
        format!("{a:.2}")
    };
    if v < 0.0 { format!("-{s}") } else { s }
}

/// "Jan 26" (`toLocaleDateString('en-US', { month: 'short', year: '2-digit' })`).
fn month_label(month: &str) -> String {
    let mut parts = month.split('-');
    let y: i32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let m: usize = parts.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    format!("{} {:02}", MONTHS[m.clamp(1, 12) - 1], y.rem_euclid(100))
}

/// A row of the time tables (projects or clients).
struct TimeRow {
    name: String,
    color: Option<String>,
    client_name: Option<String>,
    hours: f64,
    value: f64,
}

pub struct ReportsPage {
    rev: u64,
    preset: Preset,
    range: (String, String),
    tab: Tab,
    by_project: Vec<TimeRow>,
    by_client: Vec<TimeRow>,
    by_month: Vec<MonthEarnings>,
    start: Entity<DateInput>,
    end: Entity<DateInput>,
    chart: Option<Entity<BarChart>>,
    _subs: Vec<Subscription>,
}

impl ReportsPage {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let range = range_for(Preset::Year);
        let parse = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok();
        let start = cx.new(|cx| DateInput::date(parse(&range.0), cx));
        let end = cx.new(|cx| DateInput::date(parse(&range.1), cx));
        let subs = vec![
            cx.subscribe(&start, |p, _, e: &DateChanged, cx| {
                if let DateChanged::Date(d) = e {
                    p.preset = Preset::Custom;
                    p.range.0 = d.map(iso).unwrap_or_default();
                    p.load(cx);
                }
            }),
            cx.subscribe(&end, |p, _, e: &DateChanged, cx| {
                if let DateChanged::Date(d) = e {
                    p.preset = Preset::Custom;
                    p.range.1 = d.map(iso).unwrap_or_default();
                    p.load(cx);
                }
            }),
            // Reload when the window comes back into focus
            cx.observe_window_activation(window, |p, window, cx| {
                if window.is_window_active() {
                    p.load(cx);
                }
            }),
        ];
        let mut page = ReportsPage {
            rev: 0,
            preset: Preset::Year,
            range,
            tab: Tab::Projects,
            by_project: vec![],
            by_client: vec![],
            by_month: vec![],
            start,
            end,
            chart: None,
            _subs: subs,
        };
        page.fetch(cx);
        page
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        self.fetch(cx);
        cx.notify();
    }

    /// `load()`: skipped while the range is incomplete or backwards, keeping
    /// what's shown.
    fn fetch(&mut self, cx: &mut Context<Self>) {
        self.rev = store::rev(cx);
        let (s, e) = (self.range.0.clone(), self.range.1.clone());
        if s.is_empty() || e.is_empty() || s > e {
            return;
        }
        let db = store::db(cx);
        let loaded = (|| {
            let p = db.hours_by_project(&s, &e)?;
            let c = db.hours_by_client(&s, &e)?;
            let m = db.earnings_by_month(&s, &e)?;
            anyhow::Ok((p, c, m))
        })();
        if let Ok((p, c, m)) = loaded {
            self.by_project = p
                .into_iter()
                .map(|r| TimeRow { name: r.name, color: Some(r.color).filter(|c| !c.is_empty()), client_name: r.client_name, hours: r.hours, value: r.value })
                .collect();
            self.by_client = c.into_iter().map(|r| TimeRow { name: r.name, color: None, client_name: None, hours: r.hours, value: r.value }).collect();
            self.by_month = m;
        }
        self.sync_chart(cx);
    }

    fn choose_preset(&mut self, p: Preset, cx: &mut Context<Self>) {
        self.preset = p;
        if p != Preset::Custom {
            self.range = range_for(p);
            let parse = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok();
            let (a, b) = (parse(&self.range.0), parse(&self.range.1));
            self.start.update(cx, |d, cx| {
                d.set_date(a);
                cx.notify();
            });
            self.end.update(cx, |d, cx| {
                d.set_date(b);
                cx.notify();
            });
            self.load(cx);
        }
        cx.notify();
    }

    /// The income chart mounts with the tab (and its bars grow in each time).
    fn sync_chart(&mut self, cx: &mut Context<Self>) {
        if self.tab != Tab::Income || self.by_month.is_empty() {
            self.chart = None;
            return;
        }
        let labels: Vec<SharedString> = self.by_month.iter().map(|m| month_label(&m.month).into()).collect();
        let values: Vec<Vec<f64>> = self.by_month.iter().map(|m| vec![m.invoiced, m.paid]).collect();
        let chart = self.chart.get_or_insert_with(|| {
            cx.new(|_| {
                BarChart::new(
                    300.0,
                    [8.0, 8.0, 0.0, 0.0],
                    4.0,
                    true,
                    28.0,
                    vec![
                        Series { name: "Invoiced".into(), color: |t| t.blue, dot: hex(0x3E8BF7), max_bar_size: 22.0 },
                        Series { name: "Paid".into(), color: |t| t.green, dot: hex(0x30A46C), max_bar_size: 22.0 },
                    ],
                    Tip::Series,
                )
            })
        });
        chart.update(cx, |c, _| c.set_data(labels, values));
    }

    fn export(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (headers, rows): (Vec<&str>, Vec<Vec<String>>) = match self.tab {
            Tab::Projects => (
                vec!["project", "client", "hours", "billable_value"],
                self.by_project.iter().map(|r| vec![r.name.clone(), r.client_name.clone().unwrap_or_default(), to_fixed2(r.hours), to_fixed2(r.value)]).collect(),
            ),
            Tab::Clients => (vec!["client", "hours", "billable_value"], self.by_client.iter().map(|r| vec![r.name.clone(), to_fixed2(r.hours), to_fixed2(r.value)]).collect()),
            Tab::Income => (vec!["month", "invoiced", "paid"], self.by_month.iter().map(|r| vec![r.month.clone(), to_fixed2(r.invoiced), to_fixed2(r.paid)]).collect()),
        };
        if rows.is_empty() {
            toast_error("Nothing to export for this range", cx);
            return;
        }
        let kind = match self.tab {
            Tab::Income => "income-by-month",
            Tab::Projects => "hours-by-project",
            Tab::Clients => "hours-by-client",
        };
        let file = format!("{kind}-{}-to-{}.csv", self.range.0, self.range.1);
        crate::export::csv(headers.into_iter().map(String::from).collect(), rows, &file, window, cx);
    }
}

impl Page for ReportsPage {
    fn header(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> Header {
        Header::new("Reports").actions(Button::secondary("reports-export", "Export CSV").on_click(cx.listener(|p, _, w, cx| p.export(w, cx))))
    }
}

impl Render for ReportsPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.rev != store::rev(cx) {
            self.fetch(cx);
        }
        let t = theme(cx);

        // Presets and the custom range
        let this = cx.weak_entity();
        let presets = segmented(
            "reports-preset",
            &self.preset,
            vec![
                Segment::new(Preset::Week, "This week"),
                Segment::new(Preset::Month, "This month"),
                Segment::new(Preset::LastMonth, "Last month"),
                Segment::new(Preset::Quarter, "This quarter"),
                Segment::new(Preset::Year, "This year"),
                Segment::new(Preset::Custom, "Custom"),
            ],
            move |p, _, cx| {
                let _ = this.update(cx, |page, cx| page.choose_preset(p, cx));
            },
            cx,
        );
        let range_row = div()
            .flex()
            .items_center()
            .gap(r(3.))
            .mb(r(5.))
            .flex_wrap()
            .child(presets)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(r(2.))
                    .ml_auto()
                    .child(div().w(px(150.)).child(self.start.clone()))
                    .child(text("–", 13.0, 500, t.fg4))
                    .child(div().w(px(150.)).child(self.end.clone())),
            );

        // Totals
        let hours: f64 = self.by_project.iter().map(|r| r.hours).sum();
        let value: f64 = self.by_project.iter().map(|r| r.value).sum();
        let invoiced: f64 = self.by_month.iter().map(|r| r.invoiced).sum();
        let paid: f64 = self.by_month.iter().map(|r| r.paid).sum();
        let n = self.by_project.len();
        let metrics = metric_strip(vec![
            Metric::new("reports-hours", "Hours tracked", figures(hours_short(hours), 22.0, t.fg)).sub(format!("{n} project{}", if n == 1 { "" } else { "s" })),
            Metric::new("reports-value", "Billable value", money(value, 22.0, t.fg)).sub("Tracked time at project rates"),
            Metric::new("reports-invoiced", "Invoiced", money(invoiced, 22.0, t.fg)).sub("Issued in this range"),
            Metric::new("reports-paid", "Paid", money(paid, 22.0, t.fg)).sub("Received in this range"),
        ])
        .mb(r(5.));

        let this = cx.weak_entity();
        let tabs = div().flex().items_center().mb(r(3.)).child(segmented(
            "reports-tab",
            &self.tab,
            vec![Segment::new(Tab::Projects, "Time by project"), Segment::new(Tab::Clients, "Time by client"), Segment::new(Tab::Income, "Income by month")],
            move |tab, _, cx| {
                let _ = this.update(cx, |p, cx| {
                    p.tab = tab;
                    p.sync_chart(cx);
                    cx.notify();
                });
            },
            cx,
        ));

        let body = match self.tab {
            Tab::Projects | Tab::Clients => {
                let rows = if self.tab == Tab::Projects { &self.by_project } else { &self.by_client };
                if rows.is_empty() {
                    card(&t).child(EmptyState::new("reports-empty-time", "No time in this range", "Pick a different range or track some time.")).into_any_element()
                } else {
                    self.time_table(rows, hours, window, cx).into_any_element()
                }
            }
            Tab::Income => match self.chart.clone() {
                Some(chart) => card(&t).p(r(4.)).child(chart).into_any_element(),
                None => card(&t).child(EmptyState::new("reports-empty-income", "No invoices in this range", "Income shows up here once invoices are sent or paid.")).into_any_element(),
            },
        };

        div()
            .w_full()
            .max_w(px(1160.))
            .mx_auto()
            .px(r(7.))
            .pt(r(6.))
            .pb(r(14.))
            .flex()
            .flex_col()
            .child(rise_in("reports-rise-0", 0, range_row))
            .child(rise_in("reports-rise-1", 1, metrics))
            .child(rise_in("reports-rise-2", 2, tabs))
            .child(rise_in("reports-rise-3", 3, body))
    }
}

impl ReportsPage {
    fn time_table(&self, rows: &[TimeRow], total_hours: f64, window: &mut Window, cx: &mut Context<Self>) -> gpui::Div {
        let t = theme(cx);
        let columns = vec![
            Column::new(if self.tab == Tab::Projects { "Project" } else { "Client" }),
            Column::new("Hours").right().fixed(24.0 * 3.25),
            Column::new("Share of time").fixed(220.0),
            Column::new("Value").right().fixed(28.0 * 3.25),
        ];
        let top_hours = rows.first().map(|r| r.hours).filter(|h| *h != 0.0).unwrap_or(1.0);
        let mut out = Vec::new();
        for (i, row) in rows.iter().enumerate() {
            let color = parse_hex(row.color.as_deref().unwrap_or(FALLBACK_COLORS[i % FALLBACK_COLORS.len()]));
            let name_w = measure(window, &row.name, 13.0, 500, false).min(360.0);
            let client_w = row.client_name.as_deref().map(|c| measure(window, c, 12.0, 500, false)).unwrap_or(0.0);
            let mut names = div()
                .min_w_0()
                .flex()
                .flex_col()
                .child(text(row.name.clone(), 13.0, 500, t.fg).line_height(px(19.5)).max_w(px(360.)).truncate());
            if let Some(c) = &row.client_name {
                names = names.child(text(c.clone(), 12.0, 500, t.fg3).line_height(px(16.)).truncate());
            }
            let name_cell = Cell::new(div().flex().items_center().gap(r(2.5)).min_w_0().child(dot(color, r(2.))).child(names), 8.0 + 9.75 + name_w.max(client_w));

            let share = if total_hours > 0.0 { row.hours / total_hours } else { 0.0 };
            let fill = (row.hours / top_hours * 100.0).max(2.0);
            let share_cell = Cell::new(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .gap(r(2.5))
                    .child(
                        div()
                            .flex_1()
                            .h(px(6.))
                            .rounded_full()
                            .bg(t.fg.opacity(0.07))
                            .overflow_hidden()
                            .child(div().h_full().rounded_full().bg(t.accent).w(relative((fill / 100.0) as f32))),
                    )
                    .child(
                        div()
                            .w(r(9.))
                            .flex_none()
                            .flex()
                            .justify_end()
                            .font(num_font(500))
                            .text_size(px(12.))
                            .line_height(px(16.))
                            .text_color(t.fg3)
                            .child(format!("{}%", format::js_round(share * 100.0))),
                    ),
                9.75 + 29.25,
            );
            out.push(Row::new(
                ElementId::Name(format!("reports-row-{i}").into()),
                vec![
                    name_cell,
                    text_cell(window, hours_short(row.hours), 14.0, 500, t.fg, true),
                    share_cell,
                    text_cell(window, format::money(row.value), 14.0, 500, t.fg2, true),
                ],
            ));
        }
        card(&t).overflow_hidden().child(table(&t, window, "reports-time", columns, out, page_width(window) - 2.0, None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_and_labels() {
        assert_eq!(to_fixed2(0.125), "0.13");
        assert_eq!(to_fixed2(2.675), "2.67");
        assert_eq!(to_fixed2(10.0), "10.00");
        assert_eq!(month_label("2026-01"), "Jan 26");
        assert_eq!(ymd(2026, 0, 0), NaiveDate::from_ymd_opt(2025, 12, 31).unwrap());
        assert_eq!(ymd(2026, 2, 0), NaiveDate::from_ymd_opt(2026, 2, 28).unwrap());
    }
}
