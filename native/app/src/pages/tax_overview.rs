//! TaxOverview.tsx (+ ExpensePanel.tsx): what to set aside for the tax year
//! (realized and projected), income, the paid-income chart, expenses by
//! category with add/edit/delete, and the year-end exports.

mod expense_panel;

use std::time::Duration;

use billable_core::models::{Expense, TaxOverview, TaxSettings};
use chrono::{Datelike, Local};
use gpui::{
    div, px, AppContext, Context, Div, ElementId, Entity, Font, Hsla, IntoElement, ParentElement, Render, SharedString,
    Styled, StyledText, Subscription, TextRun, Window,
};

use self::expense_panel::{ExpensePanel, ExpensePanelEvent, EXIT_SECS};
use super::reports::bar_chart::{BarChart, Series, Tip};
use crate::{
    components::{
        client_form::trim_number,
        metric_strip, money, select,
        table::{measure, table, text_cell, Cell, Column, Row},
        toast_error, Button, EmptyState, Metric,
    },
    fonts::rounded_family,
    format::{self, format_day},
    icons::Lucide,
    motion::rise_in,
    shell::app::{Header, Page},
    store,
    style::{card, figures, icon, num_font, page_width, r, text},
    theme::{hex, theme, Theme},
};

const MONTHS_SHORT: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// `CATEGORY_LABEL`
fn category_label(c: &str) -> &str {
    match c {
        "equipment" => "Equipment",
        "software" => "Software",
        "home_office" => "Home office",
        "phone_internet" => "Phone and internet",
        "travel" => "Travel",
        "meals" => "Meals",
        "professional_development" => "Professional development",
        "other" => "Other",
        other => other,
    }
}

/// Text in pieces of different weights and colours that wraps as one
/// paragraph, like inline `<b>` and `<span>`s.
fn rich(parts: Vec<(String, Font, Hsla)>, size: f32, line_height: f32) -> Div {
    let mut s = String::new();
    let mut runs = Vec::new();
    for (piece, font, color) in parts {
        runs.push(TextRun { len: piece.len(), font, color, background_color: None, underline: None, strikethrough: None });
        s.push_str(&piece);
    }
    div().text_size(px(size)).line_height(px(line_height)).child(StyledText::new(s).with_runs(runs))
}

fn font(weight: u16) -> Font {
    gpui::font(rounded_family(weight))
}

pub struct TaxOverviewPage {
    rev: u64,
    year: i64,
    overview: Option<TaxOverview>,
    settings: Option<TaxSettings>,
    expenses: Vec<Expense>,
    show_how: bool,
    chart: Entity<BarChart>,
    panel: Option<Entity<ExpensePanel>>,
    panel_sub: Option<Subscription>,
}

