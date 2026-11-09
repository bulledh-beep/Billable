//! The shapes the database returns and accepts.
//!
//! Output types serialize to exactly the JSON the Electron app's IPC returned:
//! the table's columns (snake_case) plus joined and derived fields such as
//! `client_name`, `billing_state` or `days_past_due`. Every row type keeps
//! columns it doesn't model (say, from a newer app version) in `extra`, which
//! serializes flat alongside the known fields.
//!
//! Input types mirror the `data` objects the renderer sent. For partial
//! updates (`*Patch`), `None` leaves a field alone; for nullable columns
//! `Some(None)` writes `NULL`. They deserialize from the same JSON the
//! renderer sent, so the field names are the column names.

use std::collections::BTreeMap;
use std::ops::{Deref, DerefMut};

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::db::{FromCol, RowMap};

/// Declares a row struct whose fields are read by column name.
macro_rules! row_struct {
    (
        $(#[$meta:meta])*
        pub struct $name:ident {
            $( $(#[$fmeta:meta])* pub $field:ident : $ty:ty, )*
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Serialize)]
        pub struct $name {
            $( $(#[$fmeta])* pub $field: $ty, )*
            /// Columns this port doesn't model, kept as-is (serialized flat).
            #[serde(flatten)]
            pub extra: BTreeMap<String, Value>,
        }

        impl $name {
            #[allow(dead_code)]
            pub(crate) const FIELDS: &'static [&'static str] = &[$(stringify!($field)),*];

            #[allow(dead_code)]
            pub(crate) fn from_row(row: &RowMap, also_known: &[&str]) -> Self {
                Self {
                    $( $field: FromCol::from_col(row.get(stringify!($field))), )*
                    extra: row.extra(Self::FIELDS, also_known),
                }
            }
        }
    };
}

/// Implements `Deref` to the flattened base row so `entry.start_time` works.
macro_rules! deref_to {
    ($outer:ty, $field:ident, $target:ty) => {
        impl Deref for $outer {
            type Target = $target;
            fn deref(&self) -> &$target {
                &self.$field
            }
        }
        impl DerefMut for $outer {
            fn deref_mut(&mut self) -> &mut $target {
                &mut self.$field
            }
        }
    };
}

// ============ Clients ============

row_struct! {
    /// A client with its money totals (`clients:list` / `clients:get`).
    pub struct Client {
        pub id: i64,
        pub name: String,
        pub company: String,
        pub email: String,
        pub address: String,
        pub default_rate: f64,
        pub currency: String,
        pub created_at: String,
        pub project_count: i64,
        pub active_project_count: i64,
        pub unbilled_amount: f64,
        pub unbilled_hours: f64,
        pub oldest_unbilled: Option<String>,
        pub outstanding_amount: f64,
        pub overdue_amount: f64,
        pub paid_amount: f64,
        pub invoice_count: i64,
        pub last_activity: Option<String>,
    }
}

/// `updateClient` result: the client plus how many projects took the new rate.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ClientUpdate {
    #[serde(flatten)]
    pub client: Client,
    pub cascaded_projects: i64,
}
deref_to!(ClientUpdate, client, Client);

/// `mergeClients` result.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MergeResult {
    pub moved_projects: i64,
    pub moved_invoices: i64,
}

/// New client (`createClient`). Missing fields get the app's defaults.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ClientInput {
    pub name: String,
    pub company: Option<String>,
    pub email: Option<String>,
    pub address: Option<String>,
    pub default_rate: Option<f64>,
    /// Defaults to `CAD`.
    pub currency: Option<String>,
}

/// Client edit (`updateClient`). A new `default_rate` cascades to projects
/// still on the old rate.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ClientPatch {
    pub name: Option<String>,
    pub company: Option<String>,
    pub email: Option<String>,
    pub address: Option<String>,
    pub default_rate: Option<f64>,
    pub currency: Option<String>,
}

// ============ Projects ============

