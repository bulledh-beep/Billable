//! ProfileSwitcher.tsx's dialogs as modal views: "New profile" (name and
//! colour, then create and switch) and "Manage profiles" (rename, colour,
//! photo, delete). The switcher keeps `Option<Entity<..>>` of each, renders
//! it, and drops it on the view's event, like ClientFormModal.

use billable_core::{js::js_trim, profiles::Profile};
use gpui::{
    div, prelude::*, px, AnyElement, AppContext, Context, Div, ElementId, Entity, EventEmitter, Hsla,
    IntoElement, ParentElement, PathPromptOptions, Render, SharedString, Styled, Subscription, Window,
};
use gpui_component::{
    input::{InputEvent, InputState},
    tooltip::Tooltip,
};

use crate::{
    components::{confirm_dialog, fields::TextField, toast_error, toast_success, Button, Modal, ModalSize},
    icons::Lucide,
    shell::profile_switcher::avatar,
    store,
    style::{icon, label, r, text},
    theme::{hex, parse_hex, theme, Theme},
};

/// ProfileSwitcher.tsx `PROFILE_COLORS`
pub const PROFILE_COLORS: [&str; 8] = ["#F5A623", "#3498DB", "#2ECC71", "#9B59B6", "#E74C3C", "#1ABC9C", "#E67E22", "#EC407A"];

/// `ring-2 ring-offset-{offset} ring-offset-panel ring-fg/40` on a round
/// swatch: drawn as two round borders around it (a spread box-shadow keeps
/// the swatch's corner radius in GPUI, so it comes out squarish).
fn ring(t: &Theme, offset: f32, fill: Hsla) -> [Div; 3] {
    let band = |inset: f32, width: f32, color: Hsla| {
        div().absolute().top(px(-inset)).left(px(-inset)).right(px(-inset)).bottom(px(-inset)).rounded_full().border(px(width)).border_color(color)
    };
    // The swatch's own fill paints over its shadows' edges, as in Chromium.
    [band(offset + 2.0, 2.0, t.fg.opacity(0.4)), band(offset, offset, t.panel), div().absolute().size_full().rounded_full().bg(fill)]
}

fn swatch_color(c: &str) -> Hsla {
    parse_hex(c)
}

/// Esc closes the dialog from anywhere in the window (Modal.tsx listens on
/// the window, ahead of everything else), and nothing behind it sees the key.
fn close_on_escape<T: 'static>(cx: &mut Context<T>, closed: impl Fn(&mut Context<T>) + 'static) -> Subscription {
    let this = cx.weak_entity();
    cx.intercept_keystrokes(move |e, _, cx| {
        if e.keystroke.key == "escape" && !e.keystroke.modifiers.modified() {
            cx.stop_propagation();
            let _ = this.update(cx, |_, cx| closed(cx));
        }
    })
}

// ============================================================================
// New profile
// ============================================================================

pub enum NewProfileEvent {
    /// Created and switched to.
    #[allow(dead_code)]
    Created(Profile),
    Closed,
}

pub struct NewProfileModal {
    name: Entity<InputState>,
    color: &'static str,
    _subs: Vec<Subscription>,
}

impl EventEmitter<NewProfileEvent> for NewProfileModal {}

impl NewProfileModal {
    /// An empty name and a colour picked at random, the name focused.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("Side business"));
        let subs = vec![
            cx.subscribe_in(&name, window, |m, _, e: &InputEvent, window, cx| match e {
                InputEvent::PressEnter { .. } => m.create(window, cx),
                InputEvent::Change => cx.notify(),
                _ => {}
            }),
            close_on_escape(cx, |cx| cx.emit(NewProfileEvent::Closed)),
        ];
        name.update(cx, |s, cx| s.focus(window, cx));
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
        NewProfileModal { name, color: PROFILE_COLORS[(nanos as usize / 1000) % PROFILE_COLORS.len()], _subs: subs }
    }

    fn create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = js_trim(&self.name.read(cx).value()).to_string();
        if name.is_empty() {
            toast_error("Name is required", cx);
            return;
        }
        match store::store(cx).profiles.create_profile(&name, Some(self.color)) {
            Ok(created) => {
                cx.emit(NewProfileEvent::Created(created.clone()));
                // Switch straight into the new profile
                crate::shell::switch_profile(&created.id, window, cx);
            }
            Err(e) => toast_error(format!("Failed to create: {e}"), cx),
        }
    }
}

