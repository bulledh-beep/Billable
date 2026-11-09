//! Menu.tsx's "…" trigger with its popover, for the detail pages (the
//! project and client headers, entry rows). A second click on the trigger
//! closes the menu, as in the app (its Popover ignores presses on the anchor).

use std::rc::Rc;

use gpui::{div, prelude::*, AnyElement, App, Context, IntoElement, ParentElement, SharedString, Styled, Window};

use crate::{
    components::{menu::just_closed, menu_list, popover, Align, Button, MenuEntry},
    icons::Lucide,
};

/// Which menu is open on a page (`K` tells menus apart, e.g. an entry id).
pub(crate) struct MenuState<K> {
    pub open: Option<K>,
}

impl<K> Default for MenuState<K> {
    fn default() -> Self {
        MenuState { open: None }
    }
}

impl<K: PartialEq> MenuState<K> {
    pub fn is_open(&self, key: &K) -> bool {
        self.open.as_ref() == Some(key)
    }
}

/// The `btn-icon-sm` "…" button and, while open (`open`, from the page's
/// [`MenuState`]), its menu.
pub(crate) fn menu_trigger<P: 'static, K: Clone + PartialEq + 'static>(
    id: impl Into<SharedString>,
    key: K,
    open: bool,
    state: fn(&mut P) -> &mut MenuState<K>,
    items: Vec<MenuEntry>,
    cx: &mut Context<P>,
) -> AnyElement {
    let id: SharedString = id.into();
    let pop_id: SharedString = format!("{id}-pop").into();
    let closed_id = pop_id.clone();
    let mut wrap = div().relative().flex_none().child(
        Button::icon_button(SharedString::from(format!("{id}-trigger")), Lucide::MoreHorizontal)
            .small()
            .selected(open)
            .tooltip("More actions")
            .on_click(cx.listener(move |p, _, _, cx| {
                cx.stop_propagation();
                // A click on the open menu's trigger has just closed it (click outside).
                let s = state(p);
                s.open = if s.is_open(&key) || just_closed(closed_id.clone()) { None } else { Some(key.clone()) };
                cx.notify();
            })),
    );
    if open {
        let this = cx.weak_entity();
        let close: Rc<dyn Fn(&mut Window, &mut App)> = Rc::new(move |_, cx| {
            let _ = this.update(cx, |p, cx| {
                state(p).open = None;
                cx.notify();
            });
        });
        let list = menu_list(&id, items, close.clone(), cx);
        wrap = wrap.child(popover(pop_id, Align::End, None, list, move |w, cx| close(w, cx), cx));
    }
    wrap.into_any_element()
}
