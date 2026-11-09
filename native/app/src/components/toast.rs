//! Toasts, styled like App.tsx's react-hot-toast setup: bottom-right, panel
//! colour, the pop shadow, 16px corners, 14px semibold, 3.2s by default.

use std::time::{Duration, Instant};

use gpui::{div, px, Animation, AnimationExt as _, AnyElement, App, ElementId, Global, IntoElement, ParentElement, SharedString, Styled};

use super::mascot::{Mascot, Mood};
use crate::{
    icons::Lucide,
    style::icon_stroke,
    theme::theme,
};

#[derive(Clone, PartialEq)]
pub enum ToastIcon {
    None,
    Success,
    Error,
    Mascot(Mood),
    /// The mascot hopping (starting a timer).
    MascotHop(Mood),
}

#[derive(Clone)]
struct Toast {
    id: u64,
    message: SharedString,
    icon: ToastIcon,
    until: Instant,
}

#[derive(Default)]
pub struct Toasts {
    next: u64,
    list: Vec<Toast>,
}

impl Global for Toasts {}

/// Show a toast for `ms` milliseconds (react-hot-toast's default is 3200;
/// success toasts last 2000, errors 4000).
pub fn toast_for(message: impl Into<SharedString>, icon: ToastIcon, ms: u64, cx: &mut App) {
    let store = cx.default_global::<Toasts>();
    store.next += 1;
    let id = store.next;
    store.list.push(Toast { id, message: message.into(), icon, until: Instant::now() + Duration::from_millis(ms) });
    cx.refresh_windows();
    cx.spawn(async move |cx| {
        cx.background_executor().timer(Duration::from_millis(ms + 20)).await;
        let _ = cx.update(|cx| {
            cx.default_global::<Toasts>().list.retain(|t| t.id != id);
            cx.refresh_windows();
        });
    })
    .detach();
}

pub fn toast(message: impl Into<SharedString>, icon: ToastIcon, cx: &mut App) {
    toast_for(message, icon, 3200, cx)
}

pub fn toast_success(message: impl Into<SharedString>, cx: &mut App) {
    toast_for(message, ToastIcon::Success, 2000, cx)
}

pub fn toast_error(message: impl Into<SharedString>, cx: &mut App) {
    toast_for(message, ToastIcon::Error, 4000, cx)
}

/// The stack, bottom-right with an 8px gutter (put it last in the window).
pub fn render_toasts(cx: &mut App) -> Option<AnyElement> {
    let t = theme(cx);
    let now = Instant::now();
    let list: Vec<Toast> = cx.try_global::<Toasts>().map(|s| s.list.iter().filter(|t| t.until > now).cloned().collect()).unwrap_or_default();
    if list.is_empty() {
        return None;
    }
    let mut stack = div().absolute().right(px(16.)).bottom(px(16.)).flex().flex_col().items_end().gap(px(8.));
    for toast in list {
        let mut row = div()
            .flex()
            .items_center()
            .max_w(px(420.))
            .py(px(10.))
            .px(px(14.))
            .rounded(px(16.))
            .bg(t.panel)
            .shadow(t.shadow_pop())
            .text_color(t.fg)
            .text_size(px(14.))
            .font_family(crate::fonts::rounded_family(600))
            .line_height(px(14.0 * 1.3));
        let icon_el: Option<AnyElement> = match &toast.icon {
            ToastIcon::None => None,
            ToastIcon::Success => Some(
                div().size(px(20.)).rounded_full().bg(t.green).flex().items_center().justify_center().child(icon_stroke(Lucide::Check, 3.5).size(px(12.)).text_color(t.panel)).into_any_element(),
            ),
            ToastIcon::Error => Some(
                div().size(px(20.)).rounded_full().bg(t.red).flex().items_center().justify_center().child(icon_stroke(Lucide::X, 3.5).size(px(11.)).text_color(t.panel)).into_any_element(),
            ),
            ToastIcon::Mascot(m) => Some(Mascot::new(format!("toast-{}", toast.id), *m, 26.0).into_any_element()),
            ToastIcon::MascotHop(m) => Some(Mascot::new(format!("toast-{}", toast.id), *m, 26.0).motion(super::mascot::Motion::Hop).into_any_element()),
        };
        if let Some(i) = icon_el {
            row = row.child(div().flex_none().child(i));
        }
        row = row.child(div().mx(px(10.)).my(px(4.)).child(toast.message.clone()));
        let row = row.with_animation(ElementId::Name(format!("toast-in-{}", toast.id).into()), Animation::new(Duration::from_millis(350)), |r, d| {
            let ease = crate::motion::cubic_bezier(0.21, 1.02, 0.73, 1.0)(d);
            r.opacity(d.min(1.0)).relative().top(px(20.0 * (1.0 - ease)))
        });
        stack = stack.child(row);
    }
    // Above every overlay (side panels, modals), like react-hot-toast's z-index.
    Some(gpui::deferred(stack).with_priority(100).into_any_element())
}
