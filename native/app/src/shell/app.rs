//! The window's root (App.tsx): sidebar, the toolbar with the page's title
//! and actions plus the timer, the scrolling page, and toasts.

use std::time::Duration;

use gpui::{
    div, px, AnyElement, AnyView, App, AppContext, Context, Entity, FocusHandle, Global, InteractiveElement,
    IntoElement, ParentElement, Render, ScrollHandle, SharedString, StatefulInteractiveElement, Styled, WeakEntity,
    Window, WindowAppearance,
};

#[path = "glide.rs"]
pub mod glide;

use glide::{Glide, SuiteApp};

use super::{route::Route, sidebar, toolbar};
use crate::{
    components::toast::render_toasts,
    motion,
    pages,
    store::{self, Store},
    style::root_text,
    theme::{theme, Theme},
};

/// What a page puts in the window toolbar (PageHeader.tsx).
#[derive(Default)]
pub struct Header {
    pub title: SharedString,
    /// A quiet second line under the title.
    pub subtitle: Option<SharedString>,
    /// The parent page: a back button, and the subtitle if there's none.
    pub back: Option<(SharedString, Route)>,
    /// Small inline content after the title, such as a status badge.
    pub meta: Option<AnyElement>,
    /// Buttons on the right, before the timer.
    pub actions: Option<AnyElement>,
}

impl Header {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Header { title: title.into(), ..Default::default() }
    }
    pub fn subtitle(mut self, s: impl Into<SharedString>) -> Self {
        self.subtitle = Some(s.into());
        self
    }
    pub fn back(mut self, label: impl Into<SharedString>, to: Route) -> Self {
        self.back = Some((label.into(), to));
        self
    }
    pub fn meta(mut self, m: impl IntoElement) -> Self {
        self.meta = Some(m.into_any_element());
        self
    }
    pub fn actions(mut self, a: impl IntoElement) -> Self {
        self.actions = Some(a.into_any_element());
        self
    }
}

/// A screen. It renders its body and fills in the toolbar.
pub trait Page: Render + Sized + 'static {
    fn header(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Header;
}

pub trait PageHandle {
    fn view(&self) -> AnyView;
    fn header(&self, window: &mut Window, cx: &mut App) -> Header;
}

impl<P: Page> PageHandle for Entity<P> {
    fn view(&self) -> AnyView {
        self.clone().into()
    }
    fn header(&self, window: &mut Window, cx: &mut App) -> Header {
        self.update(cx, |p, cx| p.header(window, cx))
    }
}

struct Nav(WeakEntity<BillableApp>);
impl Global for Nav {}

/// Go to a route (from a page, the menu bar, the tray or a link).
pub fn navigate(route: Route, window: &mut Window, cx: &mut App) {
    if let Some(app) = cx.try_global::<Nav>().and_then(|n| n.0.upgrade()) {
        app.update(cx, |app, cx| app.go(route, window, cx));
    }
}

/// Whether Content HQ is the side showing in the window.
pub fn showing_content(cx: &App) -> bool {
    cx.try_global::<Nav>().and_then(|n| n.0.upgrade()).is_some_and(|app| *app.read(cx).route() == Route::Content)
}

/// Go back to the previous screen (the browser's history.back()).
pub fn navigate_back(window: &mut Window, cx: &mut App) {
    if let Some(app) = cx.try_global::<Nav>().and_then(|n| n.0.upgrade()) {
        app.update(cx, |app, cx| {
            if app.route == Route::Content {
                app.leave_content(None, window, cx);
                return;
            }
            let to = app.history.pop().unwrap_or(Route::Dashboard);
            if to == Route::Content {
                app.route = to;
                remember_suite(&app.route, cx);
                app.start_glide(SuiteApp::Billable, SuiteApp::Content, window, cx);
                cx.notify();
                return;
            }
            app.page = pages::open(&to, window, cx);
            app.page_route = to.clone();
            app.route = to;
            remember_suite(&app.route, cx);
            app.scroll.set_offset(gpui::point(px(0.), px(0.)));
            cx.notify();
        });
    }
}

