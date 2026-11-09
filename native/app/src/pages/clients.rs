//! Clients.tsx (+ ClientForm.tsx, MergeClientsModal.tsx): the client list
//! with money at a glance, duplicate hints, and the new/edit, merge and
//! delete dialogs.

use std::rc::Rc;

use billable_core::models::{AttentionItem, Client};
use gpui::{
    div, prelude::*, px, App, AppContext, Context, ElementId, Entity, IntoElement, ParentElement, Render, SharedString,
    Styled, Subscription, WeakEntity, Window,
};
use gpui_component::input::{InputEvent, InputState};

use crate::{
    components::{
        client_avatar, confirm_dialog,
        fields::search_field,
        menu_list, popover, ClientFormEvent, ClientFormModal, MergeClientsModal, MergeEvent,
        table::{dash_cell, measure, table, text_cell, Cell, Column, Row},
        toast_error, toast_success, Align, Button, EmptyState, MenuItem, Modal,
    },
    format::{money, relative_days},
    icons::Lucide,
    shell::{
        app::{navigate, Header, Page},
        route::Route,
    },
    store,
    style::{card, icon, page_width, r, text},
    theme::theme,
};

pub struct ClientsPage {
    rev: u64,
    clients: Vec<Client>,
    duplicates: Vec<AttentionItem>,
    search: Entity<InputState>,
    form: Option<Entity<ClientFormModal>>,
    form_sub: Option<Subscription>,
    delete: Option<Client>,
    merge: Option<Entity<MergeClientsModal>>,
    merge_sub: Option<Subscription>,
    menu: Option<i64>,
    _subs: Vec<Subscription>,
}

impl ClientsPage {
    pub fn new(action_new: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search clients"));
        let subs = vec![cx.subscribe(&search, |_, _, e: &InputEvent, cx| {
            if matches!(e, InputEvent::Change) {
                cx.notify();
            }
        })];
        let mut page = ClientsPage { rev: 0, clients: vec![], duplicates: vec![], search, form: None, form_sub: None, delete: None, merge: None, merge_sub: None, menu: None, _subs: subs };
        page.reload(cx);
        // ⌘N from the menu opens the form straight away
        if action_new {
            page.open_form(None, window, cx);
        }
        page
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        let db = store::db(cx);
        self.clients = db.list_clients().unwrap_or_default();
        self.duplicates = db.get_billing_overview().map(|o| o.attention.into_iter().filter(|a| a.kind == "duplicate_clients").collect()).unwrap_or_default();
        self.rev = store::rev(cx);
    }

    fn open_form(&mut self, client: Option<Client>, window: &mut Window, cx: &mut Context<Self>) {
        let modal = cx.new(|cx| ClientFormModal::new(client, window, cx));
        self.form_sub = Some(cx.subscribe_in(&modal, window, |p, _, e: &ClientFormEvent, window, cx| {
            p.form = None;
            if let ClientFormEvent::Saved(created, true) = e {
                navigate(Route::Client(created.id), window, cx);
            }
            cx.notify();
        }));
        self.form = Some(modal);
        cx.notify();
    }

    fn open_merge(&mut self, source: Option<i64>, target: Option<i64>, cx: &mut Context<Self>) {
        let clients = self.clients.clone();
        let modal = cx.new(|cx| MergeClientsModal::new(clients, source, target, cx));
        self.merge_sub = Some(cx.subscribe(&modal, |p, _, _: &MergeEvent, cx| {
            p.merge = None;
            cx.notify();
        }));
        self.merge = Some(modal);
        cx.notify();
    }
}

impl Page for ClientsPage {
    fn header(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> Header {
        Header::new("Clients").actions(Button::secondary("clients-new", "New client").on_click(cx.listener(|p, _, w, cx| p.open_form(None, w, cx))))
    }
}

impl Render for ClientsPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.rev != store::rev(cx) {
            self.reload(cx);
        }
        let t = theme(cx);
        let mut page = div().w_full().max_w(px(1160.)).mx_auto().px(r(7.)).pt(r(6.)).pb(r(14.)).flex().flex_col();

