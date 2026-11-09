//! InvoiceCreate.tsx (+ NumberField.tsx): the invoice builder. Pick a
//! client, tick exactly the time to bill across their projects (optionally
//! up to a "bill through" day), choose how time becomes lines, edit
//! descriptions and rates, add lines by hand, set tax, notes and terms, and
//! save as a draft or as sent. `/invoices/<id>/edit` reopens an invoice.

mod lines;
mod number_field;
mod pinned;

use std::{
    collections::{HashMap, HashSet},
    sync::atomic::{AtomicU64, Ordering},
};

use billable_core::{
    js::{js_number_to_string, js_slice, js_trim, round2},
    models::{Client, EntryRow, Invoice, InvoiceInput, InvoiceItemInput, InvoicePatch, PaymentMethod, TaxSettings},
};
use chrono::{Duration as Days, Local, NaiveDate};
use gpui::{
    anchored, deferred, div, point, prelude::*, px, AnyElement, Anchor, Context, Div, ElementId, Entity, FocusHandle, Focusable,
    IntoElement, MouseButton, ParentElement, Render, ScrollHandle, SharedString, Styled, Subscription,
    Window,
};
use gpui_component::input::{InputEvent, InputState, MoveDown, MoveUp, TextareaState};

use self::{
    lines::{build_lines, day_label, entry_day, money_in, to_fixed, Line, LineStyle, ManualLine, Override},
    number_field::{input, line_field, Look, NumberField, RateInput},
    pinned::pinned,
};
use crate::{
    components::{
        fields::{checkbox, input_box, TextArea},
        segmented, select, toast, toast_error, toast_success, Button, DateChanged, DateInput, EmptyState, Mood, Segment,
        ToastIcon,
    },
    format::{duration_short, hours_short},
    icons::Lucide,
    motion::rise_in,
    shell::{
        app::{navigate, Header, Page},
        route::Route,
    },
    store,
    style::{card, dot, figures, icon, icon_stroke, label, num_font, page_width, r, text},
    theme::{parse_hex, theme, Theme},
};

const TERMS: [(&str, &str); 5] = [("0", "Due on receipt"), ("15", "Net 15"), ("30", "Net 30"), ("45", "Net 45"), ("custom", "Custom")];

/// The list of ticked time scrolls past this height (`max-h-[420px]`).
const LIST_MAX: f32 = 420.0;
/// Project header rows (`h-10`) and entry rows (`h-9`).
const GROUP_HEAD: f32 = 32.5;
const ENTRY_ROW: f32 = 29.25;
/// Chromium's scrollbar gutter (index.css `::-webkit-scrollbar`).
const GUTTER: f32 = 11.0;
/// The fixed action bar: 52px plus its top border, from 220px in.
const BAR_LEFT: f32 = 220.0;
const BAR_HEIGHT: f32 = 53.0;

/// `readStyle()`: `billable.lineStyle`, the last generated line style picked.
fn read_style(cx: &gpui::App) -> LineStyle {
    match store::store(cx).prefs.line_style.as_deref().and_then(LineStyle::parse) {
        Some(s) if s != LineStyle::Manual => s,
        _ => LineStyle::Entry,
    }
}

/// `newKey()` for hand-added lines.
fn new_key() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    format!("m{}", NEXT.fetch_add(1, Ordering::Relaxed))
}

fn payment_note(m: &PaymentMethod) -> String {
    if m.email.is_empty() { format!("Payment by {}", m.name) } else { format!("Payment by {} to {}", m.name, m.email) }
}

fn parse_day(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(js_slice(s, 10), "%Y-%m-%d").ok()
}

fn day_iso(d: Option<NaiveDate>) -> String {
    d.map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_default()
}

/// `.num` text.
fn num(s: impl Into<SharedString>, size: f32, weight: u16, color: gpui::Hsla) -> Div {
    div().font(num_font(weight)).text_size(px(size)).text_color(color).whitespace_nowrap().child(s.into())
}

/// Clicks inside a field keep the page from taking focus back.
fn guard(el: impl IntoElement) -> Div {
    div().w_full().on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()).child(el)
}

/// A grid track (`minmax(min, Nfr)`), laid out as a flex item.
fn track(fr: f32, min: f32) -> Div {
    let mut d = div().flex_basis(px(0.)).min_w(px(min));
    d.style().flex_grow = Some(fr);
    d.style().flex_shrink = Some(0.);
    d
}

struct GenFields {
    desc: Entity<InputState>,
    rate: NumberField,
    _sub: Subscription,
}

struct ManFields {
    desc: Entity<InputState>,
    qty: NumberField,
    rate: NumberField,
    _sub: Subscription,
}

/// A project's ticked time in the "Time to bill" list.
struct ProjectGroup<'a> {
    project_id: i64,
    name: String,
    color: String,
    rate: f64,
    entries: Vec<&'a EntryRow>,
}

pub struct InvoiceCreatePage {
    edit_id: Option<i64>,
    loaded: bool,
    focus: FocusHandle,
    existing: Option<Invoice>,
    clients: Vec<Client>,
    tax: Option<TaxSettings>,
    methods: Vec<PaymentMethod>,

    client_id: i64,
    issue_date: Option<NaiveDate>,
    terms: String,
    due_date: Option<NaiveDate>,
    payment_method: String,
    gst_applicable: bool,
    gst_rate: f64,
    tax_rate: f64,

    entries: Vec<EntryRow>,
    selected: HashSet<i64>,
    bill_through: Option<NaiveDate>,
    style: LineStyle,
    overrides: HashMap<String, Override>,
    manual: Vec<ManualLine>,
    legacy_lines: Vec<ManualLine>,
    saving: bool,

    issue_input: Entity<DateInput>,
    due_input: Entity<DateInput>,
    bill_input: Entity<DateInput>,
    notes: Entity<TextareaState>,
    gst_number: Entity<InputState>,
    tax_rate_field: RateInput,
    gst_rate_field: RateInput,
    gen_fields: HashMap<String, GenFields>,
    man_fields: HashMap<String, ManFields>,
    list_scroll: ScrollHandle,
    _subs: Vec<Subscription>,
}