row_struct! {
    /// A project with hours and money (`projects:list` / `projects:get`).
    pub struct Project {
        pub id: i64,
        pub client_id: i64,
        pub name: String,
        pub description: String,
        pub rate: f64,
        /// `active` | `paused` | `complete` | `archived`
        pub status: String,
        pub color: String,
        pub created_at: String,
        pub client_name: Option<String>,
        pub client_company: Option<String>,
        pub total_hours: f64,
        pub unbilled_hours: f64,
        pub unbilled_entries: i64,
        pub oldest_unbilled: Option<String>,
        /// Sum of this project's invoice lines, before tax.
        pub invoiced_amount: f64,
        /// Invoice lines on paid invoices, before tax.
        pub paid_amount: f64,
        pub last_activity: Option<String>,
        /// Unbilled hours at the current rate, rounded to cents.
        pub unbilled_amount: f64,
        /// Same as `invoiced_amount` (kept for older callers).
        pub billed_total: f64,
    }
}

/// New project (`createProject`).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ProjectInput {
    pub client_id: i64,
    pub name: String,
    pub description: Option<String>,
    pub rate: Option<f64>,
    /// Defaults to `active`.
    pub status: Option<String>,
    /// Defaults to `#F5A623`.
    pub color: Option<String>,
}

/// Project edit (`updateProject`).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ProjectPatch {
    pub client_id: Option<i64>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub rate: Option<f64>,
    pub status: Option<String>,
    pub color: Option<String>,
}

// ============ Time entries ============

row_struct! {
    /// A `time_entries` row.
    pub struct TimeEntry {
        pub id: i64,
        pub project_id: i64,
        pub description: String,
        /// ISO instant (`2026-11-08T21:09:55.123Z`).
        pub start_time: String,
        pub end_time: Option<String>,
        pub duration_minutes: f64,
        pub paused_at: Option<String>,
        pub active_since: Option<String>,
        pub is_billable: i64,
        pub is_invoiced: i64,
        pub invoice_id: Option<i64>,
        pub created_at: String,
    }
}

/// A time entry with its project, client, invoice and billing state
/// (`time:list`, `time:get`, timer start/stop/pause/resume results).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EntryRow {
    #[serde(flatten)]
    pub entry: TimeEntry,
    pub project_name: Option<String>,
    pub project_color: Option<String>,
    pub rate: Option<f64>,
    pub client_id: Option<i64>,
    pub project_status: Option<String>,
    pub client_name: Option<String>,
    pub invoice_number: Option<String>,
    pub invoice_stored_status: Option<String>,
    pub invoice_due_date: Option<String>,
    /// `unbilled` | `nonbillable` | `draft` | `sent` | `overdue` | `paid` |
    /// `invoiced` (flagged by an older version with no invoice link).
    pub billing_state: String,
    /// The invoice's shown status when linked, else `None`.
    pub invoice_status: Option<String>,
    /// `Some(true)` on a stop result when a sub-minute timer was thrown away.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discarded: Option<bool>,
}
deref_to!(EntryRow, entry, TimeEntry);

impl EntryRow {
    pub(crate) const JOINED: &'static [&'static str] = &[
        "project_name",
        "project_color",
        "rate",
        "client_id",
        "project_status",
        "client_name",
        "invoice_number",
        "invoice_stored_status",
        "invoice_due_date",
    ];

    pub(crate) fn from_row(r: &RowMap, today: &str) -> Self {
        let entry = TimeEntry::from_row(r, Self::JOINED);
        let invoice_number: Option<String> = r.col("invoice_number");
        let invoice_stored_status: Option<String> = r.col("invoice_stored_status");
        let invoice_due_date: Option<String> = r.col("invoice_due_date");
        let linked = entry.invoice_id.is_some_and(|i| i != 0);
        let state = if linked && invoice_number.as_deref().is_some_and(|n| !n.is_empty()) {
            crate::time::effective_invoice_status(invoice_stored_status.as_deref(), invoice_due_date.as_deref(), today)
        } else if entry.is_invoiced != 0 {
            "invoiced"
        } else if entry.is_billable == 0 {
            "nonbillable"
        } else {
            "unbilled"
        };
        EntryRow {
            project_name: r.col("project_name"),
            project_color: r.col("project_color"),
            rate: r.col("rate"),
            client_id: r.col("client_id"),
            project_status: r.col("project_status"),
            client_name: r.col("client_name"),
            invoice_number,
            invoice_stored_status,
            invoice_due_date,
            billing_state: state.to_string(),
            invoice_status: linked.then(|| state.to_string()),
            discarded: None,
            entry,
        }
    }
}

