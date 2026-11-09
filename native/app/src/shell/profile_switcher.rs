//! ProfileSwitcher.tsx: the profile button at the bottom of the sidebar and
//! its menu of profiles. (New profile / Manage profiles are in
//! `pages::profiles`.)

use billable_core::profiles::Profile;
use gpui::{
    div, img, prelude::*, px, AnyElement, App, Context, Div, ElementId, Entity, IntoElement, ParentElement, Render, Styled,
    Window,
};

use gpui::Subscription;

use crate::{
    components::{ManageProfilesEvent, ManageProfilesModal, NewProfileEvent, NewProfileModal},
    icons::Lucide,
    store,
    style::{icon, r, text},
    theme::{hex, parse_hex, theme, Theme},
};

/// ProfileAvatar.tsx: the photo if set, else the first letter on the profile colour.
pub fn avatar(t: &Theme, p: &Profile, size: f32, font: f32) -> Div {
    let ring = t.rim.opacity(0.06);
    let base = div().size(px(size)).flex_none().rounded_full().border_1().border_color(ring);
    if let Some(png) = avatar_image(p) {
        return base.overflow_hidden().child(img(png).size_full());
    }
    let initial = p.name.trim().chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or("?".into());
    base.bg(parse_hex(&p.color))
        .flex()
        .items_center()
        .justify_center()
        .child(text(initial, font, 600, hex(0x1a1a1a)).line_height(px(size)))
}

/// The profile photo (a base64 PNG in profiles.json), decoded once.
pub fn avatar_image(p: &Profile) -> Option<std::sync::Arc<gpui::Image>> {
    use std::{cell::RefCell, collections::HashMap};
    thread_local! {
        static CACHE: RefCell<HashMap<String, std::sync::Arc<gpui::Image>>> = RefCell::new(HashMap::new());
    }
    let b64 = p.avatar.as_ref()?;
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if let Some(i) = c.get(b64) {
            return Some(i.clone());
        }
        let bytes = crate::format::base64_decode(b64)?;
        let img = std::sync::Arc::new(gpui::Image::from_bytes(gpui::ImageFormat::Png, bytes));
        c.insert(b64.clone(), img.clone());
        Some(img)
    })
}

pub struct ProfileSwitcher {
    open: bool,
    create: Option<Entity<NewProfileModal>>,
    manage: Option<Entity<ManageProfilesModal>>,
    modal_sub: Option<Subscription>,
}

impl ProfileSwitcher {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        ProfileSwitcher { open: false, create: None, manage: None, modal_sub: None }
    }
}

impl Render for ProfileSwitcher {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(cx);
        let s = store::store(cx);
        let active = s.active_profile();
        let profiles = s.profile_list();
        let (hover, open_bg) = (t.fg.opacity(0.05), t.fg.opacity(0.08));

        let mut trigger = div()
            .id("profile-trigger")
            .w_full()
            .flex()
            .items_center()
            .gap(r(2.))
            .h(px(30.))
            .px(r(2.))
            .rounded(px(6.))
            .cursor_pointer()
            .on_click(cx.listener(|this, _, _, cx| {
                this.open = !this.open;
                cx.notify();
            }))
            .child(avatar(&t, &active, 18.0, 9.0))
            .child(text(active.name.clone(), 13.0, 500, t.fg.opacity(0.85)).flex_1().min_w_0().truncate())
            .child(icon(Lucide::ChevronsUpDown).size(r(3.5)).text_color(t.fg4));
        trigger = if self.open { trigger.bg(open_bg) } else { trigger.hover(move |s| s.bg(hover)) };

        let mut wrap = div().relative().px(r(2.5)).flex_none().child(trigger);
        if self.open {
            let row_hover = t.fg.opacity(0.06);
            let row = |id: String| {
                div().id(ElementId::Name(id.into())).w_full().flex().items_center().gap(r(2.5)).px(r(3.)).h(px(26.)).cursor_pointer().hover(move |s| s.bg(row_hover))
            };
            let mut menu = div()
                .id("profile-menu")
                .absolute()
                .left(r(2.5))
                .right(r(2.5))
                .bottom_full()
                .mb(r(1.))
                .rounded(px(8.))
                .bg(t.panel)
                .shadow(t.shadow_pop())
                .py(r(1.))
                .overflow_hidden()
                .occlude()
                .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                    this.open = false;
                    cx.notify();
                }))
                .child(text("Profiles", 11.0, 600, t.fg3).px(r(3.)).pt(r(1.)).pb(r(1.)).line_height(px(14.)));
            for p in profiles {
                let id = p.id.clone();
                let is_active = p.id == active.id;
                let mut r_el = row(format!("profile-{}", p.id))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open = false;
                        if !is_active {
                            crate::shell::switch_profile(&id, window, cx);
                        }
                        cx.notify();
                    }))
                    .child(avatar(&t, &p, 13.0, 7.0))
                    .child(text(p.name.clone(), 13.0, 500, t.fg).flex_1().truncate().line_height(px(18.)));
                if is_active {
                    r_el = r_el.child(icon(Lucide::Check).size(r(3.5)).text_color(t.accent_text));
                }
                menu = menu.child(r_el);
            }
            menu = menu
                .child(div().border_t_1().border_color(t.line).my(r(1.)).mx(r(2.)))
                .child(
                    row("profile-new".into())
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open = false;
                            let m = cx.new(|cx| NewProfileModal::new(window, cx));
                            this.modal_sub = Some(cx.subscribe(&m, |this, _, _: &NewProfileEvent, cx| {
                                this.create = None;
                                cx.notify();
                            }));
                            this.create = Some(m);
                            cx.notify();
                        }))
                        .child(icon(Lucide::Plus).size(r(3.5)).text_color(t.fg3))
                        .child(text("New profile…", 13.0, 500, t.fg).line_height(px(18.))),
                )
                .child(
                    row("profile-manage".into())
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.open = false;
                            let m = cx.new(ManageProfilesModal::new);
                            this.modal_sub = Some(cx.subscribe(&m, |this, _, _: &ManageProfilesEvent, cx| {
                                this.manage = None;
                                cx.notify();
                            }));
                            this.manage = Some(m);
                            cx.notify();
                        }))
                        .child(icon(Lucide::Pencil).size(r(3.5)).text_color(t.fg3))
                        .child(text("Manage profiles…", 13.0, 500, t.fg).line_height(px(18.))),
                );
            wrap = wrap.child(menu);
        }
        wrap.children(self.create.clone()).children(self.manage.clone())
    }
}

pub fn render(view: &Entity<ProfileSwitcher>) -> AnyElement {
    view.clone().into_any_element()
}

#[allow(dead_code)]
fn unused(_: &App) {}
