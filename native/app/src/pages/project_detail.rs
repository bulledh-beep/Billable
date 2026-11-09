//! ProjectDetail.tsx (+ CloseOutModal.tsx): one project's money at a glance,
//! its time entries with a billed/unbilled filter, the invoices that carry
//! its time, and edit, status changes (with the close-out check when there's
//! unbilled time left), add time, and delete.

mod close_out;
mod entry_row;
pub(crate) mod menu;

use std::collections::HashSet;

use billable_core::models::{Client, EntryRow, Invoice, Project, ProjectPatch, TimeEntryPatch};
use chrono::Duration as Span;
use gpui::{
    div, prelude::*, px, AnyElement, App, AppContext, Context, ElementId, Entity, FontWeight, IntoElement, ParentElement,
    Render, SharedString, StyledText, Styled, Subscription, TextRun, WeakEntity, Window,
};

use self::{
    close_out::{close_out_modal, CloseOutChoice},
    menu::{menu_trigger, MenuState},
};
use crate::{
    components::{
        button::Variant,
        confirm_dialog,
        entry_editor::{EntryEditorEvent, EntryEditorModal, EntryEditorMode},
        metric_strip, money,
        project_form::{ProjectFormEvent, ProjectFormModal, ProjectFormValues},
        segmented, status_badge, toast_error, toast_success, Button, EmptyState, MenuItem, Metric, Modal, ModalSize, Mood,
        Segment,
    },
    fonts::rounded_family,
    format::{self, duration_short, format_day, hours_short, relative_days},
    icons::Lucide,
    motion::rise_in,
    shell::{
        app::{navigate, Header, Page},
        route::Route,
    },
    store,
    style::{card, dot, figures, group_head, icon, num_font, r, section_title, separator, text},
    theme::{parse_hex, theme, Theme},
    timer,
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum EntryFilter {
    All,
    Unbilled,
    Billed,
}

struct Data {
    project: Project,
    entries: Vec<EntryRow>,
    invoices: Vec<Invoice>,
    clients: Vec<Client>,
}

pub struct ProjectDetailPage {
    id: i64,
    rev: u64,
    data: Option<Data>,
    filter: EntryFilter,
    form: Option<Entity<ProjectFormModal>>,
    form_sub: Option<Subscription>,
    show_delete: bool,
    /// The status change waiting on the close-out choice.
    closing: Option<ProjectFormValues>,
    editor: Option<Entity<EntryEditorModal>>,
    editor_sub: Option<Subscription>,
    deleting: Option<EntryRow>,
    header_menu: MenuState<()>,
    entry_menu: MenuState<i64>,
}

/// Locked entries sit on a sent, paid or flagged invoice (EntryEditor's isLockedEntry).
pub(super) fn is_locked(e: &EntryRow) -> bool {
    matches!(e.billing_state.as_str(), "sent" | "overdue" | "paid" | "invoiced")
}

fn is_billed(e: &EntryRow) -> bool {
    e.entry.invoice_id.is_some_and(|i| i != 0) || e.billing_state == "invoiced"
}

fn patch(v: &ProjectFormValues) -> ProjectPatch {
    ProjectPatch {
        client_id: Some(v.client_id),
        name: Some(v.name.clone()),
        description: Some(v.description.clone()),
        rate: Some(v.rate),
        status: Some(v.status.clone()),
        color: Some(v.color.clone()),
    }
}

impl ProjectDetailPage {
    pub fn new(id: i64, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut page = ProjectDetailPage {
            id,
            rev: 0,
            data: None,
            filter: EntryFilter::All,
            form: None,
            form_sub: None,
            show_delete: false,
            closing: None,
            editor: None,
            editor_sub: None,
            deleting: None,
            header_menu: MenuState::default(),
            entry_menu: MenuState::default(),
        };
        page.reload(cx);
        page
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        let db = store::db(cx);
        let id = self.id;
        let data = (|| {
            let Some(project) = db.get_project(id)? else { return anyhow::Ok(None) };
            Ok(Some(Data { project, entries: db.list_time_entries(Some(id))?, invoices: db.list_invoices(None)?, clients: db.list_clients()? }))
        })();
        self.data = data.ok().flatten();
        self.rev = store::rev(cx);
    }

    /// The toolbar is drawn before the body, so both check for new data.
    fn ensure_fresh(&mut self, cx: &mut Context<Self>) {
        if self.rev != store::rev(cx) {
            self.reload(cx);
        }
    }

    /// Finished entries (the running timer's entry has no end yet).
    fn done(&self) -> Vec<&EntryRow> {
        self.data.as_ref().map(|d| d.entries.iter().filter(|e| e.entry.end_time.is_some()).collect()).unwrap_or_default()
    }

    fn unbilled_ids(&self) -> Vec<i64> {
        self.done().into_iter().filter(|e| e.billing_state == "unbilled").map(|e| e.entry.id).collect()
    }

    // ---- Project changes (with the close-out check) ----

    fn open_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(d) = self.data.as_ref() else { return };
        let (project, clients) = (d.project.clone(), d.clients.clone());
        let modal = cx.new(|cx| ProjectFormModal::new(Some(project), clients, None, window, cx));
        self.form_sub = Some(cx.subscribe(&modal, |p, _, e: &ProjectFormEvent, cx| match e {
            ProjectFormEvent::Submit(values) => p.handle_submit(values.clone(), cx),
            ProjectFormEvent::Closed => {
                p.form = None;
                cx.notify();
            }
        }));
        self.form = Some(modal);
        cx.notify();
    }

    fn apply_project(&mut self, values: &ProjectFormValues, cx: &mut Context<Self>) {
        let id = self.id;
        let patch = patch(values);
        match store::write(cx, |db| db.update_project(id, &patch)) {
            Ok(_) => toast_success("Project updated", cx),
            Err(e) => toast_error(e.to_string(), cx),
        }
    }

    fn handle_submit(&mut self, values: ProjectFormValues, cx: &mut Context<Self>) {
        self.form = None;
        cx.notify();
        let Some(d) = self.data.as_ref() else { return };
        let closing_now = (values.status == "complete" || values.status == "archived") && values.status != d.project.status;
        if closing_now && !self.unbilled_ids().is_empty() {
            self.closing = Some(values);
            return;
        }
        self.apply_project(&values, cx);
    }

    fn change_status(&mut self, status: &str, cx: &mut Context<Self>) {
        let Some(d) = self.data.as_ref() else { return };
        let p = &d.project;
        let values = ProjectFormValues {
            client_id: p.client_id,
            name: p.name.clone(),
            description: p.description.clone(),
            rate: p.rate,
            status: status.to_string(),
            color: p.color.clone(),
        };
        self.handle_submit(values, cx);
    }

    fn handle_close_out(&mut self, choice: CloseOutChoice, window: &mut Window, cx: &mut Context<Self>) {
        let Some(values) = self.closing.take() else { return };
        cx.notify();
        let id = self.id;
        let ids = self.unbilled_ids();
        let patch = patch(&values);
        if let Err(e) = store::write(cx, |db| db.update_project(id, &patch)) {
            toast_error(e.to_string(), cx);
            return;
        }
        match choice {
            CloseOutChoice::DontBill => match store::write(cx, |db| db.set_entries_billable(&ids, false)) {
                Ok(_) => toast_success(format!("Project marked {}. Its leftover time won't be billed.", values.status), cx),
                Err(e) => toast_error(e.to_string(), cx),
            },
            CloseOutChoice::Invoice => navigate(Route::InvoiceNew { client_id: None, project_id: Some(id), project_ids: vec![] }, window, cx),
            CloseOutChoice::Keep => toast_success(format!("Project marked {}. Its unbilled time stays on your Billing page.", values.status), cx),
        }
    }

    // ---- Entries ----

    fn open_editor(&mut self, entry: Option<EntryRow>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(d) = self.data.as_ref() else { return };
        let project = d.project.clone();
        let mode = if entry.is_some() { EntryEditorMode::Edit } else { EntryEditorMode::Add };
        let pid = project.id;
        let modal = cx.new(|cx| EntryEditorModal::new(mode, entry, vec![project], None, Some(pid), window, cx));
        self.editor_sub = Some(cx.subscribe(&modal, |p, _, _: &EntryEditorEvent, cx| {
            p.editor = None;
            cx.notify();
        }));
        self.editor = Some(modal);
        cx.notify();
    }

    fn add_time(&mut self, entry: &EntryRow, minutes: f64, cx: &mut Context<Self>) {
        let duration = entry.entry.duration_minutes + minutes;
        let Some(start) = format::parse_time(&entry.entry.start_time) else { return };
        let end = start + Span::milliseconds((duration * 60_000.0).round() as i64);
        let patch = TimeEntryPatch { duration_minutes: Some(duration), end_time: Some(Some(format::iso_utc(end))), ..Default::default() };
        let id = entry.entry.id;
        match store::write(cx, |db| db.update_time_entry(id, &patch)) {
            Ok(_) => toast_success(format!("Added {}", duration_short(minutes)), cx),
            Err(e) => toast_error(e.to_string(), cx),
        }
    }

    fn set_billable(&mut self, id: i64, billable: bool, cx: &mut Context<Self>) {
        if let Err(e) = store::write(cx, |db| db.set_entries_billable(&[id], billable)) {
            toast_error(e.to_string(), cx);
        }
    }

    // ---- Pieces ----

    fn header_actions(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(d) = self.data.as_ref() else { return div().into_any_element() };
        let p = &d.project;
        let id = p.id;
        let status = p.status.clone();
        let mut row = div().flex().items_center().gap(r(2.));
        if p.unbilled_amount > 0.0 {
            row = row.child(
                Button::secondary("project-invoice", "Create invoice")
                    .tooltip("Bill this project's unbilled time")
                    .on_click(move |_, w, cx| navigate(Route::InvoiceNew { client_id: None, project_id: Some(id), project_ids: vec![] }, w, cx)),
            );
        }
        row = row.child(Button::secondary("project-edit", "Edit").on_click(cx.listener(|p, _, w, cx| p.open_form(w, cx))));

        let this = cx.weak_entity();
        let status_item = |label: &'static str, glyph: Lucide, to: &'static str, this: &WeakEntity<Self>| {
            let this = this.clone();
            Some(
                MenuItem::new(label, move |_, cx| {
                    let _ = this.update(cx, |p, cx| p.change_status(to, cx));
                })
                .icon(glyph),
            )
        };
        let mut items = Vec::new();
        if status != "complete" {
            items.push(status_item("Mark complete", Lucide::CheckCircle2, "complete", &this));
        }
        if status != "active" {
            items.push(status_item("Mark active", Lucide::Play, "active", &this));
        }
        if status != "archived" {
            items.push(status_item("Archive", Lucide::Archive, "archived", &this));
        }
        items.push(None);
        items.push(Some(
            MenuItem::new("Delete project", move |_, cx| {
                let _ = this.update(cx, |p, cx| {
                    p.show_delete = true;
                    cx.notify();
                });
            })
            .icon(Lucide::Trash2)
            .danger(),
        ));
        let open = self.header_menu.is_open(&());
        row.child(menu_trigger("project-menu", (), open, |p: &mut Self| &mut p.header_menu, items, cx)).into_any_element()
    }

    fn identity(&self, t: &Theme, d: &Data) -> AnyElement {
        let p = &d.project;
        let client_id = p.client_id;
        let fg = t.fg;
        let sep = || text(" · ", 13.0, 500, t.fg4).line_height(px(18.));
        let mut line = div()
            .flex()
            .flex_wrap()
            .items_baseline()
            .text_size(px(13.))
            .line_height(px(18.))
            .text_color(t.fg2)
            .child(
                div()
                    .id("project-client-link")
                    .cursor_pointer()
                    .font_family(rounded_family(500))
                    .text_color(fg)
                    .hover(|s| s.underline())
                    .on_click(move |_, w, cx| navigate(Route::Client(client_id), w, cx))
                    .child(p.client_name.clone().unwrap_or_default()),
            )
            .child(sep())
            .child(div().font(num_font(500)).child(format::money(p.rate)))
            .child("/hr");
        if let Some(last) = p.last_activity.as_deref() {
            line = line.child(sep()).child(format!("last tracked {}", relative_days(last)));
        }
        let mut col = div().min_w_0().flex().flex_col().child(line);
        if !p.description.is_empty() {
            col = col.child(text(p.description.clone(), 13.0, 500, t.fg3).mt(r(1.)).max_w(r(168.)).line_height(px(18.)));
        }
        div()
            .flex()
            .items_start()
            .gap(r(3.))
            .mb(r(5.))
            .child(dot(parse_hex(&p.color), r(3.)).mt(r(1.5)))
            .child(col)
            .into_any_element()
    }

    fn metrics(&self, t: &Theme, d: &Data, project_invoices: &[&Invoice]) -> AnyElement {
        let p = &d.project;
        let done = self.done().len();
        let paid = project_invoices.iter().filter(|i| i.status == "paid").count();
        metric_strip(vec![
            Metric::new("project-m-tracked", "Tracked", figures(hours_short(p.total_hours), 22.0, t.fg)).sub(format!("{done} {}", if done == 1 { "entry" } else { "entries" })),
            Metric::new("project-m-unbilled", "Unbilled", money(p.unbilled_amount, 22.0, t.fg)).sub(if p.unbilled_amount > 0.0 {
                format!("{} since {}", hours_short(p.unbilled_hours), format_day(p.oldest_unbilled.as_deref().unwrap_or("")))
            } else {
                "All time is billed".into()
            }),
            Metric::new("project-m-invoiced", "Invoiced", money(p.invoiced_amount, 22.0, t.fg)).sub("Before tax"),
            Metric::new("project-m-paid", "Paid", money(p.paid_amount, 22.0, t.fg)).sub(format!("{paid} paid invoice{}", if paid == 1 { "" } else { "s" })),
        ])
        .mb(r(5.))
        .into_any_element()
    }

    fn entries_section(&self, t: &Theme, timing_this: bool, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(d) = self.data.as_ref() else { return div().into_any_element() };
        let active = d.project.status == "active";
        let pid = d.project.id;
        let done: Vec<EntryRow> = self.done().into_iter().cloned().collect();
        let filter = self.filter;
        let visible: Vec<&EntryRow> = done
            .iter()
            .filter(|e| match filter {
                EntryFilter::All => true,
                EntryFilter::Unbilled => e.billing_state == "unbilled",
                EntryFilter::Billed => is_billed(e),
            })
            .collect();
        let counts = (done.len(), done.iter().filter(|e| e.billing_state == "unbilled").count(), done.iter().filter(|e| is_billed(e)).count());

        let this = cx.weak_entity();
        let mut controls = div()
            .flex()
            .items_center()
            .gap(r(2.))
            .child(segmented(
                "project-entry-filter",
                &filter,
                vec![
                    Segment::new(EntryFilter::All, "All").count(counts.0),
                    Segment::new(EntryFilter::Unbilled, "Unbilled").count(counts.1),
                    Segment::new(EntryFilter::Billed, "Invoiced").count(counts.2),
                ],
                move |v, _, cx| {
                    let _ = this.update(cx, |p, cx| {
                        p.filter = v;
                        cx.notify();
                    });
                },
                cx,
            ))
            .child(Button::secondary("project-add-time", "Add time").small().on_click(cx.listener(|p, _, w, cx| p.open_editor(None, w, cx))));
        if !timing_this && active {
            controls = controls.child(
                Button::secondary("project-start-timer", "Start timer")
                    .small()
                    .icon(Lucide::Play)
                    .filled()
                    .icon_size(8.125)
                    .icon_color(t.accent)
                    .on_click(move |_, _, cx| timer::start(pid, None, cx)),
            );
        }

        let mut body = card(t).overflow_hidden().flex().flex_col();
        if visible.is_empty() {
            let (title, desc) = if done.is_empty() { ("No time on this project yet", "Start a timer or add time by hand.") } else { ("Nothing here", "Try another filter.") };
            body = body.child(EmptyState::new("project-entries-empty", title, desc).compact().mood(Mood::Sleepy));
        } else {
            let n = visible.len();
            let rows: Vec<AnyElement> = visible.into_iter().enumerate().map(|(i, e)| self.entry_row(t, e, i + 1 == n, window, cx)).collect();
            body = body.children(rows);
        }
        div().min_w_0().flex_1().flex().flex_col().child(group_head(section_title(t, "Time entries"), controls)).child(body).into_any_element()
    }

    fn invoices_section(&self, t: &Theme, invoices: &[&Invoice]) -> AnyElement {
        let title = div()
            .flex()
            .items_baseline()
            .child(section_title(t, "Invoices "))
            .child(div().ml(r(1.)).font(num_font(400)).text_size(px(16.)).text_color(t.fg3).child(invoices.len().to_string()));
        let mut body = card(t).overflow_hidden().flex().flex_col();
        if invoices.is_empty() {
            body = body.child(div().px(r(4.)).py(r(5.)).flex().justify_center().child(text("Not invoiced yet", 13.0, 500, t.fg3)));
        }
        let hover = t.fg.opacity(0.025);
        let (fg4, fg3) = (t.fg4, t.fg3);
        for (i, inv) in invoices.iter().enumerate() {
            let id = inv.id;
            let group: SharedString = format!("project-inv-{id}").into();
            let mut row = div()
                .id(ElementId::Name(format!("project-inv-row-{id}").into()))
                .group(group.clone())
                .relative()
                .w_full()
                .flex()
                .items_center()
                .gap(r(3.))
                .px(r(4.))
                .h(px(44.))
                .cursor_pointer()
                .hover(move |s| s.bg(hover))
                .on_click(move |_, w, cx| navigate(Route::Invoice(id), w, cx))
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .child(text(inv.invoice_number.clone(), 13.0, 500, t.fg).line_height(px(19.5)))
                        .child(text(format_day(&inv.issue_date), 12.0, 500, t.fg3).line_height(px(16.))),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_end()
                        .gap(r(0.5))
                        .child(div().font(num_font(500)).text_size(px(13.)).line_height(px(19.5)).text_color(t.fg).child(format::money(inv.total)))
                        .child(status_badge(t, &inv.status)),
                )
                .child(icon(Lucide::ChevronRight).size(r(3.5)).flex_none().text_color(fg4).group_hover(group, move |s| s.text_color(fg3)));
            if i + 1 < invoices.len() {
                row = row.child(separator(t, 16.0));
            }
            body = body.child(row);
        }
        div().w(px(280.)).flex_none().flex().flex_col().child(group_head(title, div())).child(body).into_any_element()
    }
}

