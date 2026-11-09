//! RecordPaymentModal.tsx: record payment for one or more invoices with the
//! date the money arrived and a method. OWNER: the Billing port. It saves
//! itself and emits [`RecordPaymentEvent::Done`].
//!
//! The confetti-like burst from the pay button (utils/burst.ts) outlives the
//! modal, so it's drawn by the page that opened it: pages render
//! [`burst_layer`].

use std::{
    cell::{Cell, RefCell},
    time::{Duration, Instant},
};

use billable_core::models::Invoice;
use chrono::{Local, NaiveDate};
use gpui::{
    anchored, canvas, deferred, div, point, prelude::*, px, Animation, AnimationExt as _, AnyElement, Context, Entity, FocusHandle,
    EventEmitter, Hsla, IntoElement, ParentElement, Pixels, Point, Render, SharedString, Styled, Subscription, Window,
};

use super::{
    date_input::{DateChanged, DateInput},
    select,
    toast::{toast_error, toast_for, ToastIcon},
    Button, Modal, ModalSize, Mood,
};
use crate::{
    format::{format_day, money},
    motion, store,
    style::{hint, label, num_font, r, text},
    theme::{parse_hex, theme},
};

const EXTRA_METHODS: [&str; 7] = ["e-Transfer", "Cheque", "Cash", "Credit card", "Bank transfer", "PayPal", "Other"];

pub enum RecordPaymentEvent {
    Done,
    Closed,
}

pub struct RecordPaymentModal {
    pub invoices: Vec<Invoice>,
    date: Option<NaiveDate>,
    date_input: Entity<DateInput>,
    method: String,
    methods: Vec<String>,
    saving: bool,
    /// Holds keyboard focus so Escape closes the dialog.
    focus: FocusHandle,
    _subs: Vec<Subscription>,
}

impl EventEmitter<RecordPaymentEvent> for RecordPaymentModal {}

impl RecordPaymentModal {
    pub fn new(invoices: Vec<Invoice>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let today = Local::now().date_naive();
        let date_input = cx.new(|cx| DateInput::date(Some(today), cx));
        let subs = vec![cx.subscribe(&date_input, |m, _, e: &DateChanged, cx| {
            if let DateChanged::Date(d) = e {
                m.date = *d;
                cx.notify();
            }
        })];
        // Saved methods first, then the usual ones; the default (or the first) is chosen.
        let settings = store::db(cx).get_settings().unwrap_or_default();
        let mut methods: Vec<String> = Vec::new();
        for name in settings.payment_methods().into_iter().map(|m| m.name).filter(|n| !n.is_empty()).chain(EXTRA_METHODS.iter().map(|s| s.to_string())) {
            if !methods.contains(&name) {
                methods.push(name);
            }
        }
        let method = Some(settings.default_payment_method().to_string()).filter(|m| !m.is_empty()).or_else(|| methods.first().cloned()).unwrap_or_default();
        RecordPaymentModal { invoices, date: Some(today), date_input, method, methods, saving: false, focus: cx.focus_handle(), _subs: subs }
    }

    fn total(&self) -> f64 {
        self.invoices.iter().map(|i| i.total).sum()
    }

    fn save(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(date) = self.date else { return };
        self.saving = true;
        cx.notify();
        let ids: Vec<i64> = self.invoices.iter().map(|i| i.id).collect();
        let date = date.format("%Y-%m-%d").to_string();
        let method = Some(self.method.clone()).filter(|m| !m.is_empty());
        let total = self.total();
        match store::write(cx, |db| db.mark_invoices_paid(&ids, Some(&date), method.as_deref())) {
            Ok(n) => {
                // A small, happy moment: bits pop from the button and the mascot cheers
                start_burst(at);
                let message = match self.invoices.as_slice() {
                    [one] => format!("Paid! {} · {}", one.invoice_number, money(total)),
                    _ => format!("Paid! {n} invoices · {}", money(total)),
                };
                toast_for(message, ToastIcon::MascotHop(Mood::Happy), 4000, cx);
                self.saving = false;
                cx.emit(RecordPaymentEvent::Done);
            }
            Err(e) => {
                toast_error(format!("Couldn't record payment: {e}"), cx);
                self.saving = false;
                cx.notify();
            }
        }
    }
}

