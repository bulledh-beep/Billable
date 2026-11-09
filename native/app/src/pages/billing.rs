//! Billing.tsx: the money pipeline, things that need attention, unbilled
//! time by client, and every invoice with status filters, bulk actions,
//! row menus and CSV export.

mod invoices;

use std::{collections::HashSet, rc::Rc, time::Duration};

use billable_core::models::{AttentionItem, BillingOverview, Client, Invoice};
use chrono::{Datelike, Local};
use gpui::{
    div, img, prelude::*, px, Animation, AnimationExt as _, AnyElement, App, Context, Div, ElementId, Entity, FocusHandle, Hsla,
    IntoElement, ParentElement, Render, SharedString, Styled, Subscription, Window,
};
use gpui_component::input::{InputEvent, InputState};

use crate::{
    art::{Art, Gallery, Icon},
    components::{
        client_avatar, count_badge, money, metric_strip,
        record_payment::{burst_layer, RecordPaymentEvent, RecordPaymentModal},
        status_badge, toast_error, toast_success, Button, EmptyState, MergeClientsModal, MergeEvent, Metric, Modal, ModalSize,
        Tint,
    },
    components::button::Variant,
    export,
    format::{days_since, format_day, hours_short},
    motion::{self, rise_in},
    shell::{
        app::{navigate, Header, Page},
        route::Route,
    },
    store,
    style::{card, dot, num_font, r, section_title, separator, text},
    theme::{parse_hex, theme, Theme},
};

/// A pipeline figure counting to its value, and gliding when it changes
/// (Money's `animate`).
#[derive(Clone, Copy, Default)]
struct Glide {
    from: f64,
    to: f64,
    generation: u64,
    seen: bool,
}

impl Glide {
    fn aim(&mut self, value: f64) {
        if !self.seen || self.to != value {
            self.from = if self.seen { self.to } else { 0.0 };
            self.to = value;
            self.generation += 1;
            self.seen = true;
        }
    }
}

fn glide_money(id: &str, g: Glide, size: f32, color: Hsla) -> AnyElement {
    if motion::reduced() || g.from == g.to {
        return money(g.to, size, color);
    }
    let (from, to) = (g.from, g.to);
    div()
        .with_animation(SharedString::from(format!("{id}-{}", g.generation)), Animation::new(Duration::from_millis(700)), move |d, delta| {
            let eased = 1.0 - (1.0 - delta as f64).powi(3);
            d.child(money(from + (to - from) * eased, size, color))
        })
        .into_any_element()
}

pub struct BillingPage {
    rev: u64,
    data: Option<BillingOverview>,
    clients: Vec<Client>,
    filter: String,
    /// `None` is "All years".
    year: Option<i32>,
    search: Entity<InputState>,
    selected: HashSet<i64>,
    paying: Option<Entity<RecordPaymentModal>>,
    paying_sub: Option<Subscription>,
    deleting: Option<Invoice>,
    dont_bill: Option<AttentionItem>,
    merge: Option<Entity<MergeClientsModal>>,
    merge_sub: Option<Subscription>,
    /// The invoice whose "…" menu is open.
    menu: Option<i64>,
    glides: [Glide; 5],
    /// Keyboard focus for the confirm dialogs, so Escape closes them.
    focus: FocusHandle,
    _subs: Vec<Subscription>,
}

impl BillingPage {
    pub fn new(status: Option<String>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search invoices"));
        let subs = vec![cx.subscribe(&search, |_, _, e: &InputEvent, cx| {
            if matches!(e, InputEvent::Change) {
                cx.notify();
            }
        })];
        let mut page = BillingPage {
            rev: 0,
            data: None,
            clients: vec![],
            filter: status.unwrap_or_else(|| "all".into()),
            year: None,
            search,
            selected: HashSet::new(),
            paying: None,
            paying_sub: None,
            deleting: None,
            dont_bill: None,
            merge: None,
            merge_sub: None,
            menu: None,
            glides: [Glide::default(); 5],
            focus: cx.focus_handle(),
            _subs: subs,
        };
        page.reload(cx);
        page
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        let db = store::db(cx);
        if let Ok(o) = db.get_billing_overview() {
            self.data = Some(o);
        }
        self.clients = db.list_clients().unwrap_or_default();
        self.rev = store::rev(cx);
    }

