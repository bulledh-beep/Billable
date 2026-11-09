//! UpdateBanner.tsx with useUpdater.ts's state: the "Version X is available"
//! card in the sidebar above Settings, with release notes, install and
//! relaunch (or download), progress, and dismiss.
//!
//! The banner is one app-wide view; the sidebar shows it with [`view`]. On
//! creation it loads the cached status, checks GitHub quietly, and rechecks
//! every six hours. "Check for Updates…" in the app menu reaches it through
//! `services::updater::check_from_menu`.

use std::time::Duration;

use gpui::{
    div, percentage, prelude::*, px, relative, Animation, AnimationExt as _, AnyElement, App, AppContext, Context, Entity,
    Global, IntoElement, ParentElement, Render, Styled, Task, Transformation, Window,
};
use gpui_component::tooltip::Tooltip;

use super::{toast_error, toast_success, Button};
use crate::{
    fonts::rounded_family,
    icons::Lucide,
    motion,
    services::updater::{self, MenuCheck, UpdateProgress, UpdateStatus},
    store::{self, Store},
    style::{icon, num_font, r, text},
    theme::theme,
};

/// Re-check every 6 hours so a long-running session picks up new releases.
const RECHECK: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DownloadState {
    Idle,
    Downloading,
    Installing,
    Done,
    Error,
}

pub struct UpdateBanner {
    status: Option<UpdateStatus>,
    checking: bool,
    download: DownloadState,
    progress: Option<UpdateProgress>,
    can_install: bool,
    /// billable.update.dismissed: the version the user dismissed.
    dismissed: Option<String>,
    show_notes: bool,
    _recheck: Task<()>,
}

struct Slot(Entity<UpdateBanner>);
impl Global for Slot {}

/// The banner for the sidebar (created on first use). Renders nothing unless
/// an update is available and not dismissed.
pub fn view(cx: &mut App) -> AnyElement {
    if let Some(slot) = cx.try_global::<Slot>() {
        return slot.0.clone().into_any_element();
    }
    let banner = cx.new(UpdateBanner::new);
    cx.set_global(Slot(banner.clone()));
    banner.into_any_element()
}

/// `formatSize`: "" for none, KB under a megabyte, else MB to one place.
pub fn format_size(bytes: Option<f64>) -> String {
    match bytes {
        None => String::new(),
        Some(b) if b == 0.0 || b.is_nan() => String::new(),
        Some(b) if b < 1024.0 * 1024.0 => format!("{} KB", to_fixed(b / 1024.0, 0)),
        Some(b) => format!("{} MB", to_fixed(b / 1024.0 / 1024.0, 1)),
    }
}

/// `Number.prototype.toFixed` for the positive sizes here (exact ties round
/// up, as in JS, not to even).
fn to_fixed(x: f64, digits: usize) -> String {
    let exact = format!("{:.40}", x.abs());
    let (int_part, frac) = exact.split_once('.').unwrap_or((&exact, ""));
    let keep = &frac[..digits.min(frac.len())];
    let up = frac[keep.len()..].starts_with(['5', '6', '7', '8', '9']);
    let mut d: Vec<u8> = format!("{int_part}{keep}").into_bytes();
    if up {
        let mut i = d.len();
        loop {
            if i == 0 {
                d.insert(0, b'1');
                break;
            }
            i -= 1;
            if d[i] == b'9' {
                d[i] = b'0';
            } else {
                d[i] += 1;
                break;
            }
        }
    }
    let s = String::from_utf8(d).unwrap_or_default();
    if digits == 0 {
        s
    } else {
        format!("{}.{}", &s[..s.len() - digits], &s[s.len() - digits..])
    }
}

