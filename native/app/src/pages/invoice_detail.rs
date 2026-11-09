//! InvoiceDetail.tsx: the invoice as paper (matching the PDF), with its
//! stamp, and a side panel for the amount, what happened when, the linked
//! time and the client.

mod stamp;

use std::collections::HashMap;

use billable_core::models::{Invoice, Settings, TaxSettings};
use gpui::{
    div, prelude::*, px, AnyElement, App, Context, Div, Entity, FocusHandle, HighlightStyle, InteractiveText, IntoElement,
    MouseButton, ParentElement, Render, SharedString, StyledText, Styled, Subscription, UnderlineStyle, Window,
};

use self::stamp::{Stamp, StampTone};
use super::billing::{delete_dialog, fixed2};
use crate::{
    components::{
        client_form::trim_number,
        menu_list, popover,
        record_payment::{burst_layer, RecordPaymentEvent, RecordPaymentModal},
        status_badge, toast_error, toast_success, Align, Button, MenuItem,
    },
    export,
    fonts::rounded_family,
    format::{self, duration_short, format_date, format_day, hours_short, js_round},
    icons::Lucide,
    motion::rise_in,
    shell::{
        app::{navigate, Header, Page},
        route::Route,
    },
    store,
    style::{card, dot, figures, icon, num_font, r, text},
    theme::{hex, parse_hex, rgba4, shadow, theme, Theme},
};

// The paper's own greys (zinc), the same in light and dark mode.
const INK: u32 = 0x18181b;
const MUTED: u32 = 0x52525b;
const SOFT: u32 = 0x71717a;
const FAINT: u32 = 0xa1a1aa;
const RULE: u32 = 0xe4e4e7;
const ROW_RULE: u32 = 0xf0f0f2;
const PAID_GREEN: u32 = 0x15803d;

/// formatMoney(n, currency): en-CA for CAD (plain "$"), en-US otherwise.
fn money_in(amount: f64, currency: &str) -> String {
    let (whole, cents) = format::money_parts(amount);
    let symbol = match currency {
        "CAD" | "USD" | "" => "$",
        "EUR" => "€",
        "GBP" => "£",
        "AUD" => "A$",
        "JPY" => "¥",
        "NZD" => "NZ$",
        "MXN" => "MX$",
        "INR" => "₹",
        // en-US writes other currencies as their code and a no-break space
        other => return format!("{}{other}\u{a0}{}{cents}", if amount < 0.0 { "-" } else { "" }, whole.trim_start_matches('-').replacen('$', "", 1)),
    };
    format!("{}{cents}", whole.replacen('$', symbol, 1))
}

/// What the time-based lines should add up to, grouped the way the builder groups them.
fn expected_hours_for_lines(inv: &Invoice) -> Option<f64> {
    let entries = inv.entries.as_ref()?;
    let style = inv.line_style.as_deref().filter(|s| !s.is_empty())?;
    if style == "manual" {
        return None;
    }
    let mut groups: Vec<(String, f64)> = Vec::new();
    for e in entries {
        let day = format::local_day(&e.start_time).map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_default();
        let key = match style {
            "entry" => format!("e{}", e.id),
            "project" => format!("p{}", e.project_id),
            _ => format!("d{day}:{}", e.project_id),
        };
        match groups.iter_mut().find(|(k, _)| *k == key) {
            Some(g) => g.1 += e.duration_minutes,
            None => groups.push((key, e.duration_minutes)),
        }
    }
    let total: f64 = groups.iter().map(|(_, mins)| js_round(mins / 60.0 * 100.0) / 100.0).sum();
    Some(js_round(total * 100.0) / 100.0)
}

pub struct InvoiceDetailPage {
    id: i64,
    rev: u64,
    invoice: Option<Invoice>,
    settings: Settings,
    tax: Option<TaxSettings>,
    /// The invoice is gone; we're on our way back to Billing.
    leaving: bool,
    show_delete: bool,
    paying: Option<Entity<RecordPaymentModal>>,
    paying_sub: Option<Subscription>,
    menu_open: bool,
    exporting: bool,
    link_hover: bool,
    stamp: Option<Stamp>,
    /// Keyboard focus for the delete dialog, so Escape closes it.
    focus: FocusHandle,
}

