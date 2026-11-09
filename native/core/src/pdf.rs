//! The HTML behind Billable's PDFs, ported markup-for-markup from the
//! Electron generators: `pdf.ts` (client invoices), `tax-pdf.ts` (the tax
//! summary) and `commission-pdf.ts` (commission payout invoices).
//!
//! The output is byte-for-byte what the TypeScript produced for the same
//! data, including `Intl.NumberFormat` money, `toFixed`, JS number printing
//! and the templates' whitespace. Turning the HTML into an A4 PDF is the
//! app's job (it prints it with the system WebKit).

use anyhow::{Context, Result};

use crate::js::{js_number_to_string, js_slice, js_string_to_number};
use crate::models::{Commission, CommissionInvoiceDetail, Invoice, Settings, TaxOverview, TaxSettings};
use crate::Db;

/// A document ready to print: its HTML and the file name the app suggests.
#[derive(Debug, Clone, PartialEq)]
pub struct PdfDoc {
    pub html: String,
    /// `INV-1004.pdf`, `tax-summary-2026.pdf`, `COMM-1001.pdf`.
    pub file_name: String,
}

impl Db {
    /// `generateInvoicePDF`'s HTML. Fails with "Invoice not found" like the original.
    pub fn invoice_pdf_doc(&self, invoice_id: i64) -> Result<(Invoice, PdfDoc)> {
        let invoice = self.get_invoice(invoice_id)?.context("Invoice not found")?;
        let settings = self.get_settings()?;
        let tax = self.get_tax_settings()?;
        let html = invoice_html(&invoice, &settings, tax.as_ref());
        let file_name = format!("{}.pdf", invoice.invoice_number);
        Ok((invoice, PdfDoc { html, file_name }))
    }

    /// `generateTaxSummaryPDF`'s HTML, dated today by this database's clock.
    pub fn tax_summary_pdf_doc(&self, tax_year: i64) -> Result<PdfDoc> {
        let overview = self.get_tax_overview(tax_year)?;
        let tax = self.get_tax_settings()?;
        let settings = self.get_settings()?;
        let html = tax_summary_html(&overview, tax.as_ref(), &settings, tax_year, &self.local_today());
        Ok(PdfDoc { html, file_name: format!("tax-summary-{tax_year}.pdf") })
    }

    /// `generateCommissionInvoicePDF`'s HTML. Fails with "Commission invoice not found".
    pub fn commission_invoice_pdf_doc(&self, id: i64) -> Result<PdfDoc> {
        let invoice = self.get_commission_invoice(id)?.context("Commission invoice not found")?;
        let settings = self.get_settings()?;
        let html = commission_invoice_html(&invoice, &settings, &self.local_today());
        Ok(PdfDoc { html, file_name: format!("{}.pdf", invoice.invoice_number) })
    }
}

// ============ JS / Intl helpers ============

const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// The generators' `escapeHtml` / `escape`.
fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#039;"),
            c => out.push(c),
        }
    }
    out
}

fn truthy_num(x: f64) -> bool {
    x != 0.0 && !x.is_nan()
}

/// `n || 0`
fn or0(x: f64) -> f64 {
    if truthy_num(x) {
        x
    } else {
        0.0
    }
}

/// `formatDay`: "Jul 20, 2026" from a YYYY-MM-DD string (the input as-is
/// when it isn't one).
fn format_day(ymd: &str) -> String {
    let parts: Vec<f64> = js_slice(ymd, 10).split('-').map(js_string_to_number).collect();
    let get = |i: usize| parts.get(i).copied().unwrap_or(f64::NAN);
    let (y, m, d) = (get(0), get(1), get(2));
    if !truthy_num(y) || !truthy_num(m) || !truthy_num(d) {
        return ymd.to_string();
    }
    let month = if m.fract() == 0.0 && (1.0..=12.0).contains(&m) { MONTHS[m as usize - 1] } else { "undefined" };
    format!("{month} {}, {}", js_number_to_string(d), js_number_to_string(y))
}

/// `Number.prototype.toFixed(digits)`: nearest decimal of the exact value,
/// exact ties away from zero.
fn to_fixed(x: f64, digits: usize) -> String {
    if !x.is_finite() {
        return js_number_to_string(x);
    }
    if x.abs() >= 1e21 {
        return js_number_to_string(x);
    }
    let exact = format!("{:.80}", x.abs());
    let (int_part, frac) = exact.split_once('.').unwrap_or((&exact, ""));
    let keep: String = frac.chars().take(digits).collect();
    let round_up = frac[keep.len()..].chars().next().is_some_and(|c| c >= '5');
    let mut s = format!("{int_part}{keep}");
    if round_up {
        s = increment(&s);
    }
    let split = s.len() - digits;
    let body = if digits == 0 { s.clone() } else { format!("{}.{}", &s[..split], &s[split..]) };
    if x < 0.0 {
        format!("-{body}")
    } else {
        body
    }
}

