//! The guided Add job / Edit job modal: pick solar or roofing, fill in the
//! deal, and watch the payout preview (and which bucket it lands in) as you
//! type.

use billable_core::models::{Commission, CommissionInput, CommissionPatch};
use chrono::NaiveDate;
use gpui::{
    div, prelude::*, px, AppContext, Context, ElementId, Entity, EventEmitter, IntoElement, ParentElement, Render,
    SharedString, Styled, Subscription, Window,
};
use gpui_component::input::{InputEvent, InputState, TextareaState};

use super::model::{
    js_number, js_string, number_chars, number_value, preview_commission, tone_color, today_iso, Tone, PAYMENT_OPTIONS,
    STATUS_OPTIONS,
};
use crate::{
    components::{
        fields::{TextArea, TextField},
        select, toast_error, toast_success, Button, DateChanged, DateInput, Modal, ModalSize,
    },
    format::money,
    icons::Lucide,
    store,
    style::{hint, icon, label, num_font, r, text},
    theme::theme,
};

pub enum JobFormEvent {
    Saved,
    Closed,
}

pub struct JobFormModal {
    editing: Option<Commission>,
    job_type: String,
    client: Entity<InputState>,
    date: Entity<DateInput>,
    closer: Entity<InputState>,
    kw: Entity<InputState>,
    contract: Entity<InputState>,
    status: String,
    payment: String,
    manual: Entity<InputState>,
    notes: Entity<TextareaState>,
    show_more: bool,
    _subs: Vec<Subscription>,
}

impl EventEmitter<JobFormEvent> for JobFormModal {}