impl InvoiceDetailPage {
    pub fn new(id: i64, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        InvoiceDetailPage {
            id,
            rev: 0,
            invoice: None,
            settings: Settings::default(),
            tax: None,
            leaving: false,
            show_delete: false,
            paying: None,
            paying_sub: None,
            menu_open: false,
            exporting: false,
            link_hover: false,
            stamp: None,
            focus: cx.focus_handle(),
        }
    }

    /// Load (again, when the data changed). A missing invoice goes back to Billing.
    fn ensure_loaded(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.rev == store::rev(cx) {
            return;
        }
        let db = store::db(cx);
        self.invoice = db.get_invoice(self.id).ok().flatten();
        self.settings = db.get_settings().unwrap_or_default();
        self.tax = db.get_tax_settings().ok().flatten();
        self.rev = store::rev(cx);
        if self.invoice.is_none() && !self.leaving {
            self.leaving = true;
            cx.defer_in(window, |_, window, cx| navigate(Route::Billing { status: None }, window, cx));
        }
    }

    fn currency(&self, inv: &Invoice) -> String {
        if !inv.currency.is_empty() {
            return inv.currency.clone();
        }
        self.tax.as_ref().map(|t| t.currency.clone()).filter(|c| !c.is_empty()).unwrap_or_else(|| "CAD".into())
    }

    fn mark_sent(&mut self, cx: &mut Context<Self>) {
        let Some(inv) = self.invoice.clone() else { return };
        match store::write(cx, |db| db.mark_invoices_sent(&[inv.id], None)) {
            Ok(_) => toast_success(format!("{} marked as sent", inv.invoice_number), cx),
            Err(e) => toast_error(e.to_string(), cx),
        }
    }

    fn mark_unpaid(&mut self, cx: &mut Context<Self>) {
        let id = self.id;
        match store::write(cx, |db| db.mark_invoice_unpaid(id)) {
            Ok(_) => toast_success("Payment removed. The invoice is back to awaiting payment.", cx),
            Err(e) => toast_error(e.to_string(), cx),
        }
    }

    fn export_pdf(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.exporting = true;
        export::invoice_pdf(self.id, window, cx);
        self.exporting = false;
        cx.notify();
    }

    fn open_payment(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(inv) = self.invoice.clone() else { return };
        let modal = cx.new(|cx| RecordPaymentModal::new(vec![inv], window, cx));
        self.paying_sub = Some(cx.subscribe(&modal, |p, _, _: &RecordPaymentEvent, cx| {
            p.paying = None;
            cx.notify();
        }));
        self.paying = Some(modal);
        cx.notify();
    }
}

