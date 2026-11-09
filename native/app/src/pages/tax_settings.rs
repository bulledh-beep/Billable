//! TaxSettings.tsx (+ SettingsLayout.tsx's Section and Row): business
//! identity, province and currency, GST/HST registration and rate, and the
//! income tax estimate settings.

use billable_core::models::{TaxSettings, TaxSettingsPatch};
use gpui::{
    div, prelude::*, px, AnyElement, AppContext, Context, Entity, IntoElement, ParentElement, Render, SharedString, Styled, Subscription,
    Window,
};
use gpui_component::input::{InputEvent, InputState, TextareaState};

use crate::{
    components::{
        client_form::trim_number,
        fields::{checkbox, TextArea, TextField},
        select, toast_error, toast_success, Button,
    },
    icons::Lucide,
    motion::rise_in,
    shell::app::{Header, Page},
    store,
    style::{card, icon, r, section_title, text},
    theme::{theme, Theme},
};

const PROVINCES: [(&str, &str); 13] = [
    ("AB", "Alberta"),
    ("BC", "British Columbia"),
    ("MB", "Manitoba"),
    ("NB", "New Brunswick"),
    ("NL", "Newfoundland and Labrador"),
    ("NS", "Nova Scotia"),
    ("NT", "Northwest Territories"),
    ("NU", "Nunavut"),
    ("ON", "Ontario"),
    ("PE", "Prince Edward Island"),
    ("QC", "Quebec"),
    ("SK", "Saskatchewan"),
    ("YT", "Yukon"),
];

/// `PROVINCIAL_GST_HST`: the combined rate a registered freelancer charges.
fn provincial_gst_hst(code: &str) -> Option<(f64, &'static str)> {
    Some(match code {
        "AB" | "BC" | "MB" | "NT" | "NU" | "QC" | "SK" | "YT" => (5.0, "GST 5%"),
        "NB" | "NL" | "NS" | "PE" => (15.0, "HST 15%"),
        "ON" => (13.0, "HST 13%"),
        _ => return None,
    })
}

/// `parseFloat(value) || 0`
fn number_value(s: &str) -> f64 {
    let s = s.trim();
    let end = s
        .char_indices()
        .take_while(|(i, c)| c.is_ascii_digit() || *c == '.' || ((*c == '-' || *c == '+') && *i == 0))
        .map(|(i, c)| i + c.len_utf8())
        .last()
        .unwrap_or(0);
    let mut num = &s[..end];
    while !num.is_empty() && num.parse::<f64>().is_err() {
        num = &num[..num.len() - 1];
    }
    num.parse::<f64>().ok().filter(|v| !v.is_nan()).unwrap_or(0.0)
}

/// `value={n || ''}` on a number field.
fn number_text(v: f64) -> String {
    if v == 0.0 || v.is_nan() { String::new() } else { trim_number(v) }
}

pub struct TaxSettingsPage {
    settings: Option<TaxSettings>,
    dirty: bool,
    name: Entity<InputState>,
    address: Entity<TextareaState>,
    gst_number: Entity<InputState>,
    rate: Entity<InputState>,
    bracket: Entity<InputState>,
    fiscal: Entity<InputState>,
    _subs: Vec<Subscription>,
}