impl TaxOverviewPage {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let year = Local::now().year() as i64;
        let chart = cx.new(|_| {
            BarChart::new(
                220.0,
                [4.0, 8.0, 0.0, 0.0],
                4.0,
                false,
                28.0,
                vec![Series { name: "paid".into(), color: |t| t.green, dot: hex(0x30A46C), max_bar_size: 32.0 }],
                Tip::Single { suffix: year.to_string().into() },
            )
        });
        let mut page = TaxOverviewPage { rev: 0, year, overview: None, settings: None, expenses: vec![], show_how: false, chart, panel: None, panel_sub: None };
        page.load(cx);
        page
    }

    /// `loadAll()`
    fn load(&mut self, cx: &mut Context<Self>) {
        self.rev = store::rev(cx);
        let db = store::db(cx);
        let year = self.year;
        let loaded = (|| anyhow::Ok((db.get_tax_overview(year)?, db.get_tax_settings()?, db.list_expenses(Some(year))?)))();
        match loaded {
            Ok((ov, ts, exp)) => {
                self.overview = Some(ov);
                self.settings = ts;
                self.expenses = exp;
            }
            Err(e) => toast_error(format!("Failed to load: {e}"), cx),
        }
        // A 12-month series of paid income
        if let Some(ov) = &self.overview {
            let values: Vec<Vec<f64>> = (1..=12)
                .map(|m| {
                    let key = format!("{year}-{m:02}");
                    vec![ov.monthly_income.iter().find(|x| x.month.as_deref() == Some(key.as_str())).map(|x| x.paid).unwrap_or(0.0)]
                })
                .collect();
            let labels = MONTHS_SHORT.iter().map(|m| SharedString::from(*m)).collect();
            self.chart.update(cx, |c, _| {
                c.set_tip(Tip::Single { suffix: year.to_string().into() });
                c.set_data(labels, values);
            });
        }
    }

    fn set_year(&mut self, year: i64, cx: &mut Context<Self>) {
        if year != self.year {
            self.year = year;
            self.load(cx);
            cx.notify();
        }
    }

    fn open_panel(&mut self, expense: Option<Expense>, window: &mut Window, cx: &mut Context<Self>) {
        let year = self.year;
        let panel = cx.new(|cx| ExpensePanel::new(expense, year, window, cx));
        self.panel_sub = Some(cx.subscribe(&panel, |p, _, _: &ExpensePanelEvent, cx| p.close_panel(cx)));
        self.panel = Some(panel);
        cx.notify();
    }

    /// `setPanelOpen(false)`: slide out, then let go of it.
    fn close_panel(&mut self, cx: &mut Context<Self>) {
        let Some(panel) = self.panel.clone() else { return };
        if panel.read(cx).is_closing() {
            return;
        }
        panel.update(cx, |p, cx| p.close(cx));
        let id = panel.entity_id();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs_f32(EXIT_SECS)).await;
            let _ = this.update(cx, |p, cx| {
                if p.panel.as_ref().is_some_and(|e| e.entity_id() == id) {
                    p.panel = None;
                    p.panel_sub = None;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    /// `handleExport`: the PDF, or a CSV when the year has rows.
    fn export(&mut self, kind: &str, window: &mut Window, cx: &mut Context<Self>) {
        let year = self.year;
        match kind {
            "pdf" => crate::export::tax_summary_pdf(year, window, cx),
            "invoices" => {
                if store::db(cx).list_invoices_by_year(year).map(|v| v.is_empty()).unwrap_or(true) {
                    toast_error("No data to export", cx);
                } else {
                    crate::export::invoices_csv(year, window, cx);
                }
            }
            _ => {
                if store::db(cx).list_expenses(Some(year)).map(|v| v.is_empty()).unwrap_or(true) {
                    toast_error("No data to export", cx);
                } else {
                    crate::export::expenses_csv(year, window, cx);
                }
            }
        }
    }
}

impl Page for TaxOverviewPage {
    fn header(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> Header {
        let mut header = Header::new("Tax overview");
        if self.overview.is_none() || self.settings.is_none() {
            return header;
        }
        let current = Local::now().year() as i64;
        let years: Vec<(i64, SharedString)> = (0..=5).map(|i| (current - i, SharedString::from((current - i).to_string()))).collect();
        let this = cx.weak_entity();
        header = header.actions(
            div()
                .flex()
                .items_center()
                .gap(r(2.))
                .child(div().w(r(28.)).child(select(
                    "tax-year",
                    years,
                    &self.year,
                    move |y, _, cx| {
                        let _ = this.update(cx, |p, cx| p.set_year(y, cx));
                    },
                    cx,
                )))
                .child(Button::secondary("tax-pdf", "Tax summary PDF").on_click(cx.listener(|p, _, w, cx| p.export("pdf", w, cx)))),
        );
        header
    }
}

/// A `flex justify-between gap-4` line of the set-aside breakdown.
fn line(left: impl Into<SharedString>, right: String, size: f32, lh: f32, weight: u16, left_color: Hsla, right_color: Hsla) -> Div {
    div()
        .flex()
        .justify_between()
        .gap(r(4.))
        .child(text(left, size, weight, left_color).line_height(px(lh)))
        .child(div().font(num_font(weight)).text_size(px(size)).line_height(px(lh)).text_color(right_color).child(right))
}

impl Render for TaxOverviewPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.rev != store::rev(cx) {
            self.load(cx);
        }
        let t = theme(cx);
        let (Some(ov), Some(ts)) = (self.overview.clone(), self.settings.clone()) else {
            return div().children(self.panel.clone());
        };
        let year = self.year;
        let m = format::money;

        // Realized: net business income from PAID invoices only
        let net_income = ov.total_paid - ov.gst_collected_paid - ov.total_expenses;
        let rate = ts.income_tax_bracket;
        let income_tax = net_income.max(0.0) * (rate / 100.0);
        let total = ov.gst_collected_paid + income_tax;
        // Projected: if everything invoiced (paid + outstanding) gets paid
        let projected_net = ov.total_invoiced - ov.gst_collected_total - ov.total_expenses;
        let projected_tax = projected_net.max(0.0) * (rate / 100.0);
        let projected_total = ov.gst_collected_total + projected_tax;
        let has_unpaid = ov.total_invoiced > ov.total_paid;

        // ---- Set-aside estimate ----
        let mut left = div()
            .flex_1()
            .min_w_0()
            .p(r(5.))
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(r(2.))
                    .child(text(format!("Estimated to set aside for {year}"), 12.0, 500, t.fg3).line_height(px(16.)))
                    .child(
                        // .badge text-fg-3
                        div().flex().flex_none().items_center().h(px(22.)).px(r(2.5)).rounded_full().bg(t.fg3.opacity(0.13)).child(text("Estimate", 12.0, 600, t.fg3).line_height(px(16.))),
                    ),
            )
            .child(figures(m(total), 34.0, t.fg).line_height(px(38.)).mt(r(1.5)))
            .child(
                text("Based on invoices you've been paid for. Record payments on the Billing page to keep this current.", 13.0, 500, t.fg3)
                    .line_height(px(18.))
                    .mt(r(1.5))
                    .max_w(px(416.)),
            );
        if has_unpaid {
            left = left.child(
                rich(
                    vec![
                        ("If everything invoiced gets paid: ".into(), font(500), t.fg2),
                        (m(projected_total), num_font(600), t.fg),
                        (format!(" · {} still to collect", m(ov.total_invoiced - ov.total_paid)), font(500), t.fg3),
                    ],
                    13.0,
                    18.0,
                )
                .mt(r(3.)),
            );
        }
        left = left.child(
            div().flex().mt(r(3.)).ml(-r(2.)).child(
                Button::ghost("tax-how", "How this is worked out")
                    .small()
                    .icon(if self.show_how { Lucide::ChevronDown } else { Lucide::ChevronRight })
                    .icon_color(t.accent_text)
                    .on_click(cx.listener(|p, _, _, cx| {
                        p.show_how = !p.show_how;
                        cx.notify();
                    })),
            ),
        );
        if self.show_how {
            let para = |bold: &str, rest: &str| rich(vec![(bold.into(), font(700), t.fg), (rest.into(), font(500), t.fg2)], 12.0, 16.25);
            left = left.child(
                div()
                    .mt(r(2.))
                    .p(r(3.5))
                    .rounded(r(2.))
                    .bg(t.fg.opacity(0.03))
                    .border_1()
                    .border_color(t.line)
                    .flex()
                    .flex_col()
                    .gap(r(1.5))
                    .child(para("Net business income", " is paid invoices, minus the GST/HST collected on them, minus deductible expenses."))
                    .child(para("Income tax", " is net business income times your marginal rate from Tax settings."))
                    .child(para("To set aside", " is income tax plus the GST/HST you collected, since that belongs to the CRA."))
                    .child(
                        text(
                            "A planning aid only. Your actual tax depends on CPP, credits, and your other income, so check with an accountant before filing.",
                            12.0,
                            500,
                            t.fg3,
                        )
                        .line_height(r(5.))
                        .pt(r(1.5))
                        .border_t_1()
                        .border_color(t.line),
                    ),
            );
        }
        let mut right = div()
            .w(px(300.))
            .flex_none()
            .border_l_1()
            .border_color(t.line)
            .bg(t.panel2.opacity(0.4))
            .p(r(5.))
            .flex()
            .flex_col()
            .gap(r(2.5))
            .child(line(format!("Income tax at {}%", trim_number(rate)), m(income_tax), 13.0, 18.0, 500, t.fg3, t.fg))
            .child(line("GST/HST to remit", m(ov.gst_collected_paid), 13.0, 18.0, 500, t.fg3, t.fg))
            .child(line("Total", m(total), 13.0, 18.0, 600, t.fg, t.fg).pt(r(2.5)).border_t_1().border_color(t.line));
        if has_unpaid {
            right = right.child(
                div()
                    .pt(r(2.5))
                    .border_t_1()
                    .border_color(t.line)
                    .flex()
                    .flex_col()
                    .gap(r(1.5))
                    .child(line("Projected income tax", m(projected_tax), 12.0, 16.0, 500, t.fg3, t.fg2))
                    .child(line("Projected GST/HST", m(ov.gst_collected_total), 12.0, 16.0, 500, t.fg3, t.fg2)),
            );
        }
        let set_aside = card(&t).mb(r(5.)).overflow_hidden().flex().child(left).child(right);

        // ---- Metrics ----
        let plural = |n: i64| if n == 1 { "" } else { "s" };
        let metrics = metric_strip(vec![
            Metric::new("tax-invoiced", "Invoiced", money(ov.total_invoiced, 22.0, t.fg)).sub(format!("{} invoice{}", ov.invoice_count, plural(ov.invoice_count))),
            Metric::new("tax-paid", "Paid", money(ov.total_paid, 22.0, t.fg)).sub(format!("{} paid", ov.paid_count)),
            Metric::new("tax-outstanding", "Outstanding", money(ov.total_outstanding, 22.0, t.fg)).sub("Sent, not yet paid"),
            Metric::new("tax-expenses", "Expenses", money(ov.total_expenses, 22.0, t.fg)).sub(format!("{} logged", self.expenses.len())),
            Metric::new("tax-net", "Net business income", money(net_income, 22.0, t.fg)).sub("Paid, less GST and expenses"),
        ])
        .mb(r(5.));

        // ---- Monthly chart ----
        let chart = card(&t)
            .p(r(4.))
            .mb(r(5.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .mb(r(3.))
                    .child(text("Paid income by month", 15.0, 700, t.fg).line_height(px(22.5)))
                    .child(text("Counted in the month the payment arrived", 12.0, 500, t.fg3).line_height(px(16.))),
            )
            .child(self.chart.clone());

        // ---- Expenses ----
        let expenses = self.expenses_card(&t, &ov, window, cx);

        // ---- Exports ----
        let exports = card(&t)
            .px(r(4.))
            .py(r(3.5))
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .child(text("Year-end files", 13.0, 500, t.fg).line_height(px(18.)))
                    .child(text("For your bookkeeper or accountant.", 12.0, 500, t.fg3).line_height(px(16.))),
            )
            .child(
                div()
                    .flex()
                    .gap(r(2.))
                    .child(Button::secondary("tax-invoices-csv", "Invoices CSV").small().on_click(cx.listener(|p, _, w, cx| p.export("invoices", w, cx))))
                    .child(Button::secondary("tax-expenses-csv", "Expenses CSV").small().on_click(cx.listener(|p, _, w, cx| p.export("expenses", w, cx))))
                    .child(Button::secondary("tax-summary-pdf", "Summary PDF").small().on_click(cx.listener(|p, _, w, cx| p.export("pdf", w, cx)))),
            );

        let disclaimer = div()
            .flex()
            .items_start()
            .gap(r(2.))
            .mt(r(5.))
            .child(icon(Lucide::AlertCircle).size(r(3.5)).mt(r(0.5)).text_color(t.fg3))
            .child(text("These numbers are estimates from your Billable records, not tax, legal, or financial advice.", 12.0, 500, t.fg3).line_height(r(5.)));

        div()
            .w_full()
            .max_w(px(1160.))
            .mx_auto()
            .px(r(7.))
            .pt(r(6.))
            .pb(r(14.))
            .flex()
            .flex_col()
            .child(rise_in("tax-rise-0", 0, set_aside))
            .child(rise_in("tax-rise-1", 1, metrics))
            .child(rise_in("tax-rise-2", 2, chart))
            .child(rise_in("tax-rise-3", 3, expenses))
            .child(rise_in("tax-rise-4", 4, exports))
            .child(rise_in("tax-rise-5", 5, disclaimer))
            .children(self.panel.clone())
    }
}