/// The running or paused timer as read at launch (`getActiveTimer` /
/// `getPausedTimer`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TimerRow {
    #[serde(flatten)]
    pub entry: TimeEntry,
    pub project_name: Option<String>,
    pub project_color: Option<String>,
    pub rate: Option<f64>,
    pub client_name: Option<String>,
}
deref_to!(TimerRow, entry, TimeEntry);

impl TimerRow {
    pub(crate) fn from_row(r: &RowMap) -> Self {
        TimerRow {
            entry: TimeEntry::from_row(r, &["project_name", "project_color", "rate", "client_name"]),
            project_name: r.col("project_name"),
            project_color: r.col("project_color"),
            rate: r.col("rate"),
            client_name: r.col("client_name"),
        }
    }
}

/// A time entry linked to an invoice (`Invoice::entries`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InvoiceEntry {
    #[serde(flatten)]
    pub entry: TimeEntry,
    pub project_name: Option<String>,
    pub project_color: Option<String>,
    pub rate: Option<f64>,
}
deref_to!(InvoiceEntry, entry, TimeEntry);

impl InvoiceEntry {
    pub(crate) fn from_row(r: &RowMap) -> Self {
        InvoiceEntry {
            entry: TimeEntry::from_row(r, &["project_name", "project_color", "rate"]),
            project_name: r.col("project_name"),
            project_color: r.col("project_color"),
            rate: r.col("rate"),
        }
    }
}

/// An unbilled entry with its project (`time:unbilled*`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UnbilledEntry {
    #[serde(flatten)]
    pub entry: TimeEntry,
    pub project_name: String,
    pub rate: Option<f64>,
}
deref_to!(UnbilledEntry, entry, TimeEntry);

impl UnbilledEntry {
    pub(crate) fn from_row(r: &RowMap) -> Self {
        UnbilledEntry {
            entry: TimeEntry::from_row(r, &["project_name", "rate"]),
            project_name: r.col("project_name"),
            rate: r.col("rate"),
        }
    }
}

/// `{ active, paused }` (`time:state`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TimerState {
    pub active: Option<EntryRow>,
    pub paused: Option<EntryRow>,
}

/// New manual time entry (`createTimeEntry`).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct TimeEntryInput {
    pub project_id: i64,
    pub description: Option<String>,
    pub start_time: String,
    pub end_time: Option<String>,
    pub duration_minutes: Option<f64>,
    /// Defaults to 1.
    pub is_billable: Option<i64>,
}

/// Entry edit (`updateTimeEntry`). Billing links are never written here.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct TimeEntryPatch {
    pub project_id: Option<i64>,
    pub description: Option<String>,
    pub start_time: Option<String>,
    #[serde(deserialize_with = "double_option")]
    pub end_time: Option<Option<String>>,
    pub duration_minutes: Option<f64>,
    pub is_billable: Option<i64>,
}

// ============ Invoices ============

row_struct! {
    /// An `invoices` row.
    pub struct InvoiceRow {
        pub id: i64,
        pub project_id: Option<i64>,
        pub client_id: i64,
        pub invoice_number: String,
        pub issue_date: String,
        pub due_date: String,
        /// On [`Invoice`] this is the shown status (`draft` | `sent` |
        /// `overdue` | `paid`); on [`YearInvoice`] it's the stored one.
        pub status: String,
        pub subtotal: f64,
        pub tax_rate: f64,
        pub total: f64,
        pub notes: String,
        pub pdf_path: Option<String>,
        pub created_at: String,
        pub tax_year: Option<i64>,
        pub payment_date: Option<String>,
        pub payment_method: Option<String>,
        pub currency: String,
        pub gst_hst_applicable: i64,
        pub gst_hst_number: Option<String>,
        pub gst_hst_rate: f64,
        pub gst_hst_amount: f64,
        pub sent_at: Option<String>,
        /// `entry` | `project` | `day` | `manual`
        pub line_style: Option<String>,
    }
}