impl Render for RecordPaymentModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(cx);
        let total = self.total();
        let single = if self.invoices.len() == 1 { self.invoices.first().cloned() } else { None };
        let title = match &single {
            Some(i) => format!("Record payment for {}", i.invoice_number),
            None => format!("Record payment for {} invoices", self.invoices.len()),
        };

        let mut body = div().flex().flex_col();
        match &single {
            None => {
                // rounded-[8px] border divide-y max-h-40 overflow-y-auto
                let mut list = div().id("pay-list").max_h(r(40.)).overflow_y_scroll().rounded(px(8.)).border_1().border_color(t.line).flex().flex_col();
                for (i, inv) in self.invoices.iter().enumerate() {
                    let mut row = div()
                        .flex()
                        .flex_none()
                        .items_center()
                        .justify_between()
                        .px(r(3.))
                        .h(px(32.))
                        .child(
                            div()
                                .flex()
                                .min_w_0()
                                .child(text(inv.invoice_number.clone(), 13.0, 500, t.fg))
                                .child(text(format!(" · {}", inv.client_name.clone().unwrap_or_default()), 13.0, 500, t.fg3)),
                        )
                        .child(div().font(num_font(500)).text_size(px(13.)).text_color(t.fg2).child(money(inv.total)));
                    if i > 0 {
                        row = row.border_t_1().border_color(t.line).h(px(33.));
                    }
                    list = list.child(row);
                }
                body = body.child(list.mb(r(4.)));
            }
            Some(inv) => {
                body = body.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .mb(r(4.))
                        .line_height(px(18.))
                        .child(text(format!("{} · issued {} · ", inv.client_name.clone().unwrap_or_default(), format_day(&inv.issue_date)), 13.0, 500, t.fg2))
                        .child(div().font(num_font(500)).text_size(px(13.)).text_color(t.fg).child(money(inv.total))),
                );
            }
        }

        let this = cx.entity().downgrade();
        let options: Vec<(String, SharedString)> = self.methods.iter().map(|m| (m.clone(), SharedString::from(m.clone()))).collect();
        body = body
            .child(
                div()
                    .flex()
                    .gap(r(3.))
                    .child(div().flex_1().min_w_0().child(label(&t, "Date received")).child(self.date_input.clone()))
                    .child(div().flex_1().min_w_0().child(label(&t, "Method")).child(select(
                        "pay-method",
                        options,
                        &self.method,
                        move |v, _, cx| {
                            let _ = this.update(cx, |m, cx| {
                                m.method = v;
                                cx.notify();
                            });
                        },
                        cx,
                    ))),
            )
            // `hint !mt-0`
            .child(hint(&t, "Use the day the money arrived. Tax Overview counts income in that month.").mt_0());

        let (e1, e2, e3) = (cx.entity().downgrade(), cx.entity().downgrade(), cx.entity().downgrade());
        Modal::new("record-payment", title, move |_, cx| {
            let _ = e1.update(cx, |_, cx| cx.emit(RecordPaymentEvent::Closed));
        })
        .size(ModalSize::Sm)
        .focus(&self.focus)
        .child(body)
        .footer(Button::secondary("pay-cancel", "Cancel").on_click(move |_, _, cx| {
            let _ = e2.update(cx, |_, cx| cx.emit(RecordPaymentEvent::Closed));
        }))
        .footer(
            Button::primary("pay-save", if self.saving { "Saving…".to_string() } else { format!("Mark {} paid", money(total)) })
                .disabled(self.saving || self.date.is_none())
                .on_click(move |e, _, cx| {
                    let at = e.position();
                    let _ = e3.update(cx, |m, cx| m.save(at, cx));
                }),
        )
    }
}

// ---------- The burst (utils/burst.ts) ----------

const BURST_COLORS: [&str; 6] = ["#FF9600", "#FFC800", "#58CC02", "#1CB0F6", "#CE82FF", "#FF4B4B"];
const BURST_MS: f32 = 1100.0;

#[derive(Clone)]
struct Burst {
    id: u64,
    at: Point<Pixels>,
    seed: u64,
    started: Instant,
}

thread_local! {
    static BURST: RefCell<Option<Burst>> = const { RefCell::new(None) };
    static BURST_ID: Cell<u64> = const { Cell::new(0) };
}

