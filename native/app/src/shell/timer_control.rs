//! TimerControl.tsx: the timer in the toolbar on every page. Idle, it's a
//! "Start timer" button opening a quick project picker; running or paused,
//! it's a pill with a stopwatch, the project, the time and the money so far.

use std::rc::Rc;

use billable_core::models::Project;
use gpui::{
    prelude::FluentBuilder, div, px, App, AppContext, Context, ElementId, Entity, FocusHandle, InteractiveElement, IntoElement, KeyDownEvent,
    ParentElement, Render, StatefulInteractiveElement, Styled, Subscription, Window,
};
use gpui_component::input::{InputEvent, InputState};

use super::{app::navigate, route::Route};
use crate::{
    components::{dial::stopwatch_dial, fields::TextField, popover, Align, Button},
    format::money,
    icons::Lucide,
    store,
    style::{dot, figures, kbd, r, text},
    theme::theme,
    timer,
};

pub struct TimerControl {
    open: bool,
    search: Entity<InputState>,
    description: Entity<InputState>,
    index: usize,
    focus: FocusHandle,
    _subs: Vec<Subscription>,
}

impl TimerControl {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Find a project"));
        let description = cx.new(|cx| InputState::new(window, cx).placeholder("What are you working on? (optional)"));
        let subs = vec![cx.subscribe(&search, |this, _, e: &InputEvent, cx| {
            if matches!(e, InputEvent::Change) {
                this.index = 0;
                cx.notify();
            }
        })];
        TimerControl { open: false, search, description, index: 0, focus: cx.focus_handle(), _subs: subs }
    }

    /// Active projects, most recently worked on first, filtered by the search.
    fn projects(&self, cx: &App) -> Vec<Project> {
        let q = self.search.read(cx).value().trim().to_lowercase();
        let mut list: Vec<Project> = store::db(cx).list_projects(None).unwrap_or_default().into_iter().filter(|p| p.status == "active").collect();
        list.sort_by(|a, b| {
            let ka = a.last_activity.clone().unwrap_or_else(|| a.created_at.clone());
            let kb = b.last_activity.clone().unwrap_or_else(|| b.created_at.clone());
            kb.cmp(&ka)
        });
        if q.is_empty() {
            return list;
        }
        list.into_iter()
            .filter(|p| p.name.to_lowercase().contains(&q) || p.client_name.clone().unwrap_or_default().to_lowercase().contains(&q))
            .collect()
    }

    fn open_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open = true;
        self.index = 0;
        self.search.update(cx, |s, cx| s.set_value("", window, cx));
        self.description.update(cx, |s, cx| s.set_value("", window, cx));
        self.search.update(cx, |s, cx| s.focus(window, cx));
        cx.notify();
    }

    fn choose(&mut self, project_id: i64, cx: &mut Context<Self>) {
        let desc = self.description.read(cx).value().trim().to_string();
        self.open = false;
        cx.notify();
        timer::start(project_id, Some(desc), cx);
    }
}

impl Render for TimerControl {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(cx);
        let s = store::store(cx);
        if let Some(entry) = s.timer_entry().cloned() {
            let paused = entry.entry.paused_at.is_some() || s.timer.active.is_none();
            let seconds = timer::elapsed_seconds(&entry);
            let earned = timer::earned(&entry, seconds);
            let (fg2, fg) = (t.fg2, t.fg);
            let mut pill = div()
                .flex()
                .items_center()
                .h(px(34.))
                .pl(r(1.5))
                .pr(r(1.))
                .gap(r(2.5))
                .rounded_full()
                .bg(if paused { t.fg.opacity(0.06) } else { t.accent.opacity(0.13) })
                .child(stopwatch_dial(&t, seconds, 22.0, paused))
                .child(
                    div()
                        .id("timer-project")
                        .max_w(px(170.))
                        .truncate()
                        .cursor_pointer()
                        .text_color(fg2)
                        .hover(move |s| s.text_color(fg))
                        .on_click(|_, w, cx| navigate(Route::Time { action_new: false }, w, cx))
                        .child(text(entry.project_name.clone().unwrap_or_else(|| "Timer".into()), 13.0, 600, fg2).text_color(fg2)),
                )
                .child(figures(timer::elapsed_text(seconds), 16.0, if paused { t.fg3 } else { t.fg }).line_height(px(16.)));
            if earned > 0.0 {
                pill = pill.child(figures(format!("+{}", money(earned)), 15.0, t.green).line_height(px(15.)));
            }
            pill = pill.child(div().w(px(1.)).h(r(3.5)).bg(t.line_strong).mx(r(0.5)));
            pill = if paused {
                pill.child(
                    Button::icon_button("timer-resume", Lucide::Play)
                        .small()
                        .filled()
                        .icon_size(9.75)
                        .text_color(t.accent_text)
                        .tooltip("Resume (⌘⇧P)")
                        .on_click(|_, _, cx| timer::resume(cx)),
                )
            } else {
                pill.child(
                    Button::icon_button("timer-pause", Lucide::Pause)
                        .small()
                        .filled()
                        .icon_size(9.75)
                        .tooltip("Pause (⌘⇧P)")
                        .on_click(|_, _, cx| timer::pause(cx)),
                )
            };
            return pill
                .child(
                    Button::icon_button("timer-stop", Lucide::Square)
                        .small()
                        .filled()
                        .icon_size(11.0)
                        .hover_text(t.red)
                        .tooltip("Stop (⌘⇧S)")
                        .on_click(|_, _, cx| timer::stop(cx)),
                )
                .into_any_element();
        }

