//! ClientDetail.tsx: one client's money at a glance, their projects and
//! invoices, and edit, new project, merge and delete.

use billable_core::models::{Client, Invoice, Project, ProjectInput};
use gpui::{
    div, prelude::*, px, AnyElement, AppContext, Context, ElementId, Entity, IntoElement, ParentElement, Render,
    SharedString, Styled, Subscription, Window,
};

use super::project_detail::menu::{menu_trigger, MenuState};
use crate::{
    components::{
        client_avatar, confirm_dialog, metric_strip, money,
        project_form::{ProjectFormEvent, ProjectFormModal, ProjectFormValues},
        status_badge, toast_error, toast_success, Button, ClientFormEvent, ClientFormModal, EmptyState, MenuItem,
        MergeClientsModal, MergeEvent, Metric,
    },
    format::{self, format_day, hours_short, relative_days},
    icons::Lucide,
    motion::rise_in,
    shell::{
        app::{navigate, Header, Page},
        route::Route,
    },
    store,
    style::{card, dot, group_head, icon, num_font, r, section_title, separator, text},
    fonts::rounded_family,
    theme::{brighten, parse_hex, theme, Theme},
};

struct Data {
    client: Client,
    clients: Vec<Client>,
    projects: Vec<Project>,
    invoices: Vec<Invoice>,
}

pub struct ClientDetailPage {
    id: i64,
    rev: u64,
    data: Option<Data>,
    edit: Option<Entity<ClientFormModal>>,
    edit_sub: Option<Subscription>,
    project_form: Option<Entity<ProjectFormModal>>,
    project_sub: Option<Subscription>,
    merge: Option<Entity<MergeClientsModal>>,
    merge_sub: Option<Subscription>,
    delete_open: bool,
    menu: MenuState<()>,
}

/// `.head-link` (here without a chevron): orange text, a touch brighter on hover.
fn head_link(t: &Theme, id: &str, label: &str) -> gpui::Stateful<gpui::Div> {
    let accent = t.accent_text;
    div()
        .id(ElementId::Name(id.to_string().into()))
        .flex()
        .items_center()
        .gap(r(0.5))
        .cursor_pointer()
        .text_color(accent)
        .hover(move |s| s.text_color(brighten(accent, 1.1)))
        .child(div().font_family(rounded_family(600)).text_size(px(13.)).child(label.to_string()))
}

/// A section title with its count (`Projects <span class="ml-1 font-normal text-fg-3 num">3</span>`).
fn counted_title(t: &Theme, title: &str, n: usize) -> impl IntoElement {
    div()
        .flex()
        .items_baseline()
        .child(section_title(t, format!("{title} ")))
        .child(div().ml(r(1.)).font(num_font(400)).text_size(px(16.)).text_color(t.fg3).child(n.to_string()))
}