impl InvoiceCreatePage {
    pub fn new(edit: Option<i64>, client_id: Option<i64>, project_id: Option<i64>, project_ids: Vec<i64>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let db = store::db(cx);
        let today = Local::now().date_naive();
        let clients = db.list_clients().unwrap_or_default();
        let projects = db.list_projects(None).unwrap_or_default();
        let settings = db.get_settings().unwrap_or_default();
        let tax = db.get_tax_settings().ok().flatten();
        let methods = settings.payment_methods();

        let mut loaded = true;
        let mut existing: Option<Invoice> = None;
        let mut cid = 0;
        let mut issue = Some(today);
        let mut due = Some(today + Days::days(30));
        let mut terms = "30".to_string();
        let mut notes = String::new();
        let mut tax_rate = 0.0;
        let mut gst_applicable = false;
        let mut gst_rate = 0.0;
        let mut gst_number = String::new();
        let mut payment_method = String::new();
        let mut style = read_style(cx);
        let mut selected = HashSet::new();
        let mut overrides = HashMap::new();
        let mut manual = Vec::new();
        let mut legacy_lines = Vec::new();
        let mut entries = Vec::new();

        if let Some(id) = edit {
            match db.get_invoice(id).ok().flatten() {
                None => {
                    loaded = false;
                    toast_error("Invoice not found", cx);
                    window.defer(cx, |window, cx| navigate(Route::Billing { status: None }, window, cx));
                }
                Some(inv) => {
                    cid = inv.client_id;
                    issue = parse_day(&inv.issue_date);
                    due = parse_day(&inv.due_date);
                    terms = "custom".into();
                    notes = inv.notes.clone();
                    tax_rate = if inv.tax_rate.is_finite() { inv.tax_rate } else { 0.0 };
                    gst_applicable = inv.gst_hst_applicable != 0;
                    gst_rate = inv.gst_hst_rate;
                    gst_number = inv
                        .gst_hst_number
                        .clone()
                        .filter(|n| !n.is_empty())
                        .or_else(|| tax.as_ref().map(|t| t.gst_hst_number.clone()))
                        .unwrap_or_default();
                    if let Some(m) = methods.iter().find(|m| inv.notes.contains(&m.name)) {
                        payment_method = m.name.clone();
                    }
                    let items = inv.items.clone().unwrap_or_default();
                    selected = inv.entries.as_ref().map(|es| es.iter().map(|e| e.entry.id).collect()).unwrap_or_default();
                    style = inv.line_style.as_deref().and_then(LineStyle::parse).unwrap_or(LineStyle::Manual);
                    let as_manual = |it: &billable_core::models::InvoiceItem| ManualLine {
                        key: new_key(),
                        description: it.description.clone(),
                        quantity: it.quantity,
                        unit_price: it.unit_price,
                        fixed_total: Some(it.total),
                        project_id: Some(it.project_id),
                    };
                    if style == LineStyle::Manual {
                        manual = items.iter().map(as_manual).collect();
                        legacy_lines = manual.clone();
                    } else {
                        for it in &items {
                            let Some(key) = it.source_key.clone().filter(|k| !k.is_empty()) else { continue };
                            overrides.insert(key, Override { unit_price: Some(it.unit_price), description: (it.custom_description != 0).then(|| it.description.clone()) });
                        }
                        manual = items.iter().filter(|it| it.source_key.as_deref().is_none_or(str::is_empty)).map(as_manual).collect();
                    }
                    entries = db.list_invoiceable_entries(inv.client_id, Some(id)).unwrap_or_default();
                    existing = Some(inv);
                }
            }
        } else {
            if let Some(t) = tax.as_ref().filter(|t| t.gst_hst_registered != 0) {
                gst_applicable = true;
                gst_rate = if t.default_tax_rate.is_finite() { t.default_tax_rate } else { 0.0 };
                gst_number = t.gst_hst_number.clone();
            }
            if let Some(def) = methods.iter().find(|m| m.name == settings.default_payment_method()).or(methods.first()) {
                payment_method = def.name.clone();
                notes = payment_note(def);
            }
            let preset_project = project_id.filter(|p| *p != 0);
            let preset_projects: Vec<i64> = project_ids.into_iter().filter(|p| *p != 0).collect();
            let filter = match preset_project {
                Some(p) => Some(vec![p]),
                None if !preset_projects.is_empty() => Some(preset_projects),
                None => None,
            };
            cid = client_id
                .filter(|c| *c != 0)
                .or_else(|| filter.as_ref().and_then(|f| projects.iter().find(|p| p.id == f[0]).map(|p| p.client_id)))
                .unwrap_or(0);
            if cid != 0 {
                entries = db.list_invoiceable_entries(cid, None).unwrap_or_default();
                selected = entries.iter().filter(|e| filter.as_ref().is_none_or(|f| f.contains(&e.entry.project_id))).map(|e| e.entry.id).collect();
            }
        }

        let issue_input = cx.new(|cx| DateInput::date(issue, cx));
        let due_input = cx.new(|cx| DateInput::date(due, cx));
        let latest = entries.iter().map(entry_day).max();
        let bill_input = cx.new(|cx| {
            let mut d = DateInput::date(None, cx).small();
            d.set_max(latest);
            d
        });
        let notes_state = cx.new(|cx| TextareaState::new(window, cx).placeholder("Shown at the bottom of the invoice").default_value(notes));
        let gst_number_state = cx.new(|cx| InputState::new(window, cx).placeholder("123456789 RT0001").default_value(gst_number));
        let tax_rate_field = RateInput::new(tax_rate, "0", window, cx, |p: &mut Self, n, _, _| p.tax_rate = n);
        let gst_rate_field = RateInput::new(gst_rate, "", window, cx, |p: &mut Self, n, _, _| p.gst_rate = n);
        let subs = vec![
            // Like a date input's onChange, these only fire when the value
            // itself changes (not while a half-typed date stays empty).
            cx.subscribe(&issue_input, |p, _, e: &DateChanged, cx| {
                if let DateChanged::Date(d) = e {
                    if *d != p.issue_date {
                        p.change_issue_date(*d, cx);
                    }
                }
            }),
            cx.subscribe(&due_input, |p, _, e: &DateChanged, cx| {
                if let DateChanged::Date(d) = e {
                    if *d != p.due_date {
                        p.due_date = *d;
                        p.terms = "custom".into();
                        cx.notify();
                    }
                }
            }),
            cx.subscribe(&bill_input, |p, _, e: &DateChanged, cx| {
                if let DateChanged::Date(d) = e {
                    if *d != p.bill_through {
                        p.apply_bill_through(*d);
                        cx.notify();
                    }
                }
            }),
            cx.subscribe(&gst_number_state, |_, _, e: &InputEvent, cx| {
                if matches!(e, InputEvent::Change) {
                    cx.notify();
                }
            }),
        ];

        InvoiceCreatePage {
            edit_id: edit,
            loaded,
            focus: cx.focus_handle(),
            existing,
            clients,
            tax,
            methods,
            client_id: cid,
            issue_date: issue,
            terms,
            due_date: due,
            payment_method,
            gst_applicable,
            gst_rate,
            tax_rate,
            entries,
            selected,
            bill_through: None,
            style,
            overrides,
            manual,
            legacy_lines,
            saving: false,
            issue_input,
            due_input,
            bill_input,
            notes: notes_state,
            gst_number: gst_number_state,
            tax_rate_field,
            gst_rate_field,
            gen_fields: HashMap::new(),
            man_fields: HashMap::new(),
            list_scroll: ScrollHandle::new(),
            _subs: subs,
        }
    }

    fn is_edit(&self) -> bool {
        self.edit_id.is_some()
    }

    fn currency(&self) -> String {
        self.tax.as_ref().map(|t| t.currency.clone()).filter(|c| !c.is_empty()).unwrap_or_else(|| "CAD".into())
    }

    fn client(&self) -> Option<&Client> {
        self.clients.iter().find(|c| c.id == self.client_id)
    }

    /// Invoices from before lines were tied to time can keep their hand-written lines.
    fn legacy_edit(&self) -> bool {
        self.is_edit() && self.existing.as_ref().is_some_and(|e| e.line_style.as_deref().is_none_or(str::is_empty))
    }

    fn existing_is_draft(&self) -> bool {
        self.existing.as_ref().is_some_and(|e| e.status == "draft")
    }

    fn selected_entries(&self) -> Vec<&EntryRow> {
        self.entries.iter().filter(|e| self.selected.contains(&e.entry.id)).collect()
    }