    /// Invoices in the chosen year.
    fn by_year(&self) -> Vec<Invoice> {
        let invoices = self.data.as_ref().map(|d| d.invoices.as_slice()).unwrap_or_default();
        match self.year {
            None => invoices.to_vec(),
            Some(y) => invoices.iter().filter(|i| i.issue_date.starts_with(&y.to_string())).cloned().collect(),
        }
    }

    /// Invoices passing the year, status and search filters.
    fn visible(&self, cx: &App) -> Vec<Invoice> {
        let q = self.search.read(cx).value().trim().to_lowercase();
        self.by_year()
            .into_iter()
            .filter(|i| {
                (self.filter == "all" || i.status == self.filter)
                    && (q.is_empty()
                        || i.invoice_number.to_lowercase().contains(&q)
                        || i.client_name.as_deref().unwrap_or_default().to_lowercase().contains(&q)
                        || i.project_name.as_deref().unwrap_or_default().to_lowercase().contains(&q))
            })
            .collect()
    }

    fn show_invoices(&mut self, filter: &str, cx: &mut Context<Self>) {
        self.filter = filter.to_string();
        self.year = None;
        cx.notify();
    }

    // ---- Actions ----

    pub(crate) fn mark_sent(&mut self, ids: Vec<i64>, cx: &mut Context<Self>) {
        match store::write(cx, |db| db.mark_invoices_sent(&ids, None)) {
            Ok(n) => toast_success(if n == 1 { "Marked as sent".to_string() } else { format!("{n} invoices marked as sent") }, cx),
            Err(e) => toast_error(e.to_string(), cx),
        }
        self.selected.clear();
        cx.notify();
    }

    fn open_payment(&mut self, invoices: Vec<Invoice>, window: &mut Window, cx: &mut Context<Self>) {
        if invoices.is_empty() {
            return;
        }
        let modal = cx.new(|cx| RecordPaymentModal::new(invoices, window, cx));
        self.paying_sub = Some(cx.subscribe(&modal, |p, _, e: &RecordPaymentEvent, cx| {
            p.paying = None;
            if matches!(e, RecordPaymentEvent::Done) {
                p.selected.clear();
            }
            cx.notify();
        }));
        self.paying = Some(modal);
        cx.notify();
    }

    fn open_merge(&mut self, ids: &[i64], cx: &mut Context<Self>) {
        let (source, target) = MergeClientsModal::suggest(&self.clients, ids);
        let clients = self.clients.clone();
        let modal = cx.new(|cx| MergeClientsModal::new(clients, Some(source), Some(target), cx));
        self.merge_sub = Some(cx.subscribe(&modal, |p, _, _: &MergeEvent, cx| {
            p.merge = None;
            cx.notify();
        }));
        self.merge = Some(modal);
        cx.notify();
    }

    fn confirm_dont_bill(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.dont_bill.take() else { return };
        let Some(project_id) = item.project_id else { return };
        let entries = store::db(cx).list_time_entries(Some(project_id)).unwrap_or_default();
        // Finished time only, so a running timer on this project isn't touched
        let ids: Vec<i64> = entries.iter().filter(|e| e.entry.end_time.is_some() && e.billing_state == "unbilled").map(|e| e.entry.id).collect();
        match store::write(cx, |db| db.set_entries_billable(&ids, false)) {
            Ok(_) => toast_success(format!("{}: {} entr{} marked don't bill", item.title, ids.len(), if ids.len() == 1 { "y" } else { "ies" }), cx),
            Err(e) => toast_error(e.to_string(), cx),
        }
        cx.notify();
    }

