//! Content HQ inside Billable's window (ContentHQView.tsx, SuiteBar.tsx and
//! src/main/contenthq.ts). It's a separate website, so it runs in the Mac's
//! WebKit with its own cookie store, may only move between Content HQ and
//! its Supabase backend, and sends Google sign-in to the Mac's browser (Google
//! refuses embedded browsers); Content HQ hands the finished sign-in back as
//! a `billable://signin` link.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    div, img, prelude::*, px, App, Context, IntoElement, MouseButton, ParentElement, Render, Styled, Window,
};
use wry::{dpi::{LogicalPosition, LogicalSize}, NewWindowResponse, PageLoadEvent, Rect, WebView, WebViewBuilder, WebViewBuilderExtDarwin, WebViewExtMacOS};

use super::{app::{glide, navigate}, route::Route, timer_control::TimerControl};
use crate::{
    art::{Art, Gallery},
    components::{Button, Mascot, Mood, Motion},
    icons::Lucide,
    store,
    style::{r, text},
    theme::{inset, rgba4, shadow, theme},
};

pub const DEFAULT_URL: &str = "https://content-hq-live.vercel.app";
/// Content HQ's own cookie store (Electron's `persist:contenthq` partition).
const DATA_STORE: [u8; 16] = *b"billable-chq-v1\0";
/// Google says "this browser may not be secure" to embedded views; a plain
/// Safari identity avoids that for everything else Content HQ loads.
const SAFARI_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15";

/// The address from Settings, or the live workspace (contentHqUrl).
pub fn home_url(cx: &App) -> String {
    let raw = store::db(cx).get_settings().ok().and_then(|s| s.get("content_hq_url").map(str::to_string)).unwrap_or_default();
    let raw = raw.trim();
    let ok = raw.starts_with("https://") || raw.starts_with("http://localhost") || raw.starts_with("http://127.0.0.1");
    if raw.is_empty() || !ok { DEFAULT_URL.to_string() } else { raw.to_string() }
}

fn host_of(url: &str) -> Option<&str> {
    let rest = url.split_once("://")?.1;
    Some(rest.split(['/', '?', '#']).next().unwrap_or(rest))
}

fn hostname(host: &str) -> &str {
    host.split(':').next().unwrap_or(host)
}

/// Content HQ itself and its Supabase backend (isAllowedContentUrl).
fn allowed(url: &str, home: &str) -> bool {
    let local = url.starts_with("http://localhost") || url.starts_with("http://127.0.0.1");
    if !url.starts_with("https://") && !local {
        return url == "about:blank";
    }
    let (Some(h), Some(home_h)) = (host_of(url), host_of(home)) else { return false };
    h == home_h || hostname(h).ends_with(".supabase.co")
}

/// Google's sign-in pages, which open in the Mac's browser (isGoogleSignInHost).
fn google_sign_in(url: &str) -> bool {
    if !url.starts_with("https://") {
        return false;
    }
    let Some(h) = host_of(url).map(hostname) else { return false };
    if h == "accounts.youtube.com" {
        return true;
    }
    let mut parts = h.splitn(3, '.');
    let (a, b, rest) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    (a == "accounts" || a == "consent") && b == "google" && !rest.is_empty()
}

fn open_outside(url: &str) {
    if url.starts_with("http://") || url.starts_with("https://") {
        let _ = std::process::Command::new("open").arg(url).spawn();
    }
}