/// ConfirmDialog with a message that can't be plain text (a bold invoice number).
fn confirm_rich(
    id: &str,
    title: impl Into<SharedString>,
    message: AnyElement,
    confirm_text: &str,
    on_confirm: impl Fn(&mut Window, &mut App) + 'static,
    on_close: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let on_close = std::rc::Rc::new(on_close);
    let (a, b, c) = (on_close.clone(), on_close.clone(), on_close);
    Modal::new(SharedString::from(id.to_string()), title, move |w, cx| a(w, cx))
        .size(ModalSize::Sm)
        .child(message)
        .footer(Button::secondary(SharedString::from(format!("{id}-cancel")), "Cancel").on_click(move |_, w, cx| b(w, cx)))
        .footer(Button::new(SharedString::from(format!("{id}-confirm")), Variant::Destructive).label(confirm_text.to_string()).on_click(move |_, w, cx| {
            on_confirm(w, cx);
            c(w, cx);
        }))
}

/// "It's on **INV-0042**. The invoice keeps its lines, …" with the number in bold.
fn delete_entry_message(t: &Theme, number: &str) -> AnyElement {
    let (before, after) = ("It's on ", ". The invoice keeps its lines, but this time will no longer be linked to it.");
    let s = format!("{before}{number}{after}");
    let run = |len: usize, weight: u16, color| TextRun {
        len,
        font: gpui::Font { weight: FontWeight(weight as f32), ..gpui::font(rounded_family(weight)) },
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let mut runs = vec![run(before.len(), 500, t.fg2)];
    if !number.is_empty() {
        runs.push(run(number.len(), 700, t.fg));
    }
    runs.push(run(after.len(), 500, t.fg2));
    div().text_size(px(13.)).line_height(px(20.)).child(StyledText::new(s).with_runs(runs)).into_any_element()
}

impl Page for ProjectDetailPage {
    fn header(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> Header {
        self.ensure_fresh(cx);
        let t = theme(cx);
        let Some(d) = self.data.as_ref() else { return Header::new("").back("Projects", Route::Projects { action_new: false }) };
        let mut h = Header::new(d.project.name.clone()).back("Projects", Route::Projects { action_new: false }).meta(status_badge(&t, &d.project.status));
        if let Some(c) = d.project.client_name.clone().filter(|c| !c.is_empty()) {
            h = h.subtitle(c);
        }
        h.actions(self.header_actions(cx))
    }
}

impl Render for ProjectDetailPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.ensure_fresh(cx);
        let t = theme(cx);
        let Some(d) = self.data.as_ref() else { return div().into_any_element() };
        let s = store::store(cx);
        let timing_this = (s.timer_running() || s.timer_paused()) && s.timer_entry().map(|e| e.entry.project_id) == Some(d.project.id);

        // Invoices that carry this project's time
        let ids: HashSet<i64> = self.done().iter().filter_map(|e| e.entry.invoice_id).collect();
        let project_invoices: Vec<&Invoice> = d.invoices.iter().filter(|i| ids.contains(&i.id) || i.project_id == Some(self.id)).collect();

        let identity = self.identity(&t, d);
        let metrics = self.metrics(&t, d, &project_invoices);
        let invoices = self.invoices_section(&t, &project_invoices);
        let entries = self.entries_section(&t, timing_this, window, cx);

        let mut page = div()
            .w_full()
            .max_w(px(1160.))
            .mx_auto()
            .px(r(7.))
            .pt(r(6.))
            .pb(r(14.))
            .flex()
            .flex_col()
            .child(rise_in("project-rise-0", 0, identity))
            .child(rise_in("project-rise-1", 1, metrics))
            .child(rise_in("project-rise-2", 2, div().flex().items_start().gap(r(6.)).child(entries).child(invoices)));

        // Dialogs
        let Some(d) = self.data.as_ref() else { return page.into_any_element() };
        page = page.children(self.form.clone());
        if let Some(values) = self.closing.as_ref() {
            let (a, b) = (cx.weak_entity(), cx.weak_entity());
            page = page.child(close_out_modal(
                &d.project,
                &values.status,
                move |choice, w, cx| {
                    let _ = a.update(cx, |p, cx| p.handle_close_out(choice, w, cx));
                },
                move |_, cx| {
                    let _ = b.update(cx, |p, cx| {
                        p.closing = None;
                        cx.notify();
                    });
                },
                cx,
            ));
        }
        page = page.children(self.editor.clone());
        if self.show_delete {
            let this = cx.weak_entity();
            let id = self.id;
            page = page.child(confirm_dialog(
                "delete-project",
                format!("Delete {}?", d.project.name),
                "This permanently deletes the project and all of its time entries. Invoices stay, but lose their link to this project.",
                "Delete project",
                true,
                move |w, cx| match store::write(cx, |db| db.delete_project(id)) {
                    Ok(_) => {
                        toast_success("Project deleted", cx);
                        navigate(Route::Projects { action_new: false }, w, cx);
                    }
                    Err(e) => toast_error(e.to_string(), cx),
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
        if let Some(entry) = self.deleting.clone() {
            let this = cx.weak_entity();
            let id = entry.entry.id;
            let message = match (entry.entry.invoice_id, entry.invoice_number.as_deref()) {
                (Some(_), Some(n)) => delete_entry_message(&t, n),
                (Some(_), None) => delete_entry_message(&t, ""),
                _ => text("This removes the entry for good.", 13.0, 500, t.fg2).line_height(px(20.)).into_any_element(),
            };
            page = page.child(confirm_rich(
                "delete-entry",
                "Delete this time entry?",
                message,
                "Delete",
                move |_, cx| match store::write(cx, |db| db.delete_time_entry(id)) {
                    Ok(_) => toast_success("Entry deleted", cx),
                    Err(e) => toast_error(e.to_string(), cx),
                },
                move |_, cx| {
                    let _ = this.update(cx, |p, cx| {
                        p.deleting = None;
                        cx.notify();
                    });
                },
            ));
        }
        page.into_any_element()
    }
}