    fn export_csv(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let visible = self.visible(cx);
        if visible.is_empty() {
            toast_error("No invoices to export", cx);
            return;
        }
        let headers: Vec<String> = [
            "invoice_number",
            "client",
            "project",
            "status",
            "issue_date",
            "due_date",
            "sent_date",
            "payment_date",
            "payment_method",
            "hours",
            "subtotal",
            "gst_hst",
            "total",
            "currency",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let rows: Vec<Vec<String>> = visible
            .iter()
            .map(|i| {
                vec![
                    i.invoice_number.clone(),
                    i.client_name.clone().unwrap_or_default(),
                    i.project_name.clone().unwrap_or_default(),
                    i.status.clone(),
                    i.issue_date.clone(),
                    i.due_date.clone(),
                    i.sent_at.clone().unwrap_or_default(),
                    i.payment_date.clone().unwrap_or_default(),
                    i.payment_method.clone().unwrap_or_default(),
                    fixed2(i.entry_hours),
                    fixed2(i.subtotal),
                    if i.gst_hst_applicable != 0 { fixed2(i.gst_hst_amount) } else { String::new() },
                    fixed2(i.total),
                    if i.currency.is_empty() { "CAD".into() } else { i.currency.clone() },
                ]
            })
            .collect();
        let filename = match self.year {
            None => "invoices.csv".to_string(),
            Some(y) => format!("invoices-{y}.csv"),
        };
        export::csv(headers, rows, &filename, window, cx);
    }
}

impl Page for BillingPage {
    fn header(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> Header {
        Header::new("Billing").actions(
            div()
                .flex()
                .items_center()
                .gap(r(2.))
                .child(Button::secondary("billing-export", "Export").on_click(cx.listener(|p, _, w, cx| p.export_csv(w, cx))))
                .child(
                    Button::secondary("billing-new", "New invoice")
                        .on_click(|_, w, cx| navigate(Route::InvoiceNew { client_id: None, project_id: None, project_ids: vec![] }, w, cx)),
                ),
        )
    }
}

/// `.group-head` with its items at the start (`justify-start`).
fn head_start(left: impl IntoElement, right: Option<AnyElement>) -> Div {
    div().flex().items_center().gap(r(3.)).min_h(px(26.)).mb(r(2.5)).px(r(0.5)).child(left).children(right)
}

/// JavaScript's `n.toFixed(2)`: the exact value rounded, ties away from zero.
pub(crate) fn fixed2(x: f64) -> String {
    if !x.is_finite() {
        return "0.00".into();
    }
    let exact = format!("{:.30}", x.abs());
    let (int, frac) = exact.split_once('.').unwrap_or((&exact, ""));
    let digits: Vec<u32> = frac.chars().filter_map(|c| c.to_digit(10)).collect();
    let mut cents: u128 = int.parse::<u128>().unwrap_or(0) * 100 + (digits.first().copied().unwrap_or(0) * 10 + digits.get(1).copied().unwrap_or(0)) as u128;
    if digits.get(2).copied().unwrap_or(0) >= 5 {
        cents += 1;
    }
    let sign = if x < 0.0 && cents > 0 { "-" } else { "" };
    format!("{sign}{}.{:02}", cents / 100, cents % 100)
}

fn plural(n: i64) -> &'static str {
    if n == 1 { "" } else { "s" }
}

impl Render for BillingPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.data.is_none() || self.rev != store::rev(cx) {
            self.reload(cx);
        }
        let t = theme(cx);
        let Some(data) = self.data.clone() else {
            return div().into_any_element();
        };
        let p = &data.pipeline;
        let year0 = Local::now().year();

        // Keep the selection to invoices that are still visible
        let visible = self.visible(cx);
        let visible_ids: HashSet<i64> = visible.iter().map(|i| i.id).collect();
        self.selected.retain(|id| visible_ids.contains(id));

        for (g, v) in self.glides.iter_mut().zip([p.unbilled_amount, p.draft_amount, p.awaiting_amount, p.overdue_amount, p.paid_ytd_amount]) {
            g.aim(v);
        }
        let gl = self.glides;
        let filter = self.filter.clone();