impl Page for InvoiceDetailPage {
    fn header(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Header {
        self.ensure_loaded(window, cx);
        let t = theme(cx);
        let Some(inv) = self.invoice.clone() else {
            return Header::new("");
        };
        let id = inv.id;
        let draft = inv.status == "draft";
        let mut actions = div()
            .flex()
            .items_center()
            .gap(r(2.))
            .child(
                Button::secondary("inv-pdf", if self.exporting { "Saving…" } else { "Save PDF" })
                    .disabled(self.exporting)
                    .on_click(cx.listener(|p, _, w, cx| p.export_pdf(w, cx))),
            );
        if draft {
            actions = actions.child(Button::secondary("inv-edit", "Edit").on_click(move |_, w, cx| navigate(Route::InvoiceEdit(id), w, cx)));
        }
        let mut menu = div().relative().on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()).child(
            Button::icon_button("inv-menu", Lucide::MoreHorizontal).small().selected(self.menu_open).tooltip("More actions").on_click(cx.listener(|p, _, _, cx| {
                cx.stop_propagation();
                if !p.menu_open && crate::components::menu::just_closed("inv-menu-pop") {
                    return;
                }
                p.menu_open = !p.menu_open;
                cx.notify();
            })),
        );
        if self.menu_open {
            let this = cx.weak_entity();
            let close: std::rc::Rc<dyn Fn(&mut Window, &mut App)> = {
                let this = this.clone();
                std::rc::Rc::new(move |_, cx| {
                    let _ = this.update(cx, |p, cx| {
                        p.menu_open = false;
                        cx.notify();
                    });
                })
            };
            let mut items: Vec<Option<MenuItem>> = Vec::new();
            if !draft {
                items.push(Some(MenuItem::new("Edit invoice", move |w, cx| navigate(Route::InvoiceEdit(id), w, cx)).icon(Lucide::Pencil)));
            }
            if inv.status == "paid" {
                let tu = this.clone();
                items.push(Some(
                    MenuItem::new("Mark as unpaid", move |_, cx| {
                        let _ = tu.update(cx, |p, cx| p.mark_unpaid(cx));
                    })
                    .icon(Lucide::Undo2),
                ));
            }
            items.push(None);
            let td = this.clone();
            items.push(Some(
                MenuItem::new("Delete invoice", move |_, cx| {
                    let _ = td.update(cx, |p, cx| {
                        p.show_delete = true;
                        cx.notify();
                    });
                })
                .icon(Lucide::Trash2)
                .danger(),
            ));
            let list = menu_list("inv-menu", items, close.clone(), cx);
            menu = menu.child(popover("inv-menu-pop", Align::End, None, list, move |w, cx| close(w, cx), cx));
        }
        actions = actions.child(menu);

        let mut header = Header::new(inv.invoice_number.clone()).back("Billing", Route::Billing { status: None }).meta(status_badge(&t, &inv.status)).actions(actions);
        if let Some(c) = inv.client_name.clone().filter(|c| !c.is_empty()) {
            header = header.subtitle(c);
        }
        header
    }
}

impl Render for InvoiceDetailPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.ensure_loaded(window, cx);
        let t = theme(cx);
        let Some(inv) = self.invoice.clone() else {
            return div().into_any_element();
        };

        // The stamp, drawn again only when what it says changes
        let plural = |n: i64| if n == 1 { "" } else { "s" };
        let days = inv.days_past_due.unwrap_or(0);
        let stamp_date = |d: &str| format_date(d).to_uppercase().replacen(',', "", 1);
        let wanted: Option<(&str, Option<String>, StampTone)> = match inv.status.as_str() {
            "paid" => Some(("Paid", inv.payment_date.as_deref().filter(|d| !d.is_empty()).map(stamp_date), StampTone::Green)),
            "overdue" => Some(("Overdue", Some(format!("{days} day{} late", plural(days))), StampTone::Red)),
            "draft" => Some(("Draft", Some("Not sent".into()), StampTone::Gray)),
            _ => None,
        };
        match wanted {
            Some((label, detail, tone)) => {
                let key = Stamp::key_for(label, detail.as_deref(), tone);
                if self.stamp.as_ref().map(|s| s.key()) != Some(key.as_str()) {
                    self.stamp = Some(Stamp::new(window, label, detail.as_deref(), tone));
                }
            }
            None => self.stamp = None,
        }

        let grid = div()
            .flex()
            .gap(r(6.))
            .items_start()
            .child(self.paper(&inv, window).flex_1().min_w_0())
            .child(self.side(&t, &inv, cx).w(px(300.)).flex_none());

        let mut page = div().w_full().max_w(px(1160.)).mx_auto().px(r(7.)).pt(r(6.)).pb(r(14.)).flex().flex_col().child(rise_in("inv-rise-0", 0, grid));
        page = page.children(self.paying.clone());
        if self.show_delete {
            let entries = inv.entries.clone().unwrap_or_default();
            let linked: f64 = entries.iter().map(|e| e.duration_minutes).sum::<f64>() / 60.0;
            let this = cx.weak_entity();
            let number = inv.invoice_number.clone();
            let id = inv.id;
            page = page.child(delete_dialog(
                "inv-delete",
                &self.focus,
                &inv,
                entries.len() as i64,
                linked,
                move |w, cx| {
                    match store::write(cx, |db| db.delete_invoice(id)) {
                        Ok(r) => {
                            toast_success(
                                if r.released_entries > 0 {
                                    format!("{number} deleted. {} of time is unbilled again.", hours_short(r.released_hours))
                                } else {
                                    format!("{number} deleted")
                                },
                                cx,
                            );
                            navigate(Route::Billing { status: None }, w, cx);
                        }
                        Err(e) => toast_error(e.to_string(), cx),
                    }
                },
                move |_, cx| {
                    let _ = this.update(cx, |p, cx| {
                        p.show_delete = false;
                        cx.notify();
                    });
                },
                cx,
            ));
        }
        page.children(burst_layer(window)).into_any_element()
    }
}