impl ClientDetailPage {
    pub fn new(id: i64, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut page = ClientDetailPage {
            id,
            rev: 0,
            data: None,
            edit: None,
            edit_sub: None,
            project_form: None,
            project_sub: None,
            merge: None,
            merge_sub: None,
            delete_open: false,
            menu: MenuState::default(),
        };
        page.reload(cx);
        page
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        let db = store::db(cx);
        let id = self.id;
        let data = (|| {
            let Some(client) = db.get_client(id)? else { return anyhow::Ok(None) };
            let mut projects = db.list_projects(Some(id))?;
            projects.sort_by(|a, b| {
                b.unbilled_amount.total_cmp(&a.unbilled_amount).then_with(|| {
                    let ka = a.last_activity.as_deref().unwrap_or(&a.created_at);
                    let kb = b.last_activity.as_deref().unwrap_or(&b.created_at);
                    kb.cmp(ka)
                })
            });
            let invoices = db.list_invoices(None)?.into_iter().filter(|i| i.client_id == id).collect();
            Ok(Some(Data { client, clients: db.list_clients()?, projects, invoices }))
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

    fn open_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(d) = self.data.as_ref() else { return };
        let client = d.client.clone();
        let modal = cx.new(|cx| ClientFormModal::new(Some(client), window, cx));
        self.edit_sub = Some(cx.subscribe(&modal, |p, _, _: &ClientFormEvent, cx| {
            p.edit = None;
            cx.notify();
        }));
        self.edit = Some(modal);
        cx.notify();
    }

    fn open_project_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(d) = self.data.as_ref() else { return };
        let clients = d.clients.clone();
        let id = self.id;
        let modal = cx.new(|cx| ProjectFormModal::new(None, clients, Some(id), window, cx));
        self.project_sub = Some(cx.subscribe_in(&modal, window, |p, _, e: &ProjectFormEvent, window, cx| {
            match e {
                ProjectFormEvent::Submit(values) => p.create_project(values.clone(), window, cx),
                ProjectFormEvent::Closed => p.project_form = None,
            }
            cx.notify();
        }));
        self.project_form = Some(modal);
        cx.notify();
    }

    fn create_project(&mut self, v: ProjectFormValues, window: &mut Window, cx: &mut Context<Self>) {
        let input = ProjectInput { client_id: v.client_id, name: v.name, description: Some(v.description), rate: Some(v.rate), status: Some(v.status), color: Some(v.color) };
        match store::write(cx, |db| db.create_project(&input)) {
            Ok(created) => {
                toast_success("Project created", cx);
                self.project_form = None;
                navigate(Route::Project(created.id), window, cx);
            }
            Err(e) => toast_error(e.to_string(), cx),
        }
    }

    fn open_merge(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(d) = self.data.as_ref() else { return };
        let clients = d.clients.clone();
        let id = self.id;
        let modal = cx.new(|cx| MergeClientsModal::new(clients, Some(id), None, cx));
        self.merge_sub = Some(cx.subscribe_in(&modal, window, |p, _, e: &MergeEvent, window, cx| {
            p.merge = None;
            if let MergeEvent::Merged(target) = e {
                navigate(Route::Client(*target), window, cx);
            }
            cx.notify();
        }));
        self.merge = Some(modal);
        cx.notify();
    }

    fn header_actions(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(d) = self.data.as_ref() else { return div().into_any_element() };
        let id = self.id;
        let mut row = div().flex().items_center().gap(r(2.));
        if d.client.unbilled_amount > 0.0 {
            row = row.child(
                Button::secondary("client-invoice-unbilled", "Invoice unbilled time")
                    .on_click(move |_, w, cx| navigate(Route::InvoiceNew { client_id: Some(id), project_id: None, project_ids: vec![] }, w, cx)),
            );
        }
        row = row.child(Button::secondary("client-edit", "Edit").on_click(cx.listener(|p, _, w, cx| p.open_edit(w, cx))));
        let this = cx.weak_entity();
        let (tp, tm, td) = (this.clone(), this.clone(), this);
        let mut items = vec![Some(
            MenuItem::new("New project", move |w, cx| {
                let _ = tp.update(cx, |p, cx| p.open_project_form(w, cx));
            })
            .icon(Lucide::Plus),
        )];
        if d.clients.len() > 1 {
            items.push(Some(
                MenuItem::new("Merge into another client…", move |w, cx| {
                    let _ = tm.update(cx, |p, cx| p.open_merge(w, cx));
                })
                .icon(Lucide::Merge),
            ));
        }
        items.push(None);
        items.push(Some(
            MenuItem::new("Delete client", move |_, cx| {
                let _ = td.update(cx, |p, cx| {
                    p.delete_open = true;
                    cx.notify();
                });
            })
            .icon(Lucide::Trash2)
            .danger(),
        ));
        let open = self.menu.is_open(&());
        row.child(menu_trigger("client-menu", (), open, |p: &mut Self| &mut p.menu, items, cx)).into_any_element()
    }

    fn identity(&self, t: &Theme, c: &Client) -> AnyElement {
        let mut details = div().flex().flex_wrap().items_center().gap_x(r(4.)).gap_y(r(1.)).text_size(px(12.)).line_height(px(16.)).text_color(t.fg3);
        if !c.company.is_empty() {
            details = details.child(c.company.clone());
        }
        if !c.email.is_empty() {
            details = details.child(div().flex().items_center().gap(r(1.5)).child(icon(Lucide::Mail).size(r(3.5)).text_color(t.fg3)).child(c.email.clone()));
        }
        if !c.address.is_empty() {
            let one_line = c.address.split('\n').map(|l| l.trim().trim_end_matches(',').to_string()).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(", ");
            details = details.child(
                div()
                    .flex()
                    .items_center()
                    .gap(r(1.5))
                    .min_w_0()
                    .max_w(px(420.))
                    .child(icon(Lucide::MapPin).size(r(3.5)).flex_none().text_color(t.fg3))
                    .child(div().min_w_0().truncate().child(one_line)),
            );
        }
        div()
            .flex()
            .items_center()
            .gap(r(4.))
            .mb(r(5.))
            .child(client_avatar(&c.name, 40.0))
            .child(div().min_w_0().flex().flex_col().child(text(c.name.clone(), 17.0, 600, t.fg).line_height(px(24.))).child(details))
            .into_any_element()
    }

    fn metrics(&self, t: &Theme, d: &Data) -> AnyElement {
        let c = &d.client;
        let paid_count = d.invoices.iter().filter(|i| i.status == "paid").count();
        let rate = div()
            .flex()
            .items_baseline()
            .child(money(c.default_rate, 22.0, t.fg))
            .child(text("/hr", 13.0, 400, t.fg3).line_height(px(18.)));
        metric_strip(vec![
            Metric::new("client-m-unbilled", "Unbilled", money(c.unbilled_amount, 22.0, t.fg)).sub(if c.unbilled_amount > 0.0 {
                format!("{} since {}", hours_short(c.unbilled_hours), format_day(c.oldest_unbilled.as_deref().unwrap_or("")))
            } else {
                "Nothing waiting".into()
            }),
            Metric::new("client-m-outstanding", "Outstanding", money(c.outstanding_amount, 22.0, if c.overdue_amount > 0.0 { t.red } else { t.fg })).sub(
                if c.overdue_amount > 0.0 {
                    format!("{} overdue", format::money(c.overdue_amount))
                } else if c.outstanding_amount > 0.0 {
                    "Sent, not yet paid".into()
                } else {
                    "Nothing owed".into()
                },
            ),
            Metric::new("client-m-paid", "Paid", money(c.paid_amount, 22.0, t.fg)).sub(format!("{paid_count} invoice{}", if paid_count == 1 { "" } else { "s" })),
            Metric::new("client-m-rate", "Rate", rate).sub(c.currency.clone()),
        ])
        .mb(r(5.))
        .into_any_element()
    }

    fn projects_section(&self, t: &Theme, d: &Data, cx: &mut Context<Self>) -> AnyElement {
        let new_project = head_link(t, "client-new-project", "New project").on_click(cx.listener(|p, _, w, cx| p.open_project_form(w, cx)));
        let mut body = card(t).overflow_hidden().flex().flex_col();
        if d.projects.is_empty() {
            body = body.child(EmptyState::new("client-projects-empty", "No projects yet", "Create a project to start tracking time for this client.").compact());
        }
        let hover = t.fg.opacity(0.025);
        for (i, p) in d.projects.iter().enumerate() {
            let id = p.id;
            let sub = match p.last_activity.as_deref() {
                Some(last) => format!("{} tracked · {}", hours_short(p.total_hours), relative_days(last)),
                None => format!("{} tracked", hours_short(p.total_hours)),
            };
            let mut row = div()
                .id(ElementId::Name(format!("client-project-{id}").into()))
                .relative()
                .w_full()
                .flex()
                .items_center()
                .gap(r(3.))
                .px(r(4.))
                .h(px(44.))
                .cursor_pointer()
                .hover(move |s| s.bg(hover))
                .on_click(move |_, w, cx| navigate(Route::Project(id), w, cx))
                .child(dot(parse_hex(&p.color), r(2.)))
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .child(text(p.name.clone(), 13.0, 500, t.fg).line_height(px(19.5)).truncate())
                        .child(text(sub, 12.0, 500, t.fg3).line_height(px(16.))),
                );
            if p.unbilled_amount > 0.0 {
                row = row.child(
                    div()
                        .flex()
                        .flex_col()
                        .items_end()
                        .child(div().font(num_font(500)).text_size(px(13.)).line_height(px(19.5)).text_color(t.amber).child(format::money(p.unbilled_amount)))
                        .child(text("unbilled", 11.0, 500, t.fg3).line_height(px(14.))),
                );
            }
            row = row.child(status_badge(t, &p.status));
            if i + 1 < d.projects.len() {
                row = row.child(separator(t, 36.0));
            }
            body = body.child(row);
        }
        div().min_w_0().flex_1().flex().flex_col().child(group_head(counted_title(t, "Projects", d.projects.len()), new_project)).child(body).into_any_element()
    }