impl TaxSettingsPage {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let settings = match store::db(cx).get_tax_settings() {
            Ok(s) => s,
            Err(e) => {
                toast_error(format!("Couldn't load tax settings: {e}"), cx);
                None
            }
        };
        let s = settings.clone();
        let get = |f: fn(&TaxSettings) -> String| s.as_ref().map(f).unwrap_or_default();
        let field = |value: String, placeholder: &'static str, window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| InputState::new(window, cx).placeholder(placeholder).default_value(value))
        };
        let name = field(get(|s| s.business_name.clone()), "Sole proprietor or business name", window, cx);
        let address_value = get(|s| s.business_address.clone());
        let address = cx.new(|cx| TextareaState::new(window, cx).placeholder("Street, city, province, postal code").default_value(address_value));
        let gst_number = field(get(|s| s.gst_hst_number.clone()), "123456789 RT0001", window, cx);
        let rate = field(get(|s| number_text(s.default_tax_rate)), "", window, cx);
        let bracket = field(get(|s| number_text(s.income_tax_bracket)), "", window, cx);
        let fiscal = field(get(|s| s.fiscal_year_start.clone()), "01-01", window, cx);
        let mut subs = Vec::new();
        for state in [&name, &gst_number, &rate, &bracket, &fiscal] {
            subs.push(cx.subscribe(state, |p, _, e: &InputEvent, cx| {
                if matches!(e, InputEvent::Change) {
                    p.dirty = true;
                    cx.notify();
                }
            }));
        }
        subs.push(cx.subscribe(&address, |p, _, e: &InputEvent, cx| {
            if matches!(e, InputEvent::Change) {
                p.dirty = true;
                cx.notify();
            }
        }));
        TaxSettingsPage { settings, dirty: false, name, address, gst_number, rate, bracket, fiscal, _subs: subs }
    }

    fn set_rate(&mut self, rate: f64, window: &mut Window, cx: &mut Context<Self>) {
        self.rate.update(cx, |s, cx| s.set_value(number_text(rate), window, cx));
    }

    /// `handleProvinceChange`: a registered business gets the province's rate.
    fn set_province(&mut self, province: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.settings.as_mut() else { return };
        s.province = province.clone();
        let registered = s.gst_hst_registered != 0;
        if registered {
            if let Some((rate, _)) = provincial_gst_hst(&province) {
                self.set_rate(rate, window, cx);
            }
        }
        self.dirty = true;
        cx.notify();
    }

    /// `handleGstToggle`
    fn toggle_gst(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.settings.as_mut() else { return };
        let registered = s.gst_hst_registered == 0;
        s.gst_hst_registered = registered as i64;
        let province = s.province.clone();
        if registered {
            if let Some((rate, _)) = provincial_gst_hst(&province) {
                self.set_rate(rate, window, cx);
            }
        }
        self.dirty = true;
        cx.notify();
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let Some(s) = self.settings.clone() else { return };
        let text_of = |e: &Entity<InputState>| e.read(cx).value().to_string();
        let patch = TaxSettingsPatch {
            business_name: Some(Some(text_of(&self.name))),
            business_address: Some(Some(self.address.read(cx).value().to_string())),
            gst_hst_number: Some(Some(text_of(&self.gst_number))),
            gst_hst_registered: Some(Some(s.gst_hst_registered)),
            province: Some(Some(s.province.clone())),
            fiscal_year_start: Some(Some(text_of(&self.fiscal))),
            default_tax_rate: Some(Some(number_value(&text_of(&self.rate)))),
            income_tax_bracket: Some(Some(number_value(&text_of(&self.bracket)))),
            currency: Some(Some(s.currency.clone())),
        };
        match store::write(cx, |db| db.save_tax_settings(&patch)) {
            Ok(saved) => {
                if saved.is_some() {
                    self.settings = saved;
                }
                self.dirty = false;
                toast_success("Tax settings saved", cx);
            }
            Err(e) => toast_error(format!("Couldn't save: {e}"), cx),
        }
        cx.notify();
    }
}

impl Page for TaxSettingsPage {
    fn header(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> Header {
        let header = Header::new("Tax settings");
        if self.settings.is_none() {
            return header;
        }
        header.actions(Button::primary("tax-settings-save", "Save changes").disabled(!self.dirty).on_click(cx.listener(|p, _, _, cx| p.save(cx))))
    }
}

/// SettingsLayout's `Row`: label and help on the left (220px), the control on the right.
fn row(t: &Theme, label: &str, help: Option<&str>, control: impl IntoElement, last: bool) -> AnyElement {
    let mut left = div().w(px(220.)).flex_none().pt(px(5.)).child(text(label.to_string(), 13.0, 500, t.fg).line_height(px(19.5)));
    if let Some(h) = help {
        left = left.child(text(h.to_string(), 12.0, 500, t.fg3).mt(r(1.)).line_height(r(4.))); // leading-4: 1rem
    }
    let mut el = div().flex().gap(r(6.)).px(r(5.)).py(r(4.)).child(left).child(div().flex_1().min_w_0().child(control));
    if !last {
        el = el.border_b_1().border_color(t.line);
    }
    el.into_any_element()
}

/// SettingsLayout's `Section`: a title (and description) over a card of rows.
fn section(t: &Theme, title: &str, description: Option<&str>, rows: Vec<AnyElement>) -> gpui::Div {
    let mut head = div().child(section_title(t, title.to_string()).line_height(px(24.)));
    if let Some(d) = description {
        head = head.child(text(d.to_string(), 12.0, 500, t.fg3).mt(r(0.5)).line_height(px(16.)));
    }
    div()
        .mb(r(7.))
        .child(div().flex().items_end().justify_between().gap(r(4.)).mb(r(2.5)).child(head))
        .child(card(t).children(rows))
}

/// A number field with "%" inside its right edge (`pr-7` and `right-2.5`).
fn percent_field(t: &Theme, state: &Entity<InputState>) -> impl IntoElement {
    div().w(r(28.)).child(
        TextField::new(state).tabular().trailing(
            div().relative().w(px(13.)).h_full().flex_none().child(
                div().absolute().right(px(-1.625)).top_0().bottom_0().flex().items_center().child(text("%", 13.0, 500, t.fg3)),
            ),
        ),
    )
}

impl Render for TaxSettingsPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(cx);
        let page = div().w_full().max_w(px(1160.)).mx_auto().px(r(7.)).pt(r(6.)).pb(r(14.)).flex().flex_col();
        let Some(s) = self.settings.clone() else { return page };
        let this = cx.weak_entity();