    /// Lines built from ticked time, then the hand-written ones.
    fn all_lines(&self, today: NaiveDate) -> (Vec<Line>, Vec<Line>) {
        let selected = self.selected_entries();
        let generated = if self.style == LineStyle::Manual { vec![] } else { build_lines(&selected, self.style, &self.overrides, today) };
        // Hand-added lines count toward the invoice's project when there is just one
        let mut projects: Vec<i64> = selected.iter().map(|e| e.entry.project_id).collect();
        projects.sort_unstable();
        projects.dedup();
        let sole = if projects.len() == 1 { Some(projects[0]) } else { self.existing.as_ref().and_then(|e| e.project_id) };
        let manual = self
            .manual
            .iter()
            .map(|m| Line {
                key: m.key.clone(),
                description: m.description.clone(),
                quantity: m.quantity,
                unit_price: m.unit_price,
                project_id: m.project_id.unwrap_or(sole),
                source_key: None,
                generated: false,
                default_rate: 0.0,
                custom: false,
                fixed_total: m.fixed_total,
            })
            .collect();
        (generated, manual)
    }

    /// (subtotal, GST/HST, other tax, total)
    fn totals(&self, lines: &[&Line]) -> (f64, f64, f64, f64) {
        let subtotal = round2(lines.iter().fold(0.0, |s, l| s + l.total()));
        let gst = if self.gst_applicable { round2(subtotal * (self.gst_rate / 100.0)) } else { 0.0 };
        let other = round2(subtotal * (self.tax_rate / 100.0));
        (subtotal, gst, other, round2(subtotal + gst + other))
    }

    fn change_client(&mut self, cid: i64, cx: &mut Context<Self>) {
        self.client_id = cid;
        self.overrides.clear();
        self.bill_through = None;
        self.list_scroll.set_offset(point(px(0.), px(0.)));
        if cid != 0 {
            self.entries = store::db(cx).list_invoiceable_entries(cid, self.edit_id).unwrap_or_default();
            if self.edit_id.is_none() {
                self.selected = self.entries.iter().map(|e| e.entry.id).collect();
            }
        } else {
            self.entries.clear();
            self.selected.clear();
        }
        let latest = self.entries.iter().map(entry_day).max();
        self.bill_input.update(cx, |d, cx| {
            d.set_max(latest);
            d.set_date(None);
            cx.notify();
        });
        cx.notify();
    }

    fn set_due(&mut self, d: Option<NaiveDate>, cx: &mut Context<Self>) {
        self.due_date = d;
        self.due_input.update(cx, |i, cx| {
            i.set_date(d);
            cx.notify();
        });
    }

    fn change_terms(&mut self, t: String, cx: &mut Context<Self>) {
        if let Ok(days) = t.parse::<i64>() {
            self.set_due(self.issue_date.map(|d| d + Days::days(days)), cx);
        }
        self.terms = t;
        cx.notify();
    }

    fn change_issue_date(&mut self, d: Option<NaiveDate>, cx: &mut Context<Self>) {
        self.issue_date = d;
        if let Ok(days) = self.terms.parse::<i64>() {
            self.set_due(d.map(|d| d + Days::days(days)), cx);
        }
        cx.notify();
    }

    fn change_style(&mut self, s: LineStyle, cx: &mut Context<Self>) {
        // Older invoices keep their hand-written lines until a line style is picked
        if s == LineStyle::Manual {
            self.manual = self.legacy_lines.clone();
        } else if self.style == LineStyle::Manual {
            self.manual.clear();
        }
        self.style = s;
        self.overrides.clear();
        if s != LineStyle::Manual {
            let st = cx.global_mut::<store::Store>();
            st.prefs.line_style = Some(s.as_str().into());
            st.save_prefs();
        }
        cx.notify();
    }

    fn apply_bill_through(&mut self, d: Option<NaiveDate>) {
        self.bill_through = d;
        self.selected = self.entries.iter().filter(|e| d.is_none_or(|d| entry_day(e) <= d)).map(|e| e.entry.id).collect();
    }

    fn toggle_entry(&mut self, id: i64) {
        if !self.selected.remove(&id) {
            self.selected.insert(id);
        }
    }

    fn toggle_project(&mut self, ids: &[i64], on: bool) {
        for id in ids {
            if on {
                self.selected.insert(*id);
            } else {
                self.selected.remove(id);
            }
        }
    }

    fn edit_generated(&mut self, key: &str, description: Option<String>, unit_price: Option<f64>) {
        let o = self.overrides.entry(key.to_string()).or_default();
        if description.is_some() {
            o.description = description;
        }
        if unit_price.is_some() {
            o.unit_price = unit_price;
        }
    }

    fn edit_manual(&mut self, key: &str, f: impl FnOnce(&mut ManualLine)) {
        if let Some(m) = self.manual.iter_mut().find(|m| m.key == key) {
            f(m);
        }
    }

    /// Make (and keep in step) the text fields for each line.
    fn ensure_fields(&mut self, generated: &[Line], window: &mut Window, cx: &mut Context<Self>) {
        let keys: HashSet<&str> = generated.iter().map(|l| l.key.as_str()).collect();
        self.gen_fields.retain(|k, _| keys.contains(k.as_str()));
        for l in generated {
            if let Some(f) = self.gen_fields.get(&l.key) {
                if !f.desc.focus_handle(cx).is_focused(window) && f.desc.read(cx).value().as_ref() != l.description {
                    let d = l.description.clone();
                    f.desc.update(cx, |s, cx| s.set_value(d, window, cx));
                }
                f.rate.sync(l.unit_price, window, cx);
                continue;
            }
            let desc = cx.new(|cx| InputState::new(window, cx).default_value(l.description.clone()));
            let key = l.key.clone();
            let sub = cx.subscribe_in(&desc, window, move |p, state, e: &InputEvent, _, cx| {
                if matches!(e, InputEvent::Change) {
                    let text = state.read(cx).value().to_string();
                    p.edit_generated(&key, Some(text), None);
                    cx.notify();
                }
            });
            let key = l.key.clone();
            let rate = NumberField::new(l.unit_price, 2, self.focus.clone(), window, cx, move |p: &mut Self, n, _, _| p.edit_generated(&key, None, Some(n)));
            self.gen_fields.insert(l.key.clone(), GenFields { desc, rate, _sub: sub });
        }

        let keys: HashSet<String> = self.manual.iter().map(|m| m.key.clone()).collect();
        self.man_fields.retain(|k, _| keys.contains(k));
        for m in self.manual.clone() {
            if let Some(f) = self.man_fields.get(&m.key) {
                if !f.desc.focus_handle(cx).is_focused(window) && f.desc.read(cx).value().as_ref() != m.description {
                    let d = m.description.clone();
                    f.desc.update(cx, |s, cx| s.set_value(d, window, cx));
                }
                f.qty.sync(m.quantity, window, cx);
                f.rate.sync(m.unit_price, window, cx);
                continue;
            }
            let desc = cx.new(|cx| InputState::new(window, cx).placeholder("Description").default_value(m.description.clone()));
            let key = m.key.clone();
            let sub = cx.subscribe_in(&desc, window, move |p, state, e: &InputEvent, _, cx| {
                if matches!(e, InputEvent::Change) {
                    let text = state.read(cx).value().to_string();
                    p.edit_manual(&key, |m| m.description = text);
                    cx.notify();
                }
            });
            let key = m.key.clone();
            let qty = NumberField::new(m.quantity, 2, self.focus.clone(), window, cx, move |p: &mut Self, n, _, _| {
                // Changing hours or rate means the saved amount no longer applies
                p.edit_manual(&key, |m| {
                    m.quantity = n;
                    m.fixed_total = None;
                })
            });
            let key = m.key.clone();
            let rate = NumberField::new(m.unit_price, 2, self.focus.clone(), window, cx, move |p: &mut Self, n, _, _| {
                p.edit_manual(&key, |m| {
                    m.unit_price = n;
                    m.fixed_total = None;
                })
            });
            // autoFocus={!m.description}
            if m.description.is_empty() {
                let d = desc.clone();
                window.defer(cx, move |window, cx| d.update(cx, |s, cx| s.focus(window, cx)));
            }
            self.man_fields.insert(m.key.clone(), ManFields { desc, qty, rate, _sub: sub });
        }
    }