impl Render for NewProfileModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(cx);
        let mut swatches = div().flex().gap(r(2.));
        for c in PROFILE_COLORS {
            let mut s = div().id(ElementId::Name(format!("new-profile-color-{c}").into())).size(r(7.)).flex_none().rounded_full().cursor_pointer().bg(swatch_color(c));
            if self.color == c {
                s = s.relative().children(ring(&t, 2.0, swatch_color(c)));
            }
            swatches = swatches.child(s.on_click(cx.listener(move |m, _, _, cx| {
                m.color = c;
                cx.notify();
            })));
        }
        let body = div()
            .flex()
            .flex_col()
            .gap(r(4.))
            .child(div().child(label(&t, "Name")).child(TextField::new(&self.name)))
            .child(div().child(label(&t, "Color")).child(swatches));
        let (e1, e2) = (cx.entity().downgrade(), cx.entity().downgrade());
        Modal::new("new-profile", "New profile", move |_, cx| {
            let _ = e1.update(cx, |_, cx| cx.emit(NewProfileEvent::Closed));
        })
        .size(ModalSize::Sm)
        .description("Each profile has its own clients, projects, time, invoices, expenses, and tax settings.")
        .child(body)
        .footer(Button::secondary("new-profile-cancel", "Cancel").on_click(move |_, _, cx| {
            let _ = e2.update(cx, |_, cx| cx.emit(NewProfileEvent::Closed));
        }))
        .footer(Button::primary("new-profile-create", "Create and switch").on_click(cx.listener(|m, _, window, cx| m.create(window, cx))))
    }
}

// ============================================================================
// Manage profiles
// ============================================================================

pub enum ManageProfilesEvent {
    Closed,
}

pub struct ManageProfilesModal {
    profiles: Vec<Profile>,
    /// The profile being renamed and its name field.
    editing: Option<(String, Entity<InputState>)>,
    delete_id: Option<String>,
    edit_sub: Option<Subscription>,
    _esc: Subscription,
}

impl EventEmitter<ManageProfilesEvent> for ManageProfilesModal {}