/// An invoice as the app shows it (`invoices:list`, `invoices:get`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Invoice {
    #[serde(flatten)]
    pub invoice: InvoiceRow,
    pub client_name: Option<String>,
    pub client_company: Option<String>,
    pub client_email: Option<String>,
    pub client_address: Option<String>,
    pub project_name: Option<String>,
    pub entry_count: i64,
    pub entry_hours: f64,
    pub project_count: i64,
    /// The stored status (`overdue` is derived, so this is never it for new data).
    pub stored_status: Option<String>,
    /// Days since the due date (negative before it). `None` if the due date is unreadable.
    pub days_past_due: Option<i64>,
    /// Lines, on `get_invoice` only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<InvoiceItem>>,
    /// Linked time, on `get_invoice` only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entries: Option<Vec<InvoiceEntry>>,
}
deref_to!(Invoice, invoice, InvoiceRow);

impl Invoice {
    const JOINED: &'static [&'static str] = &[
        "client_name",
        "client_company",
        "client_email",
        "client_address",
        "project_name",
        "entry_count",
        "entry_hours",
        "project_count",
    ];

    /// database.ts `withInvoiceStatus`.
    pub(crate) fn from_row(r: &RowMap, today: &str) -> Self {
        let mut invoice = InvoiceRow::from_row(r, Self::JOINED);
        let stored: Option<String> = r.col("status");
        let due: Option<String> = r.col("due_date");
        let status = crate::time::effective_invoice_status(stored.as_deref(), due.as_deref(), today);
        let days_past_due = match due.as_deref() {
            Some(d) if !d.is_empty() => crate::time::days_between(crate::js::js_slice(d, 10), today),
            _ => Some(0),
        };
        invoice.status = status.to_string();
        Invoice {
            invoice,
            client_name: r.col("client_name"),
            client_company: r.col("client_company"),
            client_email: r.col("client_email"),
            client_address: r.col("client_address"),
            project_name: r.col("project_name"),
            entry_count: r.col("entry_count"),
            entry_hours: r.col("entry_hours"),
            project_count: r.col("project_count"),
            stored_status: stored,
            days_past_due,
            items: None,
            entries: None,
        }
    }
}

/// An invoice row with its client (`listInvoicesByYear`, used for CSV/PDF).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct YearInvoice {
    #[serde(flatten)]
    pub invoice: InvoiceRow,
    pub client_name: Option<String>,
    pub client_company: Option<String>,
}
deref_to!(YearInvoice, invoice, InvoiceRow);

row_struct! {
    /// An `invoice_items` row.
    pub struct InvoiceItem {
        pub id: i64,
        pub invoice_id: i64,
        pub description: String,
        pub quantity: f64,
        pub unit_price: f64,
        pub total: f64,
        pub project_id: Option<i64>,
        /// `entry:ID`, `project:ID` or `day:YYYY-MM-DD:PROJECT` for lines built from time.
        pub source_key: Option<String>,
        pub custom_description: i64,
    }
}

/// `deleteInvoice` result: the time that went back to unbilled.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DeleteInvoiceResult {
    pub released_entries: i64,
    pub released_hours: f64,
}

/// An invoice line to save.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct InvoiceItemInput {
    pub description: Option<String>,
    pub quantity: Option<f64>,
    pub unit_price: Option<f64>,
    pub total: Option<f64>,
    pub project_id: Option<i64>,
    pub source_key: Option<String>,
    #[serde(deserialize_with = "truthy")]
    pub custom_description: bool,
}

/// New invoice (`createInvoice`). The number comes from settings.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct InvoiceInput {
    pub client_id: i64,
    pub project_id: Option<i64>,
    pub issue_date: String,
    pub due_date: String,
    /// `sent` or `paid`; anything else makes a draft.
    pub status: Option<String>,
    pub subtotal: Option<f64>,
    pub tax_rate: Option<f64>,
    pub total: Option<f64>,
    pub notes: Option<String>,
    /// Defaults to the issue date's year.
    pub tax_year: Option<i64>,
    /// Defaults to `CAD`.
    pub currency: Option<String>,
    #[serde(deserialize_with = "truthy")]
    pub gst_hst_applicable: bool,
    pub gst_hst_number: Option<String>,
    pub gst_hst_rate: Option<f64>,
    pub gst_hst_amount: Option<f64>,
    pub line_style: Option<String>,
    pub payment_date: Option<String>,
    pub payment_method: Option<String>,
    pub items: Vec<InvoiceItemInput>,
    /// Time entries to link; they must be unbilled.
    pub entry_ids: Option<Vec<i64>>,
}

