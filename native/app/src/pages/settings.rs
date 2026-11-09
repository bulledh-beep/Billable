//! Settings.tsx (+ SettingsLayout.tsx, ThemeToggle.tsx and
//! PhoneSyncSettings.tsx): business details, invoice numbering and payment
//! methods, rates and rounding, Content HQ, the phone connection,
//! appearance, updates, and backups of the open profile.
//!
//! The phone-sync and updater services aren't in the native build yet. The
//! page reaches them only through `phone_service` and `update_service` at
//! the bottom of this file, which return placeholder state for now.

use std::{collections::HashSet, path::Path, rc::Rc, time::Duration};

use billable_core::{
    js::{js_number_to_string, js_parse_float, js_parse_int, js_string_to_number},
    models::{PaymentMethod, Settings},
    Clock, Db,
};
use chrono::{DateTime, Local};
use gpui::{
    div, percentage, prelude::*, px, AnimationExt as _, AnyElement, App, AppContext, Context, Div, ElementId, Entity, EventEmitter,
    IntoElement, ParentElement, PathPromptOptions, PromptLevel, Render, SharedString, Styled, Subscription, Task,
    Transformation, Window,
};
use gpui_component::input::{InputEvent, InputState, TextareaState};
use serde::Deserialize;

use crate::{
    components::{
        confirm_dialog,
        fields::{checkbox, TextArea, TextField},
        select, toast_error, toast_success, Button,
    },
    fonts::rounded_family,
    format,
    icons::Lucide,
    motion::{self, rise_in},
    shell::{
        app::{navigate, Header, Page},
        route::Route,
    },
    store::{self, Store},
    style::{card, icon, num_font, r, section_title, text},
    theme::{rgba4, shadow, theme, Theme},
};

const CURRENCIES: [&str; 6] = ["CAD", "USD", "EUR", "GBP", "AUD", "JPY"];
const ROUNDING: [(&str, &str); 4] = [
    ("none", "Don't round"),
    ("6", "Round up to 6 minutes"),
    ("15", "Round up to 15 minutes"),
    ("30", "Round up to 30 minutes"),
];
/// utils/contentHq.ts `CONTENT_HQ_DEFAULT_URL`
const CONTENT_HQ_DEFAULT_URL: &str = "https://content-hq-live.vercel.app";

/// The settings keys this page edits, in the order they're saved.
const KEYS: [&str; 12] = [
    "business_name",
    "business_email",
    "business_address",
    "tax_id",
    "invoice_prefix",
    "invoice_next_number",
    "payment_methods",
    "default_payment_method",
    "default_rate",
    "default_currency",
    "time_rounding",
    "content_hq_url",
];

/// One payment method's pair of fields.
struct MethodRow {
    key: u64,
    name: Entity<InputState>,
    email: Entity<InputState>,
    /// The name before the latest edit (to tell if it was the starred one).
    last_name: String,
    _subs: [Subscription; 2],
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DownloadState {
    Idle,
    Downloading,
    Installing,
    Done,
    Error,
}

/// useUpdater's state.
struct UpdaterUi {
    status: Option<UpdateStatus>,
    checking: bool,
    download: DownloadState,
    progress: Option<UpdateProgress>,
    can_install: bool,
    /// The release notes' `<details>` is open.
    notes_open: bool,
}

/// PhoneSyncSettings' state.
struct PhoneUi {
    status: Option<PhoneStatus>,
    activity: Vec<PhoneActivity>,
    login: Option<LoginItem>,
    confirm_off: bool,
}

pub struct SettingsPage {
    loaded: Settings,
    dirty: bool,
    /// Keys changed since the last save (only these are written).
    touched: HashSet<&'static str>,
    business_name: Entity<InputState>,
    business_email: Entity<InputState>,
    business_address: Entity<TextareaState>,
    tax_id: Entity<InputState>,
    invoice_prefix: Entity<InputState>,
    invoice_next: Entity<InputState>,
    next_number: f64,
    methods: Vec<MethodRow>,
    method_seq: u64,
    default_method: String,
    default_rate: Entity<InputState>,
    currency: String,
    rounding: String,
    content_hq_url: Entity<InputState>,
    import_open: bool,
    phone: PhoneUi,
    updater: UpdaterUi,
    _tick: Task<()>,
    _subs: Vec<Subscription>,
}

/// `<input type=number>` only takes digits, a point, signs and exponents.
fn number_chars(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_digit() || ".eE+-".contains(*c)).collect()
}

/// `parseFloat(v) || 0`
fn float_or_zero(s: &str) -> f64 {
    let v = js_parse_float(s);
    if v.is_nan() || v == 0.0 {
        0.0
    } else {
        v
    }
}

/// `n || ''`: how the number fields show a stored number.
fn number_text(n: f64) -> String {
    if n == 0.0 || n.is_nan() {
        String::new()
    } else {
        js_number_to_string(n)
    }
}