impl ManageProfilesModal {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let profiles = store::store(cx).profile_list();
        let esc = close_on_escape(cx, |cx| cx.emit(ManageProfilesEvent::Closed));
        ManageProfilesModal { profiles, editing: None, delete_id: None, edit_sub: None, _esc: esc }
    }

    /// Reread the list, and redraw the window (the sidebar shows the active profile).
    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.profiles = store::store(cx).profile_list();
        cx.refresh_windows();
    }

    fn start_edit(&mut self, p: &Profile, window: &mut Window, cx: &mut Context<Self>) {
        let name = p.name.clone();
        let field = cx.new(|cx| InputState::new(window, cx).default_value(name));
        self.edit_sub = Some(cx.subscribe_in(&field, window, |m, _, e: &InputEvent, _, cx| {
            if let InputEvent::PressEnter { .. } = e {
                m.save_edit(cx);
            }
        }));
        field.update(cx, |s, cx| s.focus(window, cx));
        self.editing = Some((p.id.clone(), field));
        cx.notify();
    }

    fn save_edit(&mut self, cx: &mut Context<Self>) {
        let Some((id, field)) = self.editing.clone() else { return };
        let name = js_trim(&field.read(cx).value()).to_string();
        if name.is_empty() {
            toast_error("Name is required", cx);
            return;
        }
        match store::store(cx).profiles.rename_profile(&id, &name) {
            Ok(_) => {
                self.editing = None;
                self.edit_sub = None;
                self.refresh(cx);
                toast_success("Profile renamed", cx);
            }
            Err(e) => toast_error(format!("Failed: {e}"), cx),
        }
    }

    fn cancel_edit(&mut self, cx: &mut Context<Self>) {
        self.editing = None;
        self.edit_sub = None;
        cx.notify();
    }

    fn set_color(&mut self, id: &str, color: &str, cx: &mut Context<Self>) {
        match store::store(cx).profiles.update_profile_color(id, color) {
            Ok(_) => self.refresh(cx),
            Err(e) => toast_error(format!("Failed: {e}"), cx),
        }
    }

    /// profile:pick-avatar: choose an image, scale it to ≤256px, store it.
    fn pick_avatar(&mut self, id: String, cx: &mut Context<Self>) {
        let chosen = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: false, prompt: Some("Choose".into()) });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = chosen.await else { return };
            let Some(path) = paths.into_iter().next() else { return };
            let profiles = cx.update(|cx| store::store(cx).profiles.clone());
            // sips does the decoding and scaling, off the main thread
            let result = cx.background_executor().spawn(async move { profiles.set_profile_avatar(&id, &path) }).await;
            let _ = this.update(cx, |m, cx| match result {
                Ok(_) => {
                    m.refresh(cx);
                    toast_success("Profile photo updated", cx);
                }
                Err(e) => toast_error(format!("Failed: {e}"), cx),
            });
        })
        .detach();
    }

    fn clear_avatar(&mut self, id: &str, cx: &mut Context<Self>) {
        match store::store(cx).profiles.clear_profile_avatar(id) {
            Ok(_) => {
                self.refresh(cx);
                toast_success("Profile photo removed", cx);
            }
            Err(e) => toast_error(format!("Failed: {e}"), cx),
        }
    }

    fn confirm_delete(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.delete_id.take() else { return };
        match store::store(cx).profiles.delete_profile(&id) {
            Ok(()) => {
                self.refresh(cx);
                toast_success("Profile deleted", cx);
            }
            Err(e) => toast_error(format!("Failed: {e}"), cx),
        }
        cx.notify();
    }

    /// ProfileAvatar (md) with the photo controls that appear on hover.
    fn avatar_control(&self, t: &Theme, p: &Profile, cx: &mut Context<Self>) -> impl IntoElement {
        let group: SharedString = format!("profile-avatar-{}", p.id).into();
        let has_photo = p.avatar.is_some();
        let id = p.id.clone();
        let pick_tip: SharedString = if has_photo { "Change photo".into() } else { "Add photo".into() };
        let mut wrap = div()
            .id(ElementId::Name(group.clone()))
            .group(group.clone())
            .relative()
            .flex_none()
            .child(avatar(t, p, 29.25, 13.0))
            .child(
                div()
                    .id(ElementId::Name(format!("profile-pick-{}", p.id).into()))
                    .absolute()
                    .inset_0()
                    .rounded_full()
                    .bg(gpui::black().opacity(0.55))
                    .opacity(0.)
                    .group_hover(group.clone(), |s| s.opacity(1.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .tooltip(move |window, cx| Tooltip::new(pick_tip.clone()).build(window, cx))
                    .on_click(cx.listener(move |m, _, _, cx| m.pick_avatar(id.clone(), cx)))
                    .child(icon(Lucide::Camera).size(r(3.5)).text_color(hex(0xffffff))),
            );
        if has_photo {
            let id = p.id.clone();
            let red = t.red.opacity(0.15);
            wrap = wrap.child(
                div()
                    .id(ElementId::Name(format!("profile-clear-{}", p.id).into()))
                    .absolute()
                    .top(-r(1.))
                    .right(-r(1.))
                    .p(r(0.5))
                    .rounded_full()
                    .bg(t.panel)
                    .border_1()
                    .border_color(t.line)
                    .opacity(0.)
                    .group_hover(group, |s| s.opacity(1.))
                    .hover(move |s| s.bg(red))
                    .cursor_pointer()
                    .tooltip(|window, cx| Tooltip::new("Remove photo").build(window, cx))
                    .on_click(cx.listener(move |m, _, _, cx| m.clear_avatar(&id, cx)))
                    .child(icon(Lucide::ImageOff).size(r(2.5)).text_color(t.fg3)),
            );
        }
        wrap
    }

    fn profile_row(&self, t: &Theme, p: &Profile, active_id: &str, cx: &mut Context<Self>) -> AnyElement {
        let mut row = div()
            .flex()
            .items_center()
            .gap(r(3.))
            .p(r(3.))
            .rounded(px(6.5))
            .border_1()
            .border_color(t.line)
            .bg(t.panel2.opacity(0.6))
            .child(self.avatar_control(t, p, cx));
        let pid = p.id.clone();
        if let Some((_, field)) = self.editing.as_ref().filter(|(id, _)| *id == p.id) {
            row = row
                .child(div().flex_1().min_w_0().child(TextField::new(field)))
                .child(Button::primary(ElementId::Name(format!("profile-save-{pid}").into()), "Save").small().on_click(cx.listener(|m, _, _, cx| m.save_edit(cx))))
                .child(Button::secondary(ElementId::Name(format!("profile-cancel-{pid}").into()), "Cancel").small().on_click(cx.listener(|m, _, _, cx| m.cancel_edit(cx))));
            return row.into_any_element();
        }

        let is_active = p.id == active_id;
        let mut name_row = div().flex().items_center().gap(r(2.)).min_w_0().child(text(p.name.clone(), 13.0, 500, t.fg).line_height(px(18.)).truncate().min_w_0());
        if is_active {
            // `.badge text-accent-text`: currentColor on a 13% tint of itself
            name_row = name_row.child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .h(px(22.))
                    .px(r(2.5))
                    .rounded_full()
                    .bg(t.accent_text.opacity(0.13))
                    .child(text("Active", 12.0, 600, t.accent_text).whitespace_nowrap().line_height(px(16.))),
            );
        }
        let mut swatches = div().flex().gap(r(1.)).mt(r(1.5));
        for c in PROFILE_COLORS {
            let mut s = div()
                .id(ElementId::Name(format!("profile-color-{}-{c}", p.id).into()))
                .size(r(3.5))
                .flex_none()
                .rounded_full()
                .cursor_pointer()
                .bg(swatch_color(c))
                .tooltip(move |window, cx| Tooltip::new(c).build(window, cx));
            s = if p.color == c { s.relative().children(ring(t, 1.0, swatch_color(c))) } else { s.opacity(0.6).hover(|s| s.opacity(1.)) };
            let id = p.id.clone();
            swatches = swatches.child(s.on_click(cx.listener(move |m, _, _, cx| m.set_color(&id, c, cx))));
        }
        let profile = p.clone();
        row = row
            .child(div().flex_1().min_w_0().child(name_row).child(swatches))
            .child(
                Button::icon_button(ElementId::Name(format!("profile-rename-{pid}").into()), Lucide::Pencil)
                    .small()
                    .tooltip("Rename")
                    .on_click(cx.listener(move |m, _, window, cx| m.start_edit(&profile, window, cx))),
            );
        let can_delete = !is_active && self.profiles.len() > 1;
        if can_delete {
            row = row.child(
                Button::icon_button(ElementId::Name(format!("profile-delete-{pid}").into()), Lucide::Trash2)
                    .small()
                    .hover_text(t.red)
                    .tooltip("Delete profile")
                    .on_click(cx.listener(move |m, _, _, cx| {
                        m.delete_id = Some(pid.clone());
                        cx.notify();
                    })),
            );
        } else {
            // A disabled .btn-icon-sm looks the same but takes no clicks
            // (pointer-events: none, so no tooltip either).
            row = row.child(div().size(px(27.)).flex_none().flex().items_center().justify_center().child(icon(Lucide::Trash2).size(px(15.)).text_color(t.fg3)));
        }
        row.into_any_element()
    }
}