/// Invoice edit (`updateInvoice`). `items`/`entry_ids`, when given, replace
/// the invoice's lines/linked time.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct InvoicePatch {
    #[serde(deserialize_with = "double_option")]
    pub project_id: Option<Option<i64>>,
    pub client_id: Option<i64>,
    pub issue_date: Option<String>,
    pub due_date: Option<String>,
    pub status: Option<String>,
    pub subtotal: Option<f64>,
    pub tax_rate: Option<f64>,
    pub total: Option<f64>,
    pub notes: Option<String>,
    #[serde(deserialize_with = "double_option")]
    pub pdf_path: Option<Option<String>>,
    #[serde(deserialize_with = "double_option")]
    pub tax_year: Option<Option<i64>>,
    #[serde(deserialize_with = "double_option")]
    pub payment_date: Option<Option<String>>,
    #[serde(deserialize_with = "double_option")]
    pub payment_method: Option<Option<String>>,
    pub currency: Option<String>,
    pub gst_hst_applicable: Option<i64>,
    #[serde(deserialize_with = "double_option")]
    pub gst_hst_number: Option<Option<String>>,
    pub gst_hst_rate: Option<f64>,
    pub gst_hst_amount: Option<f64>,
    #[serde(deserialize_with = "double_option")]
    pub sent_at: Option<Option<String>>,
    #[serde(deserialize_with = "double_option")]
    pub line_style: Option<Option<String>>,
    pub items: Option<Vec<InvoiceItemInput>>,
    pub entry_ids: Option<Vec<i64>>,
}

// ============ Settings ============

/// All settings rows (`settings:get`). `default_rate` and
/// `invoice_next_number` are numbers (`parseFloat(value) || 0`); everything
/// else is the stored text (or `None` for a NULL value).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Settings {
    /// Key/value pairs in table order.
    pub rows: Vec<(String, Option<String>)>,
}

impl Settings {
    /// Raw stored text for `key`.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.rows.iter().find(|(k, _)| k == key).and_then(|(_, v)| v.as_deref())
    }

    /// Whether `key` exists (even with a NULL value).
    pub fn contains(&self, key: &str) -> bool {
        self.rows.iter().any(|(k, _)| k == key)
    }

    /// Stored text for `key`, or `""`.
    pub fn text(&self, key: &str) -> &str {
        self.get(key).unwrap_or("")
    }

    /// `parseFloat(value) || 0`, as `getSettings` does for numeric keys.
    pub fn number(&self, key: &str) -> f64 {
        let v = crate::js::js_parse_float(self.text(key));
        if v.is_nan() {
            0.0
        } else {
            v
        }
    }

    pub fn default_rate(&self) -> f64 {
        self.number("default_rate")
    }
    pub fn invoice_next_number(&self) -> f64 {
        self.number("invoice_next_number")
    }
    pub fn business_name(&self) -> &str {
        self.text("business_name")
    }
    pub fn business_email(&self) -> &str {
        self.text("business_email")
    }
    pub fn business_address(&self) -> &str {
        self.text("business_address")
    }
    pub fn business_logo(&self) -> &str {
        self.text("business_logo")
    }
    pub fn tax_id(&self) -> &str {
        self.text("tax_id")
    }
    pub fn default_currency(&self) -> &str {
        self.text("default_currency")
    }
    pub fn invoice_prefix(&self) -> &str {
        self.text("invoice_prefix")
    }
    /// `none` | `6` | `15` | `30`
    pub fn time_rounding(&self) -> &str {
        self.text("time_rounding")
    }
    /// `dark` | `light` (and `auto` in newer builds)
    pub fn theme(&self) -> &str {
        self.text("theme")
    }
    pub fn default_payment_method(&self) -> &str {
        self.text("default_payment_method")
    }
    /// `payment_methods` parsed from its JSON text (empty if unreadable).
    pub fn payment_methods(&self) -> Vec<PaymentMethod> {
        serde_json::from_str(self.text("payment_methods")).unwrap_or_default()
    }

    /// The object `getSettings` returned.
    pub fn to_json(&self) -> serde_json::Map<String, Value> {
        let mut out = serde_json::Map::new();
        for (k, v) in &self.rows {
            let value = if k == "default_rate" || k == "invoice_next_number" {
                let n = crate::js::js_parse_float(v.as_deref().unwrap_or("null"));
                Value::from(if n.is_nan() { 0.0 } else { n })
            } else {
                v.clone().map(Value::String).unwrap_or(Value::Null)
            };
            out.insert(k.clone(), value);
        }
        out
    }
}

