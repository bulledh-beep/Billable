//! ClientForm.tsx as a self-contained modal view. A page keeps
//! `Option<Entity<ClientFormModal>>`, renders it, and subscribes to
//! [`ClientFormEvent`] (Saved / Closed). Saving writes to the database,
//! shows the toast and reloads data.

use billable_core::models::{Client, ClientInput, ClientPatch};
use gpui::{
    div, prelude::*, px, App, AppContext, Context, Entity, EventEmitter, IntoElement, ParentElement, Render, SharedString,
    Styled, Subscription, Window,
};
use gpui_component::input::{InputEvent, InputState, TextareaState};

use super::{
    fields::{TextArea, TextField},
    select, toast_error, toast_success, Button, Modal,
};
use crate::{
    format::money,
    store,
    style::{label, r, text},
    theme::theme,
};

const CURRENCIES: [&str; 6] = ["CAD", "USD", "EUR", "GBP", "AUD", "JPY"];

pub enum ClientFormEvent {
    /// Created (`true`) or updated (`false`).
    Saved(Client, bool),
    Closed,
}

pub struct ClientFormModal {
    editing: Option<Client>,
    name: Entity<InputState>,
    company: Entity<InputState>,
    email: Entity<InputState>,
    address: Entity<TextareaState>,
    rate: Entity<InputState>,
    currency: String,
    _subs: Vec<Subscription>,
}

impl EventEmitter<ClientFormEvent> for ClientFormModal {}

/// How `<input type=number>` shows a stored number: 65 → "65", 72.5 → "72.5".
pub fn trim_number(v: f64) -> String {
    let s = format!("{v}");
    s.strip_suffix(".0").map(str::to_string).unwrap_or(s)
}

impl ClientFormModal {
    /// Empty with Settings' default rate and currency, or filled from `client`.
    pub fn new(client: Option<Client>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let settings = store::db(cx).get_settings().ok();
        let (name, company, email, address, rate, currency) = match &client {
            Some(c) => (c.name.clone(), c.company.clone(), c.email.clone(), c.address.clone(), c.default_rate, c.currency.clone()),
            None => (
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                settings.as_ref().map(|s| s.default_rate()).unwrap_or(0.0),
                settings.as_ref().and_then(|s| s.get("default_currency").map(str::to_string)).filter(|c| !c.is_empty()).unwrap_or_else(|| "CAD".into()),
            ),
        };
        let field = |value: String, placeholder: &'static str, window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| InputState::new(window, cx).placeholder(placeholder).default_value(value))
        };
        let name_state = field(name, "Jane Cooper", window, cx);
        let company = field(company, "Optional", window, cx);
        let email = field(email, "billing@company.com", window, cx);
        let address = cx.new(|cx| TextareaState::new(window, cx).placeholder("Shown on invoices").default_value(address));
        let rate = field(if rate != 0.0 { trim_number(rate) } else { String::new() }, "", window, cx);
        // Re-render as the name and rate change (Save enabled, rate note).
        let subs = vec![
            cx.subscribe(&name_state, |_, _, _: &InputEvent, cx| cx.notify()),
            cx.subscribe(&rate, |_, _, _: &InputEvent, cx| cx.notify()),
        ];
        name_state.update(cx, |s, cx| s.focus(window, cx));
        ClientFormModal { editing: client, name: name_state, company, email, address, rate, currency, _subs: subs }
    }

    fn rate_value(&self, cx: &App) -> f64 {
        self.rate.read(cx).value().trim().parse::<f64>().unwrap_or(0.0)
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let name = self.name.read(cx).value().trim().to_string();
        if name.is_empty() {
            return;
        }
        let company = self.company.read(cx).value().to_string();
        let email = self.email.read(cx).value().to_string();
        let address = self.address.read(cx).value().to_string();
        let rate = self.rate_value(cx);
        let currency = self.currency.clone();
        if let Some(existing) = self.editing.clone() {
            let patch = ClientPatch {
                name: Some(name),
                company: Some(company),
                email: Some(email),
                address: Some(address),
                default_rate: Some(rate),
                currency: Some(currency),
                ..Default::default()
            };
            match store::write(cx, |db| db.update_client(existing.id, &patch)) {
                Ok(r) => {
                    let n = r.as_ref().map(|r| r.cascaded_projects).unwrap_or(0);
                    toast_success(
                        if n > 0 { format!("Client updated · {n} project{} moved to the new rate", if n == 1 { "" } else { "s" }) } else { "Client updated".into() },
                        cx,
                    );
                    let client = r.map(|r| r.client).unwrap_or(existing);
                    cx.emit(ClientFormEvent::Saved(client, false));
                }
                Err(e) => toast_error(e.to_string(), cx),
            }
        } else {
            let input = ClientInput { name, company: Some(company), email: Some(email), address: Some(address), default_rate: Some(rate), currency: Some(currency), ..Default::default() };
            match store::write(cx, |db| db.create_client(&input)) {
                Ok(created) => {
                    toast_success("Client created", cx);
                    cx.emit(ClientFormEvent::Saved(created, true));
                }
                Err(e) => toast_error(e.to_string(), cx),
            }
        }
    }
}