impl Render for ManageProfilesModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(cx);
        let active_id = store::store(cx).active_profile().id;
        let mut list = div().flex().flex_col().gap(r(2.));
        for p in self.profiles.clone() {
            list = list.child(self.profile_row(&t, &p, &active_id, cx));
        }
        let (e1, e2) = (cx.entity().downgrade(), cx.entity().downgrade());
        let modal = Modal::new("manage-profiles", "Manage profiles", move |_, cx| {
            let _ = e1.update(cx, |_, cx| cx.emit(ManageProfilesEvent::Closed));
        })
        .description("Deleting a profile permanently removes its data. The active profile can't be deleted.")
        .child(list)
        .footer(Button::primary("manage-profiles-done", "Done").on_click(move |_, _, cx| {
            let _ = e2.update(cx, |_, cx| cx.emit(ManageProfilesEvent::Closed));
        }));
        let mut root = div().child(modal);
        if self.delete_id.is_some() {
            let (a, b) = (cx.entity().downgrade(), cx.entity().downgrade());
            root = root.child(confirm_dialog(
                "delete-profile",
                "Delete profile?",
                "This permanently deletes all data in this profile. This cannot be undone.",
                "Delete",
                true,
                move |_, cx| {
                    let _ = a.update(cx, |m, cx| m.confirm_delete(cx));
                },
                move |_, cx| {
                    let _ = b.update(cx, |m, cx| {
                        m.delete_id = None;
                        cx.notify();
                    });
                },
                cx,
            ));
        }
        root
    }
}