impl TaxOverviewPage {
    fn expenses_card(&self, t: &Theme, ov: &TaxOverview, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .gap(r(3.))
            .px(r(4.))
            .h(px(46.))
            .border_b_1()
            .border_color(t.line)
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .gap(r(2.))
                    .child(text("Expenses", 15.0, 700, t.fg).line_height(px(22.5)))
                    .child(text(format!("{} deductible", format::money(ov.total_expenses)), 12.0, 500, t.fg3).line_height(px(16.))),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(r(2.))
                    .child(Button::ghost("tax-exp-csv", "CSV").small().on_click(cx.listener(|p, _, w, cx| p.export("expenses", w, cx))))
                    .child(Button::secondary("tax-add-expense", "Add expense").small().on_click(cx.listener(|p, _, w, cx| p.open_panel(None, w, cx)))),
            );
        let mut body = card(t).mb(r(5.)).child(header);

        if !ov.expenses_by_category.is_empty() {
            let mut chips = div().flex().flex_wrap().gap(r(2.)).px(r(4.)).py(r(3.)).border_b_1().border_color(t.line);
            for c in &ov.expenses_by_category {
                chips = chips.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(r(2.))
                        .h(r(7.))
                        .pl(r(2.5))
                        .pr(r(2.))
                        .rounded(r(1.5))
                        .bg(t.fg.opacity(0.04))
                        .border_1()
                        .border_color(t.line)
                        .child(text(category_label(&c.category).to_string(), 12.0, 500, t.fg2).line_height(px(16.)))
                        .child(div().font(num_font(500)).text_size(px(12.)).line_height(px(16.)).text_color(t.fg).child(format::money(c.total))),
                );
            }
            body = body.child(chips);
        }

        if self.expenses.is_empty() {
            return body.child(EmptyState::new("tax-expenses-empty", format!("No expenses for {}", self.year), "Log deductible costs to lower your taxable income.").compact());
        }

        let columns = vec![Column::new("Date").fixed(28.0 * 3.25), Column::new("Category"), Column::new("Description"), Column::new("Amount").right()];
        let mut rows = Vec::new();
        for e in &self.expenses {
            let description = if e.description.is_empty() {
                text_cell(window, "No description", 14.0, 500, t.fg4, false)
            } else {
                // truncate max-w-[320px]
                let w = measure(window, &e.description, 14.0, 500, false).min(320.0 - 19.5);
                Cell::new(text(e.description.clone(), 14.0, 500, t.fg).max_w(px(320.0 - 19.5)).truncate(), w)
            };
            let expense = e.clone();
            rows.push(
                Row::new(
                    ElementId::Name(format!("expense-row-{}", e.id).into()),
                    vec![
                        text_cell(window, format_day(&e.date), 14.0, 500, t.fg2, true),
                        text_cell(window, category_label(&e.category).to_string(), 14.0, 500, t.fg2, false),
                        description,
                        text_cell(window, format::money(e.amount), 14.0, 500, t.fg, true),
                    ],
                )
                .height(40.0)
                .on_click(cx.listener(move |p, _, w, cx| p.open_panel(Some(expense.clone()), w, cx))),
            );
        }
        // `.card:has(> table)` clips the rows' hover to the corners
        body.overflow_hidden().child(table(t, window, "tax-expenses", columns, rows, page_width(window) - 2.0, None))
    }
}