impl Render for ClientFormModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(cx);
        let valid = !self.name.read(cx).value().trim().is_empty();
        let editing = self.editing.clone();
        let rate_changed = editing.as_ref().is_some_and(|c| (self.rate_value(cx) - c.default_rate).abs() > f64::EPSILON);
        let this = cx.entity().downgrade();
        let mut body = div()
            .flex()
            .flex_col()
            .gap(r(4.))
            .child(
                div()
                    .flex()
                    .gap(r(3.))
                    .child(div().flex_1().child(label(&t, "Name")).child(TextField::new(&self.name)))
                    .child(div().flex_1().child(label(&t, "Company")).child(TextField::new(&self.company))),
            )
            .child(div().child(label(&t, "Email")).child(TextField::new(&self.email)))
            .child(div().child(label(&t, "Billing address")).child(TextArea::new(&self.address, 2)))
            .child(
                div()
                    .flex()
                    .gap(r(3.))
                    .child(div().flex_1().child(label(&t, "Hourly rate")).child(TextField::new(&self.rate).tabular().leading_text("$")))
                    .child(div().flex_1().child(label(&t, "Currency")).child(select(
                        "client-currency",
                        CURRENCIES.iter().map(|c| (c.to_string(), SharedString::from(*c))).collect(),
                        &self.currency,
                        move |v, _, cx| {
                            let _ = this.update(cx, |m, cx| {
                                m.currency = v;
                                cx.notify();
                            });
                        },
                        cx,
                    ))),
            );
        if let (true, Some(c)) = (rate_changed, editing.as_ref()) {
            body = body.child(
                div().rounded(px(6.)).bg(t.accent.opacity(0.07)).px(r(3.)).py(r(2.)).child(
                    text(
                        format!(
                            "Projects still at {}/hr move to the new rate, so their unbilled time is priced at it. Invoiced time keeps its original rate.",
                            money(c.default_rate)
                        ),
                        12.0,
                        500,
                        t.fg2,
                    )
                    .line_height(px(16.)),
                ),
            );
        }
        let e1 = cx.entity().downgrade();
        let e2 = e1.clone();
        let e3 = e1.clone();
        Modal::new("client-form", if editing.is_some() { "Edit client" } else { "New client" }, move |_, cx| {
            let _ = e1.update(cx, |_, cx| cx.emit(ClientFormEvent::Closed));
        })
        .child(body)
        .footer(Button::secondary("client-form-cancel", "Cancel").on_click(move |_, _, cx| {
            let _ = e2.update(cx, |_, cx| cx.emit(ClientFormEvent::Closed));
        }))
        .footer(
            Button::primary("client-form-save", if editing.is_some() { "Save changes" } else { "Create client" })
                .disabled(!valid)
                .on_click(move |_, _, cx| {
                    let _ = e3.update(cx, |m, cx| m.save(cx));
                }),
        )
    }
}