/// Text on the paper: `text-sm` (13/18) unless sized otherwise.
fn ptext(s: impl Into<SharedString>, color: u32) -> Div {
    text(s, 13.0, 500, hex(color)).line_height(px(18.))
}

/// Tabular figures on the paper.
fn pnum(s: impl Into<SharedString>, weight: u16, color: u32) -> Div {
    div().font(num_font(weight)).text_size(px(13.)).line_height(px(18.)).text_color(hex(color)).whitespace_nowrap().child(s.into())
}

/// `text-2xs font-semibold uppercase tracking-[0.08em]`.
fn caps(s: &str, color: u32) -> Div {
    crate::style::text_tracked(s.to_uppercase(), 11.0, 600, hex(color), 0.08).line_height(px(14.)).whitespace_nowrap()
}

impl InvoiceDetailPage {
    /// The paper preview, laid out like the PDF.
    fn paper(&self, inv: &Invoice, window: &Window) -> Div {
        let currency = self.currency(inv);
        let m = |n: f64| money_in(n, &currency);
        let s = &self.settings;
        let tax = self.tax.as_ref();
        let business_name = [s.business_name(), tax.map(|t| t.business_name.as_str()).unwrap_or("")].into_iter().find(|v| !v.is_empty()).unwrap_or("Your business").to_string();
        let business_address = [s.business_address(), tax.map(|t| t.business_address.as_str()).unwrap_or("")].into_iter().find(|v| !v.is_empty()).unwrap_or("").to_string();
        let items = inv.items.clone().unwrap_or_default();
        let gst_on = inv.gst_hst_applicable != 0;
        let gst_amount = if gst_on { inv.subtotal * (inv.gst_hst_rate / 100.0) } else { 0.0 };
        let other_tax = inv.subtotal * (inv.tax_rate / 100.0);

        // From: business; right: "Invoice" and its number
        let mut from = div().min_w_0().flex().flex_col().child(text(business_name, 16.0, 600, hex(INK)).line_height(px(22.)));
        if !business_address.is_empty() {
            from = from.child(ptext(business_address, MUTED).mt(r(1.)));
        }
        if !s.business_email().is_empty() {
            from = from.child(ptext(s.business_email().to_string(), MUTED));
        }
        if let Some(n) = inv.gst_hst_number.as_deref().filter(|n| gst_on && !n.is_empty()) {
            from = from.child(ptext(format!("GST/HST {n}"), MUTED));
        }
        let top = div()
            .flex()
            .justify_between()
            .gap(r(8.))
            .child(from)
            .child(
                div()
                    .flex_none()
                    .flex()
                    .flex_col()
                    .items_end()
                    .child(crate::style::text_tracked("Invoice", 22.0, 600, hex(INK), -0.025).line_height(px(28.)))
                    .child(pnum(inv.invoice_number.clone(), 500, MUTED).mt(r(0.5))),
            );

        // Bill to, and the dates with the amount
        let mut bill_to = div().flex().flex_col().child(caps("Bill to", FAINT).mb(r(1.5))).child(text(inv.client_name.clone().unwrap_or_default(), 13.0, 600, hex(INK)).line_height(px(18.)));
        if let Some(c) = inv.client_company.as_deref().filter(|c| !c.is_empty() && Some(*c) != inv.client_name.as_deref()) {
            bill_to = bill_to.child(ptext(c.to_string(), MUTED));
        }
        if let Some(a) = inv.client_address.as_deref().filter(|a| !a.is_empty()) {
            bill_to = bill_to.child(ptext(a.to_string(), MUTED));
        }
        if let Some(e) = inv.client_email.as_deref().filter(|e| !e.is_empty()) {
            bill_to = bill_to.child(ptext(e.to_string(), MUTED));
        }
        let date_row = |label: &str, value: String| div().flex().justify_between().gap(r(6.)).child(ptext(label.to_string(), SOFT)).child(pnum(value, 500, INK));
        let dates = div()
            .flex_none()
            .min_w(px(220.))
            .flex()
            .flex_col()
            .child(date_row("Issue date", format_date(&inv.issue_date)))
            .child(date_row("Due date", format_date(&inv.due_date)).mt(r(1.)))
            .child(
                div()
                    .flex()
                    .justify_between()
                    .gap(r(6.))
                    .mt(r(1.))
                    .pt(r(2.))
                    .border_t_1()
                    .border_color(hex(RULE))
                    .child(text(if inv.status == "paid" { "Amount paid" } else { "Amount due" }, 13.0, 600, hex(INK)).line_height(px(18.)))
                    .child(pnum(m(inv.total), 600, INK)),
            );
        let parties = div()
            .flex()
            .gap(r(8.))
            .mt(r(9.))
            .child(div().flex_1().min_w_0().child(bill_to))
            .child(div().flex_1().min_w_0().flex().justify_end().child(dates));

        // Lines: Description takes what the fixed columns leave
        let width = |spec: f32, header: &str, values: &mut dyn Iterator<Item = String>| {
            let content = values.map(|v| crate::components::table::measure(window, &v, 13.0, 500, true)).fold(0.0f32, f32::max);
            spec.max(content).max(crate::components::table::measure(window, &header.to_uppercase(), 11.0, 600, false))
        };
        let w_hours = width(f32::from(r(20.)), "Hours", &mut items.iter().map(|i| fixed2(i.quantity)));
        let w_rate = width(f32::from(r(24.)), "Rate", &mut items.iter().map(|i| m(i.unit_price)));
        let w_amount = width(f32::from(r(28.)), "Amount", &mut items.iter().map(|i| m(i.total)));
        let right = |w: f32| div().w(px(w)).flex_none().flex().justify_end();
        let mut lines = div().mt(r(9.)).flex().flex_col().child(
            div()
                .flex()
                .items_center()
                .pb(r(2.))
                .border_b_1()
                .border_color(hex(INK))
                .child(div().flex_1().min_w_0().child(caps("Description", SOFT)))
                .child(right(w_hours).child(caps("Hours", SOFT)))
                .child(right(w_rate).child(caps("Rate", SOFT)))
                .child(right(w_amount).child(caps("Amount", SOFT))),
        );
        for item in &items {
            lines = lines.child(
                div()
                    .flex()
                    .items_center()
                    .py(r(2.5))
                    .border_b_1()
                    .border_color(hex(ROW_RULE))
                    .child(div().flex_1().min_w_0().pr(r(4.)).child(ptext(item.description.clone(), INK)))
                    .child(right(w_hours).child(pnum(fixed2(item.quantity), 500, MUTED)))
                    .child(right(w_rate).child(pnum(m(item.unit_price), 500, MUTED)))
                    .child(right(w_amount).child(pnum(m(item.total), 500, INK))),
            );
        }

        // Totals
        let total_row = |label: String, value: String| div().flex().justify_between().child(ptext(label, SOFT)).child(pnum(value, 500, INK));
        let mut totals = div().w(r(72.)).flex().flex_col().child(total_row("Subtotal".into(), m(inv.subtotal)));
        if gst_on && inv.gst_hst_rate > 0.0 {
            totals = totals.child(total_row(format!("GST/HST ({}%)", trim_number(inv.gst_hst_rate)), m(gst_amount)).mt(r(1.5)));
        }
        if inv.tax_rate > 0.0 {
            totals = totals.child(total_row(format!("Tax ({}%)", trim_number(inv.tax_rate)), m(other_tax)).mt(r(1.5)));
        }
        totals = totals.child(
            div()
                .flex()
                .justify_between()
                .mt(r(1.5))
                .pt(r(2.5))
                .border_t_1()
                .border_color(hex(INK))
                .child(text("Total", 14.0, 600, hex(INK)).line_height(px(20.)))
                .child(div().font(num_font(600)).text_size(px(14.)).line_height(px(20.)).text_color(hex(INK)).child(m(inv.total))),
        );
        if let Some(paid) = inv.payment_date.as_deref().filter(|d| inv.status == "paid" && !d.is_empty()) {
            let by = inv.payment_method.as_deref().filter(|m| !m.is_empty()).map(|m| format!(" by {m}")).unwrap_or_default();
            totals = totals.child(
                div()
                    .flex()
                    .justify_between()
                    .mt(r(1.5))
                    .pt(r(1.))
                    .child(text(format!("Paid {}{by}", format_date(paid)), 12.0, 500, hex(PAID_GREEN)).line_height(px(16.)))
                    .child(div().font(num_font(500)).text_size(px(12.)).line_height(px(16.)).text_color(hex(PAID_GREEN)).child(m(inv.total))),
            );
        }

        let mut content = div().px(r(10.)).py(r(9.)).flex().flex_col().child(top).child(parties).child(lines).child(div().flex().justify_end().mt(r(5.)).child(totals));
        if !inv.notes.is_empty() {
            content = content.child(
                div().mt(r(9.)).pt(r(4.)).border_t_1().border_color(hex(RULE)).flex().flex_col().child(caps("Notes", FAINT).mb(r(1.5))).child(ptext(inv.notes.clone(), MUTED)),
            );
        }

        div()
            .relative()
            .rounded(px(3.))
            .overflow_hidden()
            .bg(hex(0xffffff))
            .text_color(hex(INK))
            .shadow(vec![
                shadow(0.0, 0.0, 0.0, 0.5, rgba4(0, 0, 0, 0.12)),
                shadow(0.0, 2.0, 4.0, 0.0, rgba4(0, 0, 0, 0.06)),
                shadow(0.0, 12.0, 32.0, -12.0, rgba4(0, 0, 0, 0.25)),
            ])
            .child(content)
            // Positioned, so it prints over the paper's text like the original
            .children(self.stamp.as_ref().map(|s| s.render("inv-stamp")))
    }