        // ---- Where you do business ----
        let w = this.clone();
        let mut provinces: Vec<(String, SharedString)> = vec![(String::new(), "Choose a province".into())];
        provinces.extend(PROVINCES.iter().map(|(c, n)| (c.to_string(), SharedString::from(*n))));
        let province = div().w_full().max_w(px(280.)).child(select(
            "tax-province",
            provinces,
            &s.province,
            move |v, window, cx| {
                let _ = w.update(cx, |p, cx| p.set_province(v, window, cx));
            },
            cx,
        ));
        let w = this.clone();
        let currency = div().w_full().max_w(px(280.)).child(select(
            "tax-currency",
            vec![("CAD".to_string(), "CAD, Canadian dollar".into()), ("USD".to_string(), "USD, US dollar".into())],
            &s.currency,
            move |v, _, cx| {
                let _ = w.update(cx, |p, cx| {
                    if let Some(s) = p.settings.as_mut() {
                        s.currency = v;
                    }
                    p.dirty = true;
                    cx.notify();
                });
            },
            cx,
        ));
        let business = section(
            &t,
            "Where you do business",
            None,
            vec![
                row(&t, "Legal name", Some("Used on your tax summary."), TextField::new(&self.name), false),
                // The textarea is inline-block: it sits on the line's baseline, so the
                // strut's descent (13px text, 19.5px line) shows under it.
                row(&t, "Address", None, div().pb(px(4.5)).child(TextArea::new(&self.address, 2)), false),
                row(&t, "Province", Some("Sets the suggested GST/HST rate."), province, false),
                row(&t, "Reporting currency", None, currency, true),
            ],
        );

        // ---- GST/HST ----
        let registered = s.gst_hst_registered != 0;
        let toggle = div()
            .id("tax-gst-registered")
            .flex()
            .items_center()
            .gap(r(2.5))
            .h(r(8.))
            .w_full()
            .cursor_pointer()
            .on_click(cx.listener(|p, _, window, cx| p.toggle_gst(window, cx)))
            .child(checkbox(&t, "tax-gst-check", registered, false))
            .child(text("I'm registered for GST/HST", 13.0, 500, t.fg).line_height(px(18.)));
        let mut gst_rows = vec![row(&t, "Registered", Some("Turns on GST/HST for new invoices."), toggle, !registered)];
        if registered {
            gst_rows.push(row(&t, "GST/HST number", Some("Printed on invoices that charge GST/HST."), div().w_full().max_w(px(280.)).child(TextField::new(&self.gst_number)), false));
            let mut rate = div().flex().items_center().gap(r(3.)).child(percent_field(&t, &self.rate));
            if let Some((_, label)) = provincial_gst_hst(&s.province) {
                rate = rate.child(
                    div()
                        .flex()
                        .child(text(format!("{} usually charges ", s.province), 12.0, 500, t.fg3).line_height(px(16.)).whitespace_nowrap())
                        .child(text(label, 12.0, 500, t.fg2).line_height(px(16.)).whitespace_nowrap()),
                );
            }
            gst_rows.push(row(&t, "Rate", None, rate, true));
        }
        let gst = section(&t, "GST/HST", Some("You must register once you earn over $30,000 in four consecutive quarters."), gst_rows);

        // ---- Income tax estimate ----
        let income = section(
            &t,
            "Income tax estimate",
            Some("Used on Tax overview to suggest how much to set aside."),
            vec![
                row(&t, "Marginal tax rate", Some("A rough combined federal and provincial rate."), percent_field(&t, &self.bracket), false),
                row(&t, "Fiscal year starts", Some("Most sole proprietors use the calendar year, 01-01."), div().w(r(28.)).child(TextField::new(&self.fiscal).tabular()), true),
            ],
        );

        let note = div()
            .flex()
            .items_start()
            .gap(r(2.5))
            .child(icon(Lucide::Info).size(r(3.5)).mt(r(0.5)).text_color(t.fg3))
            .child(text("Billable gives estimates to help you plan. It isn't tax advice, so check with an accountant before you file or remit.", 12.0, 500, t.fg3).line_height(r(5.))); // leading-5: 1.25rem

        page.child(rise_in("tax-settings-rise", 0, div().max_w(px(860.)).flex().flex_col().child(business).child(gst).child(income).child(note)))
    }
}