thread_local! {
    /// Links from the page, handled on the next tick with the app at hand.
    static FROM_VIEW: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    /// The view lives as long as the app. It's never dropped: at exit the
    /// thread's storage is torn down in no set order, and wry's teardown
    /// reaches into storage of its own that may be gone by then.
    static VIEW: RefCell<Option<std::mem::ManuallyDrop<Rc<WebView>>>> = const { RefCell::new(None) };
    /// The first page has loaded, so the view covers the placeholder for good.
    static LOADED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Take `billable://` links the page tried to open (connect links are only
/// honoured from here, never from a browser).
pub fn take_view_links() -> Vec<String> {
    FROM_VIEW.with(|l| std::mem::take(&mut *l.borrow_mut()))
}

/// Load a URL in the view (billable://signin finishing a Google sign-in).
pub fn load(url: &str) {
    VIEW.with(|v| {
        if let Some(view) = v.borrow().as_ref() {
            let _ = view.load_url(url);
        }
    });
}

pub fn view_exists() -> bool {
    VIEW.with(|v| v.borrow().is_some())
}

/// The WKWebView itself, for the move between the apps (glide.rs).
pub fn web_object() -> Option<glide::native::Object> {
    VIEW.with(|v| {
        let view = v.borrow();
        let web = view.as_ref()?.webview();
        // SAFETY: a live WKWebView, retained once more as a plain object.
        unsafe { objc2::rc::Retained::retain(objc2::rc::Retained::as_ptr(&web) as *mut objc2::runtime::AnyObject) }
    })
}

/// Hide the native view while Billable is showing.
pub fn hide() {
    VIEW.with(|v| {
        if let Some(view) = v.borrow().as_ref() {
            let _ = view.set_visible(false);
        }
    });
}

fn script(js: &str) {
    VIEW.with(|v| {
        if let Some(view) = v.borrow().as_ref() {
            let _ = view.evaluate_script(js);
        }
    });
}

pub struct ContentHq {
    timer: gpui::Entity<TimerControl>,
    failed: Option<String>,
}

impl ContentHq {
    pub fn new(timer: gpui::Entity<TimerControl>, _cx: &mut Context<Self>) -> Self {
        ContentHq { timer, failed: None }
    }

    /// Make the web view, or size and show it.
    pub fn ensure_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let size = window.viewport_size();
        let bounds = Rect {
            position: LogicalPosition::new(0.0, 48.0).into(),
            size: LogicalSize::new(f32::from(size.width) as f64, (f32::from(size.height) - 48.0).max(0.0) as f64).into(),
        };
        if VIEW.with(|v| v.borrow().is_some()) {
            VIEW.with(|v| {
                if let Some(view) = v.borrow().as_ref() {
                    let _ = view.set_bounds(bounds);
                    let _ = view.set_visible(true);
                }
            });
            return;
        }
        let home = home_url(cx);
        let nav_home = home.clone();
        let origin = home.trim_end_matches('/').to_string();
        // Tell Content HQ it's inside Billable (the billable_shell cookie),
        // so it shows its switch back to Billable.
        let cookie = format!(
            "try{{if(location.origin==={origin:?}){{document.cookie='billable_shell=1; path=/; max-age=31536000; samesite=lax{}'}}}}catch(e){{}}",
            if home.starts_with("https") { "; secure" } else { "" }
        );
        let builder = WebViewBuilder::new()
            .with_url(&home)
            .with_bounds(bounds)
            .with_user_agent(SAFARI_UA)
            .with_data_store_identifier(DATA_STORE)
            .with_initialization_script(cookie)
            .with_on_page_load_handler(|event, _| {
                if matches!(event, PageLoadEvent::Finished) {
                    LOADED.with(|l| l.set(true));
                }
            })
            .with_navigation_handler(move |url| {
                if url.starts_with("billable:") {
                    FROM_VIEW.with(|l| l.borrow_mut().push(url));
                    return false;
                }
                if allowed(&url, &nav_home) {
                    return true;
                }
                // Google sign-in and anything else open in the Mac's browser.
                open_outside(&url);
                false
            })
            .with_new_window_req_handler(|url, _| {
                if url.starts_with("billable:") {
                    FROM_VIEW.with(|l| l.borrow_mut().push(url));
                } else if google_sign_in(&url) || !url.is_empty() {
                    open_outside(&url);
                }
                NewWindowResponse::Deny
            });
        match builder.build_as_child(&*window) {
            Ok(view) => {
                VIEW.with(|v| *v.borrow_mut() = Some(std::mem::ManuallyDrop::new(Rc::new(view))));
                self.failed = None;
            }
            Err(e) => self.failed = Some(e.to_string()),
        }
    }
}

/// The app switch with Content HQ showing (SuiteSwitch value="content").
fn suite_switch(cx: &mut App) -> impl IntoElement {
    let t = theme(cx);
    let content_mark = cx.global_mut::<Gallery>().get(Art::ContentMark, 18.0);
    let billable_icon = glide::icon(glide::SuiteApp::Billable, 18.0, cx);
    let thumb = if t.dark {
        vec![inset(0.0, 0.5, 0.0, 0.0, rgba4(255, 255, 255, 0.1)), shadow(0.0, 1.0, 3.0, 0.0, rgba4(0, 0, 0, 0.45))]
    } else {
        vec![shadow(0.0, 1.0, 2.0, 0.0, rgba4(20, 20, 40, 0.14)), shadow(0.0, 3.0, 10.0, -4.0, rgba4(20, 20, 40, 0.2))]
    };
    div()
        .flex()
        .items_center()
        .p(px(3.))
        .rounded_full()
        .bg(t.fg.opacity(0.055))
        .shadow(vec![inset(0.0, 0.0, 0.0, 0.5, gpui::Rgba { a: 0.08, ..t.fg.to_rgb() })])
        .child(
            div()
                .id("suite-billable")
                .flex()
                .items_center()
                .h(px(26.))
                .px(px(7.))
                .opacity(if t.dark { 0.7 } else { 0.5 })
                .hover(|s| s.opacity(1.0))
                .cursor_pointer()
                .on_click(|_, w, cx| super::app::navigate_back(w, cx))
                .child(billable_icon),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(r(1.5))
                .h(px(26.))
                .pl(px(4.))
                .pr(px(10.))
                .rounded_full()
                .bg(if t.dark { t.panel2 } else { t.panel })
                .shadow(thumb)
                .child(img(content_mark).size(px(18.)).flex_none())
                .child(text("Content HQ", 12.0, 600, t.fg).line_height(px(16.))),
        )
}

impl Render for ContentHq {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.ensure_view(window, cx);
        let t = theme(cx);
        let bar = div()
            .id("suite-bar")
            .h(px(48.))
            .flex_none()
            .flex()
            .items_center()
            .gap(r(1.))
            .pl(px(78.))
            .pr(r(3.))
            .border_b_1()
            .border_color(t.line)
            .bg(t.sidebar)
            .on_mouse_down(MouseButton::Left, |_, w, _| w.start_window_move())
            .child(suite_switch(cx))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(r(0.5))
                    .ml(r(3.))
                    .child(Button::icon_button("chq-back", Lucide::ArrowLeft).small().tooltip("Back").on_click(|_, _, _| script("history.back()")))
                    .child(Button::icon_button("chq-forward", Lucide::ArrowRight).small().tooltip("Forward").on_click(|_, _, _| script("history.forward()")))
                    .child(Button::icon_button("chq-reload", Lucide::RotateCw).small().tooltip("Reload").on_click(|_, _, _| script("location.reload()")))
                    .child(Button::icon_button("chq-open", Lucide::ExternalLink).small().tooltip("Open in browser").on_click(|_, _, _| {
                        VIEW.with(|v| {
                            if let Some(url) = v.borrow().as_ref().and_then(|view| view.url().ok()) {
                                open_outside(&url);
                            }
                        })
                    })),
            )
            .child(div().flex_1())
            .child(self.timer.clone());
        // Under the native view: what shows while it loads, or why it can't.
        // While the apps glide, the ground shows around the moving page
        // instead; once a page has loaded the view covers this for good, so
        // the mascot (always moving) isn't kept drawing out of sight.
        let body = div().relative().flex_1().flex().flex_col().items_center().justify_center().text_center().px(r(6.));
        let body = match &self.failed {
            None if glide::running() => body,
            None if LOADED.with(|l| l.get()) => body.bg(t.bg),
            None => body
                .bg(t.bg)
                .child(Mascot::new("chq-wave", Mood::Wave, 92.0).motion(Motion::Bob))
                .child(text("Opening Content HQ…", 16.0, 600, t.fg).mt(r(3.))),
            Some(err) => body
                .bg(t.bg)
                .child(Mascot::new("chq-worried", Mood::Worried, 92.0))
                .child(text("Couldn't reach Content HQ", 17.0, 700, t.fg).mt(r(3.)))
                .child(text(format!("{err} Check your internet connection or the address in Settings."), 13.0, 500, t.fg3).max_w(px(360.)).mt(r(1.)))
                .child(
                    div()
                        .flex()
                        .gap(r(2.))
                        .mt(r(4.))
                        .child(Button::secondary("chq-settings", "Open Settings").on_click(|_, w, cx| navigate(Route::Settings, w, cx))),
                ),
        };
        div().size_full().flex().flex_col().child(bar).child(body)
    }
}