    fn save(&mut self, mark_sent: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.client_id == 0 {
            toast_error("Choose a client", cx);
            return;
        }
        let (generated, manual) = self.all_lines(Local::now().date_naive());
        let all: Vec<&Line> = generated.iter().chain(manual.iter()).collect();
        if all.is_empty() {
            toast_error("Add at least one line", cx);
            return;
        }
        if all.iter().any(|l| js_trim(&l.description).is_empty()) {
            toast_error("Every line needs a description", cx);
            return;
        }
        self.saving = true;
        let (subtotal, gst_amount, _, total) = self.totals(&all);
        let items: Vec<InvoiceItemInput> = all
            .iter()
            .map(|l| InvoiceItemInput {
                description: Some(js_trim(&l.description).to_string()),
                quantity: Some(l.quantity),
                unit_price: Some(l.unit_price),
                total: Some(l.total()),
                project_id: l.project_id,
                source_key: l.source_key.clone(),
                custom_description: l.generated && l.custom,
            })
            .collect();
        let entry_ids: Vec<i64> = self.selected.iter().copied().collect();
        let notes = self.notes.read(cx).value().to_string();
        let gst_number = self.gst_number.read(cx).value().to_string();
        let gst = self.gst_applicable;
        let currency = self.currency();
        let result = match (self.edit_id, self.existing.clone()) {
            (Some(id), existing) => {
                let patch = InvoicePatch {
                    client_id: Some(self.client_id),
                    issue_date: Some(day_iso(self.issue_date)),
                    due_date: Some(day_iso(self.due_date)),
                    subtotal: Some(subtotal),
                    tax_rate: Some(self.tax_rate),
                    total: Some(total),
                    notes: Some(notes),
                    items: Some(items),
                    entry_ids: Some(entry_ids),
                    line_style: Some(Some(self.style.as_str().into())),
                    currency: Some(currency),
                    gst_hst_applicable: Some(if gst { 1 } else { 0 }),
                    gst_hst_number: Some(gst.then_some(gst_number)),
                    gst_hst_rate: Some(if gst { self.gst_rate } else { 0.0 }),
                    gst_hst_amount: Some(gst_amount),
                    status: (mark_sent && existing.as_ref().is_some_and(|e| e.status == "draft")).then(|| "sent".into()),
                    ..Default::default()
                };
                store::write(cx, |db| db.update_invoice(id, &patch)).map(|_| {
                    let number = existing.map(|e| e.invoice_number.clone()).unwrap_or_default();
                    (id, format!("{number} saved"), false)
                })
            }
            (None, _) => {
                let input = InvoiceInput {
                    client_id: self.client_id,
                    issue_date: day_iso(self.issue_date),
                    due_date: day_iso(self.due_date),
                    status: Some(if mark_sent { "sent" } else { "draft" }.into()),
                    subtotal: Some(subtotal),
                    tax_rate: Some(self.tax_rate),
                    total: Some(total),
                    notes: Some(notes),
                    currency: Some(currency),
                    gst_hst_applicable: gst,
                    gst_hst_number: gst.then_some(gst_number),
                    gst_hst_rate: Some(if gst { self.gst_rate } else { 0.0 }),
                    gst_hst_amount: Some(gst_amount),
                    line_style: Some(self.style.as_str().into()),
                    items,
                    entry_ids: Some(entry_ids),
                    ..Default::default()
                };
                store::write(cx, |db| db.create_invoice(&input))
                    .map(|inv| (inv.id, format!("{} created{}", inv.invoice_number, if mark_sent { " and marked as sent" } else { "" }), true))
            }
        };
        self.saving = false;
        match result {
            Ok((id, message, created)) => {
                if created {
                    toast(message, ToastIcon::MascotHop(Mood::Happy), cx);
                } else {
                    toast_success(message, cx);
                }
                navigate(Route::Invoice(id), window, cx);
            }
            Err(e) => {
                toast_error(format!("Couldn't save: {e}"), cx);
                cx.notify();
            }
        }
    }

    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.edit_id {
            Some(id) => navigate(Route::Invoice(id), window, cx),
            None => crate::shell::navigate_back(window, cx),
        }
    }
}

impl Page for InvoiceCreatePage {
    fn header(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> Header {
        if !self.loaded {
            return Header::new("");
        }
        match (&self.existing, self.is_edit()) {
            (Some(e), true) => Header::new("Edit").back(e.invoice_number.clone(), Route::Invoice(e.invoice.id)),
            (None, true) => Header::new("Edit").back("Billing", Route::Billing { status: None }),
            _ => Header::new("New invoice").back("Billing", Route::Billing { status: None }),
        }
    }
}

impl Render for InvoiceCreatePage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.loaded {
            return div().into_any_element();
        }
        let t = theme(cx);
        let today = Local::now().date_naive();
        let currency = self.currency();
        let money = |n: f64| money_in(n, &currency);
        let (generated, manual_lines) = self.all_lines(today);
        self.ensure_fields(&generated, window, cx);
        let all: Vec<&Line> = generated.iter().chain(manual_lines.iter()).collect();
        let (subtotal, gst_amount, other_tax, total) = self.totals(&all);

        let mut content = div().flex().flex_col().gap(r(5.));

        // This invoice has gone out already
        if let Some(e) = self.existing.as_ref().filter(|e| e.status != "draft") {
            content = content.child(
                div()
                    .flex()
                    .gap(r(2.5))
                    .p(r(3.))
                    .rounded(r(2.))
                    .bg(t.amber.opacity(0.1))
                    .child(div().mt(px(1.)).child(icon(Lucide::AlertTriangle).size(r(4.)).text_color(t.amber)))
                    .child(
                        text(
                            format!(
                                "This invoice has been {}. Saving changes what it says, so send the client a fresh copy if they need it.",
                                if e.status == "paid" { "paid" } else { "sent" }
                            ),
                            13.0,
                            500,
                            t.fg2,
                        )
                        .line_height(px(18.))
                        .flex_1()
                        .min_w_0(),
                    ),
            );
        }

        content = content.child(self.details(&t, cx));
        if self.client_id > 0 {
            content = content.child(self.time_card(&t, &money, today, window, cx));
            content = content.child(self.lines_card(&t, &money, &generated, &manual_lines, window, cx));
            content = content.child(self.tax_and_totals(&t, &money, subtotal, gst_amount, other_tax, total, window, cx));
        }

        let page = div()
            .id("invoice-create")
            .track_focus(&self.focus)
            // Clicking anywhere that isn't a field moves focus off the fields, like a browser.
            .on_mouse_down(MouseButton::Left, cx.listener(|p, _, window, cx| p.focus.focus(window, cx)))
            .w_full()
            .max_w(px(1160.))
            .mx_auto()
            .px(r(7.))
            .pt(r(6.))
            .pb(r(28.))
            .child(rise_in("invoice-create-rise", 0, content))
            .child(self.action_bar(&t, &money, total, window, cx));
        page.children(action_bar_edges(&t, window)).into_any_element()
    }
}