    fn side(&self, t: &Theme, inv: &Invoice, cx: &mut Context<Self>) -> Div {
        let currency = self.currency(inv);
        let paid = inv.status == "paid";
        let days = inv.days_past_due.unwrap_or(0);
        let entries = inv.entries.clone().unwrap_or_default();
        let items = inv.items.clone().unwrap_or_default();
        let linked_hours = entries.iter().map(|e| e.duration_minutes).sum::<f64>() / 60.0;
        let has_key = |k: &Option<String>| k.as_deref().is_some_and(|k| !k.is_empty());
        let line_hours: f64 = items.iter().filter(|i| has_key(&i.source_key)).map(|i| i.quantity).sum();
        let time_lines = items.iter().any(|i| has_key(&i.source_key));
        let mismatch = time_lines && expected_hours_for_lines(inv).is_some_and(|e| (e - line_hours).abs() > 0.001);

        // Amount, the next step, and the timeline
        let mut summary = card(t)
            .p(r(4.))
            .flex()
            .flex_col()
            .child(text(if paid { "Paid" } else { "Amount due" }, 12.0, 500, t.fg3).line_height(px(16.)))
            .child(figures(money_in(inv.total, &currency), 30.0, t.fg).line_height(px(34.)).mt(r(1.5)));
        if inv.status == "draft" {
            summary = summary.child(div().mt(r(3.)).child(Button::primary("inv-mark-sent", "Mark as sent").full_width().on_click(cx.listener(|p, _, _, cx| p.mark_sent(cx)))));
        }
        if inv.status == "sent" || inv.status == "overdue" {
            summary = summary.child(div().mt(r(3.)).child(Button::primary("inv-pay", "Record payment").full_width().on_click(cx.listener(|p, _, w, cx| p.open_payment(w, cx)))));
        }
        let created = format::local_day(&inv.created_at).map(|d| d.format("%Y-%m-%d").to_string()).filter(|_| !inv.created_at.is_empty()).unwrap_or_else(|| inv.issue_date.clone());
        #[derive(PartialEq)]
        enum StepTone {
            None,
            Red,
            Green,
        }
        let mut steps: Vec<(String, Option<String>, bool, StepTone)> = vec![
            ("Created".into(), Some(created), true, StepTone::None),
            ("Sent".into(), inv.sent_at.clone(), inv.status != "draft", StepTone::None),
        ];
        if inv.status == "overdue" {
            steps.push((format!("Overdue by {days} day{}", if days == 1 { "" } else { "s" }), Some(inv.due_date.clone()), true, StepTone::Red));
        } else {
            steps.push(("Due".into(), Some(inv.due_date.clone()), paid, StepTone::None));
        }
        let paid_label = inv.payment_method.as_deref().filter(|m| !m.is_empty()).map(|m| format!("Paid by {m}")).unwrap_or_else(|| "Paid".into());
        steps.push((paid_label, inv.payment_date.clone(), paid, StepTone::Green));
        let mut timeline = div().mt(r(4.)).flex().flex_col().gap(r(3.));
        for (label, date, done, tone) in steps {
            let mark = if done {
                icon(Lucide::CheckCircle2).text_color(match tone {
                    StepTone::Red => t.red,
                    StepTone::Green => t.green,
                    StepTone::None => t.fg2,
                })
            } else {
                icon(Lucide::Circle).text_color(t.fg4)
            };
            let color = if done { if tone == StepTone::Red { t.red } else { t.fg } } else { t.fg3 };
            let mut words = div().min_w_0().flex().flex_col().child(text(label, 13.0, 500, color).line_height(px(18.)));
            if let Some(d) = date.filter(|d| !d.is_empty()) {
                words = words.child(text(format_day(&d), 12.0, 500, t.fg3).line_height(px(16.)));
            }
            timeline = timeline.child(div().flex().items_start().gap(r(2.5)).child(mark.size(r(4.)).mt(px(1.))).child(words));
        }
        summary = summary.child(timeline);

        // Time on this invoice
        let mut time_head = div().flex().items_center().justify_between().gap(r(3.)).px(r(4.)).h(px(46.)).border_b_1().border_color(t.line).child(text("Time on this invoice", 15.0, 700, t.fg));
        if !entries.is_empty() {
            time_head = time_head.child(div().font(num_font(500)).text_size(px(12.)).line_height(px(16.)).text_color(t.fg3).child(hours_short(linked_hours)));
        }
        let mut time = card(t).overflow_hidden().flex().flex_col().child(time_head);
        if entries.is_empty() {
            let has_style = inv.line_style.as_deref().is_some_and(|s| !s.is_empty());
            time = time.child(
                div()
                    .px(r(4.))
                    .py(r(4.))
                    .child(text(if has_style { "No time is linked." } else { "This invoice was written by hand, so no time is linked to it." }, 13.0, 500, t.fg3).line_height(px(18.))),
            );
        } else {
            // By project, in the order they first appear
            let mut order: Vec<i64> = Vec::new();
            let mut groups: HashMap<i64, (String, String, f64, usize)> = HashMap::new();
            for e in &entries {
                let g = groups.entry(e.project_id).or_insert_with(|| {
                    order.push(e.project_id);
                    (e.project_name.clone().unwrap_or_else(|| "Project".into()), e.project_color.clone().unwrap_or_else(|| "#888".into()), 0.0, 0)
                });
                g.2 += e.duration_minutes;
                g.3 += 1;
            }
            let mut list = div().py(r(1.5)).flex().flex_col();
            for pid in order {
                let (name, color, minutes, count) = groups[&pid].clone();
                list = list.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(r(2.5))
                        .px(r(4.))
                        .h(px(32.))
                        .child(dot(parse_hex(&color), r(2.)))
                        .child(text(name, 13.0, 500, t.fg2).truncate().flex_1().min_w_0())
                        .child(div().font(num_font(500)).text_size(px(12.)).line_height(px(16.)).text_color(t.fg3).whitespace_nowrap().child(format!("{count} · {}", duration_short(minutes)))),
                );
            }
            let first = format_day(&entries[0].start_time);
            let span = if entries.len() > 1 { format!("{first} – {}", format_day(&entries[entries.len() - 1].start_time)) } else { first };
            list = list.child(div().px(r(4.)).pt(r(1.5)).pb(r(1.)).child(text(span, 12.0, 500, t.fg3).line_height(px(16.))));
            time = time.child(list);
        }
        if mismatch {
            time = time.child(self.mismatch_note(t, inv, linked_hours, line_hours, cx));
        }