/// Add one to a string of decimal digits.
fn increment(digits: &str) -> String {
    let mut b = digits.as_bytes().to_vec();
    let mut i = b.len();
    loop {
        if i == 0 {
            b.insert(0, b'1');
            break;
        }
        i -= 1;
        if b[i] == b'9' {
            b[i] = b'0';
        } else {
            b[i] += 1;
            break;
        }
    }
    String::from_utf8(b).unwrap_or_default()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Locale {
    EnUs,
    EnCa,
}

/// Symbol, fraction digits and whether a space follows the symbol, for
/// `Intl.NumberFormat(locale, { style: 'currency', currency })` (ICU's en / en-CA data).
fn currency_style(locale: Locale, code: &str) -> (String, usize, bool) {
    let ca = locale == Locale::EnCa;
    let (sym, digits): (&str, usize) = match code {
        "USD" => (if ca { "US$" } else { "$" }, 2),
        "CAD" => (if ca { "$" } else { "CA$" }, 2),
        "JPY" => (if ca { "JP¥" } else { "¥" }, 0),
        "EUR" => ("€", 2),
        "GBP" => ("£", 2),
        "AUD" => ("A$", 2),
        "NZD" => ("NZ$", 2),
        "CNY" => ("CN¥", 2),
        "HKD" => ("HK$", 2),
        "INR" => ("₹", 2),
        "MXN" => ("MX$", 2),
        "KRW" => ("₩", 0),
        "BRL" => ("R$", 2),
        "ILS" => ("₪", 2),
        "TWD" => ("NT$", 2),
        "PHP" => ("₱", 2),
        "VND" => ("₫", 0),
        "XAF" => return ("FCFA".into(), 0, true),
        "XOF" => return ("F\u{202f}CFA".into(), 0, true),
        "HUF" | "ISK" | "CLP" | "COP" | "IDR" | "PYG" | "UGX" | "RWF" | "KMF" | "GNF" | "DJF" | "BIF" | "VUV" | "XPF" => {
            return (code.into(), 0, true)
        }
        "KWD" | "BHD" | "OMR" | "JOD" | "TND" | "IQD" | "LYD" => return (code.into(), 3, true),
        other => return (other.into(), 2, true),
    };
    (sym.into(), digits, false)
}

/// `new Intl.NumberFormat(locale, { style: 'currency', currency }).format(n)`.
/// ICU rounds the shortest decimal form of the double, halves away from zero.
fn intl_currency(n: f64, locale: Locale, currency: &str) -> String {
    let code = currency.to_ascii_uppercase();
    let (sym, digits, spaced) = currency_style(locale, &code);
    let sign = if n < 0.0 { "-" } else { "" };
    let sep = if spaced { "\u{a0}" } else { "" };
    if !n.is_finite() {
        return format!("{sign}{sym}{sep}∞");
    }
    let number = round_shortest(n.abs(), digits);
    format!("{sign}{sym}{sep}{number}")
}

/// `x` (≥ 0) to `digits` places with thousands separators, rounding its
/// shortest round-trip decimal half away from zero.
fn round_shortest(x: f64, digits: usize) -> String {
    let scaled: String = if x == 0.0 {
        "0".into()
    } else {
        let e = format!("{x:e}");
        let (mant, exp) = e.split_once('e').unwrap_or((&e, "0"));
        let exp: i64 = exp.parse().unwrap_or(0);
        let ds: Vec<u8> = mant.bytes().filter(|b| b.is_ascii_digit()).collect();
        // value = 0.ds × 10^(exp + 1); keep the digits through `digits` places
        let keep = exp + 1 + digits as i64;
        if keep < 0 {
            "0".into()
        } else {
            let keep = keep as usize;
            let mut kept: String = (0..keep).map(|i| *ds.get(i).unwrap_or(&b'0') as char).collect();
            if ds.get(keep).is_some_and(|d| *d >= b'5') {
                kept = if kept.is_empty() { "1".into() } else { increment(&kept) };
            }
            if kept.is_empty() {
                "0".into()
            } else {
                kept
            }
        }
    };
    let padded = format!("{scaled:0>width$}", width = digits + 1);
    let (int_part, frac) = padded.split_at(padded.len() - digits);
    let int_part = int_part.trim_start_matches('0');
    let int_part = if int_part.is_empty() { "0" } else { int_part };
    let mut grouped = String::new();
    for (i, ch) in int_part.chars().enumerate() {
        if i > 0 && (int_part.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    if digits == 0 {
        grouped
    } else {
        format!("{grouped}.{frac}")
    }
}

fn plural(n: i64) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

// ============ pdf.ts ============

/// `generateInvoiceHTML(invoice, settings)` (with `getTaxSettings() || {}`).
pub fn invoice_html(invoice: &Invoice, settings: &Settings, tax: Option<&TaxSettings>) -> String {
    let items = invoice.items.as_deref().unwrap_or(&[]);
    let currency = [invoice.currency.as_str(), settings.default_currency()].into_iter().find(|s| !s.is_empty()).unwrap_or("CAD");
    let locale = if currency == "CAD" { Locale::EnCa } else { Locale::EnUs };
    let money = |amount: f64| intl_currency(or0(amount), locale, currency);

    let tax_business_name = tax.map(|t| t.business_name.as_str()).unwrap_or("");
    let tax_business_address = tax.map(|t| t.business_address.as_str()).unwrap_or("");
    let business_name = [settings.business_name(), tax_business_name].into_iter().find(|s| !s.is_empty()).unwrap_or("Your Business");
    let business_address = [settings.business_address(), tax_business_address].into_iter().find(|s| !s.is_empty()).unwrap_or("");
    let tax_id = if !settings.tax_id().is_empty() && settings.tax_id() != "0" { settings.tax_id() } else { "" };

    let gst_applicable = invoice.gst_hst_applicable != 0;
    let gst_rate = or0(invoice.gst_hst_rate);
    let gst_amount = if gst_applicable { invoice.subtotal * (gst_rate / 100.0) } else { 0.0 };
    let other_tax = invoice.subtotal * (or0(invoice.tax_rate) / 100.0);
    let is_paid = invoice.status == "paid";
    let payment_date = invoice.payment_date.as_deref().filter(|s| !s.is_empty());
    let payment_method = invoice.payment_method.as_deref().filter(|s| !s.is_empty());

    let mut rows = String::new();
    for item in items {
        rows.push_str("\n    <tr>\n      <td class=\"desc\">");
        rows.push_str(&esc(&item.description));
        rows.push_str("</td>\n      <td class=\"num\">");
        rows.push_str(&to_fixed(item.quantity, 2));
        rows.push_str("</td>\n      <td class=\"num\">");
        rows.push_str(&money(item.unit_price));
        rows.push_str("</td>\n      <td class=\"num strong\">");
        rows.push_str(&money(item.total));
        rows.push_str("</td>\n    </tr>");
    }

    let company = invoice.client_company.as_deref().unwrap_or("");
    let client_lines: String = [
        if !company.is_empty() && invoice.client_company != invoice.client_name { esc(company) } else { String::new() },
        match invoice.client_address.as_deref() {
            Some(a) if !a.is_empty() => esc(a).replace('\n', "<br>"),
            _ => String::new(),
        },
        match invoice.client_email.as_deref() {
            Some(e) if !e.is_empty() => esc(e),
            _ => String::new(),
        },
    ]
    .into_iter()
    .filter(|l| !l.is_empty())
    .map(|l| format!("<div class=\"muted\">{l}</div>"))
    .collect();

    let mut h = String::with_capacity(8192);
    h.push_str(INVOICE_HEAD);
    h.push_str("        ");
    if !settings.business_logo().is_empty() {
        h.push_str("<img src=\"");
        h.push_str(settings.business_logo());
        h.push_str("\" alt=\"\" style=\"max-height: 44px; margin-bottom: 12px;\">");
    }
    h.push_str("\n        <div class=\"biz-name\">");
    h.push_str(&esc(business_name));
    h.push_str("</div>\n        ");
    if !business_address.is_empty() {
        h.push_str("<div class=\"muted\" style=\"white-space: pre-line; margin-top: 2px;\">");
        h.push_str(&esc(business_address));
        h.push_str("</div>");
    }
    h.push_str("\n        ");
    if !settings.business_email().is_empty() {
        h.push_str("<div class=\"muted\">");
        h.push_str(&esc(settings.business_email()));
        h.push_str("</div>");
    }
    h.push_str("\n        ");
    if !tax_id.is_empty() {
        h.push_str("<div class=\"muted\">Business number ");
        h.push_str(&esc(tax_id));
        h.push_str("</div>");
    }
    h.push_str("\n        ");
    if let Some(n) = invoice.gst_hst_number.as_deref().filter(|n| gst_applicable && !n.is_empty()) {
        h.push_str("<div class=\"muted\">GST/HST ");
        h.push_str(&esc(n));
        h.push_str("</div>");
    }
    h.push_str("\n      </div>\n      <div>\n        <div class=\"title\">Invoice</div>\n        <div class=\"number\">");
    h.push_str(&esc(&invoice.invoice_number));
    h.push_str("</div>\n      </div>\n    </div>\n    ");
    if is_paid {
        h.push_str("<div class=\"stamp\"><span class=\"stamp-label\">Paid</span>");
        if let Some(d) = payment_date {
            h.push_str("<span class=\"stamp-date\">");
            h.push_str(&esc(&format_day(d)));
            h.push_str("</span>");
        }
        h.push_str("</div>");
    }
    h.push_str("\n\n    <div class=\"meta\">\n      <div>\n        <div class=\"label\">Bill to</div>\n        <div class=\"client-name\">");
    h.push_str(&esc(invoice.client_name.as_deref().unwrap_or("")));
    h.push_str("</div>\n        ");
    h.push_str(&client_lines);
    h.push_str("\n      </div>\n      <div class=\"facts\">\n        <div class=\"fact\"><span class=\"faint\">Issue date</span><span>");
    h.push_str(&format_day(&invoice.issue_date));
    h.push_str("</span></div>\n        <div class=\"fact\"><span class=\"faint\">Due date</span><span>");
    h.push_str(&format_day(&invoice.due_date));
    h.push_str("</span></div>\n        <div class=\"fact due\"><span>");
    h.push_str(if is_paid { "Amount paid" } else { "Amount due" });
    h.push_str("</span><span>");
    h.push_str(&money(invoice.total));
    h.push_str(
        "</span></div>\n      </div>\n    </div>\n\n    <table>\n      <thead>\n        <tr>\n          <th>Description</th>\n          <th class=\"num\">Hours</th>\n          <th class=\"num\">Rate</th>\n          <th class=\"num\">Amount</th>\n        </tr>\n      </thead>\n      <tbody>",
    );
    h.push_str(&rows);
    h.push_str("</tbody>\n    </table>\n\n    <div class=\"totals\">\n      <div class=\"totals-box\">\n        <div class=\"t-row\"><span class=\"faint\">Subtotal</span><span>");
    h.push_str(&money(invoice.subtotal));
    h.push_str("</span></div>\n        ");
    if gst_applicable && gst_rate > 0.0 {
        h.push_str("<div class=\"t-row\"><span class=\"faint\">GST/HST (");
        h.push_str(&js_number_to_string(gst_rate));
        h.push_str("%)</span><span>");
        h.push_str(&money(gst_amount));
        h.push_str("</span></div>");
    }
    h.push_str("\n        ");
    if invoice.tax_rate > 0.0 {
        h.push_str("<div class=\"t-row\"><span class=\"faint\">Tax (");
        h.push_str(&js_number_to_string(invoice.tax_rate));
        h.push_str("%)</span><span>");
        h.push_str(&money(other_tax));
        h.push_str("</span></div>");
    }
    h.push_str("\n        <div class=\"t-row total\"><span>Total ");
    if currency != "USD" {
        h.push_str("<span class=\"faint\" style=\"font-size:11px;font-weight:500\">");
        h.push_str(currency);
        h.push_str("</span>");
    }
    h.push_str("</span><span>");
    h.push_str(&money(invoice.total));
    h.push_str("</span></div>\n        ");
    if let Some(d) = payment_date.filter(|_| is_paid) {
        h.push_str("<div class=\"t-row paid\"><span>Paid ");
        h.push_str(&format_day(d));
        if let Some(m) = payment_method {
            h.push_str(" by ");
            h.push_str(&esc(m));
        }
        h.push_str("</span><span>");
        h.push_str(&money(invoice.total));
        h.push_str("</span></div>");
    }
    h.push_str("\n      </div>\n    </div>\n\n    ");
    if !invoice.notes.is_empty() {
        h.push_str("\n    <div class=\"notes\">\n      <div class=\"label\">Notes</div>\n      <p>");
        h.push_str(&esc(&invoice.notes));
        h.push_str("</p>\n    </div>");
    }
    h.push_str("\n\n    <div class=\"footer\">Thank you for your business.</div>\n  </div>\n</body>\n</html>");
    h
}

const INVOICE_HEAD: &str = r##"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  @page { size: A4; margin: 0; }
  * { margin: 0; padding: 0; box-sizing: border-box; }
  body {
    font-family: -apple-system, BlinkMacSystemFont, "SF Pro Text", "Helvetica Neue", Helvetica, Arial, sans-serif;
    color: #18181b; background: #fff; font-size: 12.5px; line-height: 1.5;
    -webkit-print-color-adjust: exact; print-color-adjust: exact;
    font-variant-numeric: tabular-nums;
  }
  .page { padding: 52px 56px 48px; }
  .top { display: flex; justify-content: space-between; align-items: flex-start; gap: 32px; }
  .biz-name { font-size: 17px; font-weight: 600; letter-spacing: -0.01em; }
  .muted { color: #52525b; }
  .faint { color: #71717a; }
  .title { font-size: 30px; font-weight: 600; letter-spacing: -0.02em; text-align: right; line-height: 1.1; }
  .number { text-align: right; color: #52525b; margin-top: 4px; }
  .page { position: relative; }
  /* A rubber PAID stamp, matching the invoice page in the app */
  .stamp {
    position: absolute; top: 46px; right: 212px;
    display: inline-flex; flex-direction: column; align-items: center;
    padding: 6px 14px 4px; border-radius: 7px;
    border: 2.5px solid #1E7F3C; box-shadow: inset 0 0 0 2px #fff, inset 0 0 0 3px #1E7F3C;
    color: #1E7F3C; opacity: 0.88; transform: rotate(-8deg);
    font-family: "DIN Condensed", "DIN Alternate", "Helvetica Neue", Arial, sans-serif; font-weight: 700;
    text-transform: uppercase;
  }
  .stamp-label { font-size: 32px; line-height: 28px; letter-spacing: 0.1em; padding-left: 0.1em; }
  .stamp-date { font-size: 10.5px; line-height: 12px; letter-spacing: 0.16em; padding-left: 0.16em; margin-top: 3px; }
  .meta { display: flex; justify-content: space-between; gap: 32px; margin-top: 44px; }
  .label { font-size: 10px; font-weight: 600; letter-spacing: 0.08em; text-transform: uppercase; color: #a1a1aa; margin-bottom: 6px; }
  .client-name { font-weight: 600; font-size: 13.5px; }
  .facts { min-width: 230px; }
  .fact { display: flex; justify-content: space-between; gap: 24px; padding: 2px 0; }
  .fact.due { border-top: 1px solid #e4e4e7; margin-top: 6px; padding-top: 8px; font-weight: 600; font-size: 14px; }
  table { width: 100%; border-collapse: collapse; margin-top: 40px; }
  th { font-size: 10px; font-weight: 600; letter-spacing: 0.08em; text-transform: uppercase; color: #71717a; text-align: left; padding: 0 0 8px; border-bottom: 1.5px solid #18181b; }
  th.num, td.num { text-align: right; white-space: nowrap; }
  th.num { width: 90px; }
  td { padding: 10px 0; border-bottom: 1px solid #f0f0f2; vertical-align: top; }
  td.desc { padding-right: 24px; }
  td.num { color: #52525b; }
  td.strong { color: #18181b; font-weight: 500; }
  .totals { display: flex; justify-content: flex-end; margin-top: 20px; }
  .totals-box { width: 270px; }
  .t-row { display: flex; justify-content: space-between; padding: 3px 0; }
  .t-row.total { border-top: 1.5px solid #18181b; margin-top: 8px; padding-top: 10px; font-size: 16px; font-weight: 600; }
  .t-row.paid { color: #15803d; font-size: 11.5px; padding-top: 6px; }
  .notes { margin-top: 44px; padding-top: 16px; border-top: 1px solid #e4e4e7; }
  .notes p { white-space: pre-line; color: #52525b; }
  .footer { margin-top: 36px; color: #a1a1aa; font-size: 11px; }
</style>
</head>
<body>
  <div class="page">
    <div class="top">
      <div>
"##;

// ============ tax-pdf.ts ============

fn category_label(category: &str) -> &str {
    match category {
        "equipment" => "Equipment and hardware",
        "software" => "Software and subscriptions",
        "home_office" => "Home office",
        "phone_internet" => "Phone and internet",
        "travel" => "Travel",
        "meals" => "Meals and entertainment",
        "professional_development" => "Professional development",
        "other" => "Other",
        c => c,
    }
}

/// `generateTaxSummaryHTML(overview, taxSettings, appSettings, taxYear)`;
/// `today` (YYYY-MM-DD) is the "Prepared" date.
pub fn tax_summary_html(overview: &TaxOverview, tax: Option<&TaxSettings>, settings: &Settings, tax_year: i64, today: &str) -> String {
    let t = |f: fn(&TaxSettings) -> &str| tax.map(f).unwrap_or("");
    let currency = [t(|t| &t.currency), settings.default_currency()].into_iter().find(|s| !s.is_empty()).unwrap_or("CAD");
    let business_name = [t(|t| &t.business_name), settings.business_name()].into_iter().find(|s| !s.is_empty()).unwrap_or("Your Business");
    let business_address = [t(|t| &t.business_address), settings.business_address()].into_iter().find(|s| !s.is_empty()).unwrap_or("");
    let province = t(|t| &t.province);
    let gst_number = t(|t| &t.gst_hst_number);
    let bracket = or0(tax.map(|t| t.income_tax_bracket).unwrap_or(0.0));
    let fmt = |n: f64| intl_currency(or0(n), Locale::EnCa, currency);

    let net_income = overview.total_paid - overview.gst_collected_paid - overview.total_expenses;
    let income_tax_estimate = net_income.max(0.0) * (bracket / 100.0);

    let expense_rows = if overview.expenses_by_category.is_empty() {
        format!("<tr><td colspan=\"2\" class=\"faint\">No expenses logged for {tax_year}.</td></tr>")
    } else {
        overview
            .expenses_by_category
            .iter()
            .map(|e| {
                format!(
                    "\n      <tr>\n        <td>{} <span class=\"faint\">{} item{}</span></td>\n        <td class=\"num\">{}</td>\n      </tr>",
                    esc(category_label(&e.category)),
                    e.count,
                    plural(e.count),
                    fmt(e.total)
                )
            })
            .collect()
    };
    let generated = format_day(today);

    let mut h = String::with_capacity(6144);
    h.push_str(TAX_HEAD);
    h.push_str(&tax_year.to_string());
    h.push_str(" tax year</div>\n      </div>\n      <div style=\"text-align: right;\">\n        <div style=\"font-size: 14px; font-weight: 600;\">");
    h.push_str(&esc(business_name));
    h.push_str("</div>\n        ");
    if !business_address.is_empty() {
        h.push_str("<div class=\"muted\" style=\"white-space: pre-line;\">");
        h.push_str(&esc(business_address));
        h.push_str("</div>");
    }
    h.push_str("\n        ");
    if !province.is_empty() {
        h.push_str("<div class=\"muted\">Province: ");
        h.push_str(&esc(province));
        h.push_str("</div>");
    }
    h.push_str("\n        ");
    if !gst_number.is_empty() {
        h.push_str("<div class=\"muted\">GST/HST ");
        h.push_str(&esc(gst_number));
        h.push_str("</div>");
    }
    h.push_str("\n        <div class=\"faint\" style=\"margin-top: 6px; font-size: 11px;\">Prepared ");
    h.push_str(&generated);
    h.push_str("</div>\n      </div>\n    </div>\n\n    <div class=\"section\">\n      <div class=\"label\">Income</div>\n      <div class=\"row\"><span>Invoiced, ");
    h.push_str(&format!("{} invoice{}", overview.invoice_count, plural(overview.invoice_count)));
    h.push_str("</span><span>");
    h.push_str(&fmt(overview.total_invoiced));
    h.push_str("</span></div>\n      <div class=\"row\"><span>Paid, ");
    h.push_str(&format!("{} invoice{}", overview.paid_count, plural(overview.paid_count)));
    h.push_str("</span><span>");
    h.push_str(&fmt(overview.total_paid));
    h.push_str("</span></div>\n      <div class=\"row\"><span>Outstanding</span><span>");
    h.push_str(&fmt(overview.total_outstanding));
    h.push_str("</span></div>\n      <div class=\"row\"><span>GST/HST collected on paid invoices</span><span>");
    h.push_str(&fmt(overview.gst_collected_paid));
    h.push_str("</span></div>\n      <div class=\"row total\"><span>Net business income</span><span>");
    h.push_str(&fmt(net_income));
    h.push_str("</span></div>\n      <div class=\"faint\" style=\"font-size: 11px; margin-top: 4px;\">Paid invoices, less GST/HST collected, less deductible expenses.</div>\n    </div>\n\n    <div class=\"section\">\n      <div class=\"label\">Expenses by category</div>\n      <table>\n        <tbody>");
    h.push_str(&expense_rows);
    h.push_str("</tbody>\n        <tfoot><tr><td>Total deductible</td><td class=\"num\">");
    h.push_str(&fmt(overview.total_expenses));
    h.push_str("</td></tr></tfoot>\n      </table>\n    </div>\n\n    <div class=\"setaside\">\n      <div style=\"font-weight: 600; margin-bottom: 4px;\">Suggested set-aside</div>\n      <div class=\"row\"><span>GST/HST to remit</span><span>");
    h.push_str(&fmt(overview.gst_collected_paid));
    h.push_str("</span></div>\n      <div class=\"row\"><span>Income tax at ");
    h.push_str(&js_number_to_string(bracket));
    h.push_str("%</span><span>");
    h.push_str(&fmt(income_tax_estimate));
    h.push_str("</span></div>\n      <div class=\"row total\" style=\"border-top-color: #18181b;\"><span>Total to set aside</span><span>");
    h.push_str(&fmt(overview.gst_collected_paid + income_tax_estimate));
    h.push_str(TAX_TAIL);
    h
}

const TAX_HEAD: &str = r##"<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <style>
    @page { size: A4; margin: 0; }
    * { margin: 0; padding: 0; box-sizing: border-box; }
    body { font-family: -apple-system, BlinkMacSystemFont, "SF Pro Text", "Helvetica Neue", Helvetica, Arial, sans-serif; color: #18181b; background: #fff; font-size: 12.5px; line-height: 1.5; font-variant-numeric: tabular-nums; -webkit-print-color-adjust: exact; print-color-adjust: exact; }
    .page { padding: 52px 56px 48px; }
    .top { display: flex; justify-content: space-between; align-items: flex-start; gap: 32px; margin-bottom: 36px; }
    .title { font-size: 28px; font-weight: 600; letter-spacing: -0.02em; line-height: 1.1; }
    .muted { color: #52525b; }
    .faint { color: #a1a1aa; }
    .section { margin-bottom: 26px; }
    .label { font-size: 10px; font-weight: 600; letter-spacing: 0.08em; text-transform: uppercase; color: #71717a; padding-bottom: 8px; border-bottom: 1.5px solid #18181b; margin-bottom: 2px; }
    .row { display: flex; justify-content: space-between; padding: 8px 0; border-bottom: 1px solid #f0f0f2; }
    .row span:first-child { color: #52525b; }
    .row.total { border-bottom: none; border-top: 1.5px solid #18181b; margin-top: 4px; padding-top: 10px; font-size: 15px; font-weight: 600; }
    .row.total span:first-child { color: #18181b; }
    table { width: 100%; border-collapse: collapse; }
    td { padding: 8px 0; border-bottom: 1px solid #f0f0f2; }
    td.num { text-align: right; }
    tfoot td { border-bottom: none; border-top: 1.5px solid #18181b; padding-top: 10px; font-weight: 600; font-size: 14px; }
    .setaside { background: #fff8eb; border: 1px solid #f5d9a8; border-radius: 8px; padding: 16px 18px; }
    .setaside .row { border-bottom-color: #f5e6c8; }
    .note { margin-top: 30px; font-size: 11px; color: #71717a; line-height: 1.6; }
  </style>
</head>
<body>
  <div class="page">
    <div class="top">
      <div>
        <div class="title">Tax summary</div>
        <div class="muted" style="margin-top: 6px;">For the "##;

const TAX_TAIL: &str = r##"</span></div>
    </div>

    <p class="note">
      Prepared from the records kept in Billable to help with bookkeeping. These are estimates, not tax, legal, or financial advice. Check with an accountant before filing or remitting.
    </p>
  </div>
</body>
</html>"##;

// ============ commission-pdf.ts ============

/// `effective(c)`: the override, else 0 while it needs review, else the calculated amount.
fn effective(c: &Commission) -> f64 {
    if let Some(v) = c.manual_override {
        return v;
    }
    if c.needs_review != 0 {
        return 0.0;
    }
    or0(c.calculated_commission)
}

fn usd(n: f64) -> String {
    intl_currency(or0(n), Locale::EnUs, "USD")
}

/// `jobs.reduce((s, j) => s + effective(j), 0)`
fn sum_effective<'a>(jobs: impl IntoIterator<Item = &'a Commission>) -> f64 {
    jobs.into_iter().fold(0.0, |s, j| s + effective(j))
}

fn line_rows(jobs: &[&Commission]) -> String {
    jobs.iter()
        .map(|j| {
            let is_solar = j.job_type == "solar";
            let detail = if is_solar {
                format!("{} kW", js_number_to_string(j.system_size_kw.unwrap_or(0.0)))
            } else {
                usd(j.contract_amount.unwrap_or(0.0))
            };
            // `null >= 30000` is false and `null <= 20000` is true in JS
            let amount = j.contract_amount.unwrap_or(0.0);
            let rule = if is_solar {
                "$50 per kW"
            } else if j.manual_override.is_some() {
                "Set by hand"
            } else if amount >= 30000.0 {
                "$30,000 or more"
            } else if amount <= 20000.0 {
                "$20,000 or less"
            } else {
                "Needs review"
            };
            format!(
                "\n      <tr>\n        <td>{}</td>\n        <td class=\"muted\">{}</td>\n        <td class=\"num\">{}</td>\n        <td class=\"muted\">{}</td>\n        <td class=\"num strong\">{}</td>\n      </tr>",
                esc(&j.client_name),
                format_day(j.appointment_date.as_deref().unwrap_or("")),
                detail,
                rule,
                usd(effective(j))
            )
        })
        .collect()
}

fn group_section(title: &str, jobs: &[&Commission], dot: &str) -> String {
    if jobs.is_empty() {
        return String::new();
    }
    let subtotal = sum_effective(jobs.iter().copied());
    format!(
        "\n    <div class=\"group\">\n      <div class=\"group-title\"><span class=\"dot\" style=\"background:{dot}\"></span>{title}<span class=\"faint\">· {} job{}</span></div>\n      <table>\n        <thead>\n          <tr>\n            <th>Client</th>\n            <th>Appointment</th>\n            <th class=\"num\">{}</th>\n            <th>Rule</th>\n            <th class=\"num\">Commission</th>\n          </tr>\n        </thead>\n        <tbody>{}</tbody>\n        <tfoot>\n          <tr>\n            <td colspan=\"4\" class=\"sub-label\">{title} subtotal</td>\n            <td class=\"num strong\">{}</td>\n          </tr>\n        </tfoot>\n      </table>\n    </div>",
        jobs.len(),
        plural(jobs.len() as i64),
        if title == "Solar" { "System size" } else { "Contract" },
        line_rows(jobs),
        usd(subtotal)
    )
}

/// `buildHTML(invoice, settings)`; `today` (YYYY-MM-DD) is the "Issued" date.
pub fn commission_invoice_html(invoice: &CommissionInvoiceDetail, settings: &Settings, today: &str) -> String {
    let jobs = &invoice.jobs;
    let solar: Vec<&Commission> = jobs.iter().filter(|j| j.job_type == "solar").collect();
    let roofing: Vec<&Commission> = jobs.iter().filter(|j| j.job_type == "roofing").collect();
    let total = sum_effective(jobs.iter());
    let payee = if settings.business_name().is_empty() { "Commission payee" } else { settings.business_name() };
    let from = invoice.date_from.as_deref().filter(|s| !s.is_empty());
    let to = invoice.date_to.as_deref().filter(|s| !s.is_empty());
    let range = if from.is_some() || to.is_some() {
        format!(
            "{} to {}",
            from.map(format_day).unwrap_or_else(|| "Start".into()),
            to.map(format_day).unwrap_or_else(|| "today".into())
        )
    } else {
        "All dates".to_string()
    };
    let cat_label = if invoice.category == "mixed" {
        "Solar and roofing".to_string()
    } else {
        let mut c = invoice.category.chars();
        match c.next() {
            Some(first) => first.to_uppercase().collect::<String>() + c.as_str(),
            None => String::new(),
        }
    };

    let mut h = String::with_capacity(8192);
    h.push_str(COMMISSION_HEAD);
    h.push_str(&esc(payee));
    h.push_str("</strong></div>\n          ");
    if !settings.business_email().is_empty() {
        h.push_str("<div class=\"muted\">");
        h.push_str(&esc(settings.business_email()));
        h.push_str("</div>");
    }
    h.push_str("\n        </div>\n        <div class=\"facts\">\n          <div class=\"fact\"><span>Invoice</span><span>");
    h.push_str(&esc(&invoice.invoice_number));
    h.push_str("</span></div>\n          <div class=\"fact\"><span>Jobs</span><span>");
    h.push_str(&cat_label);
    h.push_str("</span></div>\n          <div class=\"fact\"><span>Period</span><span>");
    h.push_str(&esc(&range));
    h.push_str("</span></div>\n          <div class=\"fact\"><span>Issued</span><span>");
    h.push_str(&format_day(today));
    h.push_str("</span></div>\n        </div>\n      </div>\n\n      ");
    h.push_str(&group_section("Solar", &solar, "#F5A623"));
    h.push_str("\n      ");
    h.push_str(&group_section("Roofing", &roofing, "#3E8BF7"));
    h.push_str("\n\n      <div class=\"total\">\n        <div class=\"total-box\"><span>Total owed</span><span>");
    h.push_str(&usd(total));
    h.push_str("</span></div>\n      </div>\n\n      ");
    if !invoice.notes.is_empty() {
        h.push_str("<div class=\"notes\"><div class=\"label\">Notes</div><p class=\"muted\" style=\"white-space:pre-line\">");
        h.push_str(&esc(&invoice.notes));
        h.push_str("</p></div>");
    }
    h.push_str("\n\n      <div class=\"foot\">Solar pays $50 per kW. Roofing pays $250 for contracts of $20,000 or less and $500 for $30,000 or more.</div>\n    </div>\n  </body></html>");
    h
}

const COMMISSION_HEAD: &str = r##"<!DOCTYPE html><html><head><meta charset="utf-8"><style>
    @page { size: A4; margin: 0; }
    * { margin: 0; padding: 0; box-sizing: border-box; }
    body { font-family: -apple-system, BlinkMacSystemFont, "SF Pro Text", "Helvetica Neue", Helvetica, Arial, sans-serif; color: #18181b; background: #fff; font-size: 12.5px; line-height: 1.5; font-variant-numeric: tabular-nums; -webkit-print-color-adjust: exact; print-color-adjust: exact; }
    .page { padding: 52px 56px 48px; }
    .top { display: flex; justify-content: space-between; align-items: flex-start; gap: 32px; margin-bottom: 40px; }
    .title { font-size: 28px; font-weight: 600; letter-spacing: -0.02em; line-height: 1.1; }
    .muted { color: #52525b; }
    .faint { color: #a1a1aa; font-weight: 400; margin-left: 6px; }
    .facts { min-width: 230px; }
    .fact { display: flex; justify-content: space-between; gap: 24px; padding: 2px 0; }
    .fact span:first-child { color: #71717a; }
    .group { margin-bottom: 28px; }
    .group-title { display: flex; align-items: center; gap: 8px; font-weight: 600; font-size: 13px; margin-bottom: 8px; }
    .dot { width: 8px; height: 8px; border-radius: 50%; }
    table { width: 100%; border-collapse: collapse; }
    th { font-size: 10px; font-weight: 600; letter-spacing: 0.08em; text-transform: uppercase; color: #71717a; text-align: left; padding: 0 0 8px; border-bottom: 1.5px solid #18181b; }
    td { padding: 9px 0; border-bottom: 1px solid #f0f0f2; }
    th.num, td.num { text-align: right; white-space: nowrap; }
    td.num { color: #52525b; }
    td.strong { color: #18181b; font-weight: 500; }
    th:not(:last-child), td:not(:last-child) { padding-right: 22px; }
    tfoot td { border-bottom: none; padding-top: 10px; }
    .sub-label { text-align: right; color: #71717a; padding-right: 24px; }
    .total { display: flex; justify-content: flex-end; margin-top: 8px; }
    .total-box { width: 270px; border-top: 1.5px solid #18181b; padding-top: 10px; display: flex; justify-content: space-between; font-size: 16px; font-weight: 600; }
    .notes { margin-top: 40px; padding-top: 16px; border-top: 1px solid #e4e4e7; }
    .label { font-size: 10px; font-weight: 600; letter-spacing: 0.08em; text-transform: uppercase; color: #a1a1aa; margin-bottom: 6px; }
    .foot { margin-top: 44px; font-size: 11px; color: #a1a1aa; }
  </style></head><body>
    <div class="page">
      <div class="top">
        <div>
          <div class="title">Commission invoice</div>
          <div class="muted" style="margin-top: 6px;">Payable to <strong style="color:#18181b">"##;

#[cfg(test)]
mod tests;
