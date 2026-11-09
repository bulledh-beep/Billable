//! "New commission invoice": bundle closed jobs (optionally only the ones
//! selected in the table) into one payout invoice, filtered by type and
//! appointment dates, with a tick per job to leave one out.

use std::collections::HashSet;

use billable_core::models::{Commission, CommissionInvoiceInput};
use chrono::NaiveDate;
use gpui::{
    div, prelude::*, px, AppContext, Context, ElementId, Entity, EventEmitter, IntoElement, ParentElement, Render,
    Styled, Subscription, Window,
};

use super::{
    model::{effective_or_zero, job_detail},
    ui::job_type_tag,
};
use crate::{
    components::{fields::checkbox, segmented, toast_error, toast_success, Button, DateChanged, DateInput, Modal, ModalSize, Segment},
    format::money,
    store,
    style::{num_font, r, text},
    theme::theme,
};

pub enum GenerateEvent {
    Created,
    Closed,
}

pub struct GenerateModal {
    commissions: Vec<Commission>,
    /// When invoicing a selection: only these jobs.
    restrict: Option<Vec<i64>>,
    /// `both` | `solar` | `roofing`
    category: String,
    from: Entity<DateInput>,
    to: Entity<DateInput>,
    include_invoiced: bool,
    excluded: HashSet<i64>,
    _subs: Vec<Subscription>,
}

impl EventEmitter<GenerateEvent> for GenerateModal {}

fn iso(d: Option<NaiveDate>) -> Option<String> {
    d.map(|d| d.format("%Y-%m-%d").to_string())
}

impl GenerateModal {
    pub fn new(commissions: Vec<Commission>, restrict: Option<Vec<i64>>, cx: &mut Context<Self>) -> Self {
        let from = cx.new(|cx| DateInput::date(None, cx));
        let to = cx.new(|cx| DateInput::date(None, cx));
        let subs = vec![
            cx.subscribe(&from, |_, _, _: &DateChanged, cx| cx.notify()),
            cx.subscribe(&to, |_, _, _: &DateChanged, cx| cx.notify()),
        ];
        GenerateModal { commissions, restrict, category: "both".into(), from, to, include_invoiced: false, excluded: HashSet::new(), _subs: subs }
    }

    fn eligible(&self, cx: &gpui::App) -> Vec<Commission> {
        let from = iso(self.from.read(cx).date_value());
        let to = iso(self.to.read(cx).date_value());
        self.commissions
            .iter()
            .filter(|c| {
                (c.status == "closed_waiting" || c.status == "paid")
                    && !(c.needs_review != 0 && c.manual_override.is_none())
                    && (self.include_invoiced || c.invoice_status.as_deref().is_none_or(|s| s == "not_invoiced"))
            })
            .filter(|c| self.restrict.as_ref().is_none_or(|ids| ids.contains(&c.id)))
            .filter(|c| self.category == "both" || c.job_type == self.category)
            .filter(|c| from.as_deref().is_none_or(|f| c.appointment_date.as_deref().unwrap_or("") >= f))
            .filter(|c| to.as_deref().is_none_or(|t| c.appointment_date.as_deref().unwrap_or("") <= t))
            .cloned()
            .collect()
    }

    fn generate(&mut self, cx: &mut Context<Self>) {
        let chosen: Vec<i64> = self.eligible(cx).iter().filter(|c| !self.excluded.contains(&c.id)).map(|c| c.id).collect();
        if chosen.is_empty() {
            toast_error("No eligible jobs selected", cx);
            return;
        }
        let input = CommissionInvoiceInput {
            job_ids: chosen,
            category: Some(self.category.clone()),
            date_from: iso(self.from.read(cx).date_value()),
            date_to: iso(self.to.read(cx).date_value()),
            notes: None,
        };
        match store::write(cx, |db| db.create_commission_invoice(&input)) {
            Ok(_) => {
                toast_success("Commission invoice created", cx);
                cx.emit(GenerateEvent::Created);
            }
            Err(e) => toast_error(format!("Failed: {e}"), cx),
        }
    }
}