/// Handle `billable://` links the page tried to open: connect (phone sync,
/// only from Content HQ's own origin), switch, signin.
pub fn handle_view_links(cx: &mut App) {
    for url in take_view_links() {
        let Some(rest) = url.strip_prefix("billable://") else { continue };
        let (kind, query) = rest.split_once('?').unwrap_or((rest, ""));
        let param = |k: &str| query.split('&').find_map(|kv| kv.strip_prefix(&format!("{k}="))).map(|v| v.to_string());
        match kind.trim_end_matches('/') {
            "connect" => {
                let page = VIEW.with(|v| v.borrow().as_ref().and_then(|view| view.url().ok())).unwrap_or_default();
                let home = home_url(cx);
                if host_of(&page).is_some() && host_of(&page) == host_of(&home) {
                    if let Some(token) = param("token") {
                        crate::platform::phone_connect_from_view(token, cx);
                    }
                }
            }
            _ => crate::platform::queue_link(url),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules() {
        let home = "https://content-hq-live.vercel.app";
        assert!(allowed("https://content-hq-live.vercel.app/home", home));
        assert!(allowed("https://abc.supabase.co/auth/v1", home));
        assert!(!allowed("https://evil.example/", home));
        assert!(!allowed("http://content-hq-live.vercel.app/", home));
        assert!(google_sign_in("https://accounts.google.com/o/oauth2"));
        assert!(google_sign_in("https://accounts.google.ca/x"));
        assert!(!google_sign_in("https://google.com/"));
    }
}
