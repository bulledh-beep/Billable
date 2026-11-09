//! EmptyState.tsx: the mascot with a title, a line of help and an action.

use gpui::{div, px, App, IntoElement, ParentElement, RenderOnce, SharedString, Styled, Window};

use super::{button::Button, mascot::{Mascot, Motion, Mood}};
use crate::{style::{r, text}, theme::theme};

type Action = (SharedString, Box<dyn Fn(&gpui::ClickEvent, &mut Window, &mut App)>);

#[derive(IntoElement)]
pub struct EmptyState {
    id: SharedString,
    mood: Mood,
    title: SharedString,
    description: SharedString,
    action: Option<Action>,
    compact: bool,
}

impl EmptyState {
    pub fn new(id: impl Into<SharedString>, title: impl Into<SharedString>, description: impl Into<SharedString>) -> Self {
        EmptyState { id: id.into(), mood: Mood::Idle, title: title.into(), description: description.into(), action: None, compact: false }
    }
    pub fn mood(mut self, m: Mood) -> Self {
        self.mood = m;
        self
    }
    pub fn compact(mut self) -> Self {
        self.compact = true;
        self
    }
    pub fn action(mut self, label: impl Into<SharedString>, f: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.action = Some((label.into(), Box::new(f)));
        self
    }
}

impl RenderOnce for EmptyState {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = theme(cx);
        let mut col = div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .text_center()
            .px(r(6.))
            .py(r(if self.compact { 6. } else { 10. }))
            .child(div().mb(r(3.)).child(Mascot::new(self.id.clone(), self.mood, if self.compact { 64. } else { 88. }).motion(Motion::Bob)))
            .child(text(self.title, 17.0, 700, t.fg).line_height(px(25.5)).h(px(25.5)))
            .child(text(self.description, 13.5, 600, t.fg3).max_w(px(340.)).mt(r(1.)).line_height(px(16.25)));
        if let Some((label, f)) = self.action {
            col = col.child(div().mt(r(4.)).child(Button::primary(format!("{}-action", self.id), label).on_click(f)));
        }
        col
    }
}