impl Render for GenerateModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(cx);
        let eligible = self.eligible(cx);
        let selected: Vec<&Commission> = eligible.iter().filter(|c| !self.excluded.contains(&c.id)).collect();
        let total: f64 = selected.iter().map(|c| effective_or_zero(c)).sum();
        let n = selected.len();
        let this = cx.entity().downgrade();

        let tc = this.clone();
        let filters = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(r(3.))
            .child(segmented(
                "comm-gen-cat",
                &self.category,
                vec![Segment::new("both".to_string(), "Solar and roofing"), Segment::new("solar".to_string(), "Solar"), Segment::new("roofing".to_string(), "Roofing")],
                move |v, _, cx| {
                    let _ = tc.update(cx, |m, cx| {
                        m.category = v;
                        cx.notify();
                    });
                },
                cx,
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(r(2.))
                    .child(div().w(px(140.)).child(self.from.clone()))
                    .child(text("–", 13.0, 500, t.fg4))
                    .child(div().w(px(140.)).child(self.to.clone())),
            )
            .child(
                div()
                    .id("comm-gen-include")
                    .flex()
                    .items_center()
                    .gap(r(2.))
                    .cursor_pointer()
                    .on_click(cx.listener(|m, _, _, cx| {
                        m.include_invoiced = !m.include_invoiced;
                        cx.notify();
                    }))
                    .child(checkbox(&t, "comm-gen-include-box", self.include_invoiced, false))
                    .child(text("Include already invoiced", 13.0, 500, t.fg2).line_height(px(18.))),
            );

        let mut list = div().id("comm-gen-list").max_h(px(300.)).overflow_y_scroll().flex().flex_col();
        if eligible.is_empty() {
            list = list.child(div().px(r(3.)).py(r(8.)).flex().justify_center().child(text("No closed jobs match these filters.", 13.0, 500, t.fg3).line_height(px(18.))));
        }
        let count = eligible.len();
        for (i, c) in eligible.iter().enumerate() {
            let id = c.id;
            let excluded = self.excluded.contains(&id);
            let mut row = div()
                .id(ElementId::Name(format!("comm-gen-row-{id}").into()))
                .flex_none()
                .flex()
                .items_center()
                .gap(r(3.))
                .px(r(3.))
                .h(if i + 1 < count { r(12.) - px(1.) } else { r(12.) })
                .cursor_pointer()
                .on_click(cx.listener(move |m, _, _, cx| {
                    if !m.excluded.remove(&id) {
                        m.excluded.insert(id);
                    }
                    cx.notify();
                }))
                .child(checkbox(&t, ElementId::Name(format!("comm-gen-box-{id}").into()), !excluded, false))
                .child(job_type_tag(&t, &c.job_type))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(text(c.client_name.clone(), 13.0, 500, t.fg).line_height(px(18.)).truncate())
                        .child(
                            text(
                                format!("{}{}", job_detail(c), if c.invoice_status.as_deref() == Some("invoiced") { " · already invoiced" } else { "" }),
                                12.0,
                                500,
                                t.fg3,
                            )
                            .line_height(px(16.)),
                        ),
                )
                .child(div().font(num_font(600)).text_size(px(13.)).line_height(px(18.)).text_color(t.fg).whitespace_nowrap().child(money(effective_or_zero(c))));
            if excluded {
                row = row.opacity(0.45);
            }
            if i + 1 < count {
                row = row.border_b_1().border_color(t.line);
            }
            list = list.child(row);
        }

        let jobs = div()
            .rounded(r(2.))
            .border_1()
            .border_color(t.line)
            .overflow_hidden()
            .child(
                div()
                    .px(r(3.))
                    .h(r(8.))
                    .bg(t.fg.opacity(0.03))
                    .border_b_1()
                    .border_color(t.line)
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(text(format!("{n} of {count} eligible jobs"), 12.0, 500, t.fg3).line_height(px(16.)))
                    .child(text("Untick to leave one out", 12.0, 500, t.fg3).line_height(px(16.))),
            )
            .child(list);

        let (e1, e2, e3) = (this.clone(), this.clone(), this);
        Modal::new("comm-gen", "New commission invoice", move |_, cx| {
            let _ = e1.update(cx, |_, cx| cx.emit(GenerateEvent::Closed));
        })
        .size(ModalSize::Lg)
        .description("Bundle closed jobs into one payout invoice.")
        .child(div().flex().flex_col().gap(r(4.)).child(filters).child(jobs))
        .footer(
            div()
                .mr_auto()
                .flex()
                .items_baseline()
                .child(text("Total ", 12.0, 500, t.fg3).line_height(px(16.)))
                .child(div().font(num_font(600)).text_size(px(14.)).line_height(px(20.)).text_color(t.fg).child(money(total))),
        )
        .footer(Button::secondary("comm-gen-cancel", "Cancel").on_click(move |_, _, cx| {
            let _ = e2.update(cx, |_, cx| cx.emit(GenerateEvent::Closed));
        }))
        .footer(
            Button::primary("comm-gen-create", format!("Create invoice for {n} job{}", if n == 1 { "" } else { "s" }))
                .disabled(n == 0)
                .on_click(move |_, _, cx| {
                    let _ = e3.update(cx, |m, cx| m.generate(cx));
                }),
        )
    }
}