        let this = cx.entity();
        let mut wrap = div().relative().child(
            Button::primary("timer-start", "Start timer")
                .icon(Lucide::Play)
                .filled()
                .icon_size(11.375)
                .tooltip("Start a timer")
                .on_click(cx.listener(|this, _, window, cx| {
                    if this.open {
                        this.open = false;
                        cx.notify();
                    } else {
                        this.open_picker(window, cx);
                    }
                })),
        );
        if self.open {
            let projects = self.projects(cx);
            let index = self.index.min(projects.len().saturating_sub(1));
            let ids: Rc<Vec<i64>> = Rc::new(projects.iter().map(|p| p.id).collect());
            let mut list = div().id("quick-start-list").max_h(px(280.)).overflow_y_scroll().p(px(5.)).flex().flex_col();
            if projects.is_empty() {
                let any = !store::db(cx).list_projects(None).unwrap_or_default().iter().any(|p| p.status == "active");
                list = list.child(text(if any { "No active projects yet" } else { "No projects match" }, 13.0, 500, t.fg3).px(r(3.)).py(r(6.)).text_center());
            }
            for (i, p) in projects.iter().enumerate() {
                let pid = p.id;
                let hl = t.fg.opacity(0.07);
                list = list.child(
                    div()
                        .id(ElementId::Name(format!("qs-{pid}").into()))
                        .w_full()
                        .flex()
                        .items_center()
                        .gap(r(2.5))
                        .px(r(2.))
                        .h(px(30.))
                        .rounded(px(5.))
                        .cursor_pointer()
                        .when(i == index, |d| d.bg(hl))
                        .on_mouse_move(cx.listener(move |this, _, _, cx| {
                            if this.index != i {
                                this.index = i;
                                cx.notify();
                            }
                        }))
                        .on_click(cx.listener(move |this, _, _, cx| this.choose(pid, cx)))
                        .child(dot(crate::theme::parse_hex(&p.color), r(2.)))
                        .child(text(p.name.clone(), 13.0, 500, t.fg).flex_1().truncate())
                        .child(text(p.client_name.clone().unwrap_or_default(), 12.0, 500, t.fg3).max_w(px(110.)).truncate()),
                );
            }
            let content = div()
                .id("quick-start")
                .track_focus(&self.focus)
                .on_key_down(cx.listener(move |this, e: &KeyDownEvent, _, cx| match e.keystroke.key.as_str() {
                    "down" => {
                        this.index = (this.index + 1).min(ids.len().saturating_sub(1));
                        cx.notify();
                    }
                    "up" => {
                        this.index = this.index.saturating_sub(1);
                        cx.notify();
                    }
                    "enter" => {
                        if let Some(&pid) = ids.get(this.index) {
                            this.choose(pid, cx);
                        }
                    }
                    "escape" => {
                        this.open = false;
                        cx.notify();
                    }
                    _ => {}
                }))
                .child(
                    div()
                        .p(r(2.))
                        .border_b_1()
                        .border_color(t.line)
                        .flex()
                        .flex_col()
                        .gap(r(1.5))
                        .child(TextField::new(&self.search).leading_icon(Lucide::Search))
                        .child(TextField::new(&self.description)),
                )
                .child(list)
                .child(
                    div()
                        .px(r(3.))
                        .h(px(30.))
                        .flex()
                        .items_center()
                        .gap(r(1.5))
                        .border_t_1()
                        .border_color(t.line)
                        .child(kbd(&t, "↑↓"))
                        .child(text("choose", 11.0, 500, t.fg3))
                        .child(div().ml(r(1.5)).child(kbd(&t, "↵")))
                        .child(text("start", 11.0, 500, t.fg3)),
                );
            wrap = wrap.child(popover("quick-start-pop", Align::End, Some(320.), content, move |_, cx| {
                this.update(cx, |this, cx| {
                    this.open = false;
                    cx.notify();
                })
            }, cx));
        }
        let _ = window;
        wrap.into_any_element()
    }
}