impl Serialize for Settings {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.to_json().serialize(s)
    }
}

/// One entry of the `payment_methods` setting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PaymentMethod {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub email: String,
}

// ============ Dashboard & reports ============

/// `dashboard:stats`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DashboardStats {
    pub hours_today: f64,
    pub hours_this_week: f64,
    pub hours_this_month: f64,
    pub unbilled_hours: f64,
    pub unbilled_amount: f64,
    pub outstanding_total: f64,
    pub overdue_total: f64,
    pub paid_total: f64,
    pub paid_ytd: f64,
}

row_struct! {
    /// `reports:hours-by-project` row.
    pub struct ProjectHours {
        pub id: i64,
        pub name: String,
        pub color: String,
        pub client_name: Option<String>,
        pub hours: f64,
        /// Billable hours × rate.
        pub value: f64,
    }
}

row_struct! {
    /// `reports:hours-by-client` row.
    pub struct ClientHours {
        pub id: i64,
        pub name: String,
        pub hours: f64,
        pub value: f64,
    }
}

row_struct! {
    /// `reports:earnings-by-month` row: invoiced by issue month, paid by payment month.
    pub struct MonthEarnings {
        /// `YYYY-MM`
        pub month: String,
        pub paid: f64,
        pub invoiced: f64,
    }
}

/// `getUnbilledTotals`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UnbilledTotals {
    pub hours: f64,
    pub amount: f64,
}

row_struct! {
    /// `getRecentProjects` row (the tray's quick-start list).
    pub struct RecentProject {
        pub id: i64,
        pub name: String,
        pub client_id: i64,
        pub color: String,
        pub status: String,
        pub rate: f64,
        pub client_name: Option<String>,
        pub last_used: String,
    }
}

// ============ Billing overview ============

/// `billing:overview`: money in each stage, unbilled work by client, and
/// things to act on.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BillingOverview {
    pub today: String,
    pub pipeline: BillingPipeline,
    pub ready_to_bill: Vec<UnbilledClient>,
    pub attention: Vec<AttentionItem>,
    pub invoices: Vec<Invoice>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BillingPipeline {
    pub unbilled_amount: f64,
    pub unbilled_hours: f64,
    pub unbilled_entries: i64,
    pub unbilled_projects: i64,
    pub draft_amount: f64,
    pub draft_count: i64,
    pub awaiting_amount: f64,
    pub awaiting_count: i64,
    pub overdue_amount: f64,
    pub overdue_count: i64,
    pub paid_ytd_amount: f64,
    pub paid_ytd_count: i64,
}

row_struct! {
    /// Unbilled work on one project.
    pub struct UnbilledProject {
        pub project_id: i64,
        pub project_name: String,
        pub project_color: String,
        pub project_status: String,
        pub rate: f64,
        pub client_id: i64,
        pub client_name: String,
        pub entry_count: i64,
        pub hours: f64,
        pub amount: f64,
        pub oldest: String,
        pub newest: String,
        /// Local day of `oldest`.
        pub oldest_day: String,
        pub age_days: Option<i64>,
    }
}

/// Unbilled work for one client.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UnbilledClient {
    pub client_id: i64,
    pub client_name: String,
    pub amount: f64,
    pub hours: f64,
    pub entry_count: i64,
    pub oldest_day: String,
    pub projects: Vec<UnbilledProject>,
}

/// Something on the Billing page's "Needs attention" list.
#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct AttentionItem {
    /// Stable key, also what `dismiss_attention` takes (`overdue:12`, `dup:3-5`…).
    pub key: String,
    /// `overdue` | `unbilled` | `draft` | `accidental_timer` | `duplicate_clients`
    pub kind: String,
    /// `danger` | `warning` | `neutral`
    pub tone: String,
    pub title: String,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invoice_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oldest_day: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_ids: Option<Vec<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_names: Option<Vec<String>>,
}

// ============ Tax ============