/// The fixed bar starts under the sidebar's edge and runs over the page's
/// scrollbar; those two strips lie outside the page area, so draw them on top.
fn action_bar_edges(t: &Theme, window: &Window) -> Vec<AnyElement> {
    let size = window.viewport_size();
    let strip = |x: f32, w: f32| {
        deferred(
            anchored()
                .anchor(Anchor::BottomLeft)
                .position(point(px(x), size.height))
                .child(div().w(px(w)).h(px(BAR_HEIGHT)).bg(t.bg).border_t_1().border_color(t.line).occlude()),
        )
        .with_priority(1)
        .into_any_element()
    };
    vec![strip(BAR_LEFT, 228.0 - BAR_LEFT), strip(f32::from(size.width) - GUTTER, GUTTER)]
}

impl InvoiceCreatePage {
    fn details(&self, t: &Theme, cx: &mut Context<Self>) -> Div {
        let this = cx.weak_entity();
        let client_field: AnyElement = if self.is_edit() {
            // <select disabled>
            let name = self.client().map(|c| c.name.clone()).unwrap_or_default();
            input_box(t, false, true)
                .pr(r(8.))
                .child(div().flex_1().min_w_0().truncate().child(name))
                .child(
                    div()
                        .absolute()
                        .right(px(10.))
                        .top_0()
                        .bottom_0()
                        .flex()
                        .items_center()
                        .child(icon_stroke(Lucide::ChevronDown, 2.6).size(px(14.)).text_color(crate::theme::hex(0x999999))),
                )
                .into_any_element()
        } else {
            let mut options: Vec<(i64, SharedString)> = vec![(0, "Choose a client".into())];
            options.extend(self.clients.iter().map(|c| (c.id, SharedString::from(c.name.clone()))));
            let w = this.clone();
            select(
                "ic-client",
                options,
                &self.client_id,
                move |v, _, cx| {
                    let _ = w.update(cx, |p, cx| p.change_client(v, cx));
                },
                cx,
            )
            .into_any_element()
        };
        let w = this.clone();
        let terms = select(
            "ic-terms",
            TERMS.iter().map(|(v, l)| (v.to_string(), SharedString::from(*l))).collect(),
            &self.terms,
            move |v, _, cx| {
                let _ = w.update(cx, |p, cx| p.change_terms(v, cx));
            },
            cx,
        );
        let mut body = card(t).p(r(5.)).child(
            div()
                .flex()
                .gap(r(4.))
                .child(track(1.6, 0.).child(label(t, "Client")).child(client_field))
                .child(track(1.0, 140.).child(label(t, "Issue date")).child(guard(self.issue_input.clone())))
                .child(track(0.9, 120.).child(label(t, "Terms")).child(terms))
                .child(track(1.0, 140.).child(label(t, "Due date")).child(guard(self.due_input.clone()))),
        );
        if let Some(c) = self.client() {
            let mut s = format!("Billing {}", c.name);
            if !c.company.is_empty() {
                s.push_str(&format!(", {}", c.company));
            }
            if !c.email.is_empty() {
                s.push_str(&format!(" · {}", c.email));
            }
            if c.address.is_empty() {
                s.push_str(" · no billing address on file");
            }
            body = body.child(text(s, 12.0, 500, t.fg3).mt(r(3.)).line_height(px(16.)));
        }
        body
    }

