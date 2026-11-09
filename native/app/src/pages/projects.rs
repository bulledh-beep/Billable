//! Projects.tsx (+ ProjectForm.tsx): every project with its status, hours and
//! money, a status filter and search, quick timers, and multi-select to
//! invoice several projects of one client together.

use std::{collections::HashSet, time::Duration};

use billable_core::models::{Client, Project, ProjectInput};
use gpui::{
    anchored, deferred, div, point, prelude::*, px, Animation, AnimationExt as _, AnyElement, AppContext, Context,
    ElementId, Entity, IntoElement, ParentElement, Render, SharedString, Styled, Subscription, Window,
};
use gpui_component::input::{InputEvent, InputState};

use crate::{
    components::{
        fields::{checkbox, search_field},
        project_form::{ProjectFormEvent, ProjectFormModal, ProjectFormValues},
        segmented, status_badge,
        table::{dash_cell, measure, table, text_cell, Cell, Column, Row},
        toast_error, toast_success, Button, EmptyState, Segment,
    },
    format::{format_day, hours_short, money, relative_days},
    icons::Lucide,
    motion::{self, rise_in},
    shell::{
        app::{navigate, Header, Page},
        route::Route,
    },
    store,
    style::{card, dot, num_font, page_width, r, text},
    theme::{parse_hex, theme, Theme},
    timer,
};

const STATUSES: [&str; 4] = ["active", "paused", "complete", "archived"];

fn status_order(s: &str) -> usize {
    STATUSES.iter().position(|x| *x == s).unwrap_or(STATUSES.len())
}

pub struct ProjectsPage {
    rev: u64,
    projects: Vec<Project>,
    clients: Vec<Client>,
    search: Entity<InputState>,
    /// `all` or a status
    filter: String,
    form: Option<Entity<ProjectFormModal>>,
    form_sub: Option<Subscription>,
    /// In the order they were picked (the invoice link lists them so).
    selected: Vec<i64>,
    _subs: Vec<Subscription>,
}

impl ProjectsPage {
    pub fn new(action_new: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search projects or clients"));
        let subs = vec![cx.subscribe(&search, |_, _, e: &InputEvent, cx| {
            if matches!(e, InputEvent::Change) {
                cx.notify();
            }
        })];
        let mut page = ProjectsPage { rev: 0, projects: vec![], clients: vec![], search, filter: "all".into(), form: None, form_sub: None, selected: vec![], _subs: subs };
        page.reload(cx);
        // ⌘⇧N from the menu
        if action_new {
            page.open_form(window, cx);
        }
        page
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        let db = store::db(cx);
        self.projects = db.list_projects(None).unwrap_or_default();
        self.clients = db.list_clients().unwrap_or_default();
        // Forget picks of projects that are gone.
        let ids: HashSet<i64> = self.projects.iter().map(|p| p.id).collect();
        self.selected.retain(|id| ids.contains(id));
        self.rev = store::rev(cx);
    }