        for d in self.duplicates.clone() {
            let names = d.client_names.clone().unwrap_or_default();
            let ids = d.client_ids.clone().unwrap_or_default();
            let key = d.key.clone();
            let b = |s: String| text(s, 13.0, 700, t.fg);
            page = page.child(
                card(&t)
                    .mb(r(4.))
                    .px(r(4.))
                    .py(r(3.))
                    .flex()
                    .items_center()
                    .gap(r(3.))
                    .child(icon(Lucide::Copy).size(r(4.)).text_color(t.fg3))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_wrap()
                            .child(b(names.first().cloned().unwrap_or_default()))
                            .child(text(" and ", 13.0, 500, t.fg2))
                            .child(b(names.get(1).cloned().unwrap_or_default()))
                            .child(text(" look like the same client.", 13.0, 500, t.fg2)),
                    )
                    .child(Button::ghost(SharedString::from(format!("dup-diff-{key}")), "They're different").small().on_click(move |_, _, cx| {
                        if let Err(e) = store::write(cx, |db| db.dismiss_attention(&key)) {
                            toast_error(e.to_string(), cx);
                        }
                    }))
                    .child(Button::secondary(SharedString::from(format!("dup-merge-{}", d.key)), "Merge").small().on_click(cx.listener(move |p, _, _, cx| {
                        let (from, into) = MergeClientsModal::suggest(&p.clients, &ids);
                        p.open_merge(Some(from), Some(into), cx);
                    }))),
            );
        }

        if self.clients.is_empty() {
            page = page.child(card(&t).child(
                EmptyState::new("clients-empty", "Add your first client", "Clients hold your rate, billing address, and projects.")
                    .action("New client", cx.listener(|p, _, w, cx| p.open_form(None, w, cx))),
            ));
        } else {
            let q = self.search.read(cx).value().trim().to_lowercase();
            let filtered: Vec<Client> = self
                .clients
                .iter()
                .filter(|c| q.is_empty() || c.name.to_lowercase().contains(&q) || c.company.to_lowercase().contains(&q) || c.email.to_lowercase().contains(&q))
                .cloned()
                .collect();
            let unbilled: f64 = self.clients.iter().map(|c| c.unbilled_amount).sum();
            let outstanding: f64 = self.clients.iter().map(|c| c.outstanding_amount).sum();
            let n = self.clients.len();
            let mut summary = div().flex().items_baseline().child(text(format!("{n} client{}", if n == 1 { "" } else { "s" }), 13.0, 500, t.fg3));
            if unbilled > 0.0 {
                summary = summary.child(text(" · ", 13.0, 500, t.fg3)).child(div().font(crate::style::num_font(500)).text_size(px(13.)).text_color(t.amber).child(money(unbilled))).child(text(" unbilled", 13.0, 500, t.fg3));
            }
            if outstanding > 0.0 {
                summary = summary.child(text(" · ", 13.0, 500, t.fg3)).child(div().font(crate::style::num_font(500)).text_size(px(13.)).text_color(t.blue).child(money(outstanding))).child(text(" outstanding", 13.0, 500, t.fg3));
            }
            page = page.child(
                div()
                    .flex()
                    .items_center()
                    .gap(r(3.))
                    .mb(r(4.))
                    .child(summary)
                    .child(div().ml_auto().w(r(72.)).child(search_field("clients-search-clear", &self.search, cx))),
            );

            let columns = vec![
                Column::new("Client"),
                Column::new("Projects").right(),
                Column::new("Unbilled").right(),
                Column::new("Outstanding").right(),
                Column::new("Paid").right(),
                Column::new("Rate").right(),
                Column::new("Last tracked"),
                Column::new("").fixed(r(10.).into()),
            ];
            let can_merge = self.clients.len() > 1;
            let mut rows = Vec::new();
            for c in &filtered {
                let id = c.id;
                let sub = if !c.company.is_empty() { c.company.clone() } else if !c.email.is_empty() { c.email.clone() } else { "—".into() };
                let name_w = measure(window, &c.name, 13.0, 500, false).max(measure(window, &sub, 12.0, 500, false).min(260.0));
                let client_cell = Cell::new(
                    div()
                        .flex()
                        .items_center()
                        .gap(r(3.))
                        .min_w_0()
                        .child(client_avatar(&c.name, 32.0))
                        .child(
                            div()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .child(text(c.name.clone(), 13.0, 500, t.fg).truncate().line_height(px(18.)))
                                .child(text(sub, 12.0, 500, t.fg3).truncate().max_w(px(260.)).line_height(px(16.))),
                        ),
                    32.0 + 9.75 + name_w,
                );
                let projects_text = if c.active_project_count > 0 { format!("{} · {} active", c.project_count, c.active_project_count) } else { c.project_count.to_string() };
                let projects_w = measure(window, &projects_text, 14.0, 500, true);
                let projects_cell = Cell::new(
                    div()
                        .flex()
                        .font(crate::style::num_font(500))
                        .text_size(px(14.))
                        .whitespace_nowrap()
                        .child(div().text_color(t.fg2).child(c.project_count.to_string()))
                        .children((c.active_project_count > 0).then(|| div().text_color(t.fg4).child(format!(" · {} active", c.active_project_count)))),
                    projects_w,
                );
                let money_cell = |amount: f64, color| if amount > 0.0 { text_cell(window, money(amount), 14.0, 500, color, true) } else { dash_cell(window, &t) };
                let menu_open = self.menu == Some(id);
                let this = cx.weak_entity();
                let close_menu: Rc<dyn Fn(&mut Window, &mut App)> = {
                    let this = this.clone();
                    Rc::new(move |_, cx| {
                        let _ = this.update(cx, |p, cx| {
                            p.menu = None;
                            cx.notify();
                        });
                    })
                };
                let edit_client = c.clone();
                let (te, tm, td) = (this.clone(), this.clone(), this.clone());
                let del_client = c.clone();
                let mut items: Vec<Option<MenuItem>> = vec![Some(MenuItem::new("Edit", move |w, cx| {
                    let ec = edit_client.clone();
                    let _ = te.update(cx, |p, cx| p.open_form(Some(ec), w, cx));
                })
                .icon(Lucide::Pencil))];
                if can_merge {
                    items.push(Some(
                        MenuItem::new("Merge into another client…", move |_, cx| {
                            let _ = tm.update(cx, |p, cx| p.open_merge(Some(id), None, cx));
                        })
                        .icon(Lucide::Merge),
                    ));
                }
                items.push(None);
                items.push(Some(
                    MenuItem::new("Delete", move |_, cx| {
                        let dc = del_client.clone();
                        let _ = td.update(cx, |p, cx| {
                            p.delete = Some(dc);
                            cx.notify();
                        });
                    })
                    .icon(Lucide::Trash2)
                    .danger(),
                ));
                let mut menu_cell = div()
                    .relative()
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        Button::icon_button(SharedString::from(format!("client-menu-{id}")), Lucide::MoreHorizontal)
                            .small()
                            .selected(menu_open)
                            .tooltip("More actions")
                            .on_click(cx.listener(move |p, _, _, cx| {
                                cx.stop_propagation();
                                // A click on the open menu's trigger closed it already (click outside)
                                p.menu = if p.menu == Some(id) || crate::components::menu::just_closed(format!("client-pop-{id}")) { None } else { Some(id) };
                                cx.notify();
                            })),
                    );
                if menu_open {
                    let list = menu_list(&format!("client-menu-{id}"), items, close_menu.clone(), cx);
                    let cm = close_menu.clone();
                    menu_cell = menu_cell.child(popover(SharedString::from(format!("client-pop-{id}")), Align::End, None, list, move |w, cx| cm(w, cx), cx));
                }
                rows.push(
                    Row::new(ElementId::Name(format!("client-row-{id}").into()), vec![
                        client_cell,
                        projects_cell,
                        money_cell(c.unbilled_amount, t.amber),
                        money_cell(c.outstanding_amount, if c.overdue_amount > 0.0 { t.red } else { t.blue }),
                        if c.paid_amount > 0.0 { text_cell(window, money(c.paid_amount), 14.0, 500, t.fg2, true) } else { dash_cell(window, &t) },
                        text_cell(window, money(c.default_rate), 14.0, 500, t.fg2, true),
                        text_cell(window, c.last_activity.as_deref().map(relative_days).unwrap_or_else(|| "—".into()), 14.0, 500, t.fg3, false),
                        Cell::new(menu_cell, 27.0),
                    ])
                    .on_click(move |_, w, cx| navigate(Route::Client(id), w, cx)),
                );
            }
            let width = page_width(window) - 2.0;
            let empty = div().h(r(20.)).flex().items_center().justify_center().child(text("No clients match", 14.0, 500, t.fg3)).into_any_element();
            page = page.child(card(&t).child(table(&t, window, "clients", columns, rows, width, Some(empty))));
        }

        page = page.children(self.form.clone()).children(self.merge.clone());
        if let Some(c) = self.delete.clone() {
            let this = cx.weak_entity();
            let id = c.id;
            page = page.child(confirm_dialog(
                "delete-client",
                format!("Delete {}?", c.name),
                "This permanently deletes the client with all of their projects, time entries, and invoices. If they were added twice, merge them instead.",
                "Delete client",
                true,
                move |_, cx| match store::write(cx, |db| db.delete_client(id)) {
                    Ok(_) => toast_success("Client deleted", cx),
                    Err(e) => toast_error(e.to_string(), cx),
                },
                move |_, cx| {
                    let _ = this.update(cx, |p, cx| {
                        p.delete = None;
                        cx.notify();
                    });
                },
                cx,
            ));
        }
        page
    }
}