    fn time_card(&self, t: &Theme, money: &dyn Fn(f64) -> String, today: NaiveDate, window: &Window, cx: &mut Context<Self>) -> Div {
        let this = cx.weak_entity();
        let selected = self.selected_entries();
        let selected_minutes: f64 = selected.iter().map(|e| e.entry.duration_minutes).sum();
        let n = self.entries.len();

        let sub: AnyElement = if n == 0 {
            let who = self.client().map(|c| c.name.clone()).unwrap_or_else(|| "This client".into());
            text(format!("{who} has no unbilled time"), 12.0, 500, t.fg3).mt(r(0.5)).line_height(px(16.)).into_any_element()
        } else {
            div()
                .flex()
                .mt(r(0.5))
                .text_size(px(12.))
                .line_height(px(16.))
                .text_color(t.fg3)
                .font_family(crate::fonts::rounded_family(500))
                .child(format!("{} of {} entries · ", selected.len(), n))
                .child(div().font(num_font(500)).child(duration_short(selected_minutes)))
                .into_any_element()
        };
        let mut head = div()
            .flex()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap(r(3.))
            .px(r(4.))
            .py(r(3.))
            .border_b_1()
            .border_color(t.line)
            .child(div().child(text("Time to bill", 15.0, 700, t.fg)).child(sub));
        if n > 0 {
            let mut through = div()
                .flex()
                .items_center()
                .gap(r(2.))
                .child(text("Bill through", 12.0, 500, t.fg3).line_height(px(16.)).whitespace_nowrap())
                .child(div().w(px(150.)).child(guard(self.bill_input.clone())));
            if self.bill_through.is_some() {
                through = through.child(Button::ghost("ic-bill-all", "All").small().on_click(cx.listener(|p, _, window, cx| {
                    p.focus.focus(window, cx);
                    p.apply_bill_through(None);
                    p.bill_input.update(cx, |d, cx| {
                        d.set_date(None);
                        cx.notify();
                    });
                    cx.notify();
                })));
            }
            let mut options = vec![
                Segment::new(LineStyle::Entry, "Line per entry"),
                Segment::new(LineStyle::Project, "Per project"),
                Segment::new(LineStyle::Day, "Per day"),
            ];
            if self.legacy_edit() {
                options.push(Segment::new(LineStyle::Manual, "Keep current lines"));
            }
            let w = this.clone();
            head = head.child(div().flex().items_center().gap(r(3.)).child(through).child(segmented(
                "ic-style",
                &self.style,
                options,
                move |s, _, cx| {
                    let _ = w.update(cx, |p, cx| p.change_style(s, cx));
                },
                cx,
            )));
        }

        let mut body = card(t).child(head);
        if self.legacy_edit() && self.style == LineStyle::Manual && n > 0 {
            body = body.child(
                text(
                    "This invoice's lines were written before invoices tracked time. Ticking time here links it to the invoice without changing the lines. Pick a line style above to rebuild the lines from the ticked time.",
                    12.0,
                    500,
                    t.fg2,
                )
                .line_height(px(16.))
                .px(r(4.))
                .py(r(2.5))
                .bg(t.fg.opacity(0.03))
                .border_b_1()
                .border_color(t.line),
            );
        }
        if n == 0 {
            return body.child(EmptyState::new("ic-time-empty", "Nothing unbilled", "Add lines by hand below, or track time for this client first.").compact());
        }

        // byProject: in order of first tracked, each project's time oldest first
        let mut groups: Vec<ProjectGroup> = Vec::new();
        for e in &self.entries {
            let i = match groups.iter().position(|g| g.project_id == e.entry.project_id) {
                Some(i) => i,
                None => {
                    groups.push(ProjectGroup {
                        project_id: e.entry.project_id,
                        name: e.project_name.clone().filter(|s| !s.is_empty()).unwrap_or_else(|| "Project".into()),
                        color: e.project_color.clone().filter(|s| !s.is_empty()).unwrap_or_else(|| "#888".into()),
                        rate: e.rate.unwrap_or(0.0),
                        entries: vec![],
                    });
                    groups.len() - 1
                }
            };
            groups[i].entries.push(e);
        }
        for g in &mut groups {
            g.entries.sort_by(|a, b| a.entry.start_time.cmp(&b.entry.start_time));
        }

        let content_h: f32 = groups.iter().map(|g| GROUP_HEAD + g.entries.len() as f32 * ENTRY_ROW).sum::<f32>() + (groups.len() as f32 - 1.0).max(0.0);
        let overflow = content_h > LIST_MAX;
        let max_scroll = (content_h - LIST_MAX).max(0.0);
        let scroll_y = if overflow { (-f32::from(self.list_scroll.offset().y)).clamp(0.0, max_scroll) } else { 0.0 };

        // `sticky top-0`: the header of the group under the top edge stays put
        // until its group scrolls away.
        let mut stuck: Option<(usize, f32)> = None;
        let mut top = 0.0;
        for (i, g) in groups.iter().enumerate() {
            let bottom = top + GROUP_HEAD + g.entries.len() as f32 * ENTRY_ROW;
            if scroll_y > top && scroll_y < bottom {
                stuck = Some((i, (bottom - GROUP_HEAD - scroll_y).min(0.0)));
            }
            top = bottom + 1.0;
        }

        let mut column = div().flex().flex_col();
        let count = groups.len();
        let mut overlay: Option<AnyElement> = None;
        for (gi, g) in groups.iter().enumerate() {
            let ids: Vec<i64> = g.entries.iter().map(|e| e.entry.id).collect();
            let on = ids.iter().filter(|id| self.selected.contains(id)).count();
            let mins: f64 = g.entries.iter().filter(|e| self.selected.contains(&e.entry.id)).map(|e| e.entry.duration_minutes).sum();
            let make_head = |id: String| {
                let ids = ids.clone();
                let n_ids = ids.len();
                let all_on = on == n_ids;
                div()
                    .id(ElementId::Name(id.into()))
                    .flex()
                    .items_center()
                    .gap(r(3.))
                    .px(r(4.))
                    .h(r(10.))
                    .bg(t.panel2.opacity(0.5))
                    .cursor_pointer()
                    .border_b_1()
                    .border_color(t.line)
                    .on_click(cx.listener(move |p, _, _, cx| {
                        p.toggle_project(&ids, !all_on);
                        cx.notify();
                    }))
                    .child(checkbox(t, ElementId::Name(format!("ic-proj-cb-{}", g.project_id).into()), all_on, on > 0 && on < n_ids))
                    .child(dot(parse_hex(&g.color), r(2.)))
                    .child(text(g.name.clone(), 13.0, 600, t.fg).line_height(px(18.)).flex_1().min_w_0().truncate())
                    .child(num(format!("{}/hr", money(g.rate)), 12.0, 500, t.fg3).line_height(px(16.)))
                    .child(num(duration_short(mins), 12.0, 500, t.fg2).line_height(px(16.)).w(r(16.)).text_right())
                    .child(num(money(mins / 60.0 * g.rate), 13.0, 500, t.fg).line_height(px(18.)).w(r(24.)).text_right())
            };
            let mut group = div().flex().flex_col();
            if gi + 1 < count {
                group = group.border_b_1().border_color(t.line);
            }
            match stuck {
                Some((si, y)) if si == gi => {
                    // The header has slid down over its own rows; its place is empty.
                    group = group.child(div().h(r(10.)).flex_none());
                    overlay = Some(
                        div()
                            .absolute()
                            .top_0()
                            .left_0()
                            .right(px(GUTTER))
                            .h(px(LIST_MAX))
                            .overflow_hidden()
                            .child(make_head(format!("ic-proj-stuck-{}", g.project_id)).absolute().left_0().right_0().top(px(y)).block_mouse_except_scroll())
                            .into_any_element(),
                    );
                }
                _ => group = group.child(make_head(format!("ic-proj-{}", g.project_id))),
            }
            for e in &g.entries {
                let id = e.entry.id;
                let day = entry_day(e);
                let after = self.bill_through.is_some_and(|b| day > b);
                let hover = t.fg.opacity(0.02);
                let mut row = div()
                    .id(ElementId::Name(format!("ic-entry-{id}").into()))
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(r(3.))
                    .pl(r(9.))
                    .pr(r(4.))
                    .h(r(9.))
                    .cursor_pointer()
                    .hover(move |s| s.bg(hover))
                    .on_click(cx.listener(move |p, _, _, cx| {
                        p.toggle_entry(id);
                        cx.notify();
                    }))
                    .child(checkbox(t, ElementId::Name(format!("ic-entry-cb-{id}").into()), self.selected.contains(&id), false))
                    .child(num(day_label(day, today), 12.0, 500, t.fg3).line_height(px(16.)).w(r(24.)).flex_none())
                    .child(text(e.entry.description.clone(), 13.0, 500, t.fg2).line_height(px(18.)).flex_1().min_w_0().truncate());
                if self.edit_id.is_some() && e.entry.invoice_id == self.edit_id {
                    row = row.child(
                        div()
                            .flex()
                            .flex_none()
                            .items_center()
                            .h(px(22.))
                            .px(r(2.5))
                            .rounded_full()
                            .bg(t.fg3.opacity(0.13))
                            .child(text("On this invoice", 12.0, 600, t.fg3).line_height(px(16.)).whitespace_nowrap()),
                    );
                }
                row = row
                    .child(num(duration_short(e.entry.duration_minutes), 12.0, 500, t.fg2).line_height(px(16.)).w(r(16.)).text_right())
                    .child(num(money(e.entry.duration_minutes / 60.0 * e.rate.unwrap_or(0.0)), 13.0, 500, t.fg2).line_height(px(18.)).w(r(24.)).text_right());
                if after {
                    row = row.opacity(0.5);
                }
                group = group.child(row);
            }
            column = column.child(group);
        }

        if !overflow {
            return body.child(column);
        }

        // max-h-[420px] overflow-y-auto, with Chromium's 11px scrollbar
        let handle = self.list_scroll.clone();
        let scroller = div()
            .id("ic-time-scroll")
            .h(px(LIST_MAX))
            .pr(px(GUTTER))
            .overflow_y_scroll()
            .track_scroll(&self.list_scroll)
            .on_scroll(move |e, window, cx| {
                // Scroll the list, not the page, while the list can still move that way.
                let dy = f32::from(e.delta.pixel_delta(window.line_height()).y);
                let before = (f32::from(handle.offset().y) - dy).clamp(-max_scroll, 0.0);
                let can = (dy < 0.0 && before > -max_scroll + 0.5) || (dy > 0.0 && before < -0.5);
                if can {
                    cx.stop_propagation();
                }
            })
            .child(column);
        let thumb_len = (LIST_MAX * LIST_MAX / content_h).max(20.0);
        let thumb_top = if max_scroll > 0.0 { (LIST_MAX - thumb_len) * scroll_y / max_scroll } else { 0.0 };
        let (rest, hover) = (t.fg.opacity(0.14), t.fg.opacity(0.26));
        let _ = window;
        let thumb = div()
            .id("ic-time-thumb")
            .group("ic-time-thumb")
            .absolute()
            .right_0()
            .w(px(GUTTER))
            .top(px(thumb_top))
            .h(px(thumb_len))
            .child(div().absolute().left(px(3.)).right(px(3.)).top(px(3.)).bottom(px(3.)).rounded_full().bg(rest).group_hover("ic-time-thumb", move |s| s.bg(hover)));
        body.child(div().relative().child(scroller).children(overlay).child(thumb))
    }