/// Remember which app of the suite is showing (billable.suite.app), so the
/// next launch reopens it.
fn remember_suite(route: &Route, cx: &mut App) {
    let app = if *route == Route::Content { "content" } else { "billable" };
    let s = cx.global_mut::<Store>();
    if s.prefs.suite_app.as_deref() != Some(app) {
        s.prefs.suite_app = Some(app.to_string());
        s.save_prefs();
    }
}

/// Go to a path like "/billing?status=sent".
pub fn navigate_path(path: &str, window: &mut Window, cx: &mut App) {
    navigate(Route::parse(path), window, cx);
}

pub struct BillableApp {
    history: Vec<Route>,
    content: Option<Entity<super::content_hq::ContentHq>>,
    phone_connect: Entity<crate::services::phone_connect::PhoneConnectModal>,
    timer: Entity<super::timer_control::TimerControl>,
    profiles: Entity<super::profile_switcher::ProfileSwitcher>,
    route: Route,
    /// Billable's page, kept while Content HQ is up so Billable glides away
    /// and back as itself.
    page: Box<dyn PageHandle>,
    /// The route `page` shows (`Route::Content` until Billable has one, when
    /// the app opened on Content HQ).
    page_route: Route,
    /// The move between the apps, while it runs.
    glide: Option<Glide>,
    scroll: ScrollHandle,
    focus: FocusHandle,
    ticking: bool,
    /// Whether the page overflows, so it gives up 11px to its scrollbar
    /// (Chromium only takes the space when there is something to scroll).
    gutter: bool,
}