impl JobFormModal {
    /// A blank job, or `job` to edit.
    pub fn new(job: Option<Commission>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let today = today_iso();
        let (client, job_type, date, closer, status, payment, kw, contract, manual, notes) = match &job {
            Some(c) => (
                c.client_name.clone(),
                c.job_type.clone(),
                c.appointment_date.clone().filter(|d| !d.is_empty()).unwrap_or(today).chars().take(10).collect::<String>(),
                c.closer_name.clone(),
                c.status.clone(),
                c.payment_status.clone(),
                c.system_size_kw.map(js_string).unwrap_or_default(),
                c.contract_amount.map(js_string).unwrap_or_default(),
                c.manual_override.map(js_string).unwrap_or_default(),
                c.notes.clone(),
            ),
            None => (
                String::new(),
                "solar".into(),
                today,
                String::new(),
                "appointment_set".into(),
                "unpaid".into(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
            ),
        };
        let show_more = job.as_ref().is_some_and(|c| c.manual_override.is_some() || !c.notes.is_empty());
        let field = |value: String, placeholder: &'static str, window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| InputState::new(window, cx).placeholder(placeholder).default_value(value))
        };
        let number = |value: String, placeholder: &'static str, window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| InputState::new(window, cx).placeholder(placeholder).default_value(value).validate(|s, _| number_chars(s)))
        };
        let client_state = field(client, "Client name", window, cx);
        let closer = field(closer, "Optional", window, cx);
        let kw = number(kw, "6.16", window, cx);
        let contract = number(contract, "25000", window, cx);
        let manual = number(manual, "Replaces the calculated payout", window, cx);
        let notes = cx.new(|cx| TextareaState::new(window, cx).placeholder("Anything worth remembering").default_value(notes));
        let day = NaiveDate::parse_from_str(&date, "%Y-%m-%d").ok();
        let date = cx.new(|cx| DateInput::date(day, cx));
        let notify = |_: &mut Self, _: Entity<InputState>, _: &InputEvent, cx: &mut Context<Self>| cx.notify();
        let subs = vec![
            cx.subscribe(&client_state, notify),
            cx.subscribe(&kw, notify),
            cx.subscribe(&contract, notify),
            cx.subscribe(&manual, notify),
            cx.subscribe(&date, |_, _, _: &DateChanged, cx| cx.notify()),
        ];
        client_state.update(cx, |s, cx| s.focus(window, cx));
        JobFormModal {
            editing: job,
            job_type,
            client: client_state,
            date,
            closer,
            kw,
            contract,
            status,
            payment,
            manual,
            notes,
            show_more,
            _subs: subs,
        }
    }

    fn is_closed_status(&self) -> bool {
        self.status == "closed_waiting" || self.status == "paid"
    }

    fn validation_error(&self, cx: &gpui::App) -> Option<&'static str> {
        let kw = number_value(&self.kw, cx);
        let contract = number_value(&self.contract, cx);
        let manual = number_value(&self.manual, cx);
        let closed = self.is_closed_status();
        if self.client.read(cx).value().trim().is_empty() {
            return Some("Add the client name");
        }
        if self.date.read(cx).date_value().is_none() {
            return Some("Add the appointment date");
        }
        if !manual.is_empty() && js_number(&manual) < 0.0 {
            return Some("The override can't be negative");
        }
        if !kw.is_empty() && js_number(&kw) < 0.0 {
            return Some("kW can't be negative");
        }
        if !contract.is_empty() && js_number(&contract) < 0.0 {
            return Some("The contract amount can't be negative");
        }
        if closed && self.job_type == "solar" && (kw.is_empty() || js_number(&kw) <= 0.0) {
            return Some("Closed solar jobs need a system size");
        }
        if closed && self.job_type == "roofing" && (contract.is_empty() || js_number(&contract) <= 0.0) {
            return Some("Closed roofing jobs need a contract amount");
        }
        None
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        if let Some(e) = self.validation_error(cx) {
            toast_error(e, cx);
            return;
        }
        let kw = number_value(&self.kw, cx);
        let contract = number_value(&self.contract, cx);
        let manual = number_value(&self.manual, cx);
        let client_name = self.client.read(cx).value().trim().to_string();
        let closer_name = self.closer.read(cx).value().trim().to_string();
        let notes = self.notes.read(cx).value().to_string();
        let date = self.date.read(cx).date_value().map(|d| d.format("%Y-%m-%d").to_string());
        let system_size_kw = (self.job_type == "solar" && !kw.is_empty()).then(|| js_number(&kw));
        let contract_amount = (self.job_type == "roofing" && !contract.is_empty()).then(|| js_number(&contract));
        let manual_override = (!manual.is_empty()).then(|| js_number(&manual));
        let job_type = self.job_type.clone();
        let (status, payment) = (self.status.clone(), self.payment.clone());
        let result = if let Some(existing) = &self.editing {
            let id = existing.id;
            let patch = CommissionPatch {
                client_name: Some(client_name),
                job_type: Some(job_type),
                appointment_date: Some(date),
                closer_name: Some(Some(closer_name)),
                status: Some(status),
                payment_status: Some(payment),
                system_size_kw: Some(system_size_kw),
                contract_amount: Some(contract_amount),
                manual_override: Some(manual_override),
                notes: Some(Some(notes)),
            };
            store::write(cx, |db| db.update_commission(id, &patch)).map(|_| "Commission updated")
        } else {
            let input = CommissionInput {
                client_name,
                job_type: Some(job_type),
                appointment_date: date,
                closer_name: Some(closer_name),
                status: Some(status),
                payment_status: Some(payment),
                system_size_kw,
                contract_amount,
                manual_override,
                notes: Some(notes),
            };
            store::write(cx, |db| db.create_commission(&input)).map(|_| "Commission job added")
        };
        match result {
            Ok(msg) => {
                toast_success(msg, cx);
                cx.emit(JobFormEvent::Saved);
            }
            Err(e) => toast_error(format!("Failed to save: {e}"), cx),
        }
    }

    /// One of the two job type cards.
    fn type_card(&self, value: &'static str, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(cx);
        let on = self.job_type == value;
        let solar = value == "solar";
        let (hover_border, hover_bg) = (t.line_strong, t.fg.opacity(0.02));
        let mut card = div()
            .id(ElementId::Name(format!("comm-type-{value}").into()))
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .p(r(3.5))
            .rounded(r(2.))
            .border_1()
            .cursor_pointer()
            .on_click(cx.listener(move |m, _, _, cx| {
                m.job_type = value.to_string();
                cx.notify();
            }));
        card = if on {
            card.border_color(t.accent.opacity(0.6)).bg(t.accent.opacity(0.07))
        } else {
            card.border_color(t.line).hover(move |s| s.border_color(hover_border).bg(hover_bg))
        };
        card.child(
            div()
                .flex()
                .items_center()
                .gap(r(2.))
                .child(icon(if solar { Lucide::Sun } else { Lucide::Home }).size(r(4.)).text_color(if on { t.accent_text } else { t.fg3 }))
                .child(text(if solar { "Solar" } else { "Roofing" }, 13.0, 600, t.fg).line_height(px(18.))),
        )
        .child(text(if solar { "$50 per kW" } else { "$250 to $500 per closed job" }, 12.0, 500, t.fg3).mt(r(1.)).line_height(px(16.)))
    }
}