/// `tax:get-overview`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TaxOverview {
    pub tax_year: i64,
    pub total_invoiced: f64,
    pub total_paid: f64,
    pub total_outstanding: f64,
    pub gst_collected_paid: f64,
    pub gst_collected_total: f64,
    pub invoice_count: i64,
    pub paid_count: i64,
    pub expenses_by_category: Vec<ExpenseBucket>,
    pub total_expenses: f64,
    pub monthly_income: Vec<MonthlyIncome>,
}

row_struct! {
    /// Expenses in one category.
    pub struct ExpenseBucket {
        pub category: String,
        pub total: f64,
        pub count: i64,
    }
}

row_struct! {
    /// Income for one month (`YYYY-MM`) of a tax year.
    pub struct MonthlyIncome {
        pub month: Option<String>,
        pub paid: f64,
        pub invoiced: f64,
    }
}

row_struct! {
    /// The single `tax_settings` row.
    pub struct TaxSettings {
        pub id: i64,
        pub business_name: String,
        pub business_address: String,
        pub gst_hst_number: String,
        pub gst_hst_registered: i64,
        /// Two-letter province code or `""`.
        pub province: String,
        /// `MM-DD`
        pub fiscal_year_start: String,
        pub default_tax_rate: f64,
        pub income_tax_bracket: f64,
        /// `CAD` | `USD`
        pub currency: String,
        pub updated_at: String,
    }
}

/// Tax settings edit (`saveTaxSettings`); merged over the current row.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct TaxSettingsPatch {
    #[serde(deserialize_with = "double_option")]
    pub business_name: Option<Option<String>>,
    #[serde(deserialize_with = "double_option")]
    pub business_address: Option<Option<String>>,
    #[serde(deserialize_with = "double_option")]
    pub gst_hst_number: Option<Option<String>>,
    #[serde(deserialize_with = "double_option")]
    pub gst_hst_registered: Option<Option<i64>>,
    #[serde(deserialize_with = "double_option")]
    pub province: Option<Option<String>>,
    #[serde(deserialize_with = "double_option")]
    pub fiscal_year_start: Option<Option<String>>,
    #[serde(deserialize_with = "double_option")]
    pub default_tax_rate: Option<Option<f64>>,
    #[serde(deserialize_with = "double_option")]
    pub income_tax_bracket: Option<Option<f64>>,
    #[serde(deserialize_with = "double_option")]
    pub currency: Option<Option<String>>,
}

row_struct! {
    /// An `expenses` row.
    pub struct Expense {
        pub id: i64,
        /// `YYYY-MM-DD`
        pub date: String,
        /// `equipment` | `software` | `home_office` | `phone_internet` |
        /// `travel` | `meals` | `professional_development` | `other`
        pub category: String,
        pub description: String,
        pub amount: f64,
        pub tax_year: i64,
        pub receipt_note: String,
        pub receipt_id: Option<i64>,
        pub created_at: String,
    }
}

/// New expense (`createExpense`). Date defaults to today, tax year to the date's year.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ExpenseInput {
    pub date: Option<String>,
    pub category: Option<String>,
    pub description: Option<String>,
    pub amount: Option<f64>,
    pub tax_year: Option<i64>,
    pub receipt_note: Option<String>,
    pub receipt_id: Option<i64>,
}

/// Expense edit (`updateExpense`). A new date without a tax year re-derives it.
/// Like the original, any other column can be written through `extra`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ExpensePatch {
    pub date: Option<String>,
    pub category: Option<String>,
    pub description: Option<String>,
    pub amount: Option<f64>,
    #[serde(deserialize_with = "double_option")]
    pub tax_year: Option<Option<i64>>,
    pub receipt_note: Option<String>,
    #[serde(deserialize_with = "double_option")]
    pub receipt_id: Option<Option<i64>>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

// ============ Commissions ============

row_struct! {
    /// A commission job.
    pub struct Commission {
        pub id: i64,
        pub client_name: String,
        /// `solar` | `roofing`
        pub job_type: String,
        pub appointment_date: Option<String>,
        pub closer_name: String,
        /// `appointment_set` | `appointment_attended` | `closed_waiting` |
        /// `paid` | `lost` | `cancelled` | `needs_review`
        pub status: String,
        /// `unpaid` | `pending` | `paid`
        pub payment_status: String,
        pub system_size_kw: Option<f64>,
        pub contract_amount: Option<f64>,
        /// Rule-based commission (0 in the roofing review gap).
        pub calculated_commission: f64,
        /// When set, overrides the calculated commission.
        pub manual_override: Option<f64>,
        /// 1 when roofing is $20,001–$29,999 with no override.
        pub needs_review: i64,
        pub notes: String,
        pub created_at: String,
        pub updated_at: String,
        pub invoice_id: Option<i64>,
        /// `not_invoiced` | `invoiced` | `paid`
        pub invoice_status: Option<String>,
        pub invoiced_at: Option<String>,
        pub paid_at: Option<String>,
    }
}