impl BillableApp {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let weak = cx.weak_entity();
        cx.set_global(Nav(weak));
        cx.observe_window_appearance(window, |_, _, cx| cx.notify()).detach();
        // Looping motion pauses in the background and resumes on focus.
        cx.observe_window_activation(window, |_, _, cx| cx.notify()).detach();
        // Dev and test hook: open on a given screen, e.g. BILLABLE_START_ROUTE=/billing
        // Otherwise reopen Content HQ if that's where you left off (App.tsx).
        let route = std::env::var("BILLABLE_START_ROUTE").map(|p| Route::parse(&p)).unwrap_or_else(|_| {
            if store::store(cx).prefs.suite_app.as_deref() == Some("content") { Route::Content } else { Route::Dashboard }
        });
        let page = pages::open(&route, window, cx);
        let timer = cx.new(|cx| super::timer_control::TimerControl::new(window, cx));
        let profiles = cx.new(super::profile_switcher::ProfileSwitcher::new);
        let phone_connect = cx.new(crate::services::phone_connect::PhoneConnectModal::new);
        let page_route = route.clone();
        let mut app = BillableApp { history: Vec::new(), content: None, phone_connect, timer, profiles, route, page, page_route, glide: None, scroll: ScrollHandle::new(), focus: cx.focus_handle(), ticking: false, gutter: true };
        app.ensure_ticking(cx);
        // Dev and test hook: switch between the apps this many times, 1.5s
        // apart, to watch the glide (with BILLABLE_GLIDE_LOG for its frames).
        if let Some(n) = std::env::var("BILLABLE_GLIDE_DEMO").ok().and_then(|v| v.parse::<u32>().ok()) {
            cx.spawn_in(window, async move |this, cx| {
                cx.background_executor().timer(Duration::from_millis(2500)).await;
                for _ in 0..n {
                    let ok = this.update_in(cx, |app, window, cx| {
                        let to = if app.route == Route::Content { Route::Dashboard } else { Route::Content };
                        app.go(to, window, cx);
                    });
                    if ok.is_err() {
                        break;
                    }
                    cx.background_executor().timer(Duration::from_millis(1500)).await;
                }
            })
            .detach();
        }
        app
    }

    fn content_view(&mut self, cx: &mut Context<Self>) -> Entity<super::content_hq::ContentHq> {
        let timer = self.timer.clone();
        self.content.get_or_insert_with(|| cx.new(|cx| super::content_hq::ContentHq::new(timer, cx))).clone()
    }

    /// Glide from one app to the other (App.tsx switchApp), from the icon
    /// for `to` in the switch showing now. Nothing moves under Reduce Motion.
    fn start_glide(&mut self, from: SuiteApp, to: SuiteApp, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(old) = self.glide.take() {
            old.finish();
        }
        // Content HQ's page is there from the start of the move, to fade in.
        if to == SuiteApp::Content {
            let content = self.content_view(cx);
            content.update(cx, |c, cx| c.ensure_view(window, cx));
        }
        let web = super::content_hq::web_object();
        let t = theme(cx);
        self.glide = Glide::start(from, to, web, &t, window, cx);
        if self.glide.is_some() {
            // Frames are asked for while it runs; this makes sure it's tidied
            // away even if none come (the window hidden, say).
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(Duration::from_millis(glide::TOTAL_MS as u64 + 80)).await;
                let _ = this.update(cx, |_, cx| cx.notify());
            })
            .detach();
        }
    }

    /// Over to Content HQ; Billable's page stays as it is underneath.
    fn open_content(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let previous = std::mem::replace(&mut self.route, Route::Content);
        self.history.push(previous);
        if self.history.len() > 50 {
            self.history.remove(0);
        }
        remember_suite(&self.route, cx);
        self.start_glide(SuiteApp::Billable, SuiteApp::Content, window, cx);
        cx.notify();
    }

    /// Back to Billable from Content HQ: where you were (`None`), or another
    /// screen.
    fn leave_content(&mut self, to: Option<Route>, window: &mut Window, cx: &mut Context<Self>) {
        let back = to.unwrap_or_else(|| if self.page_route == Route::Content { Route::Dashboard } else { self.page_route.clone() });
        if back != self.page_route {
            self.page = pages::open(&back, window, cx);
            self.page_route = back.clone();
            self.scroll.set_offset(gpui::point(px(0.), px(0.)));
        }
        if self.history.last() == Some(&back) {
            self.history.pop();
        } else {
            self.history.push(Route::Content);
            if self.history.len() > 50 {
                self.history.remove(0);
            }
        }
        self.route = back;
        remember_suite(&self.route, cx);
        self.start_glide(SuiteApp::Content, SuiteApp::Billable, window, cx);
        if self.glide.is_none() {
            super::content_hq::hide();
        }
        cx.notify();
    }

    pub fn route(&self) -> &Route {
        &self.route
    }

    pub fn go(&mut self, route: Route, window: &mut Window, cx: &mut Context<Self>) {
        if route == self.route {
            return;
        }
        if route == Route::Content {
            self.open_content(window, cx);
            return;
        }
        // Going back to Billable from Content HQ returns to where you were.
        if self.route == Route::Content {
            let to = if route == Route::Dashboard { None } else { Some(route) };
            self.leave_content(to, window, cx);
            return;
        }
        self.page = pages::open(&route, window, cx);
        self.page_route = route.clone();
        let previous = std::mem::replace(&mut self.route, route);
        remember_suite(&self.route, cx);
        self.history.push(previous);
        if self.history.len() > 50 {
            self.history.remove(0);
        }
        self.scroll.set_offset(gpui::point(px(0.), px(0.)));
        cx.notify();
    }

    /// While a timer runs, redraw once a second for the toolbar's clock.
    fn ensure_ticking(&mut self, cx: &mut Context<Self>) {
        if self.ticking {
            return;
        }
        self.ticking = true;
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(Duration::from_millis(1000)).await;
            let alive = this
                .update(cx, |_, cx| {
                    if store::store(cx).timer_running() {
                        cx.notify();
                    }
                })
                .is_ok();
            if !alive {
                break;
            }
        })
        .detach();
    }

    fn apply_theme(window: &Window, cx: &mut App) {
        let pref = store::store(cx).prefs.theme.clone().unwrap_or_else(|| "auto".into());
        let dark = match pref.as_str() {
            "dark" => true,
            "light" => false,
            _ => matches!(window.appearance(), WindowAppearance::Dark | WindowAppearance::VibrantDark),
        };
        let t = if dark { Theme::dark() } else { Theme::light() };
        if cx.try_global::<Theme>() != Some(&t) {
            cx.set_global(t);
            // Text selection and the caret in fields follow Billable's colours.
            let c = gpui_component::Theme::global_mut(cx);
            c.selection = t.accent.opacity(0.28);
            c.caret = t.fg;
            c.muted_foreground = t.fg4;
            c.foreground = t.fg;
        }
    }
}

