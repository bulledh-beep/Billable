//! `<select class="input">`: the closed control is Billable's field with a
//! chevron; opening it shows the Mac's own pop-up menu with a check on the
//! current choice, exactly as Chromium does on macOS.

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use gpui::{
    px, Action, App, ElementId, InteractiveElement, IntoElement, ParentElement, SharedString, StatefulInteractiveElement,
    Styled, Window,
};
use gpui_component::native_menu::NativeMenu;

use super::fields::select_box;
use crate::theme::theme;

/// Sent by the native menu when an option is chosen.
#[derive(Clone, PartialEq, Debug, Action)]
#[action(namespace = billable, no_json)]
pub struct PickOption {
    pub menu: u64,
    pub index: usize,
}

type Pick = Rc<dyn Fn(usize, &mut Window, &mut App)>;

thread_local! {
    static PENDING: RefCell<(u64, HashMap<u64, Pick>)> = RefCell::new((0, HashMap::new()));
}

/// Route a chosen option to the select that opened the menu (registered on
/// the window root with `on_action`).
pub fn handle_pick(a: &PickOption, window: &mut Window, cx: &mut App) {
    let f = PENDING.with(|p| p.borrow_mut().1.remove(&a.menu));
    PENDING.with(|p| p.borrow_mut().1.clear());
    if let Some(f) = f {
        f(a.index, window, cx);
    }
}

/// A dropdown of `(value, label)` options.
pub fn select<T: Clone + PartialEq + 'static>(
    id: impl Into<ElementId>,
    options: Vec<(T, SharedString)>,
    value: &T,
    on_change: impl Fn(T, &mut Window, &mut App) + 'static,
    cx: &App,
) -> impl IntoElement {
    let t = theme(cx);
    let current = options.iter().position(|(v, _)| v == value);
    let label = current.map(|i| options[i].1.clone()).unwrap_or_default();
    let on_change = Rc::new(on_change);
    let options = Rc::new(options);
    select_box(&t, id, label, current.is_none()).on_click(move |e, window, cx| {
        let menu_id = PENDING.with(|p| {
            let mut p = p.borrow_mut();
            p.0 += 1;
            p.0
        });
        let opts = options.clone();
        let f = on_change.clone();
        let pick: Pick = Rc::new(move |i, w, cx| {
            if let Some((v, _)) = opts.get(i) {
                f(v.clone(), w, cx);
            }
        });
        PENDING.with(|p| {
            p.borrow_mut().1.insert(menu_id, pick);
        });
        let mut menu = NativeMenu::new();
        for (i, (_, label)) in options.iter().enumerate() {
            menu = menu.menu_with_check(label.clone(), Some(i) == current, Box::new(PickOption { menu: menu_id, index: i }));
        }
        let at = e.position();
        menu.show(gpui::point(at.x - px(12.), at.y - px(10.)), window, cx);
    })
}

#[allow(unused)]
fn _unused() -> impl IntoElement {
    gpui::div().child("")
}
