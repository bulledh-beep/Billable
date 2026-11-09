//! ProjectForm.tsx as a modal view. OWNER: the Projects port. The page that
//! opens it does the saving (as in the app), on [`ProjectFormEvent::Submit`].
//!
//! A page keeps `Option<Entity<ProjectFormModal>>`, renders it, and
//! subscribes: `Submit` carries the values (name trimmed), `Closed` means
//! Cancel, ×, Escape or the backdrop.

use billable_core::models::{Client, Project};
use gpui::{
    div, prelude::*, px, App, AppContext, Context, ElementId, Entity, EventEmitter, IntoElement, ParentElement, Render,
    SharedString, Styled, Subscription, Window,
};
use gpui_component::input::{InputEvent, InputState, TextareaState};

use super::{
    client_form::trim_number,
    fields::{TextArea, TextField},
    select, Button, Modal,
};
use crate::{
    format::money,
    style::{hint, label, r},
    theme::{parse_hex, shadow, theme},
};

pub const PROJECT_COLORS: [&str; 8] = ["#F5A623", "#E5484D", "#3E8BF7", "#30A46C", "#8E4EC6", "#12A594", "#F76B15", "#D6409F"];

#[derive(Clone, Debug)]
pub struct ProjectFormValues {
    pub client_id: i64,
    pub name: String,
    pub description: String,
    pub rate: f64,
    pub status: String,
    pub color: String,
}

pub enum ProjectFormEvent {
    Submit(ProjectFormValues),
    Closed,
}

pub struct ProjectFormModal {
    pub project: Option<Project>,
    pub clients: Vec<Client>,
    /// Used when the form opens; kept for callers that read it back.
    #[allow(dead_code)]
    pub default_client_id: Option<i64>,
    client_id: i64,
    status: String,
    color: String,
    name: Entity<InputState>,
    rate: Entity<InputState>,
    description: Entity<TextareaState>,
    _subs: Vec<Subscription>,
}

impl EventEmitter<ProjectFormEvent> for ProjectFormModal {}

/// `Math.floor(Math.random() * PROJECT_COLORS.length)`
fn random_color() -> String {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
    PROJECT_COLORS[(nanos as usize / 1000) % PROJECT_COLORS.len()].to_string()
}

/// `value={form.rate || ''}`
fn rate_text(rate: f64) -> String {
    if rate != 0.0 { trim_number(rate) } else { String::new() }
}

impl ProjectFormModal {
    /// Filled from `project` (edit), or empty for the default client (or the
    /// first one) at its rate, with a random colour.
    pub fn new(project: Option<Project>, clients: Vec<Client>, default_client_id: Option<i64>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (client_id, name, description, rate, status, color) = match &project {
            Some(p) => (p.client_id, p.name.clone(), p.description.clone(), p.rate, p.status.clone(), p.color.clone()),
            None => {
                let client = clients.iter().find(|c| Some(c.id) == default_client_id).or(clients.first());
                (client.map(|c| c.id).unwrap_or(0), String::new(), String::new(), client.map(|c| c.default_rate).unwrap_or(0.0), "active".to_string(), random_color())
            }
        };
        let name_state = cx.new(|cx| InputState::new(window, cx).placeholder("Website redesign").default_value(name));
        let rate_state = cx.new(|cx| InputState::new(window, cx).default_value(rate_text(rate)));
        let description = cx.new(|cx| TextareaState::new(window, cx).placeholder("Optional").default_value(description));
        // Enter in a one-line field submits (the form's onKeyDown); typing
        // re-renders for the Create button's enabled state.
        let on_input = |m: &mut Self, _: Entity<InputState>, e: &InputEvent, cx: &mut Context<Self>| match e {
            InputEvent::PressEnter { .. } => m.submit(cx),
            _ => cx.notify(),
        };
        let subs = vec![cx.subscribe(&name_state, on_input), cx.subscribe(&rate_state, on_input)];
        name_state.update(cx, |s, cx| s.focus(window, cx));
        ProjectFormModal { project, clients, default_client_id, client_id, status, color, name: name_state, rate: rate_state, description, _subs: subs }
    }

    fn rate_value(&self, cx: &App) -> f64 {
        self.rate.read(cx).value().trim().parse::<f64>().ok().filter(|v| v.is_finite()).unwrap_or(0.0)
    }

    fn valid(&self, cx: &App) -> bool {
        !self.name.read(cx).value().trim().is_empty() && self.client_id != 0
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        if !self.valid(cx) {
            return;
        }
        let values = ProjectFormValues {
            client_id: self.client_id,
            name: self.name.read(cx).value().trim().to_string(),
            description: self.description.read(cx).value().to_string(),
            rate: self.rate_value(cx),
            status: self.status.clone(),
            color: self.color.clone(),
        };
        cx.emit(ProjectFormEvent::Submit(values));
    }