impl Render for JobFormModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(cx);
        let kw = number_value(&self.kw, cx);
        let contract = number_value(&self.contract, cx);
        let manual = number_value(&self.manual, cx);
        let solar = self.job_type == "solar";
        let closed = self.is_closed_status();
        let error = self.validation_error(cx);
        let (value, review) = preview_commission(&self.job_type, &kw, &contract, &manual);
        let (bucket, tone, bucket_hint) = if review {
            ("Needs review", Tone::Review, "Roofing jobs between $20,001 and $29,999 need a manual payout.")
        } else if self.payment == "paid" {
            ("Paid payout", Tone::Paid, "This job has been paid.")
        } else if closed {
            ("Owed payout", Tone::Owed, "This job is closed but not paid yet.")
        } else {
            ("Potential payout", Tone::Potential, "Won't count as owed until the deal is marked closed.")
        };
        let value = if review { None } else { value };
        let this = cx.entity().downgrade();

        let amount_field = if solar {
            TextField::new(&self.kw).tabular()
        } else {
            TextField::new(&self.contract).tabular().leading_text("$")
        };
        let amount_hint = if solar {
            if kw.is_empty() {
                "Payout is kW × $50".to_string()
            } else {
                format!("{kw} kW × $50 = {}", money(js_number(&kw) * 50.0))
            }
        } else {
            "$20,000 or less pays $250. $30,000 or more pays $500. In between needs a manual payout.".to_string()
        };

        let (ts, tp) = (this.clone(), this.clone());
        let status_select = select(
            "comm-form-status",
            STATUS_OPTIONS.iter().map(|(v, l)| (v.to_string(), SharedString::from(*l))).collect(),
            &self.status,
            move |v, _, cx| {
                let _ = ts.update(cx, |m, cx| {
                    m.status = v;
                    cx.notify();
                });
            },
            cx,
        );
        let payment_select = select(
            "comm-form-payment",
            PAYMENT_OPTIONS.iter().map(|(v, l)| (v.to_string(), SharedString::from(*l))).collect(),
            &self.payment,
            move |v, _, cx| {
                let _ = tp.update(cx, |m, cx| {
                    m.payment = v;
                    cx.notify();
                });
            },
            cx,
        );

        let preview_box = {
            let b = div().rounded(r(2.)).p(r(3.5)).border_1();
            let b = if tone == Tone::Review { b.border_color(t.amber.opacity(0.4)).bg(t.amber.opacity(0.06)) } else { b.border_color(t.line).bg(t.fg.opacity(0.02)) };
            b.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(text(bucket, 13.0, 500, t.fg2).line_height(px(18.)))
                    .child(
                        div()
                            .font(num_font(600))
                            .text_size(px(16.))
                            .line_height(px(22.))
                            .text_color(tone_color(&t, tone))
                            .child(value.map(money).unwrap_or_else(|| "Needs review".into())),
                    ),
            )
            .child(text(bucket_hint, 12.0, 500, t.fg3).mt(r(1.)).line_height(px(16.)))
        };

        let mut more = div().border_t_1().border_color(t.line).pt(r(3.)).child(
            div().flex().ml(-r(2.)).child(
                Button::ghost("comm-form-more", "Override and notes")
                    .small()
                    .icon(if self.show_more { Lucide::ChevronUp } else { Lucide::ChevronDown })
                    .icon_color(t.accent_text)
                    .on_click(cx.listener(|m, _, _, cx| {
                        m.show_more = !m.show_more;
                        cx.notify();
                    })),
            ),
        );
        if self.show_more {
            more = more.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(r(3.))
                    .mt(r(3.))
                    .child(div().child(label(&t, "Manual payout")).child(TextField::new(&self.manual).tabular().leading_text("$")))
                    .child(div().child(label(&t, "Notes")).child(TextArea::new(&self.notes, 2))),
            );
        }

        let body = div()
            .flex()
            .flex_col()
            .gap(r(5.))
            .child(div().flex().gap(r(3.)).child(self.type_card("solar", cx)).child(self.type_card("roofing", cx)))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(r(3.))
                    .child(div().child(label(&t, "Client")).child(TextField::new(&self.client)))
                    .child(
                        div()
                            .flex()
                            .gap(r(3.))
                            .child(div().flex_1().min_w_0().child(label(&t, "Appointment date")).child(self.date.clone()))
                            .child(div().flex_1().min_w_0().child(label(&t, "Closer")).child(TextField::new(&self.closer))),
                    ),
            )
            .child(
                div()
                    .child(label(&t, if solar { "System size (kW)" } else { "Contract amount" }))
                    .child(amount_field)
                    .child(hint(&t, amount_hint)),
            )
            .child(
                div()
                    .flex()
                    .gap(r(3.))
                    .child(div().flex_1().min_w_0().child(label(&t, "Deal status")).child(status_select))
                    .child(div().flex_1().min_w_0().child(label(&t, "Payment")).child(payment_select)),
            )
            .child(preview_box)
            .child(more);

        let (e1, e2, e3) = (this.clone(), this.clone(), this);
        Modal::new("comm-job-form", if self.editing.is_some() { "Edit job" } else { "Add job" }, move |_, cx| {
            let _ = e1.update(cx, |_, cx| cx.emit(JobFormEvent::Closed));
        })
        .size(ModalSize::Lg)
        .child(body)
        .footer(text(error.unwrap_or(""), 12.0, 500, t.fg3).line_height(px(16.)).truncate().min_w_0().mr_auto())
        .footer(Button::secondary("comm-form-cancel", "Cancel").on_click(move |_, _, cx| {
            let _ = e2.update(cx, |_, cx| cx.emit(JobFormEvent::Closed));
        }))
        .footer(Button::primary("comm-form-save", "Save job").disabled(error.is_some()).on_click(move |_, _, cx| {
            let _ = e3.update(cx, |m, cx| m.save(cx));
        }))
    }
}