impl BillableApp {
    /// Billable's screen: the sidebar, and the toolbar over the page.
    fn billable_layer(&mut self, window: &mut Window, cx: &mut Context<Self>) -> gpui::Div {
        let t = theme(cx);
        let header = self.page.header(window, cx);
        let section = self.page_route.section();
        // The last layout tells us whether the page overflows; if that changed,
        // lay out again with or without the scrollbar's 11px.
        let overflowing = self.scroll.max_offset().y > px(0.5) || (self.gutter && self.scroll.max_offset().y > px(0.));
        if self.scroll.bounds().size.height > px(0.) && overflowing != self.gutter {
            self.gutter = overflowing;
            cx.on_next_frame(window, |_, _, cx| cx.notify());
        }
        let gutter = if self.gutter { 11.0 } else { 0.0 };
        crate::style::PAGE_GUTTER.with(|g| g.set(self.gutter));

        let main = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .bg(t.bg)
            .child(toolbar::render(header, self.timer.clone(), window, cx))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .id("page-scroll")
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .left_0()
                            .right(px(gutter))
                            .overflow_y_scroll()
                            .track_scroll(&self.scroll)
                            .child(self.page.view()),
                    )
                    .child(scrollbar(&t, &self.scroll)),
            );

        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .child(sidebar::render(section, self.profiles.clone().into_any_element(), window, cx))
            .child(main)
    }
}

impl Render for BillableApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        motion::sync(cx);
        Self::apply_theme(window, cx);
        let t = theme(cx);

        // The move between the apps: done, or on to its next frame.
        if self.glide.as_ref().is_some_and(|g| g.finished()) {
            if self.route != Route::Content {
                super::content_hq::hide();
            }
            if let Some(g) = self.glide.take() {
                g.finish();
            }
        }
        if let Some(g) = self.glide.as_mut() {
            g.tick(window);
        }
        let on_content = self.route == Route::Content;
        let gliding = self.glide.is_some();

        let mut billable = (!on_content || gliding).then(|| self.billable_layer(window, cx));
        let mut content = (on_content || gliding).then(|| {
            let view = self.content_view(cx);
            div().absolute().top_0().left_0().size_full().child(view)
        });
        if let Some(g) = &self.glide {
            billable = billable.map(|l| l.opacity(g.opacity(SuiteApp::Billable)));
            content = content.map(|l| l.opacity(g.opacity(SuiteApp::Content)));
        }

        let mut root = root_text(div(), &t)
            .id("billable-root")
            .track_focus(&self.focus)
            .on_action(crate::components::select::handle_pick)
            .size_full()
            .relative()
            .flex()
            .bg(t.bg)
            .children(self.glide.as_ref().map(|g| g.ground()));
        // The app being opened is on top.
        root = if on_content { root.children(billable).children(content) } else { root.children(content).children(billable) };
        if let Some(g) = &self.glide {
            root = root.child(g.mark(cx));
        }
        if !on_content {
            root = root.child(crate::components::whats_new::view(cx));
        }
        root.child(self.phone_connect.clone()).children(render_toasts(cx))
    }
}

/// index.css's ::-webkit-scrollbar: an 11px gutter with a 5px pill
/// (an 11px thumb inside a 3px transparent border), fg at 14%.
fn scrollbar(t: &Theme, scroll: &ScrollHandle) -> gpui::Div {
    let track = div().absolute().top_0().bottom_0().right_0().w(px(11.));
    let view = scroll.bounds().size.height;
    let max = scroll.max_offset().y;
    if max <= px(0.) || view <= px(0.) {
        return track;
    }
    let content = view + max;
    let thumb = (view * (view / content)).max(px(20.));
    let ratio = (f32::from(-scroll.offset().y) / f32::from(max)).clamp(0.0, 1.0);
    let top = (view - thumb) * ratio;
    track.child(div().absolute().left(px(3.)).w(px(5.)).top(top + px(3.)).h(thumb - px(6.)).rounded_full().bg(t.fg.opacity(0.14)))
}

/// Open the main window's root view.
pub fn root(window: &mut Window, cx: &mut App) -> Entity<BillableApp> {
    cx.new(|cx| BillableApp::new(window, cx))
}

/// Check the store loaded; shows why if not.
pub fn store_ready(cx: &App) -> bool {
    cx.try_global::<Store>().is_some()
}