fn start_burst(at: Point<Pixels>) {
    if motion::reduced() {
        return;
    }
    let id = BURST_ID.with(|c| {
        c.set(c.get() + 1);
        c.get()
    });
    let seed = Instant::now().elapsed().as_nanos() as u64 ^ (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(7));
    BURST.with(|b| *b.borrow_mut() = Some(Burst { id, at, seed: seed | 1, started: Instant::now() }));
}

#[derive(Clone, Copy)]
struct Bit {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    size: f32,
    rot: f32,
    vr: f32,
    color: Hsla,
    round: bool,
}

fn bits(b: &Burst) -> Vec<Bit> {
    let mut s = b.seed;
    let mut rand = move || {
        // xorshift64*
        s ^= s >> 12;
        s ^= s << 25;
        s ^= s >> 27;
        (s.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f32 / (1u64 << 53) as f32
    };
    let (x, y) = (f32::from(b.at.x), f32::from(b.at.y));
    (0..26)
        .map(|i| {
            let angle = -std::f32::consts::FRAC_PI_2 + (rand() - 0.5) * std::f32::consts::PI * 1.3;
            let speed = 3.5 + rand() * 4.5;
            Bit {
                x,
                y,
                vx: angle.cos() * speed,
                vy: angle.sin() * speed,
                size: 4.0 + rand() * 4.0,
                rot: rand() * std::f32::consts::PI,
                vr: (rand() - 0.5) * 0.4,
                color: parse_hex(BURST_COLORS[i % BURST_COLORS.len()]),
                round: i % 3 == 0,
            }
        })
        .collect()
}

/// The pop of coloured bits after a payment, drawn over the window for
/// 1.1s. Pages that open [`RecordPaymentModal`] put this in their render.
pub fn burst_layer(window: &mut Window) -> Option<AnyElement> {
    let b = BURST.with(|b| b.borrow().clone())?;
    if b.started.elapsed() > Duration::from_millis(BURST_MS as u64 + 200) {
        BURST.with(|x| x.borrow_mut().take());
        return None;
    }
    let start = bits(&b);
    let viewport = window.viewport_size();
    let layer = div().w(viewport.width).h(viewport.height).with_animation(
        SharedString::from(format!("pay-burst-{}", b.id)),
        Animation::new(Duration::from_millis(BURST_MS as u64)),
        move |d, delta| {
            let start = start.clone();
            let t = delta * BURST_MS;
            d.child(
                canvas(
                    |_, _, _| (),
                    move |_, _, window, _| {
                        let alpha = (1.0 - (t - BURST_MS * 0.55).max(0.0) / (BURST_MS * 0.45)).max(0.0);
                        if alpha <= 0.0 {
                            return;
                        }
                        // One physics step per 60fps frame, as requestAnimationFrame did
                        let steps = (t / (1000.0 / 60.0)).ceil() as usize;
                        for mut bit in start {
                            for _ in 0..steps {
                                bit.vy += 0.22;
                                bit.vx *= 0.98;
                                bit.x += bit.vx;
                                bit.y += bit.vy;
                                bit.rot += bit.vr;
                            }
                            let color = bit.color.opacity(alpha);
                            if bit.round {
                                let rad = bit.size / 2.2;
                                let bounds = gpui::Bounds::new(point(px(bit.x - rad), px(bit.y - rad)), gpui::size(px(rad * 2.0), px(rad * 2.0)));
                                window.paint_quad(gpui::fill(bounds, color).corner_radii(px(rad)));
                            } else {
                                let (c, s) = (bit.rot.cos(), bit.rot.sin());
                                let (hw, hh) = (bit.size / 2.0, bit.size / 4.0);
                                let corner = |dx: f32, dy: f32| point(px(bit.x + dx * c - dy * s), px(bit.y + dx * s + dy * c));
                                let mut path = gpui::PathBuilder::fill();
                                path.move_to(corner(-hw, -hh));
                                path.line_to(corner(hw, -hh));
                                path.line_to(corner(hw, hh));
                                path.line_to(corner(-hw, hh));
                                path.close();
                                if let Ok(p) = path.build() {
                                    window.paint_path(p, color);
                                }
                            }
                        }
                    },
                )
                .size_full(),
            )
        },
    );
    Some(deferred(anchored().position(point(px(0.), px(0.))).child(layer)).with_priority(30).into_any_element())
}