    fn invoices_section(&self, t: &Theme, d: &Data) -> AnyElement {
        let id = self.id;
        let mut body = card(t).overflow_hidden().flex().flex_col();
        if d.invoices.is_empty() {
            body = body.child(EmptyState::new("client-invoices-empty", "No invoices yet", "Invoices you create for this client show up here.").compact());
        }
        let hover = t.fg.opacity(0.025);
        let (fg4, fg3) = (t.fg4, t.fg3);
        for (i, inv) in d.invoices.iter().enumerate() {
            let inv_id = inv.id;
            let group: SharedString = format!("client-inv-{inv_id}").into();
            let mut sub = format_day(&inv.issue_date);
            if inv.status == "paid" && inv.payment_date.as_deref().is_some_and(|p| !p.is_empty()) {
                sub.push_str(&format!(" · paid {}", format_day(inv.payment_date.as_deref().unwrap_or(""))));
            } else if inv.status != "draft" {
                sub.push_str(&format!(" · due {}", format_day(&inv.due_date)));
            }
            let mut row = div()
                .id(ElementId::Name(format!("client-inv-row-{inv_id}").into()))
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
                .on_click(move |_, w, cx| navigate(Route::Invoice(inv_id), w, cx))
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .child(text(inv.invoice_number.clone(), 13.0, 500, t.fg).line_height(px(19.5)))
                        .child(text(sub, 12.0, 500, t.fg3).line_height(px(16.)).truncate()),
                )
                .child(div().font(num_font(500)).text_size(px(13.)).line_height(px(19.5)).text_color(t.fg).whitespace_nowrap().child(format::money(inv.total)))
                .child(status_badge(t, &inv.status))
                .child(icon(Lucide::ChevronRight).size(r(3.5)).flex_none().text_color(fg4).group_hover(group, move |s| s.text_color(fg3)));
            if i + 1 < d.invoices.len() {
                row = row.child(separator(t, 16.0));
            }
            body = body.child(row);
        }
        let new_invoice = head_link(t, "client-new-invoice", "New invoice")
            .on_click(move |_, w, cx| navigate(Route::InvoiceNew { client_id: Some(id), project_id: None, project_ids: vec![] }, w, cx));
        div().min_w_0().flex_1().flex().flex_col().child(group_head(counted_title(t, "Invoices", d.invoices.len()), new_invoice)).child(body).into_any_element()
    }
}