    fn open_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let clients = self.clients.clone();
        let modal = cx.new(|cx| ProjectFormModal::new(None, clients, None, window, cx));
        self.form_sub = Some(cx.subscribe_in(&modal, window, |p, _, e: &ProjectFormEvent, window, cx| {
            match e {
                ProjectFormEvent::Submit(values) => p.create(values.clone(), window, cx),
                ProjectFormEvent::Closed => p.form = None,
            }
            cx.notify();
        }));
        self.form = Some(modal);
        cx.notify();
    }

    fn create(&mut self, v: ProjectFormValues, window: &mut Window, cx: &mut Context<Self>) {
        let input = ProjectInput { client_id: v.client_id, name: v.name, description: Some(v.description), rate: Some(v.rate), status: Some(v.status), color: Some(v.color) };
        match store::write(cx, |db| db.create_project(&input)) {
            Ok(created) => {
                toast_success("Project created", cx);
                self.form = None;
                navigate(Route::Project(created.id), window, cx);
            }
            Err(e) => toast_error(e.to_string(), cx),
        }
    }

    fn toggle_select(&mut self, id: i64, cx: &mut Context<Self>) {
        if let Some(i) = self.selected.iter().position(|x| *x == id) {
            self.selected.remove(i);
        } else {
            self.selected.push(id);
        }
        cx.notify();
    }

    fn filtered(&self, cx: &Context<Self>) -> Vec<Project> {
        let q = self.search.read(cx).value().trim().to_lowercase();
        let mut list: Vec<Project> = self
            .projects
            .iter()
            .filter(|p| self.filter == "all" || p.status == self.filter)
            .filter(|p| q.is_empty() || p.name.to_lowercase().contains(&q) || p.client_name.as_deref().unwrap_or("").to_lowercase().contains(&q))
            .cloned()
            .collect();
        list.sort_by(|a, b| {
            status_order(&a.status).cmp(&status_order(&b.status)).then_with(|| {
                let ka = a.last_activity.as_deref().unwrap_or(&a.created_at);
                let kb = b.last_activity.as_deref().unwrap_or(&b.created_at);
                kb.cmp(ka)
            })
        });
        list
    }

    fn table(&self, t: &Theme, filtered: &[Project], window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let s = store::store(cx);
        let (running, paused) = (s.timer_running(), s.timer_paused());
        let current = s.timer_entry().map(|e| e.entry.project_id).filter(|_| running || paused);
        let selected: HashSet<i64> = self.selected.iter().copied().collect();

        let columns = vec![
            Column::new("").fixed(r(8.).into()),
            Column::new("Project"),
            Column::new("Status"),
            Column::new("Tracked").right(),
            Column::new("Unbilled").right(),
            Column::new("Invoiced").right(),
            Column::new("Last tracked"),
            Column::new("").fixed(r(12.).into()),
        ];
        let mut rows = Vec::new();
        for p in filtered {
            let id = p.id;
            let group: SharedString = format!("project-row-{id}").into();
            let is_selected = selected.contains(&id);
            let is_current = current == Some(id);

            // Checkbox: shows on hover, or once picked
            let mut check = div()
                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(checkbox(t, SharedString::from(format!("project-check-{id}")), is_selected, false).on_click(cx.listener(move |p, _, _, cx| {
                    cx.stop_propagation();
                    p.toggle_select(id, cx);
                })));
            if !is_selected {
                check = check.opacity(0.).group_hover(group.clone(), |s| s.opacity(1.));
            }

            // Project: dot, name, client · rate
            let client = p.client_name.clone().unwrap_or_default();
            let rate = money(p.rate);
            let name_w = measure(window, &p.name, 13.0, 500, false).min(300.0);
            let sub_w = measure(window, &format!("{client} · "), 12.0, 500, false) + measure(window, &rate, 12.0, 500, true) + measure(window, "/hr", 12.0, 500, false);
            let project_cell = Cell::new(
                div()
                    .flex()
                    .items_center()
                    .gap(r(2.5))
                    .min_w_0()
                    .child(dot(parse_hex(&p.color), r(2.)))
                    .child(
                        div()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(text(p.name.clone(), 13.0, 500, t.fg).line_height(px(18.)).truncate().max_w(px(300.)))
                            .child(
                                div()
                                    .flex()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_size(px(12.))
                                    .line_height(px(16.))
                                    .text_color(t.fg3)
                                    .child(format!("{client} · "))
                                    .child(div().font(num_font(500)).child(rate))
                                    .child("/hr"),
                            ),
                    ),
                6.5 + 8.125 + name_w.max(sub_w),
            );

            // The badge: its label plus px-2.5 either side
            let label = crate::components::badge::status_tone(&p.status).1;
            let status_w = measure(window, if label.is_empty() { &p.status } else { label }, 12.0, 600, false) + 16.25;
            let status_cell = Cell::new(status_badge(t, &p.status), status_w);

            let unbilled_cell = if p.unbilled_amount > 0.0 {
                let amount = money(p.unbilled_amount);
                let since = format!("since {}", format_day(p.oldest_unbilled.as_deref().unwrap_or("")));
                let w = measure(window, &amount, 14.0, 500, true).max(measure(window, &since, 11.0, 500, false));
                Cell::new(
                    div()
                        .flex()
                        .flex_col()
                        .items_end()
                        .whitespace_nowrap()
                        .child(div().font(num_font(500)).text_size(px(14.)).line_height(px(21.)).text_color(t.amber).child(amount))
                        .child(text(since, 11.0, 500, t.fg3).line_height(px(14.))),
                    w,
                )
            } else {
                dash_cell(window, t)
            };
            let invoiced_cell = if p.invoiced_amount > 0.0 { text_cell(window, money(p.invoiced_amount), 14.0, 500, t.fg2, true) } else { dash_cell(window, t) };
            let last = p.last_activity.as_deref().map(relative_days).unwrap_or_else(|| "—".into());

            // The timer button
            let mut action = div().on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation());
            if is_current {
                action = action.child(
                    Button::icon_button(SharedString::from(format!("project-timer-{id}")), if paused { Lucide::Play } else { Lucide::Pause })
                        .small()
                        .filled()
                        .text_color(t.accent_text)
                        .tooltip(if paused { "Resume timer" } else { "Pause timer" })
                        .on_click(move |_, _, cx| {
                            cx.stop_propagation();
                            if paused { timer::resume(cx) } else { timer::pause(cx) }
                        }),
                );
            } else if p.status == "active" {
                action = action
                    .opacity(0.)
                    .group_hover(group.clone(), |s| s.opacity(1.))
                    .child(Button::icon_button(SharedString::from(format!("project-timer-{id}")), Lucide::Play).small().tooltip("Start timer").on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        timer::start(id, None, cx)
                    }));
            }

            let mut row = Row::new(ElementId::Name(format!("project-row-{id}").into()), vec![
                Cell::new(check, 17.0),
                project_cell,
                status_cell,
                text_cell(window, hours_short(p.total_hours), 14.0, 500, t.fg2, true),
                unbilled_cell,
                invoiced_cell,
                text_cell(window, last, 14.0, 500, t.fg3, false),
                Cell::new(action, 27.0),
            ])
            .group(group)
            .on_click(move |_, w, cx| navigate(Route::Project(id), w, cx));
            if is_selected {
                row = row.background(t.accent.opacity(0.05));
            }
            rows.push(row);
        }

        // tfoot: the count and totals
        if filtered.len() > 1 {
            let hours: f64 = filtered.iter().map(|p| p.total_hours).sum();
            let unbilled: f64 = filtered.iter().map(|p| p.unbilled_amount).sum();
            let empty = || Cell::new(div(), 0.0);
            rows.push(
                Row::new("projects-total", vec![
                    empty(),
                    text_cell(window, format!("{} projects", filtered.len()), 12.0, 500, t.fg3, false),
                    empty(),
                    text_cell(window, hours_short(hours), 12.0, 500, t.fg2, true),
                    if unbilled > 0.0 { text_cell(window, money(unbilled), 12.0, 500, t.amber, true) } else { empty() },
                    empty(),
                    empty(),
                    empty(),
                ])
                .height(33.0),
            );
        }

        let width = page_width(window) - 2.0;
        let empty = div().h(r(20.)).flex().items_center().justify_center().child(text("No projects match", 14.0, 500, t.fg3)).into_any_element();
        card(t).overflow_hidden().child(table(t, window, "projects", columns, rows, width, Some(empty))).into_any_element()
    }

    /// The floating bar while projects are picked.
    fn selection_bar(&self, t: &Theme, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let selected: Vec<&Project> = self.projects.iter().filter(|p| self.selected.contains(&p.id)).collect();
        if selected.is_empty() {
            return None;
        }
        let clients: HashSet<i64> = selected.iter().map(|p| p.client_id).collect();
        let unbilled: f64 = selected.iter().map(|p| p.unbilled_amount).sum();
        let can_invoice = clients.len() == 1 && unbilled > 0.0;
        let ids = self.selected.clone();

        let mut bar = div()
            .id("projects-selection")
            .occlude()
            .relative()
            .flex()
            .items_center()
            .gap(r(4.))
            .pl(r(4.))
            .pr(r(2.))
            .h(r(12.))
            .rounded(px(10.))
            .bg(t.panel)
            .shadow(t.shadow_pop())
            .child(text(format!("{} selected", selected.len()), 13.0, 500, t.fg).line_height(px(18.)))
            .child(
                div()
                    .flex()
                    .text_size(px(13.))
                    .line_height(px(18.))
                    .text_color(t.fg3)
                    .child(div().font(num_font(500)).text_color(t.amber).child(money(unbilled)))
                    .child(" unbilled"),
            );
        if clients.len() > 1 {
            bar = bar.child(text("Pick projects from one client to invoice them together", 12.0, 500, t.fg3).line_height(px(16.)));
        }
        bar = bar
            .child(Button::primary("projects-invoice-selected", "Create invoice").small().disabled(!can_invoice).on_click(move |_, w, cx| {
                navigate(Route::InvoiceNew { client_id: None, project_id: None, project_ids: ids.clone() }, w, cx)
            }))
            .child(Button::icon_button("projects-clear-selection", Lucide::X).small().tooltip("Clear selection").on_click(cx.listener(|p, _, _, cx| {
                p.selected.clear();
                cx.notify();
            })));
        let bar: AnyElement = if motion::reduced() {
            bar.into_any_element()
        } else {
            bar.with_animation("projects-selection-in", Animation::new(Duration::from_millis(150)), |b, d| b.opacity(d).top(px(12.0 * (1.0 - d)))).into_any_element()
        };

        // fixed bottom-5, centred at 50% + 110px of the window (over the main column)
        let viewport = window.viewport_size();
        let height = r(12.);
        Some(
            deferred(
                anchored().position(point(px(0.), viewport.height - r(5.) - height)).child(div().w(viewport.width).pl(px(220.)).flex().justify_center().child(bar)),
            )
            .with_priority(5)
            .into_any_element(),
        )
    }
}

