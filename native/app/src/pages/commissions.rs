//! Commissions.tsx: solar and roofing appointment payouts. Jobs with their
//! money buckets (Owed, Invoiced, Paid, Expected), stage tabs and filters,
//! quick and bulk status actions, the guided Add job form, and commission
//! invoices (bundle closed jobs, view, mark paid, PDF, delete).

mod generate;
mod job_form;
mod model;
#[cfg(test)]
mod tests;
mod ui;

use std::{collections::BTreeSet, rc::Rc, time::Duration};

use billable_core::models::{Commission, CommissionInvoice, CommissionInvoiceDetail, CommissionStatusPatch};
use gpui::{
    anchored, deferred, div, point, prelude::*, px, Animation, AnimationExt as _, AnyElement, App, AppContext, Context,
    ElementId, Entity, IntoElement, ParentElement, PromptButton, PromptLevel, Render, SharedString, Styled, Subscription,
    WeakEntity, Window,
};
use gpui_component::input::{InputEvent, InputState};

use self::{
    generate::{GenerateEvent, GenerateModal},
    job_form::{JobFormEvent, JobFormModal},
    model::*,
    ui::{grid, header_text, job_type_tag, job_type_tag_width, pill, pill_width, small_button_width, RowLook},
};
use crate::{
    components::{
        confirm_dialog,
        fields::{checkbox, search_field},
        menu_list, metric_strip, money as money_figure, popover, segmented,
        table::{measure, text_cell, Cell, Column, Row},
        toast_error, toast_success, Align, Button, DateChanged, DateInput, EmptyState, MenuItem, Metric, Modal,
        ModalSize, Segment,
    },
    format::{format_day, js_round, money},
    icons::Lucide,
    motion::{self, rise_in},
    shell::app::{Header, Page},
    store,
    style::{card, icon, num_font, page_width, r, text},
    theme::theme,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Jobs,
    Invoices,
}

/// Which row's "…" menu is open.
#[derive(Clone, Copy, PartialEq, Eq)]
enum OpenMenu {
    Job(i64),
    Invoice(i64),
}

struct Stats {
    owed: f64,
    invoiced: f64,
    paid: f64,
    expected: f64,
    closed_deals: usize,
    lost_deals: usize,
    close_rate: f64,
    avg_per_closed: f64,
}

pub struct CommissionsPage {
    rev: u64,
    commissions: Vec<Commission>,
    invoices: Vec<CommissionInvoice>,
    view: View,
    stage: Stage,
    selected: BTreeSet<i64>,
    job_filter: String,
    date_from: Entity<DateInput>,
    date_to: Entity<DateInput>,
    search: Entity<InputState>,
    form: Option<Entity<JobFormModal>>,
    form_sub: Option<Subscription>,
    generate: Option<Entity<GenerateModal>>,
    generate_sub: Option<Subscription>,
    view_invoice: Option<CommissionInvoiceDetail>,
    delete_id: Option<i64>,
    delete_invoice_id: Option<i64>,
    menu: Option<OpenMenu>,
    _subs: Vec<Subscription>,
}