    fn lines_card(&self, t: &Theme, money: &dyn Fn(f64) -> String, generated: &[Line], manual: &[Line], window: &Window, cx: &mut Context<Self>) -> Div {
        let head = div()
            .flex()
            .items_center()
            .justify_between()
            .gap(r(3.))
            .px(r(4.))
            .h(px(46.))
            .border_b_1()
            .border_color(t.line)
            .child(text("Invoice lines", 15.0, 700, t.fg))
            .child(Button::ghost("ic-add-line", "Add line").small().on_click(cx.listener(|p, _, _, cx| {
                p.manual.push(ManualLine { key: new_key(), description: String::new(), quantity: 1.0, unit_price: 0.0, fixed_total: None, project_id: None });
                cx.notify();
            })));
        let body = card(t).child(head);
        if generated.is_empty() && manual.is_empty() {
            return body.child(div().px(r(4.)).py(r(8.)).text_center().child(text("Tick time above or add a line.", 13.0, 500, t.fg3).line_height(px(18.))));
        }

        // `.tbl` with w-24 / w-32 / w-28 / w-10 columns; Description takes the rest.
        let measure = |s: &str| crate::components::table::measure(window, s, 14.0, 500, true);
        let amounts = generated.iter().map(|l| round2(l.quantity * l.unit_price)).chain(manual.iter().map(|l| l.total()));
        let amount_w = amounts.map(|a| measure(&money(a)) + 19.5).fold(91.0f32, f32::max);
        let hours_w = generated.iter().map(|l| 9.75 + 3.25 + measure(&to_fixed(l.quantity, 2)) + 19.5).fold(78.0f32, f32::max);
        let resettable = |l: &Line| l.custom || l.unit_price != l.default_rate;
        let has_button = !manual.is_empty() || generated.iter().any(resettable);
        // The 27px buttons widen the last column past its 32.5px.
        let action_w = if has_button { 9.75 + 27.0 + 13.0 } else { 32.5 };
        let rate_w = 104.0;
        let total = page_width(window) - 2.0;
        let desc_w = (total - hours_w - rate_w - amount_w - action_w).max(0.0);
        let widths = [desc_w, hours_w, rate_w, amount_w, action_w];
        let pads = [(13.0, 9.75), (9.75, 9.75), (9.75, 9.75), (9.75, 9.75), (9.75, 13.0)];
        let cell = |i: usize, right: bool, pad: Option<(f32, f32)>| {
            let (l, rr) = pad.unwrap_or(pads[i]);
            let d = div().w(px(widths[i])).flex_none().h_full().pl(px(l)).pr(px(rr)).flex().items_center().min_w_0();
            if right { d.justify_end() } else { d }
        };
        let th = |s: &'static str| text(s, 12.0, 600, t.fg3).whitespace_nowrap();
        let header = div()
            .flex()
            .h(px(36.))
            .border_b_1()
            .border_color(t.line)
            // Padding matches the text inside the editable fields below
            .child(cell(0, false, Some((22.0, 9.75))).child(th("Description")))
            .child(cell(1, true, None).child(th("Hours")))
            .child(cell(2, true, Some((9.75, 18.0))).child(th("Rate")))
            .child(cell(3, true, None).child(th("Amount")))
            .child(cell(4, false, None));

        let mut table = div().w(px(total)).flex().flex_col().child(header);
        let count = generated.len() + manual.len();
        let mut index = 0;
        let row = |cells: Vec<Div>, last: bool| {
            let mut r = div().flex().flex_none().h(px(if last { 50.0 } else { 49.0 }));
            if !last {
                r = r.border_b_1().border_color(t.line);
            }
            r.children(cells)
        };
        for l in generated {
            index += 1;
            let Some(f) = self.gen_fields.get(&l.key) else { continue };
            let key = l.key.clone();
            let mut reset = cell(4, false, None);
            if resettable(l) {
                reset = reset.child(
                    Button::icon_button(ElementId::Name(format!("ic-reset-{}", l.key).into()), Lucide::RotateCcw)
                        .small()
                        .tooltip("Reset to the tracked description and rate")
                        .on_click(cx.listener(move |p, _, window, cx| {
                            p.focus.focus(window, cx);
                            p.overrides.remove(&key);
                            cx.notify();
                        })),
                );
            }
            let hours = div()
                .id(ElementId::Name(format!("ic-hours-{}", l.key).into()))
                .flex()
                .items_center()
                .gap(r(1.))
                .tooltip(|w, cx| gpui_component::tooltip::Tooltip::new("Hours come from the ticked time").build(w, cx))
                .child(icon(Lucide::Lock).size(r(3.)).text_color(t.fg4))
                .child(num(to_fixed(l.quantity, 2), 14.0, 500, t.fg2));
            table = table.child(row(
                vec![
                    cell(0, false, None).child(line_field(t, &f.desc, Look::Ghost, false, false, window, cx)),
                    cell(1, true, None).child(hours),
                    cell(2, false, None).child(line_field(t, &f.rate.state, Look::Ghost, true, true, window, cx)),
                    cell(3, true, None).child(num(money(round2(l.quantity * l.unit_price)), 14.0, 500, t.fg)),
                    reset,
                ],
                index == count,
            ));
        }
        for l in manual {
            index += 1;
            let Some(f) = self.man_fields.get(&l.key) else { continue };
            let key = l.key.clone();
            table = table.child(row(
                vec![
                    cell(0, false, None).child(line_field(t, &f.desc, Look::Plain, false, false, window, cx)),
                    cell(1, false, None).child(line_field(t, &f.qty.state, Look::Plain, true, true, window, cx)),
                    cell(2, false, None).child(line_field(t, &f.rate.state, Look::Plain, true, true, window, cx)),
                    cell(3, true, None).child(num(money(l.total()), 14.0, 500, t.fg)),
                    cell(4, false, None).child(
                        Button::icon_button(ElementId::Name(format!("ic-remove-{}", l.key).into()), Lucide::Trash2)
                            .small()
                            .hover_text(t.red)
                            .tooltip("Remove line")
                            .on_click(cx.listener(move |p, _, window, cx| {
                                p.focus.focus(window, cx);
                                p.manual.retain(|m| m.key != key);
                                cx.notify();
                            })),
                    ),
                ],
                index == count,
            ));
        }
        body.overflow_hidden().child(table)
    }