impl Page for ProjectsPage {
    fn header(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> Header {
        Header::new("Projects").actions(Button::secondary("projects-new", "New project").on_click(cx.listener(|p, _, w, cx| p.open_form(w, cx))))
    }
}

impl Render for ProjectsPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.rev != store::rev(cx) {
            self.reload(cx);
        }
        let t = theme(cx);
        let mut page = div().w_full().max_w(px(1160.)).mx_auto().px(r(7.)).pt(r(6.)).pb(r(14.)).flex().flex_col();

        if self.projects.is_empty() {
            let no_clients = self.clients.is_empty();
            let empty = EmptyState::new(
                "projects-empty",
                "Create your first project",
                if no_clients { "Add a client first, then create a project to track time against." } else { "Projects hold your time and set the hourly rate." },
            );
            let empty = if no_clients {
                empty.action("Add a client", |_, w, cx| navigate(Route::Clients { action_new: true }, w, cx))
            } else {
                empty.action("New project", cx.listener(|p, _, w, cx| p.open_form(w, cx)))
            };
            page = page.child(rise_in("projects-rise-0", 0, card(&t).child(empty)));
        } else {
            let mut counts = [0usize; 4];
            for p in &self.projects {
                if let Some(i) = STATUSES.iter().position(|s| *s == p.status) {
                    counts[i] += 1;
                }
            }
            let this = cx.weak_entity();
            let filter = segmented(
                "projects-filter",
                &self.filter,
                vec![
                    Segment::new("all".to_string(), "All").count(self.projects.len()),
                    Segment::new("active".to_string(), "Active").count(counts[0]),
                    Segment::new("paused".to_string(), "Paused").count(counts[1]),
                    Segment::new("complete".to_string(), "Complete").count(counts[2]),
                    Segment::new("archived".to_string(), "Archived").count(counts[3]),
                ],
                move |v, _, cx| {
                    let _ = this.update(cx, |p, cx| {
                        p.filter = v;
                        cx.notify();
                    });
                },
                cx,
            );
            page = page.child(rise_in(
                "projects-rise-0",
                0,
                div().flex().items_center().gap(r(3.)).mb(r(4.)).child(filter).child(div().ml_auto().w(r(72.)).child(search_field("projects-search-clear", &self.search, cx))),
            ));
            let filtered = self.filtered(cx);
            let table = self.table(&t, &filtered, window, cx);
            page = page.child(rise_in("projects-rise-1", 1, table));
        }

        page = page.children(self.selection_bar(&t, window, cx)).children(self.form.clone());
        page
    }
}