impl UpdateBanner {
    fn new(cx: &mut Context<Self>) -> Self {
        let dismissed = store::store(cx).prefs.update_dismissed.clone();
        // On mount: whether installing works, the cached status, then a quiet
        // fresh check (cached for five minutes by the service).
        let initial = updater::check(false, cx);
        cx.spawn(async move |this, cx| {
            let result = initial.await;
            let _ = this.update(cx, |b, cx| {
                if let Ok(s) = result {
                    b.status = Some(s);
                }
                b.checking = false;
                cx.notify();
            });
        })
        .detach();
        let recheck = cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(RECHECK).await;
            let Ok(task) = this.update(cx, |_, cx| updater::check(false, cx)) else {
                break;
            };
            if let Ok(s) = task.await {
                let _ = this.update(cx, |b, cx| {
                    b.status = Some(s);
                    cx.notify();
                });
            }
        });
        let weak = cx.weak_entity();
        updater::watch_menu_checks(
            "banner",
            move |event, cx| {
                let _ = weak.update(cx, |b, cx| b.menu_check(event, cx));
            },
            cx,
        );
        UpdateBanner {
            status: updater::cached(),
            checking: true,
            download: DownloadState::Idle,
            progress: None,
            can_install: updater::can_install(),
            dismissed,
            show_notes: false,
            _recheck: recheck,
        }
    }

    /// `checkNow` from the app menu: a toast when up to date or on failure.
    fn menu_check(&mut self, event: MenuCheck<'_>, cx: &mut Context<Self>) {
        match event {
            MenuCheck::Started => self.checking = true,
            MenuCheck::Finished(result) => {
                self.checking = false;
                match result {
                    Ok(s) => {
                        self.status = Some(s.clone());
                        if !s.update_available {
                            toast_success(format!("You're on the latest version ({})", s.current_version), cx);
                        }
                    }
                    Err(e) => toast_error(format!("Update check failed: {e}"), cx),
                }
            }
        }
        cx.notify();
    }

    fn on_progress(&self, cx: &mut Context<Self>) -> impl Fn(UpdateProgress, &mut App) + 'static {
        let this = cx.weak_entity();
        move |p, cx| {
            let _ = this.update(cx, |b, cx| {
                b.progress = Some(p);
                cx.notify();
            });
        }
    }

    /// Download the disk image and open it (when the app can't replace itself).
    fn download(&mut self, url: String, cx: &mut Context<Self>) {
        self.download = DownloadState::Downloading;
        self.progress = None;
        cx.notify();
        let task = updater::download(url, self.on_progress(cx), cx);
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |b, cx| {
                match result {
                    Ok(_) => {
                        b.download = DownloadState::Done;
                        toast_success("Update downloaded — drag Billable into Applications to install", cx);
                    }
                    Err(e) => {
                        b.download = DownloadState::Error;
                        toast_error(format!("Download failed: {e}"), cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Download, replace the app and relaunch; falls back to downloading.
    fn install(&mut self, cx: &mut Context<Self>) {
        let Some(url) = self.status.as_ref().and_then(|s| s.download_url.clone()) else {
            toast_error("No download available for this release", cx);
            return;
        };
        if !self.can_install {
            return self.download(url, cx);
        }
        self.download = DownloadState::Downloading;
        self.progress = None;
        cx.notify();
        let task = updater::install(url, self.on_progress(cx), cx);
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |b, cx| {
                match result {
                    Ok(()) => {
                        b.download = DownloadState::Installing;
                        toast_success("Update installed — Billable will quit and relaunch", cx);
                    }
                    Err(e) => {
                        b.download = DownloadState::Error;
                        toast_error(format!("Install failed: {e}"), cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn dismiss(&mut self, cx: &mut Context<Self>) {
        let Some(v) = self.status.as_ref().and_then(|s| s.latest_version.clone()) else {
            return;
        };
        let store = cx.global_mut::<Store>();
        store.prefs.update_dismissed = Some(v.clone());
        store.save_prefs();
        self.dismissed = Some(v);
        cx.notify();
    }

    /// Shown when an update is available and this version wasn't dismissed.
    fn show(&self) -> bool {
        self.status.as_ref().is_some_and(|s| s.update_available && s.latest_version.is_some() && self.dismissed != s.latest_version)
    }
}

impl Render for UpdateBanner {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(status) = self.status.clone().filter(|_| self.show()) else {
            return div().into_any_element();
        };
        let t = theme(cx);
        let installing = self.download == DownloadState::Installing;
        let downloading = self.download == DownloadState::Downloading;
        let idle = self.download == DownloadState::Idle;
        let percent = self.progress.map(|p| billable_core::js::js_number_to_string(p.percent));

        // Header: icon, title and the state's second line, dismiss.
        let head_icon = if installing {
            icon(Lucide::CheckCircle2).size(r(3.5)).text_color(t.green)
        } else {
            icon(Lucide::ArrowDownCircle).size(r(3.5)).text_color(t.accent_text)
        };
        let title = if installing {
            "Installing, Billable will relaunch".to_string()
        } else {
            format!("Version {} is available", status.latest_version.clone().unwrap_or_default())
        };
        let mut body = div().flex_1().min_w_0().child(text(title, 12.0, 600, t.fg).line_height(px(16.)).truncate());
        if idle {
            body = body.child(
                text(format!("You're on {} · {}", status.current_version, format_size(status.download_size_bytes)), 11.0, 500, t.fg3)
                    .line_height(px(14.))
                    .mt(r(0.5)),
            );
        }
        if downloading {
            let width = self.progress.map(|p| (p.percent / 100.0).clamp(0.0, 1.0) as f32).unwrap_or(0.05);
            let fill = div().h_full().bg(t.accent).w(relative(width));
            let fill = if width >= 1.0 { fill.rounded_full() } else { fill.rounded_l(r(0.5)) };
            body = body.child(
                div()
                    .mt(r(1.5))
                    .child(div().h(r(1.)).rounded_full().bg(t.fg.opacity(0.1)).overflow_hidden().child(fill))
                    .child(
                        div()
                            .mt(r(1.))
                            .text_size(px(11.))
                            .line_height(px(14.))
                            .text_color(t.fg3)
                            .font(num_font(500))
                            .child(percent.as_ref().map(|p| format!("{p}% · downloading")).unwrap_or_else(|| "Starting…".into())),
                    ),
            );
        }
        if installing {
            body = body.child(text("The new version will open in a moment.", 11.0, 500, t.fg3).line_height(px(14.)).mt(r(0.5)));
        }
        let mut head = div().flex().items_start().gap(r(2.)).child(head_icon.mt(r(0.5))).child(body);
        if !installing && !downloading {
            let hover = t.fg.opacity(0.1);
            head = head.child(
                div()
                    .id("update-banner-dismiss")
                    .flex_none()
                    .p(r(0.5))
                    .rounded(r(1.))
                    .cursor_pointer()
                    .hover(move |s| s.bg(hover))
                    .tooltip(|window, cx| Tooltip::new("Dismiss this update").build(window, cx))
                    .on_click(cx.listener(|b, _, _, cx| b.dismiss(cx)))
                    .child(icon(Lucide::X).size(r(3.)).text_color(t.fg3)),
            );
        }

        let mut card = div()
            .relative()
            .mx(r(2.5))
            .mb(r(2.))
            .px(r(3.))
            .py(r(2.5))
            .rounded(px(8.))
            .bg(t.panel)
            .border_1()
            .border_color(t.line)
            .shadow(t.shadow_card())
            .child(head);

        // Release notes preview
        if let Some(notes) = status.release_notes.clone().filter(|n| idle && !n.is_empty()) {
            let (fg, fg2) = (t.fg, t.fg2);
            let toggle = div()
                .id("update-banner-notes-toggle")
                .group("update-banner-notes")
                .flex()
                .items_center()
                .gap(r(1.))
                .cursor_pointer()
                .font_family(rounded_family(500))
                .text_size(px(11.))
                .line_height(px(14.))
                .text_color(fg2)
                .hover(move |s| s.text_color(fg))
                .on_click(cx.listener(|b, _, _, cx| {
                    b.show_notes = !b.show_notes;
                    cx.notify();
                }))
                .child(
                    icon(if self.show_notes { Lucide::ChevronUp } else { Lucide::ChevronDown })
                        .size(r(3.))
                        .text_color(fg2)
                        .group_hover("update-banner-notes", move |s| s.text_color(fg)),
                )
                .child(if self.show_notes { "Hide" } else { "What's new" });
            let mut section = div().mt(r(2.)).pt(r(2.)).border_t_1().border_color(t.line).child(div().flex().child(toggle));
            if self.show_notes {
                section = section.child(
                    div()
                        .id("update-banner-notes")
                        .mt(r(1.5))
                        .max_h(r(40.))
                        .overflow_y_scroll()
                        .child(text(notes, 11.0, 500, t.fg2).line_height(px(11.0 * 1.625))),
                );
            }
            card = card.child(section);
        }

        match self.download {
            DownloadState::Idle => {
                let label = if self.can_install { "Install and relaunch" } else { "Download update" };
                card = card.child(
                    div().mt(r(2.)).child(
                        Button::primary("update-banner-install", label)
                            .small()
                            .icon(Lucide::Download)
                            .full_width()
                            .on_click(cx.listener(|b, _, _, cx| b.install(cx))),
                    ),
                );
            }
            DownloadState::Downloading => {
                // A disabled .btn-secondary.btn-sm with a spinning RefreshCw.
                let spinner = icon(Lucide::RefreshCw).size(px(13.)).text_color(t.fg4);
                let spinner: AnyElement = if motion::reduced() {
                    spinner.into_any_element()
                } else {
                    spinner
                        .with_animation("update-banner-spin", Animation::new(Duration::from_secs(1)).repeat(), |s, d| {
                            s.with_transformation(Transformation::rotate(percentage(d)))
                        })
                        .into_any_element()
                };
                card = card.child(
                    div()
                        .mt(r(2.))
                        .w_full()
                        .h(px(27.))
                        .px(r(3.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap(r(1.5))
                        .rounded_full()
                        .bg(t.fg.opacity(0.06))
                        .whitespace_nowrap()
                        .child(spinner)
                        .child(text(format!("Downloading {}%…", percent.clone().unwrap_or_else(|| "0".into())), 12.0, 600, t.fg4)),
                );
            }
            DownloadState::Installing => {
                card = card.child(
                    div()
                        .mt(r(2.))
                        .w_full()
                        .h(r(7.))
                        .rounded(r(1.5))
                        .bg(t.green.opacity(0.1))
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap(r(1.5))
                        .child(icon(Lucide::CheckCircle2).size(r(3.)).text_color(t.green))
                        .child(text("Quitting and relaunching…", 12.0, 500, t.green).line_height(px(16.))),
                );
            }
            DownloadState::Error => {
                card = card.child(
                    div().mt(r(2.)).child(Button::danger("update-banner-retry", "Retry").small().full_width().on_click(cx.listener(|b, _, _, cx| b.install(cx)))),
                );
            }
            DownloadState::Done => {}
        }

        // Fades in (0.3s ease) and drops 10px into place (framer's default
        // spring for y).
        let card = card.flex_none();
        if motion::reduced() {
            return card.into_any_element();
        }
        let ease = motion::cubic_bezier(0.25, 0.1, 0.35, 1.0);
        card.with_animation("update-banner-in", Animation::new(Duration::from_millis(600)), move |c, delta| {
            let secs = delta * 0.6;
            let fade = ease((secs / 0.3).min(1.0));
            let y = motion::spring(secs, 500.0, 25.0, 1.0);
            c.opacity(fade).top(px(-10.0 * (1.0 - y)))
        })
        .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_like_format_size() {
        assert_eq!(format_size(None), "");
        assert_eq!(format_size(Some(0.0)), "");
        assert_eq!(format_size(Some(512.0)), "1 KB");
        assert_eq!(format_size(Some(1536.0)), "2 KB");
        assert_eq!(format_size(Some(113488898.0)), "108.2 MB");
        // An exact tie rounds up like toFixed (108.25 MB).
        assert_eq!(format_size(Some(108.25 * 1048576.0)), "108.3 MB");
        assert_eq!(format_size(Some(1048576.0)), "1.0 MB");
    }
}