/// `computeCommission` result.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct CommissionCalc {
    pub calculated: f64,
    pub needs_review: i64,
}

/// New commission job (`createCommission`).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct CommissionInput {
    pub client_name: String,
    pub job_type: Option<String>,
    pub appointment_date: Option<String>,
    pub closer_name: Option<String>,
    pub status: Option<String>,
    pub payment_status: Option<String>,
    pub system_size_kw: Option<f64>,
    pub contract_amount: Option<f64>,
    pub manual_override: Option<f64>,
    pub notes: Option<String>,
}

/// Full commission edit (`updateCommission`); recomputes the commission.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct CommissionPatch {
    pub client_name: Option<String>,
    pub job_type: Option<String>,
    #[serde(deserialize_with = "double_option")]
    pub appointment_date: Option<Option<String>>,
    #[serde(deserialize_with = "double_option")]
    pub closer_name: Option<Option<String>>,
    pub status: Option<String>,
    pub payment_status: Option<String>,
    #[serde(deserialize_with = "double_option")]
    pub system_size_kw: Option<Option<f64>>,
    #[serde(deserialize_with = "double_option")]
    pub contract_amount: Option<Option<f64>>,
    #[serde(deserialize_with = "double_option")]
    pub manual_override: Option<Option<f64>>,
    #[serde(deserialize_with = "double_option")]
    pub notes: Option<Option<String>>,
}

/// Lifecycle-only commission edit (`patchCommission`); doesn't recompute.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct CommissionStatusPatch {
    #[serde(deserialize_with = "double_option")]
    pub status: Option<Option<String>>,
    #[serde(deserialize_with = "double_option")]
    pub payment_status: Option<Option<String>>,
    #[serde(deserialize_with = "double_option")]
    pub invoice_status: Option<Option<String>>,
    #[serde(deserialize_with = "double_option")]
    pub invoiced_at: Option<Option<String>>,
    #[serde(deserialize_with = "double_option")]
    pub paid_at: Option<Option<String>>,
    #[serde(deserialize_with = "double_option")]
    pub invoice_id: Option<Option<i64>>,
}

row_struct! {
    /// A commission invoice.
    pub struct CommissionInvoice {
        pub id: i64,
        /// `COMM-1001`…
        pub invoice_number: String,
        /// `solar` | `roofing` | `mixed`
        pub category: String,
        pub date_from: Option<String>,
        pub date_to: Option<String>,
        /// `draft` | `sent` | `paid` | `cancelled`
        pub status: String,
        pub job_count: i64,
        pub total: f64,
        pub notes: String,
        pub created_at: String,
        pub updated_at: String,
        pub paid_at: Option<String>,
    }
}

/// A commission invoice with its jobs (`commission-invoices:get`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CommissionInvoiceDetail {
    #[serde(flatten)]
    pub invoice: CommissionInvoice,
    pub jobs: Vec<Commission>,
}
deref_to!(CommissionInvoiceDetail, invoice, CommissionInvoice);

/// New commission invoice (`createCommissionInvoice`).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct CommissionInvoiceInput {
    #[serde(rename = "jobIds")]
    pub job_ids: Vec<i64>,
    /// `solar` | `roofing` | `mixed`; `both`/empty picks from the jobs.
    pub category: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub notes: Option<String>,
}

// ============ serde helpers ============

/// Tells an absent field (`None`) from an explicit `null` (`Some(None)`).
fn double_option<'de, T, D>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Option::<T>::deserialize(de).map(Some)
}

/// JS truthiness (`1`, `true`, `"x"` → true; `0`, `null`, `""` → false).
fn truthy<'de, D: Deserializer<'de>>(de: D) -> Result<bool, D::Error> {
    Ok(crate::js::js_truthy(&Value::deserialize(de)?))
}
