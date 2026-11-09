//! Menu.tsx: the floating `Popover` panel and the "…" action menu.
//!
//! A popover is drawn above everything (deferred) and anchored to its
//! trigger: put the trigger and `popover(...)` together in a `relative`
//! wrapper. It closes on a click outside, like the original.

use std::{rc::Rc, time::Duration};

use gpui::{
    anchored, deferred, div, point, px, Animation, AnimationExt as _, AnyElement, App, Anchor, Div, ElementId,
    InteractiveElement, IntoElement, ParentElement, SharedString, StatefulInteractiveElement, Styled, Window,
};

use crate::{
    icons::Lucide,
    style::{icon, r, text},
    theme::{theme, Theme},
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Align {
    /// The panel's right edge lines up with the trigger's.
    End,
    /// The panel's left edge lines up with the trigger's.
    Start,
}

thread_local! {
    static CLOSED: std::cell::RefCell<Option<(SharedString, std::time::Instant)>> = const { std::cell::RefCell::new(None) };
}

/// Whether the popover `id` closed just now: a click on its own trigger
/// first closes it (mouse down outside) and must not reopen it on mouse up.
pub fn just_closed(id: impl Into<SharedString>) -> bool {
    let id = id.into();
    CLOSED.with(|c| c.borrow().as_ref().is_some_and(|(cid, at)| *cid == id && at.elapsed().as_millis() < 400))
}

/// The popover panel itself: 12px corners on the panel colour with the pop shadow.
pub fn popover_panel(t: &Theme) -> Div {
    div().rounded(px(12.)).bg(t.panel).shadow(t.shadow_pop())
}

/// A popover anchored under (or, near the window's bottom, above) its trigger.
pub fn popover(
    id: impl Into<ElementId>,
    align: Align,
    width: Option<f32>,
    content: impl IntoElement,
    on_close: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let t = theme(cx);
    let id: ElementId = id.into();
    let key: SharedString = id.to_string().into();
    let mut panel = popover_panel(&t)
        .id(id.clone())
        .occlude()
        .on_mouse_down_out(move |_, w, cx| {
            CLOSED.with(|c| *c.borrow_mut() = Some((key.clone(), std::time::Instant::now())));
            on_close(w, cx)
        })
        .child(content);
    if let Some(w) = width {
        panel = panel.w(px(w));
    }
    let panel = panel.with_animation(ElementId::Name(format!("{id}-fade").into()), Animation::new(Duration::from_millis(120)), |p, d| p.opacity(d));
    let corner = match align {
        Align::End => Anchor::TopRight,
        Align::Start => Anchor::TopLeft,
    };
    let mut spot = div().absolute().top_full();
    spot = match align {
        Align::End => spot.right_0(),
        Align::Start => spot.left_0(),
    };
    spot.child(deferred(anchored().anchor(corner).offset(point(px(0.), px(6.))).snap_to_window_with_margin(px(8.)).child(panel)).with_priority(20))
        .into_any_element()
}

/// One row of a menu (`MenuItem` in Menu.tsx).
#[derive(Clone)]
pub struct MenuItem {
    pub label: SharedString,
    pub icon: Option<Lucide>,
    pub danger: bool,
    pub disabled: bool,
    pub hint: Option<SharedString>,
    pub on_click: Rc<dyn Fn(&mut Window, &mut App)>,
}

impl MenuItem {
    pub fn new(label: impl Into<SharedString>, on_click: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        MenuItem { label: label.into(), icon: None, danger: false, disabled: false, hint: None, on_click: Rc::new(on_click) }
    }
    pub fn icon(mut self, i: Lucide) -> Self {
        self.icon = Some(i);
        self
    }
    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }
    pub fn disabled(mut self, d: bool) -> Self {
        self.disabled = d;
        self
    }
    pub fn hint(mut self, h: impl Into<SharedString>) -> Self {
        self.hint = Some(h.into());
        self
    }
}

/// `None` is a separator.
pub type MenuEntry = Option<MenuItem>;

/// The list inside a "…" menu. `close` runs before an item's action.
pub fn menu_list(id: &str, entries: Vec<MenuEntry>, close: Rc<dyn Fn(&mut Window, &mut App)>, cx: &App) -> Div {
    let t = theme(cx);
    // Drop separators at the ends and doubled ones, like the original.
    let mut tidy: Vec<MenuEntry> = Vec::new();
    for e in entries {
        if e.is_none() && (tidy.is_empty() || tidy.last().is_some_and(|l| l.is_none())) {
            continue;
        }
        tidy.push(e);
    }
    while tidy.last().is_some_and(|l| l.is_none()) {
        tidy.pop();
    }
    let mut list = div().p(px(5.)).min_w(px(190.)).flex().flex_col();
    for (i, entry) in tidy.into_iter().enumerate() {
        let Some(item) = entry else {
            list = list.child(div().my(px(5.)).mx(r(2.)).border_t_1().border_color(t.line));
            continue;
        };
        let (fg, hover_bg) = if item.danger { (t.red, t.red.opacity(0.1)) } else { (t.fg, t.fg.opacity(0.07)) };
        let mut row = div()
            .id(ElementId::Name(format!("{id}-item-{i}").into()))
            .w_full()
            .flex()
            .items_center()
            .gap(r(2.))
            .h(px(31.))
            .px(r(2.5))
            .rounded(px(8.))
            .text_color(fg);
        if item.disabled {
            row = row.opacity(0.4);
        } else {
            let close = close.clone();
            let f = item.on_click.clone();
            row = row.cursor_pointer().hover(move |s| s.bg(hover_bg)).on_click(move |_, w, cx| {
                cx.stop_propagation();
                close(w, cx);
                f(w, cx);
            });
        }
        if let Some(i) = item.icon {
            row = row.child(icon(i).size(px(14.)).text_color(if item.danger { t.red } else { t.fg3 }));
        }
        row = row.child(text(item.label.clone(), 13.5, 500, fg).flex_1().truncate());
        if let Some(h) = item.hint.clone() {
            row = row.child(text(h, 11.0, 500, t.fg4).line_height(px(14.)));
        }
        list = list.child(row);
    }
    list
}