impl CommissionsPage {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Client or closer").clean_on_escape());
        let date_from = cx.new(|cx| DateInput::date(None, cx));
        let date_to = cx.new(|cx| DateInput::date(None, cx));
        let subs = vec![
            cx.subscribe(&search, |_, _, e: &InputEvent, cx| {
                if matches!(e, InputEvent::Change) {
                    cx.notify();
                }
            }),
            cx.subscribe(&date_from, |_, _, _: &DateChanged, cx| cx.notify()),
            cx.subscribe(&date_to, |_, _, _: &DateChanged, cx| cx.notify()),
        ];
        let mut page = CommissionsPage {
            rev: 0,
            commissions: vec![],
            invoices: vec![],
            view: View::Jobs,
            stage: Stage::Owed,
            selected: BTreeSet::new(),
            job_filter: "all".into(),
            date_from,
            date_to,
            search,
            form: None,
            form_sub: None,
            generate: None,
            generate_sub: None,
            view_invoice: None,
            delete_id: None,
            delete_invoice_id: None,
            menu: None,
            _subs: subs,
        };
        page.reload(cx);
        page
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        let db = store::db(cx);
        match (db.list_commissions(), db.list_commission_invoices()) {
            (Ok(c), Ok(inv)) => {
                self.commissions = c;
                self.invoices = inv;
            }
            (Err(e), _) | (_, Err(e)) => toast_error(format!("Failed to load: {e}"), cx),
        }
        let ids: BTreeSet<i64> = self.commissions.iter().map(|c| c.id).collect();
        self.selected.retain(|id| ids.contains(id));
        self.rev = store::rev(cx);
    }

    fn ensure_fresh(&mut self, cx: &mut Context<Self>) {
        if self.rev != store::rev(cx) {
            self.reload(cx);
        }
    }

    /// Jobs passing the search, type and date filters (stats and tabs use these).
    fn base_filtered(&self, cx: &App) -> Vec<Commission> {
        let iso = |d: Option<chrono::NaiveDate>| d.map(|d| d.format("%Y-%m-%d").to_string());
        let from = iso(self.date_from.read(cx).date_value());
        let to = iso(self.date_to.read(cx).date_value());
        let q = self.search.read(cx).value().to_lowercase();
        self.commissions
            .iter()
            .filter(|c| {
                if self.job_filter != "all" && c.job_type != self.job_filter {
                    return false;
                }
                let day = c.appointment_date.as_deref().unwrap_or("");
                if from.as_deref().is_some_and(|f| day < f) || to.as_deref().is_some_and(|t| day > t) {
                    return false;
                }
                q.is_empty() || c.client_name.to_lowercase().contains(&q) || c.closer_name.to_lowercase().contains(&q)
            })
            .cloned()
            .collect()
    }

    fn stats(base: &[Commission]) -> Stats {
        let sum = |f: fn(&Commission) -> bool| base.iter().filter(|c| f(c)).map(effective_or_zero).sum::<f64>();
        let (owed, invoiced, paid, expected) = (sum(is_owed), sum(is_invoiced), sum(is_paid), sum(is_active));
        let closed_deals = base.iter().filter(|c| is_earned(c)).count();
        let lost_deals = base.iter().filter(|c| c.status == "lost").count();
        let decided = closed_deals + lost_deals;
        let close_rate = if decided > 0 { closed_deals as f64 / decided as f64 * 100.0 } else { 0.0 };
        let avg_per_closed = if closed_deals > 0 { (owed + invoiced + paid) / closed_deals as f64 } else { 0.0 };
        Stats { owed, invoiced, paid, expected, closed_deals, lost_deals, close_rate, avg_per_closed }
    }

    // ---- Add / edit job ----

    fn open_form(&mut self, job: Option<Commission>, window: &mut Window, cx: &mut Context<Self>) {
        let modal = cx.new(|cx| JobFormModal::new(job, window, cx));
        self.form_sub = Some(cx.subscribe(&modal, |p, _, _: &JobFormEvent, cx| {
            p.form = None;
            cx.notify();
        }));
        self.form = Some(modal);
        cx.notify();
    }

    // ---- Invoice generation ----

    fn open_generate(&mut self, restrict: Option<Vec<i64>>, cx: &mut Context<Self>) {
        let jobs = self.commissions.clone();
        let modal = cx.new(|cx| GenerateModal::new(jobs, restrict, cx));
        self.generate_sub = Some(cx.subscribe(&modal, |p, _, e: &GenerateEvent, cx| {
            p.generate = None;
            if let GenerateEvent::Created = e {
                p.selected.clear();
                p.view = View::Invoices;
            }
            cx.notify();
        }));
        self.generate = Some(modal);
        cx.notify();
    }

    // ---- Quick & bulk status actions ----

    fn quick_patch(&mut self, id: i64, patch: CommissionStatusPatch, done: &'static str, cx: &mut Context<Self>) {
        match store::write(cx, |db| db.patch_commission(id, &patch)) {
            Ok(_) => toast_success(done, cx),
            Err(e) => toast_error(format!("Failed: {e}"), cx),
        }
    }

    fn mark_paid(&mut self, c: &Commission, cx: &mut Context<Self>) {
        let invoice_status = if c.invoice_status.as_deref() == Some("invoiced") { Some("paid".to_string()) } else { c.invoice_status.clone() };
        let patch = CommissionStatusPatch {
            payment_status: Some(Some("paid".into())),
            status: Some(Some("paid".into())),
            paid_at: Some(Some(today_iso())),
            invoice_status: Some(invoice_status),
            ..Default::default()
        };
        self.quick_patch(c.id, patch, "Marked paid", cx);
    }

    fn mark_closed(&mut self, id: i64, cx: &mut Context<Self>) {
        self.quick_patch(id, CommissionStatusPatch { status: Some(Some("closed_waiting".into())), ..Default::default() }, "Marked closed", cx);
    }

    fn bulk_patch(&mut self, patch: CommissionStatusPatch, label: &str, cx: &mut Context<Self>) {
        let ids: Vec<i64> = self.selected.iter().copied().collect();
        if ids.is_empty() {
            return;
        }
        match store::write(cx, |db| db.bulk_patch_commissions(&ids, &patch)) {
            Ok(_) => {
                self.selected.clear();
                toast_success(format!("{} {label}", ids.len()), cx);
            }
            Err(e) => toast_error(format!("Failed: {e}"), cx),
        }
        cx.notify();
    }

    // ---- Invoice list actions ----

    /// Asks with the Mac's own alert, as `confirm()` does in the app.
    fn confirm_invoice_paid(&mut self, number: String, job_count: i64, id: i64, close_detail: bool, window: &mut Window, cx: &mut Context<Self>) {
        let answer = window.prompt(
            PromptLevel::Info,
            &format!("Mark {number} paid? All {job_count} jobs on it will be marked paid."),
            None,
            &[PromptButton::ok("OK"), PromptButton::cancel("Cancel")],
            cx,
        );
        cx.spawn(async move |this, cx| {
            let ok = answer.await == Ok(0);
            let _ = this.update(cx, |p, cx| {
                if ok {
                    match store::write(cx, |db| db.update_commission_invoice_status(id, "paid")) {
                        Ok(_) => toast_success("Invoice marked paid", cx),
                        Err(e) => toast_error(format!("Failed: {e}"), cx),
                    }
                }
                if close_detail {
                    p.view_invoice = None;
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn open_invoice_detail(&mut self, id: i64, cx: &mut Context<Self>) {
        match store::db(cx).get_commission_invoice(id) {
            Ok(full) => self.view_invoice = full,
            Err(e) => toast_error(format!("Failed: {e}"), cx),
        }
        cx.notify();
    }

    fn export_csv(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ensure_fresh(cx);
        let visible: Vec<Commission> = self.base_filtered(cx).into_iter().filter(|c| in_stage(c, self.stage)).collect();
        if visible.is_empty() {
            toast_error("No commissions to export", cx);
            return;
        }
        let headers = [
            "client",
            "job_type",
            "appointment_date",
            "closer",
            "status",
            "payment_status",
            "invoice_status",
            "system_size_kw",
            "contract_amount",
            "payout_state",
            "commission",
            "notes",
        ]
        .map(String::from)
        .to_vec();
        let rows = visible
            .iter()
            .map(|c| {
                let (label, value, _) = payout_info(c);
                vec![
                    c.client_name.clone(),
                    c.job_type.clone(),
                    c.appointment_date.clone().unwrap_or_default(),
                    c.closer_name.clone(),
                    status_label(&c.status),
                    payment_label(&c.payment_status),
                    c.invoice_status.clone().unwrap_or_else(|| "not_invoiced".into()),
                    c.system_size_kw.map(js_string).unwrap_or_default(),
                    c.contract_amount.map(js_string).unwrap_or_default(),
                    label.to_string(),
                    value.map(|v| format!("{v:.2}")).unwrap_or_else(|| "Needs Review".into()),
                    c.notes.clone(),
                ]
            })
            .collect();
        crate::export::csv(headers, rows, "commissions.csv", window, cx);
    }

    fn close_menu_fn(this: &WeakEntity<Self>) -> Rc<dyn Fn(&mut Window, &mut App)> {
        let this = this.clone();
        Rc::new(move |_, cx| {
            let _ = this.update(cx, |p, cx| {
                p.menu = None;
                cx.notify();
            });
        })
    }

    /// The "…" button and, while open, its menu.
    fn row_menu(&self, key: OpenMenu, id_str: String, items: Vec<Option<MenuItem>>, cx: &mut Context<Self>) -> AnyElement {
        let open = self.menu == Some(key);
        let t = theme(cx);
        let close = Self::close_menu_fn(&cx.weak_entity());
        let mut el = div().relative().child(
            Button::icon_button(SharedString::from(format!("{id_str}-btn")), Lucide::MoreHorizontal)
                .small()
                .selected(open)
                // Button leaves its icon uncoloured (GPUI svgs don't inherit text colour)
                .icon_color(if open { t.fg } else { t.fg3 })
                .tooltip("More actions")
                .on_click(cx.listener(move |p, _, _, cx| {
                    cx.stop_propagation();
                    p.menu = if p.menu == Some(key) { None } else { Some(key) };
                    cx.notify();
                })),
        );
        if open {
            let list = menu_list(&id_str, items, close.clone(), cx);
            el = el.child(popover(SharedString::from(format!("{id_str}-pop")), Align::End, None, list, move |w, cx| close(w, cx), cx));
        }
        el.into_any_element()
    }

    // ---- Rendering ----

    fn jobs_view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let t = theme(cx);
        let base = self.base_filtered(cx);
        let visible: Vec<&Commission> = base.iter().filter(|c| in_stage(c, self.stage)).collect();
        let s = Self::stats(&base);
        let count = |st: Stage| if st == Stage::All { base.len() } else { base.iter().filter(|c| in_stage(c, st)).count() };
        let this = cx.weak_entity();

        let metric = |id: &'static str, label: &'static str, amount: f64, sub: &'static str, stage: Stage, cx: &mut Context<Self>| {
            Metric::new(id, label, money_figure(amount, 22.0, t.fg)).sub(sub).active(self.stage == stage).on_click(cx.listener(move |p, _, _, cx| {
                p.stage = stage;
                cx.notify();
            }))
        };
        let metrics = metric_strip(vec![
            metric("comm-m-owed", "Owed", s.owed, "Closed, not invoiced or paid", Stage::Owed, cx),
            metric("comm-m-invoiced", "Invoiced", s.invoiced, "On an invoice, unpaid", Stage::Invoiced, cx),
            metric("comm-m-paid", "Paid", s.paid, "Paid out", Stage::Paid, cx),
            metric("comm-m-expected", "Expected", s.expected, "If active jobs close", Stage::Active, cx),
        ]);

        let stat = |label: &str, value: String| {
            div()
                .flex()
                .items_baseline()
                .child(text(format!("{label} "), 12.0, 500, t.fg3).line_height(px(16.)))
                .child(div().font(num_font(600)).text_size(px(12.)).line_height(px(16.)).text_color(t.fg2).child(value))
        };
        let stats_row = div()
            .flex()
            .items_center()
            .gap(r(5.))
            .px(r(1.))
            .mb(r(5.))
            .child(stat("Closed deals", s.closed_deals.to_string()))
            .child(stat("Lost", s.lost_deals.to_string()))
            .child(stat("Close rate", format!("{}%", js_round(s.close_rate))))
            .child(stat("Average per closed deal", money(s.avg_per_closed)));

        let ts = this.clone();
        let tf = this.clone();
        let filters = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(r(2.))
            .mb(r(4.))
            .child(segmented(
                "comm-stage",
                &self.stage,
                STAGE_TABS.iter().map(|(v, l)| Segment::new(*v, *l).count(count(*v))).collect(),
                move |v, _, cx| {
                    let _ = ts.update(cx, |p, cx| {
                        p.stage = v;
                        p.selected.clear();
                        cx.notify();
                    });
                },
                cx,
            ))
            .child(
                div()
                    .ml_auto()
                    .flex()
                    .items_center()
                    .gap(r(2.))
                    .child(div().w(r(32.)).child(crate::components::select(
                        "comm-type-filter",
                        vec![("all".to_string(), "All types".into()), ("solar".to_string(), "Solar".into()), ("roofing".to_string(), "Roofing".into())],
                        &self.job_filter,
                        move |v, _, cx| {
                            let _ = tf.update(cx, |p, cx| {
                                p.job_filter = v;
                                cx.notify();
                            });
                        },
                        cx,
                    )))
                    .child(div().w(px(140.)).child(self.date_from.clone()))
                    .child(text("–", 13.0, 500, t.fg4))
                    .child(div().w(px(140.)).child(self.date_to.clone()))
                    .child(div().w(r(52.)).child(search_field("comm-search-clear", &self.search, cx))),
            );

        let body: AnyElement = if visible.is_empty() {
            let first = self.commissions.is_empty();
            let mut empty = EmptyState::new(
                "comm-empty",
                if first { "Add your first job".to_string() } else { format!("Nothing in {}", stage_label(self.stage)) },
                if first { "Log a solar or roofing appointment to start tracking payouts." } else { "Try another tab or clear the filters." },
            );
            if first {
                empty = empty.action("Add job", cx.listener(|p, _, w, cx| p.open_form(None, w, cx)));
            }
            card(&t).child(empty).into_any_element()
        } else {
            self.jobs_table(&visible, window, cx).into_any_element()
        };

        vec![
            rise_in("comm-rise-1", 1, metrics.mb(r(3.))),
            rise_in("comm-rise-2", 2, stats_row),
            rise_in("comm-rise-3", 3, filters),
            rise_in("comm-rise-4", 4, body),
        ]
    }

    fn jobs_table(&mut self, visible: &[&Commission], window: &mut Window, cx: &mut Context<Self>) -> gpui::Div {
        let t = theme(cx);
        let all_selected = !visible.is_empty() && visible.iter().all(|c| self.selected.contains(&c.id));
        let visible_ids: Vec<i64> = visible.iter().map(|c| c.id).collect();
        let columns = vec![
            Column::new("").fixed(r(8.).into()),
            Column::new("Client"),
            Column::new("Type"),
            Column::new("Appointment"),
            Column::new("Status"),
            Column::new("Payout").right(),
            Column::new("").fixed(r(40.).into()),
        ];
        let headers = vec![
            (
                checkbox(&t, "comm-select-all", all_selected, false)
                    .on_click(cx.listener(move |p, _, _, cx| {
                        if all_selected {
                            p.selected.clear();
                        } else {
                            p.selected = visible_ids.iter().copied().collect();
                        }
                        cx.notify();
                    }))
                    .into_any_element(),
                17.0,
            ),
            header_text(&t, window, "Client"),
            header_text(&t, window, "Type"),
            header_text(&t, window, "Appointment"),
            header_text(&t, window, "Status"),
            header_text(&t, window, "Payout"),
            (div().into_any_element(), 0.0),
        ];
        let this = cx.weak_entity();
        let mut rows = Vec::new();
        for &c in visible {
            let id = c.id;
            let is_selected = self.selected.contains(&id);
            let group: SharedString = format!("comm-row-{id}").into();

            let check = Cell::new(
                checkbox(&t, ElementId::Name(format!("comm-check-{id}").into()), is_selected, false).on_click(cx.listener(move |p, _, _, cx| {
                    if !p.selected.remove(&id) {
                        p.selected.insert(id);
                    }
                    cx.notify();
                })),
                17.0,
            );

            let closer = if c.closer_name.is_empty() { "No closer".to_string() } else { c.closer_name.clone() };
            let client_w = measure(window, &c.client_name, 13.0, 500, false).max(measure(window, &closer, 12.0, 500, false));
            let client = Cell::new(
                div()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(text(c.client_name.clone(), 13.0, 500, t.fg).line_height(px(18.)).truncate())
                    .child(text(closer, 12.0, 500, t.fg3).line_height(px(16.)).truncate()),
                client_w,
            );

            let kind = Cell::new(job_type_tag(&t, &c.job_type), job_type_tag_width(window, &c.job_type));
            let appointment = text_cell(window, appointment_day(c), 14.0, 500, t.fg2, false);
            let label = status_label(&c.status);
            let status = Cell::new(pill(status_color(&t, &c.status), label.clone()), pill_width(window, &label));

            let (payout_label, payout_value, tone) = payout_info(c);
            let payout = match payout_value {
                None => Cell::new(
                    div()
                        .flex()
                        .items_center()
                        .gap(r(1.))
                        .child(icon(Lucide::AlertTriangle).size(r(3.5)).text_color(t.amber))
                        .child(text("Review", 13.0, 500, t.amber).line_height(px(18.)).whitespace_nowrap()),
                    f32::from(r(3.5)) + f32::from(r(1.)) + measure(window, "Review", 13.0, 500, false),
                ),
                Some(v) => {
                    let m = money(v);
                    let w = measure(window, &m, 13.0, 600, true).max(measure(window, payout_label, 11.0, 500, false));
                    Cell::new(
                        div()
                            .flex()
                            .flex_col()
                            .items_end()
                            .whitespace_nowrap()
                            .child(div().font(num_font(600)).text_size(px(13.)).line_height(px(18.)).text_color(tone_color(&t, tone)).child(m))
                            .child(text(payout_label, 11.0, 500, t.fg3).line_height(px(14.))),
                        w,
                    )
                }
            };

            // Actions: Paid, Mark closed (on hover), and the menu
            let closed_unpaid = is_owed(c) || is_invoiced(c);
            let can_close = is_active(c) && c.status != "needs_review";
            let mut actions = div().w_full().flex().items_center().gap(r(1.)).justify_end();
            let mut actions_w = 27.0;
            if closed_unpaid {
                let job = c.clone();
                actions = actions.child(Button::secondary(SharedString::from(format!("comm-paid-{id}")), "Paid").small().on_click(cx.listener(move |p, _, _, cx| p.mark_paid(&job, cx))));
                actions_w += small_button_width(window, "Paid", false) + f32::from(r(1.));
            }
            if can_close {
                actions = actions.child(
                    div()
                        .opacity(0.)
                        .group_hover(group.clone(), |s| s.opacity(1.))
                        .child(Button::ghost(SharedString::from(format!("comm-close-{id}")), "Mark closed").small().on_click(cx.listener(move |p, _, _, cx| p.mark_closed(id, cx)))),
                );
                actions_w += small_button_width(window, "Mark closed", false) + f32::from(r(1.));
            }
            let (te, td) = (this.clone(), this.clone());
            let edit_job = c.clone();
            let items = vec![
                Some(
                    MenuItem::new("Edit", move |w, cx| {
                        let job = edit_job.clone();
                        let _ = te.update(cx, |p, cx| p.open_form(Some(job), w, cx));
                    })
                    .icon(Lucide::Pencil),
                ),
                None,
                Some(
                    MenuItem::new("Delete", move |_, cx| {
                        let _ = td.update(cx, |p, cx| {
                            p.delete_id = Some(id);
                            cx.notify();
                        });
                    })
                    .icon(Lucide::Trash2)
                    .danger(),
                ),
            ];
            actions = actions.child(self.row_menu(OpenMenu::Job(id), format!("comm-job-menu-{id}"), items, cx));

            let edit = c.clone();
            let look = RowLook {
                background: is_selected.then(|| t.accent.opacity(0.05)),
                hover: (!is_selected).then(|| t.fg.opacity(0.02)),
                pointer: false,
                group: Some(group),
                on_click: Some(Box::new({
                    let this = this.clone();
                    move |e: &gpui::ClickEvent, w: &mut Window, cx: &mut App| {
                        if e.click_count() == 2 {
                            let job = edit.clone();
                            let _ = this.update(cx, |p, cx| p.open_form(Some(job), w, cx));
                        }
                    }
                })),
            };
            rows.push((
                Row::new(ElementId::Name(format!("comm-job-{id}").into()), vec![check, client, kind, appointment, status, payout, Cell::new(actions, actions_w)]),
                look,
            ));
        }
        let width = page_width(window) - 2.0;
        card(&t).overflow_hidden().child(grid(&t, columns, headers, rows, width))
    }

    fn invoices_view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let t = theme(cx);
        if self.invoices.is_empty() {
            return card(&t)
                .child(
                    EmptyState::new("comm-inv-empty", "No commission invoices yet", "Bundle closed solar or roofing jobs into one payout invoice.")
                        .action("New invoice", cx.listener(|p, _, _, cx| p.open_generate(None, cx))),
                )
                .into_any_element();
        }
        let columns = vec![
            Column::new("Invoice"),
            Column::new("Jobs"),
            Column::new("Created"),
            Column::new("Status"),
            Column::new("Total").right(),
            Column::new("").fixed(r(44.).into()),
        ];
        let headers = vec![
            header_text(&t, window, "Invoice"),
            header_text(&t, window, "Jobs"),
            header_text(&t, window, "Created"),
            header_text(&t, window, "Status"),
            header_text(&t, window, "Total"),
            (div().into_any_element(), 0.0),
        ];
        let this = cx.weak_entity();
        let mut rows = Vec::new();
        for inv in self.invoices.clone() {
            let id = inv.id;
            let category = category_label(&inv.category);
            let number_w = measure(window, &inv.invoice_number, 13.0, 500, false).max(measure(window, category, 12.0, 500, false));
            let invoice = Cell::new(
                div()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(text(inv.invoice_number.clone(), 13.0, 500, t.fg).line_height(px(18.)).truncate())
                    .child(text(category, 12.0, 500, t.fg3).line_height(px(16.)).truncate()),
                number_w,
            );
            let jobs = text_cell(window, inv.job_count.to_string(), 14.0, 500, t.fg2, true);
            let created = text_cell(window, format_day(&inv.created_at), 14.0, 500, t.fg2, false);
            let status_text = capitalize(&inv.status);
            let status = Cell::new(pill(invoice_status_color(&t, &inv.status), status_text.clone()), pill_width(window, &status_text));
            let total = text_cell(window, money(inv.total), 14.0, 600, t.fg, true);

            let mut actions = div().w_full().flex().items_center().justify_end().gap(r(1.));
            let mut actions_w = 27.0;
            if inv.status != "paid" && inv.status != "cancelled" {
                let (number, count) = (inv.invoice_number.clone(), inv.job_count);
                actions = actions.child(
                    Button::secondary(SharedString::from(format!("comm-inv-paid-{id}")), "Mark paid")
                        .small()
                        .icon(Lucide::CheckCircle2)
                        .icon_color(t.fg)
                        .on_click(cx.listener(move |p, _, w, cx| p.confirm_invoice_paid(number.clone(), count, id, false, w, cx))),
                );
                actions_w += small_button_width(window, "Mark paid", true) + f32::from(r(1.));
            }
            let (tv, tdel) = (this.clone(), this.clone());
            let items = vec![
                Some(
                    MenuItem::new("View jobs", move |_, cx| {
                        let _ = tv.update(cx, |p, cx| p.open_invoice_detail(id, cx));
                    })
                    .icon(Lucide::Eye),
                ),
                Some(
                    MenuItem::new("Download PDF", move |w, cx| crate::export::commission_invoice_pdf(id, w, cx))
                    .icon(Lucide::Download),
                ),
                None,
                Some(
                    MenuItem::new("Delete", move |_, cx| {
                        let _ = tdel.update(cx, |p, cx| {
                            p.delete_invoice_id = Some(id);
                            cx.notify();
                        });
                    })
                    .icon(Lucide::Trash2)
                    .danger(),
                ),
            ];
            actions = actions.child(self.row_menu(OpenMenu::Invoice(id), format!("comm-inv-menu-{id}"), items, cx));
            // The actions cell doesn't open the invoice (stopPropagation)
            let actions = div().w_full().on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation()).child(actions);

            let to = this.clone();
            rows.push((
                Row::new(ElementId::Name(format!("comm-inv-{id}").into()), vec![invoice, jobs, created, status, total, Cell::new(actions, actions_w)]),
                RowLook {
                    hover: Some(t.fg.opacity(0.03)),
                    pointer: true,
                    on_click: Some(Box::new(move |_, _, cx| {
                        let _ = to.update(cx, |p, cx| p.open_invoice_detail(id, cx));
                    })),
                    ..Default::default()
                },
            ));
        }
        let width = page_width(window) - 2.0;
        card(&t).overflow_hidden().child(grid(&t, columns, headers, rows, width)).into_any_element()
    }

    fn invoice_detail(&mut self, inv: &CommissionInvoiceDetail, cx: &mut Context<Self>) -> AnyElement {
        let t = theme(cx);
        let n = inv.job_count;
        let description = format!("{} · {n} job{} · {}", category_label(&inv.category), if n == 1 { "" } else { "s" }, money(inv.total));
        let this = cx.weak_entity();
        let close = this.clone();
        let mut list = div().id("comm-inv-detail-list").rounded(r(2.)).border_1().border_color(t.line).max_h(px(360.)).overflow_y_scroll().flex().flex_col();
        let count = inv.jobs.len();
        for (i, j) in inv.jobs.iter().enumerate() {
            let mut row = div()
                .flex_none()
                .flex()
                .items_center()
                .gap(r(3.))
                .px(r(3.))
                .h(if i + 1 < count { r(12.) - px(1.) } else { r(12.) })
                .child(job_type_tag(&t, &j.job_type))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(text(j.client_name.clone(), 13.0, 500, t.fg).line_height(px(18.)).truncate())
                        .child(text(job_detail(j), 12.0, 500, t.fg3).line_height(px(16.))),
                )
                .child(div().font(num_font(600)).text_size(px(13.)).line_height(px(18.)).text_color(t.fg).whitespace_nowrap().child(money(effective_or_zero(j))));
            if i + 1 < count {
                row = row.border_b_1().border_color(t.line);
            }
            list = list.child(row);
        }
        let id = inv.id;
        let mut modal = Modal::new("comm-inv-detail", inv.invoice_number.clone(), move |_, cx| {
            let _ = close.update(cx, |p, cx| {
                p.view_invoice = None;
                cx.notify();
            });
        })
        .size(ModalSize::Lg)
        .description(SharedString::from(description))
        .child(list)
        .footer(Button::secondary("comm-inv-detail-pdf", "Download PDF").on_click(move |_, w, cx| crate::export::commission_invoice_pdf(id, w, cx)));
        if inv.status != "paid" && inv.status != "cancelled" {
            let (number, jobs) = (inv.invoice_number.clone(), inv.job_count);
            modal = modal.footer(
                Button::primary("comm-inv-detail-paid", "Mark paid")
                    .icon(Lucide::CheckCircle2)
                    .icon_color(crate::theme::hex(0xffffff))
                    .on_click(cx.listener(move |p, _, w, cx| p.confirm_invoice_paid(number.clone(), jobs, id, true, w, cx))),
            );
        }
        modal.into_any_element()
    }

    /// The floating bar while jobs are selected (bottom, centred on the page area).
    fn bulk_bar(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let t = theme(cx);
        let n = self.selected.len();
        let vp = window.viewport_size();
        let bar = div()
            .id("comm-bulk")
            .occlude()
            .flex()
            .items_center()
            .gap(r(2.))
            .pl(r(4.))
            .pr(r(2.))
            .h(r(12.))
            .rounded(px(10.))
            .bg(t.panel)
            .shadow(t.shadow_pop())
            .child(text(format!("{n} selected"), 13.0, 500, t.fg).line_height(px(18.)).mr(r(1.)))
            .child(Button::ghost("comm-bulk-close", "Mark closed").small().on_click(cx.listener(|p, _, _, cx| {
                p.bulk_patch(CommissionStatusPatch { status: Some(Some("closed_waiting".into())), ..Default::default() }, "marked closed", cx)
            })))
            .child(Button::secondary("comm-bulk-invoice", "Invoice them").small().on_click(cx.listener(|p, _, _, cx| {
                let ids: Vec<i64> = p.selected.iter().copied().collect();
                p.open_generate(Some(ids), cx);
            })))
            .child(Button::primary("comm-bulk-paid", "Mark paid").small().on_click(cx.listener(|p, _, _, cx| {
                let patch = CommissionStatusPatch {
                    payment_status: Some(Some("paid".into())),
                    status: Some(Some("paid".into())),
                    paid_at: Some(Some(today_iso())),
                    ..Default::default()
                };
                p.bulk_patch(patch, "marked paid", cx)
            })))
            .child(Button::icon_button("comm-bulk-clear", Lucide::X).small().icon_color(t.fg3).tooltip("Clear selection").on_click(cx.listener(|p, _, _, cx| {
                p.selected.clear();
                cx.notify();
            })));
        let bar: AnyElement = if motion::reduced() {
            bar.into_any_element()
        } else {
            bar.with_animation("comm-bulk-in", Animation::new(Duration::from_millis(150)), |b, d| {
                let e = 1.0 - (1.0 - d).powi(2);
                b.opacity(e).relative().top(px(12.0 * (1.0 - e)))
            })
            .into_any_element()
        };
        // fixed bottom-5 left-[calc(50%+110px)] -translate-x-1/2
        let layer = div().w(vp.width).h(vp.height).flex().flex_col().justify_end().items_center().pb(r(5.)).pl(px(220.)).child(bar);
        deferred(anchored().position(point(px(0.), px(0.))).child(layer)).with_priority(5).into_any_element()
    }
}

