//! The window: sidebar, toolbar, navigation and the timer.

pub mod app;
pub mod content_hq;
pub mod profile_switcher;
pub mod route;
pub mod sidebar;
pub mod timer_control;
pub mod toolbar;

pub use app::{navigate, navigate_back, navigate_path, BillableApp, Header, Page};
pub use route::Route;

use gpui::{App, Window};

/// Switch to another profile (ipc `profile:switch`): stop a running timer,
/// open that profile's database and reload everything.
pub fn switch_profile(id: &str, window: &mut Window, cx: &mut App) {
    if crate::store::store(cx).timer.active.is_some() {
        crate::timer::stop(cx);
    }
    if let Err(e) = crate::store::switch_profile(id, cx) {
        crate::components::toast_error(format!("Couldn't switch profile: {e}"), cx);
        return;
    }
    navigate(Route::Dashboard, window, cx);
}