impl Page for ClientDetailPage {
    fn header(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> Header {
        self.ensure_fresh(cx);
        let Some(d) = self.data.as_ref() else { return Header::new("").back("Clients", Route::Clients { action_new: false }) };
        Header::new(d.client.name.clone()).back("Clients", Route::Clients { action_new: false }).actions(self.header_actions(cx))
    }
}

impl Render for ClientDetailPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.ensure_fresh(cx);
        let t = theme(cx);
        let Some(d) = self.data.as_ref() else { return div().into_any_element() };
        let identity = self.identity(&t, &d.client);
        let metrics = self.metrics(&t, d);
        let invoices = self.invoices_section(&t, d);
        let projects = self.projects_section(&t, d, cx);

        let mut page = div()
            .w_full()
            .max_w(px(1160.))
            .mx_auto()
            .px(r(7.))
            .pt(r(6.))
            .pb(r(14.))
            .flex()
            .flex_col()
            .child(rise_in("client-rise-0", 0, identity))
            .child(rise_in("client-rise-1", 1, metrics))
            .child(rise_in("client-rise-2", 2, div().flex().items_start().gap(r(6.)).child(projects).child(invoices)));

        page = page.children(self.edit.clone()).children(self.project_form.clone()).children(self.merge.clone());
        if self.delete_open {
            let name = self.data.as_ref().map(|d| d.client.name.clone()).unwrap_or_default();
            let this = cx.weak_entity();
            let id = self.id;
            page = page.child(confirm_dialog(
                "delete-client",
                format!("Delete {name}?"),
                "This permanently deletes the client with all of their projects, time entries, and invoices. If they were added twice, merge them instead.",
                "Delete client",
                true,
                move |w, cx| match store::write(cx, |db| db.delete_client(id)) {
                    Ok(_) => {
                        toast_success("Client deleted", cx);
                        navigate(Route::Clients { action_new: false }, w, cx);
                    }
                    Err(e) => toast_error(e.to_string(), cx),
                },
                move |_, cx| {
                    let _ = this.update(cx, |p, cx| {
                        p.delete_open = false;
                        cx.notify();
                    });
                },
                cx,
            ));
        }
        page.into_any_element()
    }
}
