//! WhatsNewModal.tsx: the release notes, once, on the first launch after an
//! update.
//!
//! On creation it compares this build's version with the last one seen
//! (billable.lastSeenVersion, now `prefs.last_seen_version`). A fresh install
//! just records the version. A newer version fetches its release notes and
//! shows them if there are any; closing marks the version seen. Network
//! errors stay quiet (and it tries again next launch).

use gpui::{
    div, prelude::*, px, AnyElement, App, AppContext, Context, Entity, FocusHandle, Global, IntoElement, ParentElement,
    Render, Styled, Window,
};

use super::{Button, Modal};
use crate::{
    icons::Lucide,
    services::updater::{self, ReleaseInfo},
    store::{self, Store},
    style::{icon, r, text},
    theme::theme,
};

pub struct WhatsNewModal {
    open: bool,
    info: Option<ReleaseInfo>,
    version: String,
    focus: FocusHandle,
}

struct Slot(Entity<WhatsNewModal>);
impl Global for Slot {}

/// The What's New modal for the window root (created on first use; renders
/// nothing unless it has notes to show).
pub fn view(cx: &mut App) -> AnyElement {
    if let Some(slot) = cx.try_global::<Slot>() {
        return slot.0.clone().into_any_element();
    }
    let modal = cx.new(WhatsNewModal::new);
    cx.set_global(Slot(modal.clone()));
    modal.into_any_element()
}

fn mark_seen(version: &str, cx: &mut App) {
    let store = cx.global_mut::<Store>();
    store.prefs.last_seen_version = Some(version.to_string());
    store.save_prefs();
}

impl WhatsNewModal {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let modal = WhatsNewModal { open: false, info: None, version: String::new(), focus: cx.focus_handle() };
        let current = updater::current_version();
        match store::store(cx).prefs.last_seen_version.clone() {
            // Fresh install: remember this version, no modal.
            None => mark_seen(current, cx),
            Some(seen) if seen == current => {}
            Some(_) => {
                let notes = updater::release_notes(current, cx);
                cx.spawn(async move |this, cx| {
                    let Ok(notes) = notes.await else {
                        return;
                    };
                    let _ = this.update(cx, |m, cx| match notes {
                        Some(info) if !billable_core::js::js_trim(&info.body).is_empty() => {
                            m.version = current.to_string();
                            m.info = Some(info);
                            m.open = true;
                            cx.notify();
                        }
                        // No notes: mark it seen so we don't keep trying.
                        _ => mark_seen(current, cx),
                    });
                })
                .detach();
            }
        }
        modal
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        if !self.version.is_empty() {
            mark_seen(&self.version, cx);
        }
        self.open = false;
        cx.notify();
    }
}

impl Render for WhatsNewModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(info) = self.info.clone().filter(|_| self.open) else {
            return div().into_any_element();
        };
        let t = theme(cx);
        let this = cx.weak_entity();
        let close = move |_: &mut Window, cx: &mut App| {
            let _ = this.update(cx, |m, cx| m.close(cx));
        };
        let close_btn = close.clone();
        let (fg3, accent) = (t.fg3, t.accent_text);
        let url = info.html_url.clone();
        let link = div()
            .id("whats-new-github")
            .group("whats-new-github")
            .mr_auto()
            .flex()
            .items_center()
            .gap(r(1.5))
            .cursor_pointer()
            .child(
                text("View on GitHub", 12.0, 500, fg3).line_height(px(16.)).group_hover("whats-new-github", move |s| s.text_color(accent)),
            )
            .child(icon(Lucide::ExternalLink).size(r(3.)).text_color(fg3).group_hover("whats-new-github", move |s| s.text_color(accent)))
            .on_click(move |_, _, cx| cx.open_url(&url));
        Modal::new("whats-new", format!("What's new in {}", info.name), close)
            .description(format!("You're now on version {}.", self.version))
            .focus(&self.focus)
            .child(
                div()
                    .id("whats-new-notes")
                    .max_h(r(80.))
                    .overflow_y_scroll()
                    .child(text(info.body, 13.0, 500, t.fg2).line_height(r(6.))),
            )
            .footer(link)
            .footer(Button::primary("whats-new-ok", "Got it").on_click(move |_, w, cx| close_btn(w, cx)))
            .into_any_element()
    }
}