impl Page for CommissionsPage {
    fn header(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> Header {
        let mut actions = div().flex().items_center().gap(r(2.));
        if self.view == View::Jobs {
            actions = actions.child(Button::secondary("comm-csv", "CSV").on_click(cx.listener(|p, _, w, cx| p.export_csv(w, cx))));
        }
        actions = actions
            .child(Button::secondary("comm-new-invoice", "New invoice").on_click(cx.listener(|p, _, _, cx| p.open_generate(None, cx))))
            .child(Button::secondary("comm-add-job", "Add job").on_click(cx.listener(|p, _, w, cx| p.open_form(None, w, cx))));
        Header::new("Commissions").actions(actions)
    }
}

impl Render for CommissionsPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.ensure_fresh(cx);
        let t = theme(cx);
        let this = cx.weak_entity();

        let tv = this.clone();
        let view_row = div()
            .flex()
            .items_center()
            .gap(r(3.))
            .mb(r(5.))
            .child(segmented(
                "comm-view",
                &self.view,
                vec![Segment::new(View::Jobs, "Jobs").count(self.commissions.len()), Segment::new(View::Invoices, "Invoices").count(self.invoices.len())],
                move |v, _, cx| {
                    let _ = tv.update(cx, |p, cx| {
                        p.view = v;
                        p.menu = None;
                        cx.notify();
                    });
                },
                cx,
            ))
            .child(text("Solar and roofing appointment payouts", 13.0, 500, t.fg3).line_height(px(18.)));