        // Client
        let client_id = inv.client_id;
        let mut client = card(t).p(r(4.)).flex().flex_col().child(text("Client", 12.0, 500, t.fg3).line_height(px(16.)).mb(r(1.5))).child(
            div().flex().child(
                div()
                    .id("inv-client-link")
                    .cursor_pointer()
                    .hover(|s| s.underline())
                    .line_height(px(19.5))
                    .child(text(inv.client_name.clone().unwrap_or_default(), 13.0, 600, t.fg))
                    .on_click(move |_, w, cx| navigate(Route::Client(client_id), w, cx)),
            ),
        );
        if let Some(email) = inv.client_email.as_deref().filter(|e| !e.is_empty()) {
            client = client.child(
                div().flex().items_center().gap(r(1.5)).mt(r(1.5)).child(icon(Lucide::Mail).size(r(3.)).text_color(t.fg3)).child(text(email.to_string(), 12.0, 500, t.fg3).line_height(px(16.))),
            );
        }
        match inv.client_address.as_deref().filter(|a| !a.is_empty()) {
            Some(address) => {
                client = client.child(
                    div()
                        .flex()
                        .items_start()
                        .gap(r(1.5))
                        .mt(r(1.))
                        .child(icon(Lucide::MapPin).size(r(3.)).mt(r(0.5)).text_color(t.fg3))
                        .child(text(address.to_string(), 12.0, 500, t.fg3).line_height(px(16.)).min_w_0()),
                );
            }
            None => {
                client = client.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(r(1.5))
                        .mt(r(1.5))
                        .child(icon(Lucide::AlertTriangle).size(r(3.)).text_color(t.amber))
                        .child(text("No billing address on file", 12.0, 500, t.amber).line_height(px(16.))),
                );
            }
        }
        div().flex().flex_col().gap(r(4.)).child(summary).child(time).child(client)
    }

    /// "The linked time adds up to …, but the lines say …", with a link to the draft.
    fn mismatch_note(&self, t: &Theme, inv: &Invoice, linked: f64, lines: f64, cx: &mut Context<Self>) -> AnyElement {
        let mut s = format!("The linked time adds up to {}, but the lines say {}.", hours_short(linked), hours_short(lines));
        let mut link = None;
        if inv.status == "draft" {
            s.push(' ');
            let start = s.len();
            s.push_str("Open the draft");
            link = Some(start..s.len());
            s.push_str(" to update the lines.");
        }
        let words: AnyElement = match link {
            Some(range) => {
                let mut style = HighlightStyle { color: Some(t.accent_text), ..Default::default() };
                if self.link_hover {
                    style.underline = Some(UnderlineStyle { thickness: px(1.), color: Some(t.accent_text), wavy: false });
                }
                let styled = StyledText::new(s).with_highlights(vec![(range.clone(), style)]).with_font_family_overrides(vec![(range.clone(), SharedString::from(rounded_family(600)))]);
                let id = inv.id;
                let this = cx.weak_entity();
                let hover_range = range.clone();
                InteractiveText::new("inv-open-draft", styled)
                    .on_click(vec![range], move |_, w, cx| navigate(Route::InvoiceEdit(id), w, cx))
                    .on_hover(move |ix, _, _, cx| {
                        let over = ix.is_some_and(|i| hover_range.contains(&i));
                        let _ = this.update(cx, |p, cx| {
                            if p.link_hover != over {
                                p.link_hover = over;
                                cx.notify();
                            }
                        });
                    })
                    .into_any_element()
            }
            None => div().child(s).into_any_element(),
        };
        div()
            .mx(r(3.))
            .mb(r(3.))
            .p(r(2.5))
            .rounded(px(6.))
            .bg(t.amber.opacity(0.1))
            .flex()
            .gap(r(2.))
            .child(icon(Lucide::AlertTriangle).size(r(3.5)).mt(px(1.)).text_color(t.amber))
            .child(div().flex_1().min_w_0().font_family(rounded_family(500)).text_size(px(12.)).line_height(px(16.)).text_color(t.fg2).child(words))
            .into_any_element()
    }
}