impl SettingsPage {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let loaded = store::db(cx).get_settings().unwrap_or_default();
        let field = |value: String, placeholder: &'static str, window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| InputState::new(window, cx).placeholder(placeholder).default_value(value))
        };
        let business_name = field(loaded.business_name().to_string(), "Your name or company", window, cx);
        let business_email = field(loaded.business_email().to_string(), "billing@example.com", window, cx);
        let business_address = cx.new(|cx| {
            TextareaState::new(window, cx).placeholder("Street, city, province, postal code").default_value(loaded.business_address().to_string())
        });
        let tax_id = field(if loaded.tax_id() == "0" { String::new() } else { loaded.tax_id().to_string() }, "123456789", window, cx);
        let invoice_prefix = field(loaded.invoice_prefix().to_string(), "INV-", window, cx);
        let next_number = loaded.invoice_next_number();
        let invoice_next = field(number_text(next_number), "", window, cx);
        let default_rate = field(number_text(loaded.default_rate()), "", window, cx);
        let content_hq_url = field(loaded.text("content_hq_url").to_string(), CONTENT_HQ_DEFAULT_URL, window, cx);

        let mut subs = vec![
            Self::watch(&business_name, "business_name", window, cx),
            Self::watch(&business_email, "business_email", window, cx),
            Self::watch(&business_address, "business_address", window, cx),
            Self::watch(&tax_id, "tax_id", window, cx),
            Self::watch(&invoice_prefix, "invoice_prefix", window, cx),
            Self::watch(&content_hq_url, "content_hq_url", window, cx),
            cx.subscribe_in(&invoice_next, window, |p, state, e: &InputEvent, window, cx| {
                if matches!(e, InputEvent::Change) {
                    p.on_next_number(state, window, cx);
                }
            }),
            cx.subscribe_in(&default_rate, window, |p, state, e: &InputEvent, window, cx| {
                if matches!(e, InputEvent::Change) {
                    let text = state.read(cx).value().to_string();
                    let clean = number_chars(&text);
                    if clean != text {
                        state.update(cx, |s, cx| s.replace_all(clean, window, cx));
                        return;
                    }
                    p.touch("default_rate", cx);
                }
            }),
        ];

        // Esc closes Settings, with the unsaved-changes guard. Keystrokes a
        // dialog handles itself never get here.
        subs.push(cx.observe_keystrokes(|p, e, window, cx| {
            if e.keystroke.key == "escape" && !e.keystroke.modifiers.modified() {
                p.on_escape(window, cx);
            }
        }));

        // Keep "last synced 12s ago" current
        let tick = cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(Duration::from_secs(15)).await;
            if this.update(cx, |_, cx| cx.notify()).is_err() {
                break;
            }
        });

        let mut page = SettingsPage {
            dirty: false,
            touched: HashSet::new(),
            business_name,
            business_email,
            business_address,
            tax_id,
            invoice_prefix,
            invoice_next,
            next_number,
            methods: vec![],
            method_seq: 0,
            default_method: loaded.default_payment_method().to_string(),
            default_rate,
            currency: loaded.default_currency().to_string(),
            rounding: loaded.time_rounding().to_string(),
            content_hq_url,
            import_open: false,
            phone: PhoneUi { status: None, activity: vec![], login: None, confirm_off: false },
            updater: UpdaterUi { status: None, checking: false, download: DownloadState::Idle, progress: None, can_install: false, notes_open: false },
            _tick: tick,
            _subs: subs,
            loaded,
        };
        for m in page.loaded.payment_methods() {
            page.push_method(m, window, cx);
        }
        page.load_phone(cx);
        page.load_updater(cx);
        page
    }

    /// Mark `key` changed when its field is edited.
    fn watch<V: EventEmitter<InputEvent>>(state: &Entity<V>, key: &'static str, window: &mut Window, cx: &mut Context<Self>) -> Subscription {
        cx.subscribe_in(state, window, move |p, _, e: &InputEvent, _, cx| {
            if matches!(e, InputEvent::Change) {
                p.touch(key, cx);
            }
        })
    }

    fn touch(&mut self, key: &'static str, cx: &mut Context<Self>) {
        self.touched.insert(key);
        self.dirty = true;
        cx.notify();
    }

    /// `parseInt(value) || 1`, shown back the way a controlled number input
    /// does (the typed text stays while it equals the number).
    fn on_next_number(&mut self, state: &Entity<InputState>, window: &mut Window, cx: &mut Context<Self>) {
        let text = state.read(cx).value().to_string();
        let clean = number_chars(&text);
        if clean != text {
            state.update(cx, |s, cx| s.replace_all(clean, window, cx));
            return;
        }
        let n = js_parse_int(&text);
        let n = if n.is_nan() || n == 0.0 { 1.0 } else { n };
        self.next_number = n;
        if js_string_to_number(&text) != n {
            let shown = js_number_to_string(n);
            state.update(cx, |s, cx| s.replace_all(shown, window, cx));
        }
        self.touch("invoice_next_number", cx);
    }

    // ---- Payment methods ----

    fn push_method(&mut self, m: PaymentMethod, window: &mut Window, cx: &mut Context<Self>) {
        self.method_seq += 1;
        let key = self.method_seq;
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("e-Transfer").default_value(m.name.clone()));
        let email = cx.new(|cx| InputState::new(window, cx).placeholder("Where to send it (optional)").default_value(m.email.clone()));
        let subs = [
            cx.subscribe_in(&name, window, move |p, state, e: &InputEvent, _, cx| {
                if !matches!(e, InputEvent::Change) {
                    return;
                }
                let value = state.read(cx).value().to_string();
                let Some(row) = p.methods.iter_mut().find(|r| r.key == key) else { return };
                let was_default = p.default_method == row.last_name;
                row.last_name = value.clone();
                if was_default {
                    p.default_method = value;
                    p.touched.insert("default_payment_method");
                }
                p.touch("payment_methods", cx);
            }),
            cx.subscribe_in(&email, window, |p, _, e: &InputEvent, _, cx| {
                if matches!(e, InputEvent::Change) {
                    p.touch("payment_methods", cx);
                }
            }),
        ];
        self.methods.push(MethodRow { key, name, email, last_name: m.name, _subs: subs });
    }

    fn method_values(&self, cx: &App) -> Vec<PaymentMethod> {
        self.methods.iter().map(|m| PaymentMethod { name: m.name.read(cx).value().to_string(), email: m.email.read(cx).value().to_string() }).collect()
    }

    fn remove_method(&mut self, key: u64, cx: &mut Context<Self>) {
        let Some(i) = self.methods.iter().position(|m| m.key == key) else { return };
        let was_default = self.default_method == self.methods[i].name.read(cx).value().as_ref();
        self.methods.remove(i);
        if was_default {
            self.default_method = self.methods.first().map(|m| m.name.read(cx).value().to_string()).unwrap_or_default();
            self.touched.insert("default_payment_method");
        }
        self.touch("payment_methods", cx);
    }

    fn star_method(&mut self, key: u64, cx: &mut Context<Self>) {
        if let Some(m) = self.methods.iter().find(|m| m.key == key) {
            self.default_method = m.name.read(cx).value().to_string();
            self.touch("default_payment_method", cx);
        }
    }

    // ---- Save and close ----

    fn current_value(&self, key: &str, cx: &App) -> String {
        let input = |s: &Entity<InputState>| s.read(cx).value().to_string();
        match key {
            "business_name" => input(&self.business_name),
            "business_email" => input(&self.business_email),
            "business_address" => self.business_address.read(cx).value().to_string(),
            "tax_id" => input(&self.tax_id),
            "invoice_prefix" => input(&self.invoice_prefix),
            "invoice_next_number" => js_number_to_string(self.next_number),
            "payment_methods" => serde_json::to_string(&self.method_values(cx)).unwrap_or_else(|_| "[]".into()),
            "default_payment_method" => self.default_method.clone(),
            "default_rate" => js_number_to_string(float_or_zero(&input(&self.default_rate))),
            "default_currency" => self.currency.clone(),
            "time_rounding" => self.rounding.clone(),
            "content_hq_url" => input(&self.content_hq_url),
            _ => String::new(),
        }
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let data: Vec<(&str, String)> = KEYS.iter().filter(|k| self.touched.contains(*k)).map(|k| (*k, self.current_value(k, cx))).collect();
        match store::write(cx, |db| db.update_settings(&data)) {
            Ok(saved) => {
                self.loaded = saved;
                self.touched.clear();
                self.dirty = false;
                toast_success("Settings saved", cx);
            }
            Err(e) => toast_error(e.to_string(), cx),
        }
        cx.notify();
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.dirty {
            navigate(Route::Dashboard, window, cx);
            return;
        }
        let answer = window.prompt(PromptLevel::Info, "You have unsaved changes. Close without saving?", None, &["OK", "Cancel"], cx);
        cx.spawn_in(window, async move |_, cx| {
            if answer.await == Ok(0) {
                let _ = cx.update(|window, cx| navigate(Route::Dashboard, window, cx));
            }
        })
        .detach();
    }

    fn on_escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // A dialog that's open closes first, as its own Esc handler would
        if self.import_open || self.phone.confirm_off {
            self.import_open = false;
            self.phone.confirm_off = false;
            cx.notify();
            return;
        }
        self.close(window, cx);
    }

    // ---- Backups ----

    fn export_backup(&mut self, cx: &mut Context<Self>) {
        let name = format!("billable-backup-{}.db", store::db(cx).local_today());
        let home = std::env::var_os("HOME").map(std::path::PathBuf::from).unwrap_or_else(|| "/".into());
        let docs = home.join("Documents");
        let dir = if docs.is_dir() { docs } else { home };
        let chosen = cx.prompt_for_new_path(&dir, Some(&name));
        cx.spawn(async move |_, cx| {
            let path = match chosen.await {
                Ok(Ok(Some(path))) => path,
                Ok(Err(e)) => {
                    cx.update(|cx| toast_error(format!("Couldn't export: {e}"), cx));
                    return;
                }
                _ => return,
            };
            cx.update(|cx| match store::db(cx).export_to(&path) {
                Ok(()) => toast_success("Backup saved", cx),
                Err(e) => toast_error(format!("Couldn't export: {e}"), cx),
            });
        })
        .detach();
    }

    fn import_backup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let chosen = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: false, prompt: None });
        cx.spawn_in(window, async move |_, cx| {
            let Ok(Ok(Some(paths))) = chosen.await else { return };
            let Some(source) = paths.into_iter().next() else { return };
            let _ = cx.update(|window, cx| restore_backup(&source, window, cx));
        })
        .detach();
    }

    // ---- Phone ----

    fn phone_sink(&self, cx: &mut Context<Self>) -> Sink<PhoneReply> {
        let this = cx.weak_entity();
        Rc::new(move |reply, cx| {
            let _ = this.update(cx, |p, cx| p.apply_phone(reply, cx));
        })
    }

    fn apply_phone(&mut self, reply: PhoneReply, cx: &mut Context<Self>) {
        if let Some(s) = reply.status {
            self.phone.status = Some(s);
        }
        if let Some(a) = reply.activity {
            self.phone.activity = a;
        }
        if let Some(l) = reply.login {
            self.phone.login = Some(l);
        }
        cx.notify();
    }

    fn load_phone(&mut self, cx: &mut Context<Self>) {
        self.phone_call(PhoneCall::Load, cx);
    }

    fn phone_call(&mut self, call: PhoneCall, cx: &mut Context<Self>) {
        let disconnecting = matches!(call, PhoneCall::Disconnect);
        let sink = self.phone_sink(cx);
        let task = phone_service(call, sink, cx);
        cx.spawn(async move |this, cx| {
            let Ok(reply) = task.await else { return };
            let remote = reply.remote_disconnected;
            let _ = this.update(cx, |p, cx| p.apply_phone(reply, cx));
            if disconnecting {
                cx.update(|cx| {
                    if remote == Some(true) {
                        toast_success("Disconnected. Content HQ deleted its copy of your Billable data.", cx);
                    } else {
                        crate::components::toast::toast_for(
                            "Disconnected on this Mac, but Content HQ couldn’t be reached. Disconnect in Content HQ’s settings too, so it deletes its copy.",
                            crate::components::ToastIcon::None,
                            7000,
                            cx,
                        );
                    }
                });
            }
        })
        .detach();
    }

    // ---- Updates ----

    fn update_sink(&self, cx: &mut Context<Self>) -> Sink<UpdaterReply> {
        let this = cx.weak_entity();
        Rc::new(move |reply, cx| {
            let _ = this.update(cx, |p, cx| p.apply_update(reply, cx));
        })
    }

    fn apply_update(&mut self, reply: UpdaterReply, cx: &mut Context<Self>) {
        if let Some(s) = reply.status {
            self.updater.status = Some(s);
        }
        if let Some(c) = reply.can_install {
            self.updater.can_install = c;
        }
        if let Some(p) = reply.progress {
            self.updater.progress = Some(p);
        }
        cx.notify();
    }

    /// On mount: the cached status and whether installing works, then a quiet
    /// fresh check (network errors on launch aren't shown).
    fn load_updater(&mut self, cx: &mut Context<Self>) {
        let sink = self.update_sink(cx);
        let load = update_service(UpdateCall::Load, sink.clone(), cx);
        cx.spawn(async move |this, cx| {
            if let Ok(reply) = load.await {
                let _ = this.update(cx, |p, cx| p.apply_update(reply, cx));
            }
            let Ok(check) = this.update(cx, |p, cx| {
                p.updater.checking = true;
                cx.notify();
                update_service(UpdateCall::Check { force: false }, sink, cx)
            }) else {
                return;
            };
            let result = check.await;
            let _ = this.update(cx, |p, cx| {
                if let Ok(reply) = result {
                    p.apply_update(reply, cx);
                }
                p.updater.checking = false;
                cx.notify();
            });
        })
        .detach();
    }

    fn check_now(&mut self, cx: &mut Context<Self>) {
        self.updater.checking = true;
        cx.notify();
        let sink = self.update_sink(cx);
        let task = update_service(UpdateCall::Check { force: true }, sink, cx);
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |p, cx| {
                p.updater.checking = false;
                match result {
                    Ok(reply) => {
                        let latest = reply.status.as_ref().filter(|s| !s.update_available).map(|s| s.current_version.clone());
                        p.apply_update(reply, cx);
                        if let Some(v) = latest {
                            toast_success(format!("You're on the latest version ({v})"), cx);
                        }
                    }
                    Err(e) => toast_error(format!("Update check failed: {e}"), cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Install and relaunch, or (when the app can't replace itself) download
    /// the disk image.
    fn install_update(&mut self, cx: &mut Context<Self>) {
        let Some(url) = self.updater.status.as_ref().and_then(|s| s.download_url.clone()) else {
            toast_error("No download available for this release", cx);
            return;
        };
        let install = self.updater.can_install;
        self.updater.download = DownloadState::Downloading;
        self.updater.progress = None;
        cx.notify();
        let sink = self.update_sink(cx);
        let task = update_service(if install { UpdateCall::Install { url } } else { UpdateCall::Download { url } }, sink, cx);
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |p, cx| {
                match (result, install) {
                    (Ok(reply), true) => {
                        p.apply_update(reply, cx);
                        p.updater.download = DownloadState::Installing;
                        toast_success("Update installed — Billable will quit and relaunch", cx);
                    }
                    (Ok(reply), false) => {
                        p.apply_update(reply, cx);
                        p.updater.download = DownloadState::Done;
                        toast_success("Update downloaded — drag Billable into Applications to install", cx);
                    }
                    (Err(e), true) => {
                        p.updater.download = DownloadState::Error;
                        toast_error(format!("Install failed: {e}"), cx);
                    }
                    (Err(e), false) => {
                        p.updater.download = DownloadState::Error;
                        toast_error(format!("Download failed: {e}"), cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}

/// ipc.ts `settings:import-db`: check it's a SQLite file, stop the timer,
/// keep a copy of the current data, swap the file in, and reopen the
/// profile (the Electron app relaunched instead).
fn restore_backup(source: &Path, window: &mut Window, cx: &mut App) {
    use std::io::Read;
    let mut header = [0u8; 16];
    let read = std::fs::File::open(source).and_then(|mut f| f.read(&mut header));
    if !matches!(read, Ok(n) if n >= 15 && &header[..15] == b"SQLite format 3") {
        toast_error(match read {
            Err(e) => e.to_string(),
            Ok(_) => "That file is not a Billable database".into(),
        }, cx);
        return;
    }
    let s = store::store(cx);
    if let Some(id) = s.timer.active.as_ref().map(|e| e.entry.id) {
        let _ = s.db.stop_timer(id, Some(chrono::Utc::now()));
    }
    let active = s.profiles.get_active_profile_id();
    let target = s.db.path().to_path_buf();
    // The open connection has to close before the file is replaced; an
    // in-memory database stands in until the profile is reopened.
    let stand_in = match Db::open(":memory:", Clock::system()) {
        Ok(db) => db,
        Err(e) => {
            toast_error(e.to_string(), cx);
            return;
        }
    };
    let current = std::mem::replace(&mut cx.global_mut::<Store>().db, stand_in);
    let imported = current.import_from(source);
    // import_from closed the database whatever happened: open it again.
    let reopened = store::switch_profile(&active, cx);
    match (imported, reopened) {
        (Ok(_), Ok(())) => {
            navigate(Route::Dashboard, window, cx);
            toast_success("Backup restored", cx);
        }
        (Err(e), Ok(())) => toast_error(e.to_string(), cx),
        (Ok(backup), Err(e)) => {
            // The backup won't open: put the copy of the previous data back.
            for ext in ["-wal", "-shm"] {
                let _ = std::fs::remove_file(format!("{}{ext}", target.display()));
            }
            let restored = std::fs::copy(&backup, &target).is_ok() && store::switch_profile(&active, cx).is_ok();
            toast_error(
                if restored { format!("That backup couldn't be opened ({e}). Your data was left as it was.") } else { format!("That backup couldn't be opened: {e}") },
                cx,
            );
        }
        (Err(e), Err(_)) => toast_error(e.to_string(), cx),
    }
}

fn set_theme(value: &str, cx: &mut App) {
    let s = cx.global_mut::<Store>();
    s.prefs.theme = Some(value.to_string());
    s.save_prefs();
    cx.refresh_windows();
}

// ============================================================================
// SettingsLayout.tsx
// ============================================================================

/// A labelled row: label and help on the left (220px), the control on the right.
struct SRow {
    label: SharedString,
    help: Option<SharedString>,
    content: AnyElement,
}

fn srow(label: impl Into<SharedString>, help: Option<SharedString>, content: impl IntoElement) -> SRow {
    SRow { label: label.into(), help, content: content.into_any_element() }
}

fn section(t: &Theme, title: &str, description: Option<&str>, rows: Vec<SRow>) -> Div {
    let mut head = div().child(section_title(t, title.to_string()));
    if let Some(d) = description {
        head = head.child(text(d.to_string(), 12.0, 500, t.fg3).mt(r(0.5)).line_height(px(16.)));
    }
    let n = rows.len();
    let mut body = card(t);
    for (i, row) in rows.into_iter().enumerate() {
        let mut left = div().w(px(220.)).flex_none().pt(px(5.)).child(text(row.label, 13.0, 500, t.fg).line_height(px(19.5)));
        if let Some(h) = row.help {
            left = left.child(text(h, 12.0, 500, t.fg3).mt(r(1.)).line_height(r(4.)));
        }
        let mut el = div().flex().gap(r(6.)).px(r(5.)).py(r(4.)).child(left).child(div().flex_1().min_w_0().child(row.content));
        if i + 1 < n {
            el = el.border_b_1().border_color(t.line);
        }
        body = body.child(el);
    }
    div().mb(r(7.)).child(div().flex().items_end().justify_between().gap(r(4.)).mb(r(2.5)).child(head)).child(body)
}

/// A `.btn-icon-sm` between two `w-full` inputs: Chromium's flex shrink
/// squeezes it down to its 15px icon (the inputs keep the rest).
fn squeezed(button: Button) -> Div {
    div().w(px(15.)).flex_none().child(button.full_width())
}

/// A button sized to its content inside a row.
fn fit(el: impl IntoElement) -> Div {
    div().flex().child(el)
}

impl Page for SettingsPage {
    fn header(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> Header {
        Header::new("Settings").actions(
            div()
                .flex()
                .items_center()
                .gap(r(2.))
                .child(Button::primary("settings-save", "Save changes").disabled(!self.dirty).on_click(cx.listener(|p, _, _, cx| p.save(cx))))
                .child(
                    Button::icon_button("settings-close", Lucide::X)
                        .tooltip("Close settings (Esc)")
                        .on_click(cx.listener(|p, _, w, cx| p.close(w, cx))),
                ),
        )
    }
}

impl Render for SettingsPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(cx);
        let mut block = div().max_w(px(860.)).flex().flex_col();

        // Business
        block = block.child(section(&t, "Business", Some("Shown at the top of every invoice."), vec![
            srow("Business name", None, TextField::new(&self.business_name)),
            srow("Email", None, TextField::new(&self.business_email)),
            // A textarea is an inline-block whose baseline is its bottom edge: the
            // row's line box adds the strut's descent (3px + 1.75px half-leading)
            // below it.
            srow("Address", None, div().pb(px(4.75)).child(TextArea::new(&self.business_address, 2))),
            srow(
                "Business number",
                Some("Optional. Printed on invoices when filled in. Your GST/HST number lives in Tax settings.".into()),
                div().max_w(px(280.)).child(TextField::new(&self.tax_id)),
            ),
        ]));

        // Invoices
        let prefix = self.invoice_prefix.read(cx).value().to_string();
        let numbering = div()
            .flex()
            .items_center()
            .gap(r(2.))
            .child(div().w(r(24.)).flex_none().child(TextField::new(&self.invoice_prefix)))
            .child(div().w(r(28.)).flex_none().child(TextField::new(&self.invoice_next).tabular()))
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .whitespace_nowrap()
                    .line_height(px(16.))
                    .child(text("Next: ", 12.0, 500, t.fg3))
                    .child(div().font(num_font(500)).text_size(px(12.)).text_color(t.fg2).child(format!("{prefix}{}", js_number_to_string(self.next_number)))),
            );
        let mut methods = div().flex().flex_col().gap(r(2.));
        for m in &self.methods {
            let key = m.key;
            let is_default = self.default_method == m.name.read(cx).value().as_ref();
            let mut star = Button::icon_button(ElementId::Name(format!("pm-star-{key}").into()), Lucide::Star)
                .small()
                .tooltip(if is_default { "Default method" } else { "Make default" })
                .on_click(cx.listener(move |p, _, _, cx| p.star_method(key, cx)));
            if is_default {
                star = star.filled().text_color(t.accent_text).hover_text(t.accent_text);
            }
            methods = methods.child(
                div()
                    .flex()
                    .items_center()
                    .gap(r(2.))
                    .child(squeezed(star))
                    .child(div().flex_1().min_w_0().child(TextField::new(&m.name)))
                    .child(div().flex_1().min_w_0().child(TextField::new(&m.email)))
                    .child(squeezed(
                        Button::icon_button(ElementId::Name(format!("pm-remove-{key}").into()), Lucide::Trash2)
                            .small()
                            .hover_text(t.red)
                            .on_click(cx.listener(move |p, _, _, cx| p.remove_method(key, cx))),
                    )),
            );
        }
        methods = methods.child(fit(div().ml(-r(1.)).child(Button::ghost("pm-add", "Add method").small().on_click(cx.listener(|p, _, w, cx| {
            p.push_method(PaymentMethod::default(), w, cx);
            p.touch("payment_methods", cx);
        })))));
        block = block.child(section(&t, "Invoices", None, vec![
            srow("Numbering", Some("The next invoice gets this number, then it counts up.".into()), numbering),
            srow("Payment methods", Some("The starred one is filled in on new invoices.".into()), methods),
        ]));

        // Time and rates
        let this = cx.weak_entity();
        let currency_value = if CURRENCIES.contains(&self.currency.as_str()) { self.currency.clone() } else { CURRENCIES[0].to_string() };
        let currency = select(
            "settings-currency",
            CURRENCIES.iter().map(|c| (c.to_string(), SharedString::from(*c))).collect(),
            &currency_value,
            {
                let this = this.clone();
                move |v, _, cx| {
                    let _ = this.update(cx, |p, cx| {
                        p.currency = v;
                        p.touch("default_currency", cx);
                    });
                }
            },
            cx,
        );
        let rounding_value = if ROUNDING.iter().any(|(v, _)| *v == self.rounding) { self.rounding.clone() } else { ROUNDING[0].0.to_string() };
        let rounding = select(
            "settings-rounding",
            ROUNDING.iter().map(|(v, l)| (v.to_string(), SharedString::from(*l))).collect(),
            &rounding_value,
            move |v, _, cx| {
                let _ = this.update(cx, |p, cx| {
                    p.rounding = v;
                    p.touch("time_rounding", cx);
                });
            },
            cx,
        );
        block = block.child(section(&t, "Time and rates", None, vec![
            srow("Default hourly rate", Some("Used for new clients.".into()), div().w(r(40.)).child(TextField::new(&self.default_rate).leading_text("$").tabular())),
            srow("Currency", None, div().w(r(40.)).child(currency)),
            srow("Timer rounding", Some("Applied when a timer stops. Timers under a minute are always discarded.".into()), div().w(r(60.)).child(rounding)),
        ]));

        // Content HQ
        block = block.child(section(&t, "Content HQ", Some("Your content workspace, opened inside Billable from the sidebar."), vec![
            srow(
                "Address",
                Some("Leave blank for the live workspace. Changes apply the next time Billable opens.".into()),
                div().max_w(px(360.)).child(TextField::new(&self.content_hq_url)),
            ),
            srow(
                "Sign-in",
                Some("Content HQ keeps you signed in inside Billable. Sign out here to switch accounts.".into()),
                fit(Button::secondary("content-hq-sign-out", "Sign out of Content HQ").on_click(|_, _, cx| {
                    let task = content_hq_sign_out(cx);
                    cx.spawn(async move |cx| {
                        if task.await.is_ok() {
                            cx.update(|cx| toast_success("Signed out of Content HQ. It will ask you to sign in next time.", cx));
                        }
                    })
                    .detach();
                })),
            ),
        ]));

        // Billable on your phone
        if let Some(phone) = self.phone_section(&t, cx) {
            block = block.child(phone);
        }

        // Appearance
        let pref = store::store(cx).prefs.theme.clone().filter(|v| v == "dark" || v == "light" || v == "auto").unwrap_or_else(|| "auto".into());
        block = block.child(section(&t, "Appearance", None, vec![srow(
            "Theme",
            Some("Auto follows your Mac. Shared by all profiles.".into()),
            fit(theme_toggle(&t, &pref)),
        )]));

        // Updates
        block = block.child(self.updates_section(&t, cx));

        // Data
        block = block.child(section(&t, "Data", Some("Backups cover the profile you're in now."), vec![
            srow(
                "Back up",
                Some("Saves a complete copy of this profile's clients, time, invoices, and expenses.".into()),
                fit(Button::secondary("settings-export", "Export backup").on_click(cx.listener(|p, _, _, cx| p.export_backup(cx)))),
            ),
            srow(
                "Restore",
                Some("Replaces this profile's data with a backup. A copy of the current data is kept next to it first.".into()),
                fit(Button::secondary("settings-import", "Restore from backup…").on_click(cx.listener(|p, _, _, cx| {
                    p.import_open = true;
                    cx.notify();
                }))),
            ),
        ]));

        let mut page = div()
            .relative()
            .w_full()
            .max_w(px(1160.))
            .mx_auto()
            .px(r(7.))
            .pt(r(6.))
            .pb(r(14.))
            .child(rise_in("settings-rise-0", 0, block));

        if self.import_open {
            let (a, b) = (cx.weak_entity(), cx.weak_entity());
            page = page.child(confirm_dialog(
                "restore-backup",
                "Restore from a backup?",
                "This replaces everything in the current profile with the backup you choose, then restarts Billable. A copy of the current data is saved first, so nothing is lost.",
                "Choose backup…",
                false,
                move |w, cx| {
                    let _ = a.update(cx, |p, cx| p.import_backup(w, cx));
                },
                move |_, cx| {
                    let _ = b.update(cx, |p, cx| {
                        p.import_open = false;
                        cx.notify();
                    });
                },
                cx,
            ));
        }
        if self.phone.confirm_off {
            let (a, b) = (cx.weak_entity(), cx.weak_entity());
            page = page.child(confirm_dialog(
                "phone-disconnect",
                "Disconnect your phone?",
                "Content HQ will stop getting updates from Billable and delete its copy. Your records on this Mac aren’t touched.",
                "Disconnect",
                true,
                move |_, cx| {
                    let _ = a.update(cx, |p, cx| p.phone_call(PhoneCall::Disconnect, cx));
                },
                move |_, cx| {
                    let _ = b.update(cx, |p, cx| {
                        p.phone.confirm_off = false;
                        cx.notify();
                    });
                },
                cx,
            ));
        }
        page
    }
}

/// ThemeToggle.tsx: Light / Dark / Auto on the `.seg` track.
fn theme_toggle(t: &Theme, current: &str) -> Div {
    let mut track = div().flex().flex_none().items_center().p(px(3.)).rounded_full().bg(t.fg.opacity(0.06));
    let options = [("light", Lucide::Sun, "Light"), ("dark", Lucide::Moon, "Dark"), ("auto", Lucide::Monitor, "Auto")];
    for (i, (value, glyph, label)) in options.into_iter().enumerate() {
        let active = current == value;
        let hover = t.fg;
        let group: SharedString = format!("theme-seg-{i}").into();
        let mut item = div()
            .id(ElementId::Name(group.clone()))
            .group(group.clone())
            .flex()
            .items_center()
            .gap(r(1.5))
            .h(px(27.))
            .px(r(3.5))
            .rounded_full()
            .cursor_pointer()
            .whitespace_nowrap()
            .text_color(if active { t.fg } else { t.fg3 })
            .hover(move |s| s.text_color(hover));
        if active {
            item = item
                .bg(if t.dark { t.fg.opacity(0.14) } else { t.panel })
                .shadow(vec![shadow(0.0, 1.0, 2.0, 0.0, rgba4(0, 0, 0, 0.1)), shadow(0.0, 2.0, 8.0, -2.0, rgba4(0, 0, 0, 0.1))]);
        }
        item = item
            .on_click(move |_, _, cx| set_theme(value, cx))
            // SVGs don't inherit the text colour: give the icon the item's
            .child(icon(glyph).size(r(3.5)).text_color(if active { t.fg } else { t.fg3 }).group_hover(group, move |s| s.text_color(hover)))
            .child(div().font_family(rounded_family(600)).text_size(px(13.)).child(label));
        track = track.child(item);
    }
    track
}

impl SettingsPage {
    /// PhoneSyncSettings.tsx (hidden until the status has loaded).
    fn phone_section(&self, t: &Theme, cx: &mut Context<Self>) -> Option<Div> {
        let status = self.phone.status.clone()?;
        let dot_color = match status.state {
            PhoneSyncState::Ok => t.green,
            PhoneSyncState::Syncing => t.blue,
            PhoneSyncState::Offline | PhoneSyncState::Unavailable => t.amber,
            PhoneSyncState::Error | PhoneSyncState::Mismatch => t.red,
            PhoneSyncState::Revoked | PhoneSyncState::Off => t.gray,
        };
        let status_line = |s: String| {
            div()
                .flex()
                .items_start()
                .gap(r(2.))
                .child(div().mt(px(7.)).size(r(2.)).flex_none().rounded_full().bg(dot_color))
                .child(text(s, 13.0, 500, t.fg).line_height(px(18.)).min_w_0())
        };
        let host = status.host.clone().unwrap_or_default();
        let connection = if status.connected {
            let line = match status.state {
                PhoneSyncState::Ok => format!(
                    "Syncing with {host}{}",
                    status.last_sync_at.as_deref().filter(|s| !s.is_empty()).map(|s| format!(" · last synced {}", format::relative(s, Local::now()).to_lowercase())).unwrap_or_default()
                ),
                PhoneSyncState::Syncing => format!("Connecting to {host}…"),
                _ => status.message.clone().filter(|m| !m.is_empty()).unwrap_or_else(|| format!("Connected to {host}")),
            };
            div()
                .flex()
                .flex_col()
                .gap(r(3.))
                .child(status_line(line))
                .children(status.profile_name.clone().filter(|n| !n.is_empty()).map(|n| text(format!("Linked to the {n} profile."), 12.0, 500, t.fg3).line_height(px(16.))))
                .child(
                    div()
                        .flex()
                        .gap(r(2.))
                        .child(Button::secondary("phone-sync-now", "Sync now").on_click(cx.listener(|p, _, _, cx| p.phone_call(PhoneCall::SyncNow, cx))))
                        .child(Button::ghost("phone-disconnect", "Disconnect").on_click(cx.listener(|p, _, _, cx| {
                            p.phone.confirm_off = true;
                            cx.notify();
                        }))),
                )
        } else {
            let line = match (&status.state, status.message.as_deref().filter(|m| !m.is_empty())) {
                (PhoneSyncState::Revoked, Some(m)) => m.to_string(),
                _ => "Not connected".to_string(),
            };
            div()
                .flex()
                .flex_col()
                .gap(r(3.))
                .child(status_line(line))
                .child(fit(Button::secondary("phone-open-content", "Open Content HQ").on_click(|_, w, cx| navigate(Route::Content, w, cx))))
        };

        let login = match self.phone.login.filter(|l| l.available) {
            Some(l) => {
                let enabled = l.enabled;
                div()
                    .flex()
                    .flex_col()
                    .gap(r(1.5))
                    .child(fit(
                        div()
                            .id("phone-login")
                            .flex()
                            .items_center()
                            .gap(r(2.))
                            .cursor_pointer()
                            .on_click(cx.listener(move |p, _, _, cx| p.phone_call(PhoneCall::SetOpenAtLogin(!enabled), cx)))
                            .child(checkbox(t, "phone-login-box", enabled, false))
                            .child(text("Open Billable when I log in", 13.0, 500, t.fg).line_height(px(18.))),
                    ))
                    .children(l.needs_approval.then(|| text("Allow Billable in System Settings → General → Login Items.", 12.0, 500, t.amber).line_height(px(16.))))
            }
            None => div().child(text("Available in the installed app.", 13.0, 500, t.fg3).pt(px(5.)).line_height(px(18.))),
        };

        let activity = if self.phone.activity.is_empty() {
            div().child(text("Nothing yet.", 13.0, 500, t.fg3).pt(px(5.)).line_height(px(18.)))
        } else {
            let now = Local::now();
            let mut list = div().flex().flex_col().gap(r(2.)).pt(px(3.));
            for a in &self.phone.activity {
                let rejected = a.status == ActivityStatus::Rejected;
                let summary = if rejected { format!("Couldn’t apply: {}", a.summary.to_lowercase()) } else { a.summary.clone() };
                list = list.child(
                    div()
                        .child(
                            div()
                                .flex()
                                .items_start()
                                .justify_between()
                                .gap(r(3.))
                                .child(text(summary, 13.0, 500, if rejected { t.fg2 } else { t.fg }).line_height(px(18.)).min_w_0())
                                // items-baseline: the 12/16 line's baseline (0.5 + 12) meets
                                // the 13/18 line's (1 + 13), 1.5px down
                                .child(text(format::relative(&a.applied_at, now), 12.0, 500, t.fg3).line_height(px(16.)).mt(px(1.5)).flex_none()),
                        )
                        .children(a.message.clone().filter(|m| !m.is_empty()).map(|m| text(m, 12.0, 500, if rejected { t.red } else { t.fg3 }).mt(r(0.5)).line_height(px(16.)))),
                );
            }
            list
        };

        Some(section(t, "Billable on your phone", Some("Use Billable from Content HQ on your iPhone. Your Mac keeps the real records."), vec![
            srow(
                "Connection",
                Some(
                    if status.connected {
                        "Changes on your phone reach this Mac within about 20 seconds while Billable is open, or as soon as your Mac wakes up."
                    } else {
                        "In Content HQ, open Settings → Billable and click Connect this Mac."
                    }
                    .into(),
                ),
                connection,
            ),
            srow("Open at login", Some("Billable has to be running to pick up changes from your phone. This starts it when your Mac starts.".into()), login),
            srow("Recent changes from your phone", None, activity),
        ]))
    }

    /// The Updates section: version, check now, install, release notes.
    fn updates_section(&self, t: &Theme, cx: &mut Context<Self>) -> Div {
        let u = &self.updater;
        let status = u.status.clone();
        let help: SharedString = match status.as_ref().and_then(|s| s.last_checked_at.as_deref()).and_then(|s| DateTime::parse_from_rfc3339(s).ok()) {
            Some(at) => format!("Checked {}", at.with_timezone(&Local).format("%b %-d, %-I:%M %p")).into(),
            None => "Checks GitHub when the app opens.".into(),
        };

        let mut version = div()
            .flex()
            .items_baseline()
            .text_size(px(13.))
            .line_height(px(18.))
            .child(div().font(num_font(500)).text_color(t.fg).child(status.as_ref().map(|s| s.current_version.clone()).filter(|v| !v.is_empty()).unwrap_or_else(|| "—".into())));
        if let Some(s) = &status {
            if s.update_available {
                version = version.child(text(format!(" · {} available", s.latest_version.clone().unwrap_or_default()), 13.0, 500, t.accent_text));
            } else if s.latest_version.is_some() {
                version = version.child(text(" · up to date", 13.0, 500, t.fg3));
            }
        }

        // .btn-secondary.btn-sm with a RefreshCw that spins while checking
        let checking = u.checking;
        let spinner = icon(Lucide::RefreshCw).size(px(13.)).text_color(if checking { t.fg4 } else { t.fg });
        let spinner: AnyElement = if checking && !motion::reduced() {
            spinner
                .with_animation("update-spin", gpui::Animation::new(Duration::from_secs(1)).repeat(), |s, d| s.with_transformation(Transformation::rotate(percentage(d))))
                .into_any_element()
        } else {
            spinner.into_any_element()
        };
        let hover_bg = t.fg.opacity(0.1);
        let mut check = div()
            .id("update-check")
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .gap(r(1.5))
            .h(px(27.))
            .px(r(3.))
            .rounded_full()
            .bg(t.fg.opacity(0.06))
            .text_color(if checking { t.fg4 } else { t.fg })
            .whitespace_nowrap()
            .child(spinner)
            .child(div().font_family(rounded_family(600)).text_size(px(12.)).child(if checking { "Checking…" } else { "Check now" }));
        if !checking {
            check = check.cursor_pointer().hover(move |s| s.bg(hover_bg)).on_click(cx.listener(|p, _, _, cx| p.check_now(cx)));
        }
        let mut buttons = div().ml_auto().flex().items_center().gap(r(2.)).child(check);
        if status.as_ref().is_some_and(|s| s.update_available) {
            let busy = matches!(u.download, DownloadState::Downloading | DownloadState::Installing);
            let label = match u.download {
                DownloadState::Downloading => format!("Downloading {}%", u.progress.map(|p| js_number_to_string(p.percent)).unwrap_or_else(|| "0".into())),
                DownloadState::Installing => "Installing…".into(),
                _ if u.can_install => "Install and relaunch".into(),
                _ => "Download update".into(),
            };
            buttons = buttons.child(
                Button::primary("update-install", label).small().icon(Lucide::Download).disabled(busy).on_click(cx.listener(|p, _, _, cx| p.install_update(cx))),
            );
        }

        let mut content = div().child(div().flex().items_center().gap(r(3.)).child(version).child(buttons));
        if let Some(s) = status.as_ref().filter(|s| s.update_available) {
            if let Some(notes) = s.release_notes.clone().filter(|n| !n.is_empty()) {
                let (fg, fg3, accent) = (t.fg, t.fg3, t.accent_text);
                let mut summary = div()
                    .id("update-notes-toggle")
                    .flex()
                    .items_center()
                    .gap(r(1.5))
                    .cursor_pointer()
                    .font_family(rounded_family(500))
                    .text_size(px(12.))
                    .line_height(px(16.))
                    .text_color(t.fg2)
                    .hover(move |st| st.text_color(fg))
                    .on_click(cx.listener(|p, _, _, cx| {
                        p.updater.notes_open = !p.updater.notes_open;
                        cx.notify();
                    }))
                    .child(if u.notes_open { "Hide notes" } else { "What's new" });
                if let Some(url) = s.release_url.clone().filter(|u| !u.is_empty()) {
                    summary = summary.child(
                        div()
                            .id("update-github")
                            .group("update-github")
                            .ml(r(2.))
                            .flex()
                            .items_center()
                            .gap(r(1.))
                            .text_color(fg3)
                            .hover(move |st| st.text_color(accent))
                            .on_click(move |_, _, cx| {
                                cx.stop_propagation();
                                cx.open_url(&url);
                            })
                            .child("GitHub")
                            .child(icon(Lucide::ExternalLink).size(r(3.)).text_color(fg3).group_hover("update-github", move |st| st.text_color(accent))),
                    );
                }
                let mut details = div().mt(r(3.)).child(summary);
                if u.notes_open {
                    details = details.child(
                        div()
                            .id("update-notes")
                            .mt(r(2.))
                            .p(r(3.))
                            .rounded(px(4.875))
                            .bg(t.panel2)
                            .max_h(r(60.))
                            .overflow_y_scroll()
                            .child(text(notes, 12.0, 500, t.fg2).line_height(r(5.))),
                    );
                }
                content = content.child(details);
            }
        }

        section(t, "Updates", None, vec![srow("Version", Some(help), content)])
    }
}

// ============================================================================
// Backend hooks
//
// The page talks to the phone-sync service (Electron: phone-sync.ts behind
// ipc `phone:*`) and the updater (updater.ts behind ipc `updater:*`) only
// through `phone_service` and `update_service` below. Both return
// placeholder state today; wire them to the real services. Each takes a
// `Sink` the service may keep to push later state into the page (the
// `phone:status` / `phone:applied` / `updater:progress` events); it does
// nothing once the page is gone. Replies only carry the fields that changed.
// The types mirror the TypeScript shapes, so they deserialize from the same
// JSON.
// ============================================================================

/// Pushes state into the open Settings page.
pub type Sink<T> = Rc<dyn Fn(T, &mut App)>;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PhoneSyncState {
    Off,
    Syncing,
    Ok,
    Offline,
    Unavailable,
    Revoked,
    Mismatch,
    Error,
}

/// `PhoneStatus` (ipc `phone:status`).
#[allow(dead_code)]
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhoneStatus {
    pub connected: bool,
    pub state: PhoneSyncState,
    pub host: Option<String>,
    pub profile_name: Option<String>,
    pub connected_at: Option<String>,
    pub last_sync_at: Option<String>,
    pub message: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActivityStatus {
    Applied,
    Rejected,
}

/// One change that came from the phone (ipc `phone:activity`).
#[allow(dead_code)]
#[derive(Clone, Debug, Deserialize)]
pub struct PhoneActivity {
    pub id: String,
    pub kind: String,
    pub status: ActivityStatus,
    pub summary: String,
    pub message: Option<String>,
    pub applied_at: String,
}

/// Open at login (ipc `phone:open-at-login`).
#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginItem {
    pub available: bool,
    pub enabled: bool,
    pub needs_approval: bool,
}

#[allow(dead_code)]
pub enum PhoneCall {
    /// `phone.status()`, `phone.openAtLogin()` and `phone.activity(8)`.
    Load,
    /// `phone.syncNow()`: replies with the new status.
    SyncNow,
    /// `phone.disconnect()`: the new status and `remote_disconnected`.
    Disconnect,
    /// `phone.setOpenAtLogin(enabled)`: replies with the login item.
    SetOpenAtLogin(bool),
}

#[derive(Clone, Default)]
pub struct PhoneReply {
    pub status: Option<PhoneStatus>,
    pub activity: Option<Vec<PhoneActivity>>,
    pub login: Option<LoginItem>,
    /// After a disconnect: whether Content HQ deleted its copy.
    pub remote_disconnected: Option<bool>,
}

/// BACKEND HOOK: the phone-sync service (services/phone.rs). `Load` also
/// keeps `push` to send later `phone:status` changes and, after changes
/// from the phone, the fresh activity list.
fn phone_service(call: PhoneCall, push: Sink<PhoneReply>, cx: &mut App) -> Task<anyhow::Result<PhoneReply>> {
    use crate::services::phone as svc;

    fn status(s: svc::Status) -> PhoneStatus {
        let state = match s.state {
            svc::SyncState::Off => PhoneSyncState::Off,
            svc::SyncState::Syncing => PhoneSyncState::Syncing,
            svc::SyncState::Ok => PhoneSyncState::Ok,
            svc::SyncState::Offline => PhoneSyncState::Offline,
            svc::SyncState::Unavailable => PhoneSyncState::Unavailable,
            svc::SyncState::Revoked => PhoneSyncState::Revoked,
            svc::SyncState::Mismatch => PhoneSyncState::Mismatch,
            svc::SyncState::Error => PhoneSyncState::Error,
        };
        PhoneStatus {
            connected: s.connected,
            state,
            host: s.host,
            profile_name: s.profile_name,
            connected_at: s.connected_at,
            last_sync_at: s.last_sync_at,
            message: s.message,
        }
    }
    fn activity(cx: &App) -> Vec<PhoneActivity> {
        svc::activity(8, cx)
            .into_iter()
            .map(|a| PhoneActivity {
                id: a.id,
                kind: a.kind,
                status: if a.status == "rejected" { ActivityStatus::Rejected } else { ActivityStatus::Applied },
                summary: a.summary,
                message: a.message,
                applied_at: a.applied_at,
            })
            .collect()
    }
    fn login(l: svc::LoginItem) -> LoginItem {
        LoginItem { available: l.available, enabled: l.enabled, needs_approval: l.needs_approval }
    }

    svc::ensure_started(cx);
    let reply = match call {
        PhoneCall::Load => {
            svc::listen(
                "settings",
                Rc::new(move |event: &svc::PhoneEvent, cx: &mut App| match event {
                    svc::PhoneEvent::Status(s) => push(PhoneReply { status: Some(status(s.clone())), ..Default::default() }, cx),
                    svc::PhoneEvent::Applied(_) => {
                        let list = activity(cx);
                        push(PhoneReply { activity: Some(list), ..Default::default() }, cx)
                    }
                    svc::PhoneEvent::ConnectRequest(_) => {}
                }),
            );
            PhoneReply { status: Some(status(svc::status())), activity: Some(activity(cx)), login: Some(login(svc::open_at_login())), remote_disconnected: None }
        }
        PhoneCall::SyncNow => PhoneReply { status: Some(status(svc::sync_now(cx))), ..Default::default() },
        PhoneCall::Disconnect => {
            let task = svc::disconnect(cx);
            return cx.spawn(async move |_| {
                let (s, remote) = task.await;
                Ok(PhoneReply { status: Some(status(s)), remote_disconnected: Some(remote), ..Default::default() })
            });
        }
        PhoneCall::SetOpenAtLogin(enabled) => PhoneReply { login: Some(login(svc::set_open_at_login(enabled))), ..Default::default() },
    };
    Task::ready(Ok(reply))
}

/// `UpdateStatus` (ipc `updater:check` / `updater:cached`) and
/// `UpdateProgress` (the `updater:progress` event), from the updater service.
pub use crate::services::updater::{UpdateProgress, UpdateStatus};

pub enum UpdateCall {
    /// `updater.cached()` and `updater.canInstall()`.
    Load,
    /// `updater.check(force)`: replies with the fresh status.
    Check { force: bool },
    /// `updater.install(url)`: download, replace the app and relaunch. Push
    /// progress through the sink while downloading.
    Install { url: String },
    /// `updater.download(url)`: download and open the disk image.
    Download { url: String },
}

#[derive(Clone, Default)]
pub struct UpdaterReply {
    pub status: Option<UpdateStatus>,
    pub can_install: Option<bool>,
    pub progress: Option<UpdateProgress>,
}

/// BACKEND HOOK: the updater (services/updater.rs, updater.ts behind ipc
/// `updater:*`). Load also follows "Check for Updates…" from the app menu,
/// as useUpdater does here: the menu's check pushes its fresh status in (the
/// sidebar banner shows the toast).
fn update_service(call: UpdateCall, push: Sink<UpdaterReply>, cx: &mut App) -> Task<anyhow::Result<UpdaterReply>> {
    use crate::services::updater::{self, MenuCheck};
    let progress = {
        let push = push.clone();
        move |p: UpdateProgress, cx: &mut App| push(UpdaterReply { progress: Some(p), ..Default::default() }, cx)
    };
    match call {
        UpdateCall::Load => {
            updater::watch_menu_checks(
                "settings",
                move |event, cx| {
                    if let MenuCheck::Finished(Ok(status)) = event {
                        push(UpdaterReply { status: Some(status.clone()), ..Default::default() }, cx);
                    }
                },
                cx,
            );
            Task::ready(Ok(UpdaterReply { status: updater::cached(), can_install: Some(updater::can_install()), progress: None }))
        }
        UpdateCall::Check { force } => {
            let check = updater::check(force, cx);
            cx.spawn(async move |_| check.await.map(|s| UpdaterReply { status: Some(s), ..Default::default() }))
        }
        UpdateCall::Install { url } => {
            let install = updater::install(url, progress, cx);
            cx.spawn(async move |_| install.await.map(|()| UpdaterReply::default()))
        }
        UpdateCall::Download { url } => {
            let download = updater::download(url, progress, cx);
            cx.spawn(async move |_| download.await.map(|_| UpdaterReply::default()))
        }
    }
}

/// Content HQ's "Sign out" (ipc `contentHq:signOut` clears the web view's
/// signed-in session). The native build has no Content HQ web view yet, so
/// there's no session to clear.
fn content_hq_sign_out(_cx: &mut App) -> Task<anyhow::Result<()>> {
    Task::ready(Ok(()))
}