    fn set_client(&mut self, id: i64, window: &mut Window, cx: &mut Context<Self>) {
        self.client_id = id;
        // A new project takes the client's rate; an edit keeps its own.
        if self.project.is_none() {
            if let Some(c) = self.clients.iter().find(|c| c.id == id) {
                let text = rate_text(c.default_rate);
                self.rate.update(cx, |s, cx| s.set_value(text, window, cx));
            }
        }
        cx.notify();
    }
}

impl Render for ProjectFormModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(cx);
        let editing = self.project.is_some();
        let valid = self.valid(cx);
        let this = cx.entity().downgrade();

        let mut client_opts: Vec<(i64, SharedString)> = vec![(0, "Choose a client".into())];
        client_opts.extend(self.clients.iter().map(|c| (c.id, SharedString::from(c.name.clone()))));
        let wc = this.clone();
        let client_select = select(
            "project-form-client",
            client_opts,
            &self.client_id,
            move |v, window, cx| {
                let _ = wc.update(cx, |m, cx| m.set_client(v, window, cx));
            },
            cx,
        );

        let mut body = div()
            .flex()
            .flex_col()
            .gap(r(4.))
            .child(div().child(label(&t, "Name")).child(TextField::new(&self.name)))
            .child(
                div()
                    .flex()
                    .gap(r(3.))
                    .child(div().flex_1().min_w_0().child(label(&t, "Client")).child(client_select))
                    .child(div().flex_1().min_w_0().child(label(&t, "Hourly rate")).child(TextField::new(&self.rate).tabular().leading_text("$"))),
            );
        if !editing && self.client_id > 0 {
            let client = self.clients.iter().find(|c| c.id == self.client_id);
            let name = client.map(|c| c.name.clone()).unwrap_or_default();
            let rate = client.map(|c| c.default_rate).unwrap_or(0.0);
            // `.hint -mt-2`: the parent's space-y-4 outranks both margins, so
            // it sits a full gap below the fields.
            body = body.child(hint(&t, format!("Uses {name}'s rate of {}/hr. Change it for this project if needed.", money(rate))).mt(px(0.)));
        }
        body = body.child(div().child(label(&t, "Description")).child(TextArea::new(&self.description, 2)));

        // Colour swatches
        let mut swatches = div().flex().items_center().gap(r(1.5)).h(r(8.));
        for (i, c) in PROJECT_COLORS.iter().enumerate() {
            let chosen = self.color == *c;
            let color = parse_hex(c);
            let mut swatch = div().id(ElementId::Name(format!("project-color-{i}").into())).size(r(5.)).flex_none().rounded_full().bg(color).cursor_pointer();
            if chosen {
                // ring-2 ring-offset-2 ring-offset-panel ring-fg/40
                let ring = t.fg.opacity(0.4).to_rgb();
                swatch = swatch.shadow(vec![shadow(0.0, 0.0, 0.0, 4.0, ring), shadow(0.0, 0.0, 0.0, 2.0, t.panel.to_rgb())]);
            }
            let w = this.clone();
            let value = c.to_string();
            swatch = swatch.on_click(move |_, _, cx| {
                let v = value.clone();
                let _ = w.update(cx, |m, cx| {
                    m.color = v;
                    cx.notify();
                });
            });
            swatches = swatches.child(swatch);
        }
        let color_field = div().flex_1().min_w_0().child(label(&t, "Color")).child(swatches);
        let mut last_row = div().flex().gap(r(3.));
        if editing {
            let ws = this.clone();
            let opts: Vec<(String, SharedString)> = [("active", "Active"), ("paused", "Paused"), ("complete", "Complete"), ("archived", "Archived")]
                .iter()
                .map(|(v, l)| (v.to_string(), SharedString::from(*l)))
                .collect();
            last_row = last_row
                .child(div().flex_1().min_w_0().child(label(&t, "Status")).child(select(
                    "project-form-status",
                    opts,
                    &self.status,
                    move |v, _, cx| {
                        let _ = ws.update(cx, |m, cx| {
                            m.status = v;
                            cx.notify();
                        });
                    },
                    cx,
                )))
                .child(color_field);
        } else {
            // grid-cols-2 with one child: the colours take the first column.
            last_row = last_row.child(color_field).child(div().flex_1().min_w_0());
        }
        body = body.child(last_row);

        let (e1, e2, e3) = (this.clone(), this.clone(), this);
        Modal::new("project-form", if editing { "Edit project" } else { "New project" }, move |_, cx| {
            let _ = e1.update(cx, |_, cx| cx.emit(ProjectFormEvent::Closed));
        })
        .child(body)
        .footer(Button::secondary("project-form-cancel", "Cancel").on_click(move |_, _, cx| {
            let _ = e2.update(cx, |_, cx| cx.emit(ProjectFormEvent::Closed));
        }))
        .footer(
            Button::primary("project-form-save", if editing { "Save changes" } else { "Create project" })
                .disabled(!valid)
                .on_click(move |_, _, cx| {
                    let _ = e3.update(cx, |m, cx| m.submit(cx));
                }),
        )
    }
}
