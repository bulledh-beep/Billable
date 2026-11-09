//! The app menu (src/main/menu.ts): the same items, shortcuts and the live
//! Timer menu ("Pause Timer"/"Resume Timer", "Recording: Project").

use gpui::{actions, App, KeyBinding, Menu, MenuItem, OsAction};

use super::{show_main_window, with_main_window};
use crate::{shell::navigate_path, store, timer};

actions!(
    billable,
    [
        About,
        CheckForUpdates,
        OpenSettings,
        Hide,
        HideOthers,
        ShowAll,
        Quit,
        NewClient,
        NewProject,
        NewTimeEntry,
        NewInvoice,
        CloseWindow,
        TogglePauseTimer,
        StopTimer,
        GoDashboard,
        GoClients,
        GoProjects,
        GoTime,
        GoBilling,
        GoReports,
        GoContent,
        ToggleFullScreen,
        Minimize,
        Zoom,
        BringAllToFront,
    ]
);

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-,", OpenSettings, None),
        KeyBinding::new("cmd-h", Hide, None),
        KeyBinding::new("cmd-alt-h", HideOthers, None),
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-n", NewClient, None),
        KeyBinding::new("cmd-shift-n", NewProject, None),
        KeyBinding::new("cmd-t", NewTimeEntry, None),
        KeyBinding::new("cmd-shift-i", NewInvoice, None),
        KeyBinding::new("cmd-w", CloseWindow, None),
        KeyBinding::new("cmd-shift-p", TogglePauseTimer, None),
        KeyBinding::new("cmd-shift-s", StopTimer, None),
        KeyBinding::new("cmd-1", GoDashboard, None),
        KeyBinding::new("cmd-2", GoClients, None),
        KeyBinding::new("cmd-3", GoProjects, None),
        KeyBinding::new("cmd-4", GoTime, None),
        KeyBinding::new("cmd-5", GoBilling, None),
        KeyBinding::new("cmd-6", GoReports, None),
        KeyBinding::new("cmd-7", GoContent, None),
        KeyBinding::new("cmd-ctrl-f", ToggleFullScreen, None),
        KeyBinding::new("cmd-m", Minimize, None),
    ]);

    let go = |path: &'static str| move |cx: &mut App| with_main_window(cx, move |w, cx| navigate_path(path, w, cx));
    cx.on_action(|_: &About, cx| super::about_panel(cx));
    cx.on_action(|_: &CheckForUpdates, cx| crate::platform::updater_check_from_menu(cx));
    cx.on_action(move |_: &OpenSettings, cx| go("/settings")(cx));
    cx.on_action(|_: &Hide, cx| cx.hide());
    cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
    cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.on_action(move |_: &NewClient, cx| go("/clients?action=new")(cx));
    cx.on_action(move |_: &NewProject, cx| go("/projects?action=new")(cx));
    cx.on_action(move |_: &NewTimeEntry, cx| go("/time?action=new")(cx));
    cx.on_action(move |_: &NewInvoice, cx| go("/invoices/new")(cx));
    cx.on_action(|_: &CloseWindow, cx| with_main_window(cx, |w, _| w.remove_window()));
    cx.on_action(|_: &TogglePauseTimer, cx| timer::toggle_pause(cx));
    cx.on_action(|_: &StopTimer, cx| timer::stop(cx));
    cx.on_action(move |_: &GoDashboard, cx| go("/")(cx));
    cx.on_action(move |_: &GoClients, cx| go("/clients")(cx));
    cx.on_action(move |_: &GoProjects, cx| go("/projects")(cx));
    cx.on_action(move |_: &GoTime, cx| go("/time")(cx));
    cx.on_action(move |_: &GoBilling, cx| go("/billing")(cx));
    cx.on_action(move |_: &GoReports, cx| go("/reports")(cx));
    cx.on_action(move |_: &GoContent, cx| go("/content")(cx));
    cx.on_action(|_: &ToggleFullScreen, cx| with_main_window(cx, |w, _| w.toggle_fullscreen()));
    cx.on_action(|_: &Minimize, cx| with_main_window(cx, |w, _| w.minimize_window()));
    cx.on_action(|_: &Zoom, cx| with_main_window(cx, |w, _| w.zoom_window()));
    cx.on_action(|_: &BringAllToFront, cx| show_main_window(cx));
    refresh(cx);
}

/// Rebuild the menu bar (the Timer menu follows the timer).
pub fn refresh(cx: &mut App) {
    let s = store::store(cx);
    let active = s.timer.active.clone();
    let paused = s.timer.paused.clone();
    let current = active.clone().or(paused.clone());
    let pause_label = if active.is_some() { "Pause Timer" } else if paused.is_some() { "Resume Timer" } else { "Pause Timer" };
    let status = match &current {
        Some(e) => format!("{}: {}", if active.is_some() { "Recording" } else { "Paused" }, e.project_name.clone().unwrap_or_else(|| "Unknown Project".into())),
        None => "No timer running".into(),
    };
    use gpui_component::input as edit;
    cx.set_menus([
        Menu::new("Billable").items([
            MenuItem::action("About Billable", About),
            MenuItem::action("Check for Updates…", CheckForUpdates),
            MenuItem::separator(),
            MenuItem::action("Settings", OpenSettings),
            MenuItem::separator(),
            MenuItem::action("Hide Billable", Hide),
            MenuItem::action("Hide Others", HideOthers),
            MenuItem::action("Show All", ShowAll),
            MenuItem::separator(),
            MenuItem::action("Quit Billable", Quit),
        ]),
        Menu::new("File").items([
            MenuItem::action("New Client", NewClient),
            MenuItem::action("New Project", NewProject),
            MenuItem::action("New Time Entry", NewTimeEntry),
            MenuItem::action("New Invoice", NewInvoice),
            MenuItem::separator(),
            MenuItem::action("Close Window", CloseWindow),
        ]),
        Menu::new("Edit").items([
            MenuItem::os_action("Undo", edit::Undo, OsAction::Undo),
            MenuItem::os_action("Redo", edit::Redo, OsAction::Redo),
            MenuItem::separator(),
            MenuItem::os_action("Cut", edit::Cut, OsAction::Cut),
            MenuItem::os_action("Copy", edit::Copy, OsAction::Copy),
            MenuItem::os_action("Paste", edit::Paste, OsAction::Paste),
            MenuItem::os_action("Select All", edit::SelectAll, OsAction::SelectAll),
        ]),
        Menu::new("Timer").items([
            MenuItem::action(pause_label, TogglePauseTimer).disabled(current.is_none()),
            MenuItem::action("Stop Timer", StopTimer).disabled(current.is_none()),
            MenuItem::separator(),
            MenuItem::action(status, StopTimer).disabled(true),
        ]),
        Menu::new("View").items([
            MenuItem::action("Dashboard", GoDashboard),
            MenuItem::action("Clients", GoClients),
            MenuItem::action("Projects", GoProjects),
            MenuItem::action("Time Tracking", GoTime),
            MenuItem::action("Billing", GoBilling),
            MenuItem::action("Reports", GoReports),
            MenuItem::separator(),
            MenuItem::action("Content HQ", GoContent),
            MenuItem::separator(),
            MenuItem::action("Toggle Full Screen", ToggleFullScreen),
        ]),
        Menu::new("Window").items([
            MenuItem::action("Minimize", Minimize),
            MenuItem::action("Zoom", Zoom),
            MenuItem::separator(),
            MenuItem::action("Bring All to Front", BringAllToFront),
        ]),
    ]);
}