        // ---- The money pipeline ----
        let metrics = metric_strip(vec![
            Metric::new("bill-m-ready", "Ready to bill", glide_money("bill-m-ready-v", gl[0], 22.0, t.fg))
                .icon(Icon::Hourglass)
                .tint(Tint::Orange)
                .sub(if p.unbilled_hours > 0.0 {
                    format!("{} across {} project{}", hours_short(p.unbilled_hours), p.unbilled_projects, plural(p.unbilled_projects))
                } else {
                    "All caught up".into()
                })
                // scrollIntoView needs the shell's page scroll; see the port notes
                .on_click(|_, _, _| {}),
            Metric::new("bill-m-drafts", "Drafts", glide_money("bill-m-drafts-v", gl[1], 22.0, t.fg))
                .icon(Icon::Receipt)
                .tint(Tint::Purple)
                .sub(format!("{} not sent yet", p.draft_count))
                .active(filter == "draft")
                .on_click(cx.listener(|p, _, _, cx| p.show_invoices("draft", cx))),
            Metric::new("bill-m-awaiting", "Awaiting payment", glide_money("bill-m-awaiting-v", gl[2], 22.0, t.fg))
                .icon(Icon::Send)
                .tint(Tint::Blue)
                .sub(format!("{} invoice{} sent", p.awaiting_count, plural(p.awaiting_count)))
                .active(filter == "sent")
                .on_click(cx.listener(|p, _, _, cx| p.show_invoices("sent", cx))),
            Metric::new("bill-m-overdue", "Overdue", glide_money("bill-m-overdue-v", gl[3], 22.0, if p.overdue_amount > 0.0 { t.red } else { t.fg }))
                .icon(Icon::Alert)
                .tint(Tint::Red)
                .sub(if p.overdue_count > 0 { format!("{} past due", p.overdue_count) } else { "Nothing late".into() })
                .active(filter == "overdue")
                .on_click(cx.listener(|p, _, _, cx| p.show_invoices("overdue", cx))),
            Metric::new("bill-m-paid", format!("Paid in {year0}"), glide_money("bill-m-paid-v", gl[4], 22.0, t.fg))
                .icon(Icon::Trophy)
                .tint(Tint::Green)
                .sub(format!("{} invoice{}", p.paid_ytd_count, plural(p.paid_ytd_count)))
                .active(filter == "paid")
                .on_click(cx.listener(|p, _, _, cx| p.show_invoices("paid", cx))),
        ]);

        let mut blocks: Vec<AnyElement> = vec![metrics.mb(r(6.)).into_any_element()];
        if !data.attention.is_empty() {
            blocks.push(self.attention(&t, &data, cx).mb(r(6.)).into_any_element());
        }
        blocks.push(self.ready(&t, &data).mb(r(6.)).into_any_element());
        blocks.push(self.invoices_section(&t, &data, &visible, window, cx).into_any_element());

        let mut page = div().w_full().max_w(px(1160.)).mx_auto().px(r(7.)).pt(r(6.)).pb(r(14.)).flex().flex_col();
        for (i, b) in blocks.into_iter().enumerate() {
            page = page.child(rise_in(ElementId::Name(format!("bill-rise-{i}").into()), i, b));
        }

        // Bulk actions
        let selected: Vec<Invoice> = visible.iter().filter(|i| self.selected.contains(&i.id)).cloned().collect();
        if !selected.is_empty() {
            page = page.child(self.bulk_bar(&t, &selected, window, cx));
        }

        page = page.children(self.paying.clone()).children(self.merge.clone());
        if let Some(inv) = self.deleting.clone() {
            let this = cx.weak_entity();
            let close = this.clone();
            let shown = inv.clone();
            page = page.child(delete_dialog(
                "bill-delete",
                &self.focus,
                &shown,
                shown.entry_count,
                shown.entry_hours,
                move |_, cx| {
                    let number = inv.invoice_number.clone();
                    let id = inv.id;
                    match store::write(cx, |db| db.delete_invoice(id)) {
                        Ok(r) => toast_success(
                            if r.released_entries > 0 {
                                format!("{number} deleted. {} of time is unbilled again.", hours_short(r.released_hours))
                            } else {
                                format!("{number} deleted")
                            },
                            cx,
                        ),
                        Err(e) => toast_error(e.to_string(), cx),
                    }
                    let _ = this.update(cx, |p, cx| {
                        p.deleting = None;
                        cx.notify();
                    });
                },
                move |_, cx| {
                    let _ = close.update(cx, |p, cx| {
                        p.deleting = None;
                        cx.notify();
                    });
                },
                cx,
            ));
        }
        if let Some(item) = self.dont_bill.clone() {
            let this = cx.weak_entity();
            let close = this.clone();
            page = page.child(confirm(
                "bill-dont-bill",
                &self.focus,
                format!("Don't bill {}?", item.title),
                text(
                    format!(
                        "Its unbilled time ({}) is marked non-billable. It stays in your time log, just not as money owed. You can change entries back from the Time page.",
                        crate::format::money(item.amount.unwrap_or(0.0))
                    ),
                    13.0,
                    500,
                    t.fg2,
                )
                .line_height(px(20.))
                .into_any_element(),
                "Don't bill it",
                false,
                move |_, cx| {
                    let _ = this.update(cx, |p, cx| p.confirm_dont_bill(cx));
                },
                move |_, cx| {
                    let _ = close.update(cx, |p, cx| {
                        p.dont_bill = None;
                        cx.notify();
                    });
                },
                cx,
            ));
        }
        page.children(burst_layer(window)).into_any_element()
    }
}

