//! PhoneConnectModal.tsx: Content HQ (inside Billable's window) asked to
//! connect for phone sync. Nothing is stored until you say yes here.
//!
//! Mount one for the whole window (it renders nothing until a request
//! arrives) and keep the entity: it listens to the phone-sync service
//! itself. It emits [`PhoneConnectEvent::Answered`] after each answer, for
//! anyone who wants to know. Enter connects, Escape cancels.

use gpui::{
    div, prelude::*, px, AnyElement, App, Context, Empty, EventEmitter, FocusHandle, Hsla, IntoElement, KeyDownEvent,
    ParentElement, Render, StyledText, TextRun, Window,
};

use crate::{
    components::{toast::toast_for, toast_error, Button, Modal, ModalSize, Mood, ToastIcon},
    fonts::rounded_family,
    services::phone::{self, ConnectRequest, PhoneEvent},
    style::r,
    theme::theme,
};

pub enum PhoneConnectEvent {
    /// `accepted`: the person clicked Connect; `connected`: it worked.
    Answered { accepted: bool, connected: bool },
}

pub struct PhoneConnectModal {
    request: Option<ConnectRequest>,
    focus: FocusHandle,
}

impl EventEmitter<PhoneConnectEvent> for PhoneConnectModal {}

impl PhoneConnectModal {
    /// Also starts phone sync if nothing has yet.
    pub fn new(cx: &mut Context<Self>) -> Self {
        let this = cx.weak_entity();
        phone::listen(
            "connect-modal",
            std::rc::Rc::new(move |event: &PhoneEvent, cx: &mut App| {
                if let PhoneEvent::ConnectRequest(request) = event {
                    let request = request.clone();
                    let _ = this.update(cx, |m, cx| {
                        m.request = request;
                        cx.notify();
                    });
                }
            }),
        );
        phone::ensure_started(cx);
        PhoneConnectModal { request: phone::pending_request(), focus: cx.focus_handle() }
    }

    fn answer(&mut self, accept: bool, cx: &mut Context<Self>) {
        let Some(r) = self.request.take() else { return };
        cx.notify();
        let status = phone::confirm_connect(&r.request_id, accept, cx);
        if accept {
            if status.connected {
                toast_for(
                    "Connected. Billable will show up in Content HQ on your phone in a moment.",
                    ToastIcon::MascotHop(Mood::Happy),
                    4000,
                    cx,
                );
            } else {
                toast_error(
                    status.message.clone().filter(|m| !m.is_empty()).unwrap_or_else(|| "Billable couldn\u{2019}t connect. Try again from Content HQ.".into()),
                    cx,
                );
            }
        }
        cx.emit(PhoneConnectEvent::Answered { accepted: accept, connected: status.connected });
    }
}

/// A paragraph mixing weights and colours (`<span className="font-semibold text-fg">`).
fn rich(parts: &[(&str, u16, Hsla)], size: f32, line_height: f32) -> AnyElement {
    let mut s = String::new();
    let mut runs = Vec::new();
    for (p, weight, color) in parts {
        s.push_str(p);
        runs.push(TextRun { len: p.len(), font: gpui::font(rounded_family(*weight)), color: *color, background_color: None, underline: None, strikethrough: None });
    }
    div().text_size(px(size)).line_height(px(line_height)).child(StyledText::new(s).with_runs(runs)).into_any_element()
}

impl Render for PhoneConnectModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(request) = self.request.clone() else { return Empty.into_any_element() };
        let t = theme(cx);
        let profile = request.profile_name.as_ref().map(|n| format!(" ({n} profile)")).unwrap_or_default();

        // space-y-3 text-sm text-fg-2 leading-5
        let first = match &request.account {
            Some(account) => rich(
                &[
                    (&format!("Connect Billable{profile} to the Content HQ account "), 500, t.fg2),
                    (account, 600, t.fg),
                    ("? Only continue if that\u{2019}s your account.", 500, t.fg2),
                ],
                13.0,
                16.25,
            ),
            None => rich(
                &[
                    ("Content HQ at ", 500, t.fg2),
                    (&request.host, 600, t.fg),
                    (&format!(" wants to connect to Billable{profile}."), 500, t.fg2),
                ],
                13.0,
                16.25,
            ),
        };
        let second = format!(
            "Billable will send a summary of your jobs, hours, invoices and expenses to{}. From your phone you can then track time, log hours, add expenses and record payments.",
            if request.account.is_some() { " that account" } else { " the Content HQ account signed in here" }
        );
        let body = div()
            .flex()
            .flex_col()
            .gap(r(3.))
            .child(first)
            .child(rich(&[(&second, 500, t.fg2)], 13.0, 16.25))
            .child(rich(
                &[("Client emails, addresses, invoice lines and payment details stay on this Mac. You can disconnect any time in Settings.", 500, t.fg3)],
                12.0,
                16.0,
            ));

        let (e1, e2, e3) = (cx.weak_entity(), cx.weak_entity(), cx.weak_entity());
        let modal = Modal::new("phone-connect", "Use Billable on your phone?", move |_, cx| {
            let _ = e1.update(cx, |m, cx| m.answer(false, cx));
        })
        .size(ModalSize::Sm)
        .focus(&self.focus)
        .child(body)
        .footer(Button::secondary("phone-connect-cancel", "Cancel").on_click(move |_, _, cx| {
            let _ = e2.update(cx, |m, cx| m.answer(false, cx));
        }))
        .footer(Button::primary("phone-connect-ok", "Connect").on_click(move |_, _, cx| {
            let _ = e3.update(cx, |m, cx| m.answer(true, cx));
        }));

        // Connect has autoFocus in the original: Enter answers yes.
        div()
            .on_key_down(cx.listener(|m, e: &KeyDownEvent, _, cx| {
                if e.keystroke.key == "enter" {
                    cx.stop_propagation();
                    m.answer(true, cx);
                }
            }))
            .child(modal)
            .into_any_element()
    }
}