    #[allow(clippy::too_many_arguments)]
    fn tax_and_totals(&self, t: &Theme, money: &dyn Fn(f64) -> String, subtotal: f64, gst_amount: f64, other_tax: f64, total: f64, window: &Window, cx: &mut Context<Self>) -> Div {
        let this = cx.weak_entity();
        let mut top = div().flex().gap(r(4.));
        if !self.methods.is_empty() {
            let w = this.clone();
            let methods = self.methods.clone();
            let options = self
                .methods
                .iter()
                .map(|m| (m.name.clone(), SharedString::from(if m.email.is_empty() { m.name.clone() } else { format!("{} · {}", m.name, m.email) })))
                .collect();
            top = top.child(div().flex_1().min_w_0().child(label(t, "Payment method")).child(select(
                "ic-payment",
                options,
                &self.payment_method,
                move |v: String, window, cx| {
                    let note = methods.iter().find(|m| m.name == v).map(payment_note);
                    let _ = w.update(cx, |p, cx| {
                        p.payment_method = v;
                        if let Some(note) = note {
                            p.notes.update(cx, |s, cx| s.set_value(note, window, cx));
                        }
                        cx.notify();
                    });
                },
                cx,
            )));
        }
        top = top.child(div().flex_1().min_w_0().child(label(t, "Other tax (%)")).child(self.rate_field(t, true, window, cx)));
        if self.methods.is_empty() {
            top = top.child(div().flex_1().min_w_0());
        }

        let hint = match self.tax.as_ref().filter(|t| t.gst_hst_registered != 0) {
            Some(tx) if !tx.province.is_empty() => format!("From your tax settings ({})", tx.province),
            Some(_) => "From your tax settings".to_string(),
            None => "You're not registered in Tax settings".to_string(),
        };
        let mut gst_box = div().rounded(r(2.)).border_1().border_color(t.line).p(r(3.)).child(
            div()
                .id("ic-gst-toggle")
                .flex()
                .items_center()
                .gap(r(2.5))
                .cursor_pointer()
                .on_click(cx.listener(|p, _, _, cx| {
                    p.gst_applicable = !p.gst_applicable;
                    cx.notify();
                }))
                .child(checkbox(t, "ic-gst-cb", self.gst_applicable, false))
                .child(text("Charge GST/HST", 13.0, 500, t.fg).line_height(px(18.)))
                .child(text(hint, 12.0, 500, t.fg3).line_height(px(16.))),
        );
        if self.gst_applicable {
            let focused = self.gst_number.focus_handle(cx).is_focused(window);
            gst_box = gst_box.child(
                div()
                    .flex()
                    .gap(r(3.))
                    .mt(r(3.))
                    .child(div().flex_1().min_w_0().child(label(t, "Rate (%)")).child(self.rate_field(t, false, window, cx)))
                    .child(
                        div().flex_1().min_w_0().child(label(t, "GST/HST number")).child(guard(
                            input_box(t, focused, false).child(div().flex_1().min_w_0().h_full().child(input(&self.gst_number, false, false))),
                        )),
                    ),
            );
        }
        let left = card(t)
            .flex_1()
            .min_w_0()
            .p(r(5.))
            .flex()
            .flex_col()
            .gap(r(4.))
            .child(top)
            .child(gst_box)
            .child(div().child(label(t, "Notes and payment instructions")).child(guard(TextArea::new(&self.notes, 3))));

        let line = |l: String, v: String| {
            div().flex().justify_between().child(text(l, 13.0, 500, t.fg3).line_height(px(18.))).child(num(v, 13.0, 500, t.fg).line_height(px(18.)))
        };
        let mut sums = div().flex().flex_col().gap(r(2.)).child(line("Subtotal".into(), money(subtotal)));
        if self.gst_applicable && self.gst_rate > 0.0 {
            sums = sums.child(line(format!("GST/HST ({}%)", js_number_to_string(self.gst_rate)), money(gst_amount)));
        }
        if self.tax_rate > 0.0 {
            sums = sums.child(line(format!("Other tax ({}%)", js_number_to_string(self.tax_rate)), money(other_tax)));
        }
        sums = sums.child(
            div()
                .flex()
                .justify_between()
                .items_baseline()
                .pt(r(3.))
                .border_t_1()
                .border_color(t.line)
                .child(text("Total", 13.0, 600, t.fg).line_height(px(18.)))
                .child(figures(money(total), 28.0, t.fg).line_height(px(26.))),
        );
        let selected_minutes: f64 = self.selected_entries().iter().map(|e| e.entry.duration_minutes).sum();
        if selected_minutes > 0.0 {
            sums = sums.child(
                text(format!("{} of tracked time will be marked as billed on this invoice.", hours_short(selected_minutes / 60.0)), 12.0, 500, t.fg3)
                    .pt(r(1.))
                    .line_height(px(16.)),
            );
        }
        let right = card(t).w(px(320.)).flex_none().p(r(5.)).child(sums);
        div().flex().gap(r(5.)).items_start().child(left).child(right)
    }

    /// The "Other tax (%)" (`other`) or GST/HST "Rate (%)" number field.
    fn rate_field(&self, t: &Theme, other: bool, window: &Window, cx: &mut Context<Self>) -> Div {
        let field = if other { &self.tax_rate_field } else { &self.gst_rate_field };
        let focused = field.state.focus_handle(cx).is_focused(window);
        guard(
            input_box(t, focused, false)
                // ↑/↓ step by 0.01, as Chromium's number field does
                .capture_action(cx.listener(move |p, _: &MoveUp, window, cx| p.step_rate(other, true, window, cx)))
                .capture_action(cx.listener(move |p, _: &MoveDown, window, cx| p.step_rate(other, false, window, cx)))
                .child(div().flex_1().min_w_0().h_full().child(input(&field.state, false, true))),
        )
    }

    fn step_rate(&mut self, other: bool, up: bool, window: &mut Window, cx: &mut Context<Self>) {
        if other {
            self.tax_rate = self.tax_rate_field.step(up, window, cx);
        } else {
            self.gst_rate = self.gst_rate_field.step(up, window, cx);
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn action_bar(&self, t: &Theme, money: &dyn Fn(f64) -> String, total: f64, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let edit = self.is_edit();
        let title = if edit {
            format!("Edit {}", self.existing.as_ref().map(|e| e.invoice_number.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| "invoice".into()))
        } else {
            "New invoice".to_string()
        };
        let mut summary = div().flex().text_size(px(13.)).text_color(t.fg3).whitespace_nowrap();
        if self.client_id > 0 {
            summary = summary.child(format!("{title} · ")).child(num(money(total), 13.0, 600, t.fg));
        } else {
            summary = summary.child(title);
        }
        let no_client = self.client_id == 0;
        let mut buttons = div()
            .ml_auto()
            .flex()
            .items_center()
            .gap(r(2.))
            .child(Button::secondary("ic-cancel", "Cancel").on_click(cx.listener(|p, _, window, cx| p.cancel(window, cx))));
        if !edit || self.existing_is_draft() {
            buttons = buttons
                .child(
                    Button::secondary("ic-save-draft", if edit { "Save draft" } else { "Save as draft" })
                        .disabled(self.saving || no_client)
                        .on_click(cx.listener(|p, _, window, cx| p.save(false, window, cx))),
                )
                .child(
                    Button::primary("ic-save-sent", "Save and mark as sent")
                        .disabled(self.saving || no_client)
                        .on_click(cx.listener(|p, _, window, cx| p.save(true, window, cx))),
                );
        } else {
            buttons = buttons.child(Button::primary("ic-save", "Save changes").disabled(self.saving).on_click(cx.listener(|p, _, window, cx| p.save(false, window, cx))));
        }
        let size = window.viewport_size();
        let bar = div()
            .id("ic-action-bar")
            .absolute()
            .top_0()
            .left_0()
            .w(size.width - px(BAR_LEFT))
            .h(px(BAR_HEIGHT))
            .bg(t.bg)
            .border_t_1()
            .border_color(t.line)
            .occlude()
            .on_mouse_down(MouseButton::Left, cx.listener(|p, _, window, cx| p.focus.focus(window, cx)))
            .child(div().max_w(px(1160.)).mx_auto().px(r(7.)).h(px(52.)).flex().items_center().gap(r(3.)).child(summary).child(buttons));
        pinned(point(px(BAR_LEFT), size.height - px(BAR_HEIGHT)), bar)
    }
}