impl BillingPage {
    fn attention(&self, t: &Theme, data: &BillingOverview, cx: &mut Context<Self>) -> Div {
        let items = &data.attention;
        let title = div().flex().items_center().child(section_title(t, "Needs attention"))// `align-middle` inside the title's line box sits the badge ~2px below centre
            .child(div().ml(r(2.)).relative().top(px(2.)).child(count_badge(t, items.len())));
        let mut list = card(t).overflow_hidden().flex().flex_col();
        for (i, item) in items.iter().enumerate() {
            let art = match item.kind.as_str() {
                "unbilled" => Icon::Hourglass,
                "overdue" => Icon::Alert,
                "draft" => Icon::Receipt,
                "accidental_timer" => Icon::Time,
                _ => Icon::Clients,
            };
            let image = cx.global_mut::<Gallery>().get(Art::Icon(art), 26.0);
            let mut row = div()
                .relative()
                .flex()
                .items_center()
                .gap(r(3.5))
                .px(r(4.))
                .h(px(60.))
                .child(img(image).size(r(8.)).flex_none())
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .child(text(item.title.clone(), 13.0, 500, t.fg).truncate().line_height(px(19.5)))
                        .child(text(item.detail.clone(), 12.0, 500, t.fg3).truncate().line_height(px(16.))),
                );
            if let Some(amount) = item.amount {
                row = row.child(
                    div()
                        .w(r(24.))
                        .flex_none()
                        .flex()
                        .justify_end()
                        .font(num_font(500))
                        .text_size(px(13.))
                        .text_color(t.fg2)
                        .child(crate::format::money(amount)),
                );
            }
            row = row.child(div().flex().flex_none().items_center().gap(r(1.5)).children(self.attention_actions(item, i, data, cx)));
            if i + 1 < items.len() {
                row = row.child(separator(t, 60.0));
            }
            list = list.child(row);
        }
        div().flex().flex_col().child(crate::style::group_head(title, div())).child(list)
    }

    fn attention_actions(&self, item: &AttentionItem, i: usize, data: &BillingOverview, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let id = |s: &str| SharedString::from(format!("att-{i}-{s}"));
        let ghost = |s: &str, label: &str| Button::ghost(id(s), label.to_string()).small();
        let secondary = |s: &str, label: &str| Button::secondary(id(s), label.to_string()).small();
        let it = item.clone();
        match item.kind.as_str() {
            "unbilled" => {
                let pid = item.project_id;
                vec![
                    ghost("dont", "Don't bill")
                        .on_click(cx.listener(move |p, _, _, cx| {
                            p.dont_bill = Some(it.clone());
                            cx.notify();
                        }))
                        .into_any_element(),
                    secondary("create", "Create invoice")
                        .on_click(move |_, w, cx| navigate(Route::InvoiceNew { client_id: None, project_id: pid, project_ids: vec![] }, w, cx))
                        .into_any_element(),
                ]
            }
            "overdue" => {
                let inv_id = item.invoice_id.unwrap_or(0);
                let inv = data.invoices.iter().find(|i| Some(i.id) == item.invoice_id).cloned();
                let mut v = vec![ghost("open", "Open").on_click(move |_, w, cx| navigate(Route::Invoice(inv_id), w, cx)).into_any_element()];
                if let Some(inv) = inv {
                    v.push(
                        secondary("pay", "Record payment")
                            .on_click(cx.listener(move |p, _, w, cx| p.open_payment(vec![inv.clone()], w, cx)))
                            .into_any_element(),
                    );
                }
                v
            }
            "draft" => {
                let inv_id = item.invoice_id.unwrap_or(0);
                vec![
                    ghost("open", "Open").on_click(move |_, w, cx| navigate(Route::Invoice(inv_id), w, cx)).into_any_element(),
                    secondary("sent", "Mark as sent").on_click(cx.listener(move |p, _, _, cx| p.mark_sent(vec![inv_id], cx))).into_any_element(),
                ]
            }
            "accidental_timer" => {
                let key = item.key.clone();
                let entry = item.entry_id;
                vec![
                    ghost("keep", "Keep it")
                        .on_click(move |_, _, cx| {
                            if let Err(e) = store::write(cx, |db| db.dismiss_attention(&key)) {
                                toast_error(e.to_string(), cx);
                            }
                        })
                        .into_any_element(),
                    secondary("remove", "Remove")
                        .on_click(move |_, _, cx| {
                            let Some(entry) = entry else { return };
                            match store::write(cx, |db| db.delete_time_entry(entry)) {
                                Ok(_) => toast_success("Accidental timer removed", cx),
                                Err(e) => toast_error(e.to_string(), cx),
                            }
                        })
                        .into_any_element(),
                ]
            }
            "duplicate_clients" => {
                let key = item.key.clone();
                let ids = item.client_ids.clone().unwrap_or_default();
                vec![
                    ghost("diff", "They're different")
                        .on_click(move |_, _, cx| {
                            if let Err(e) = store::write(cx, |db| db.dismiss_attention(&key)) {
                                toast_error(e.to_string(), cx);
                            }
                        })
                        .into_any_element(),
                    secondary("merge", "Merge").on_click(cx.listener(move |p, _, _, cx| p.open_merge(&ids, cx))).into_any_element(),
                ]
            }
            _ => vec![],
        }
    }

    fn ready(&self, t: &Theme, data: &BillingOverview) -> Div {
        let hint_text = (data.pipeline.unbilled_amount > 0.0).then(|| text("Tracked time that isn't on an invoice yet", 12.0, 500, t.fg3).line_height(px(16.)).into_any_element());
        let head = head_start(section_title(t, "Ready to bill"), hint_text);
        if data.ready_to_bill.is_empty() {
            return div().flex().flex_col().child(head).child(
                card(t).child(EmptyState::new("bill-ready-empty", "Everything is billed", "New billable time shows up here until it's on an invoice.").compact()),
            );
        }
        let mut list = card(t).overflow_hidden().flex().flex_col();
        let count = data.ready_to_bill.len();
        let (hover_bg, fg, fg2) = (t.fg.opacity(0.025), t.fg, t.fg2);
        for (gi, group) in data.ready_to_bill.iter().enumerate() {
            let cid = group.client_id;
            let n = group.projects.len() as i64;
            let header = div()
                .flex()
                .items_center()
                .gap(r(3.))
                .px(r(4.))
                .h(px(48.))
                .child(client_avatar(&group.client_name, 26.0))
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .items_start()
                        .child(
                            div()
                                .id(SharedString::from(format!("ready-client-{cid}")))
                                .cursor_pointer()
                                .hover(|s| s.underline())
                                .line_height(px(19.5))
                                .child(text(group.client_name.clone(), 13.0, 600, t.fg))
                                .on_click(move |_, w, cx| navigate(Route::Client(cid), w, cx)),
                        )
                        .child(
                            text(
                                format!("{} project{} · {} · oldest {}", n, plural(n), hours_short(group.hours), format_day(&group.oldest_day)),
                                12.0,
                                500,
                                t.fg3,
                            )
                            .line_height(px(16.)),
                        ),
                )
                .child(div().w(r(24.)).flex_none().flex().justify_end().font(num_font(600)).text_size(px(13.)).text_color(t.fg).child(crate::format::money(group.amount)))
                .child(
                    div().w(px(118.)).flex_none().flex().justify_end().child(
                        Button::secondary(SharedString::from(format!("ready-all-{cid}")), "Invoice all")
                            .small()
                            .on_click(move |_, w, cx| navigate(Route::InvoiceNew { client_id: Some(cid), project_id: None, project_ids: vec![] }, w, cx)),
                    ),
                );
            let mut projects = div().pb(r(1.5)).flex().flex_col();
            for p in &group.projects {
                let pid = p.project_id;
                let stale = days_since(&p.oldest_day) >= 30;
                let g = SharedString::from(format!("ready-p-{pid}"));
                let mut row = div()
                    .id(SharedString::from(format!("ready-row-{pid}")))
                    .group(g.clone())
                    .flex()
                    .items_center()
                    .gap(r(3.))
                    .pl(px(55.))
                    .pr(r(4.))
                    .h(px(36.))
                    .hover(move |s| s.bg(hover_bg))
                    .child(dot(parse_hex(&p.project_color), r(2.)))
                    .child(
                        div()
                            .id(SharedString::from(format!("ready-pname-{pid}")))
                            .min_w_0()
                            .cursor_pointer()
                            .font_family(crate::fonts::rounded_family(500))
                            .text_size(px(13.))
                            .text_color(fg2)
                            .hover(move |s| s.text_color(fg))
                            .truncate()
                            .child(p.project_name.clone())
                            .on_click(move |_, w, cx| navigate(Route::Project(pid), w, cx)),
                    );
                if p.project_status == "complete" || p.project_status == "archived" {
                    row = row.child(status_badge(t, &p.project_status));
                }
                row = row
                    .child(div().flex_1())
                    .child(text(format!("since {}", format_day(&p.oldest_day)), 12.0, 500, if stale { t.amber } else { t.fg3 }).flex_none().whitespace_nowrap())
                    .child(div().w(r(14.)).flex_none().flex().justify_end().font(num_font(500)).text_size(px(12.)).text_color(t.fg3).child(hours_short(p.hours)))
                    .child(div().w(r(24.)).flex_none().flex().justify_end().font(num_font(500)).text_size(px(13.)).text_color(t.fg2).child(crate::format::money(p.amount)))
                    .child(
                        div().w(px(118.)).flex_none().flex().justify_end().child(
                            div().opacity(0.).group_hover(g, |s| s.opacity(1.)).child(
                                Button::ghost(SharedString::from(format!("ready-inv-{pid}")), "Invoice")
                                    .small()
                                    .on_click(move |_, w, cx| navigate(Route::InvoiceNew { client_id: None, project_id: Some(pid), project_ids: vec![] }, w, cx)),
                            ),
                        ),
                    );
                projects = projects.child(row);
            }
            let mut block = div().flex().flex_col().child(header).child(projects);
            if gi + 1 < count {
                block = block.border_b_1().border_color(t.line);
            }
            list = list.child(block);
        }
        div().flex().flex_col().child(head).child(list)
    }
}