        let mut page = div().w_full().max_w(px(1160.)).mx_auto().px(r(7.)).pt(r(6.)).pb(r(14.)).flex().flex_col().child(rise_in("comm-rise-0", 0, view_row));
        match self.view {
            View::Jobs => {
                page = page.children(self.jobs_view(window, cx));
                if !self.selected.is_empty() {
                    page = page.child(self.bulk_bar(window, cx));
                }
            }
            View::Invoices => {
                let body = self.invoices_view(window, cx);
                page = page.child(rise_in("comm-inv-rise-1", 1, body));
            }
        }

        page = page.children(self.form.clone()).children(self.generate.clone());
        if let Some(inv) = self.view_invoice.clone() {
            page = page.child(self.invoice_detail(&inv, cx));
        }
        if let Some(id) = self.delete_id {
            let tx = this.clone();
            page = page.child(confirm_dialog(
                "comm-delete-job",
                "Delete this job?",
                "This removes the commission job for good.",
                "Delete",
                true,
                move |_, cx| match store::write(cx, |db| db.delete_commission(id)) {
                    Ok(_) => toast_success("Commission deleted", cx),
                    Err(e) => toast_error(format!("Failed: {e}"), cx),
                },
                move |_, cx| {
                    let _ = tx.update(cx, |p, cx| {
                        p.delete_id = None;
                        cx.notify();
                    });
                },
                cx,
            ));
        }
        if let Some(id) = self.delete_invoice_id {
            let tx = this.clone();
            page = page.child(confirm_dialog(
                "comm-delete-invoice",
                "Delete this invoice?",
                "Its unpaid jobs go back to Owed so you can invoice them again.",
                "Delete",
                true,
                move |_, cx| match store::write(cx, |db| db.delete_commission_invoice(id)) {
                    Ok(_) => toast_success("Invoice deleted", cx),
                    Err(e) => toast_error(format!("Failed: {e}"), cx),
                },
                move |_, cx| {
                    let _ = tx.update(cx, |p, cx| {
                        p.delete_invoice_id = None;
                        cx.notify();
                    });
                },
                cx,
            ));
        }
        page
    }
}