/// ConfirmDialog with rich content in its message.
#[allow(clippy::too_many_arguments)]
pub(crate) fn confirm(
    id: &str,
    focus: &FocusHandle,
    title: impl Into<SharedString>,
    message: AnyElement,
    confirm_text: &str,
    danger: bool,
    on_confirm: impl Fn(&mut Window, &mut App) + 'static,
    on_close: impl Fn(&mut Window, &mut App) + 'static,
    _cx: &App,
) -> impl IntoElement {
    let on_close = Rc::new(on_close);
    let (close_a, close_b, close_c) = (on_close.clone(), on_close.clone(), on_close);
    let go = Button::new(SharedString::from(format!("{id}-confirm")), if danger { Variant::Destructive } else { Variant::Primary })
        .label(confirm_text.to_string())
        .on_click(move |_, w, cx| {
            on_confirm(w, cx);
            close_c(w, cx);
        });
    Modal::new(SharedString::from(id.to_string()), title, move |w, cx| close_a(w, cx))
        .size(ModalSize::Sm)
        .focus(focus)
        .child(message)
        .footer(Button::secondary(SharedString::from(format!("{id}-cancel")), "Cancel").on_click(move |_, w, cx| close_b(w, cx)))
        .footer(go)
}

/// "Delete INV-1002?": paid invoices warn about income, and linked time goes
/// back to unbilled.
pub(crate) fn delete_dialog(
    id: &str,
    focus: &FocusHandle,
    inv: &Invoice,
    entry_count: i64,
    entry_hours: f64,
    on_confirm: impl Fn(&mut Window, &mut App) + 'static,
    on_close: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> impl IntoElement {
    let t = theme(cx);
    let line = |s: String, color: Hsla| text(s, 13.0, 500, color).line_height(px(20.));
    let mut message = div().flex().flex_col();
    if inv.status == "paid" {
        message = message.child(line("This invoice is paid. Deleting it also removes the payment from your income totals.".into(), t.red).mb(r(2.)));
    }
    message = message.child(if entry_count > 0 {
        line(
            format!(
                "Its {entry_count} time entr{} ({}) go back to unbilled so you can bill them again.",
                if entry_count == 1 { "y" } else { "ies" },
                hours_short(entry_hours)
            ),
            t.fg2,
        )
    } else {
        line("This can't be undone.".into(), t.fg2)
    });
    confirm(id, focus, format!("Delete {}?", inv.invoice_number), message.into_any_element(), "Delete invoice", true, on_confirm, on_close, cx)
}
