//! TimeTracking.tsx's `EntryRow` (Project detail shows it too, with
//! `show_project: false`) and `RunningClock`.

use std::rc::Rc;

use billable_core::models::{EntryRow, TimerRow};
use gpui::{
    div, prelude::*, px, AnyElement, App, Div, ElementId, Hsla, IntoElement, MouseButton, ParentElement, SharedString,
    StyledText, Styled, TextRun, Window,
};
use gpui_component::tooltip::Tooltip;

use crate::{
    components::{billing_chip, dial::stopwatch_dial, entry_editor::is_locked_entry, menu_list, popover, Align, Button, MenuItem},
    fonts::rounded_family,
    format::{duration_short, format_time, money, parse_time},
    icons::Lucide,
    shell::{app::navigate, route::Route},
    style::{dot, figures, num_font, r, separator, text},
    theme::{parse_hex, Theme},
    timer,
};

pub type Handler = Rc<dyn Fn(&mut Window, &mut App)>;

/// What a row's buttons and menu do. Leave out `restart`, `add_time` or
/// `set_billable` to drop those menu items, as the original's optional props.
pub struct RowHandlers {
    pub edit: Handler,
    pub delete: Handler,
    pub restart: Option<Handler>,
    pub add_time: Option<Rc<dyn Fn(f64, &mut Window, &mut App)>>,
    pub set_billable: Option<Rc<dyn Fn(bool, &mut Window, &mut App)>>,
    /// Open or close this row's "…" menu.
    pub toggle_menu: Handler,
    pub close_menu: Handler,
}

/// What a finished entry is worth (unbilled or not); 0 when it isn't billable.
pub fn entry_value(e: &EntryRow) -> f64 {
    if e.entry.is_billable != 0 {
        e.entry.duration_minutes / 60.0 * e.rate.unwrap_or(0.0)
    } else {
        0.0
    }
}

/// `.num` text.
pub fn num(s: impl Into<SharedString>, size: f32, weight: u16, color: Hsla) -> Div {
    div().font(num_font(weight)).text_size(px(size)).text_color(color).child(s.into())
}

/// A line mixing weights and colours (`<b>` or a `font-normal` span inside text).
pub fn rich(parts: &[(&str, u16, Hsla)], size: f32, line_height: f32) -> Div {
    let mut s = String::new();
    let mut runs = Vec::new();
    for (p, weight, color) in parts {
        s.push_str(p);
        runs.push(TextRun { len: p.len(), font: gpui::font(rounded_family(*weight)), color: *color, background_color: None, underline: None, strikethrough: None });
    }
    div().text_size(px(size)).line_height(px(line_height)).child(StyledText::new(s).with_runs(runs))
}

/// A grid column that takes `grow` shares of the free space, from a zero basis.
fn fr(el: Div, grow: f32) -> Div {
    let mut el = el.flex_basis(px(0.));
    el.style().flex_grow = Some(grow);
    el.style().flex_shrink = Some(1.0);
    el
}

/// BillingChip: links to its invoice when it's on one.
fn chip(t: &Theme, entry: &EntryRow) -> Option<AnyElement> {
    let c = billing_chip(t, &entry.billing_state, entry.invoice_number.as_deref())?;
    let Some(invoice) = entry.entry.invoice_id.filter(|i| *i != 0).filter(|_| entry.billing_state != "nonbillable") else {
        return Some(c.into_any_element());
    };
    let tip: SharedString = format!("Open {}", entry.invoice_number.clone().unwrap_or_default()).into();
    Some(
        div()
            .id(ElementId::Name(format!("entry-chip-{}", entry.entry.id).into()))
            .cursor_pointer()
            .hover(|s| s.underline())
            .tooltip(move |w, cx| Tooltip::new(tip.clone()).build(w, cx))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(move |_, w, cx| {
                cx.stop_propagation();
                navigate(Route::Invoice(invoice), w, cx);
            })
            .child(c)
            .into_any_element(),
    )
}

pub fn entry_row(entry: &EntryRow, show_project: bool, last: bool, menu_open: bool, h: RowHandlers, cx: &App) -> AnyElement {
    let t = crate::theme::theme(cx);
    let id = entry.entry.id;
    let locked = is_locked_entry(Some(entry));
    let unbilled = entry.billing_state == "unbilled";
    let nonbillable = entry.billing_state == "nonbillable";
    let value = entry_value(entry);
    let group: SharedString = format!("entry-row-{id}").into();

    let first = if show_project {
        let mut info = div()
            .min_w_0()
            .flex()
            .flex_col()
            .child(text(entry.project_name.clone().unwrap_or_default(), 13.0, 500, t.fg).truncate().line_height(px(18.)));
        if let Some(c) = entry.client_name.as_deref().filter(|c| !c.is_empty()) {
            info = info.child(text(c.to_string(), 12.0, 500, t.fg3).truncate().line_height(px(16.)));
        }
        div()
            .min_w_0()
            .flex()
            .items_center()
            .gap(r(2.5))
            .child(dot(parse_hex(entry.project_color.as_deref().unwrap_or("#8E8E93")), r(2.)))
            .child(info)
    } else {
        let day = parse_time(&entry.entry.start_time).map(|d| d.format("%a, %b %-d").to_string()).unwrap_or_default();
        div().min_w_0().flex().items_center().gap(r(2.5)).child(num(day, 13.0, 500, t.fg2).line_height(px(18.)))
    };

    let mut description = div()
        .id(ElementId::Name(format!("entry-desc-{id}").into()))
        .min_w_0()
        .child(text(entry.entry.description.clone(), 13.0, 500, t.fg2).truncate().line_height(px(18.)));
    if !entry.entry.description.is_empty() {
        let tip: SharedString = entry.entry.description.clone().into();
        description = description.tooltip(move |w, cx| Tooltip::new(tip.clone()).build(w, cx));
    }

    let range = format!(
        "{} – {}",
        format_time(&entry.entry.start_time),
        entry.entry.end_time.as_deref().map(format_time).unwrap_or_else(|| "…".into())
    );

    // The "…" menu
    let mut items: Vec<Option<MenuItem>> = Vec::new();
    if let Some(f) = h.restart.clone() {
        items.push(Some(MenuItem::new("Start timer for this", move |w, cx| f(w, cx)).icon(Lucide::RotateCcw)));
    }
    if let (false, Some(add)) = (locked, h.add_time.clone()) {
        items.push(None);
        for (label, minutes) in [("Add 15 minutes", 15.0), ("Add 30 minutes", 30.0), ("Add 1 hour", 60.0)] {
            let add = add.clone();
            items.push(Some(MenuItem::new(label, move |w, cx| add(minutes, w, cx)).icon(Lucide::Plus)));
        }
    }
    if let Some(set) = h.set_billable.clone() {
        if unbilled || nonbillable {
            items.push(None);
        }
        if unbilled {
            let set = set.clone();
            items.push(Some(MenuItem::new("Don't bill this", move |w, cx| set(false, w, cx)).icon(Lucide::Ban)));
        }
        if nonbillable {
            // CircleDollarSign isn't in the Lucide set yet; HandCoins is the nearest money glyph.
            items.push(Some(MenuItem::new("Make billable", move |w, cx| set(true, w, cx)).icon(Lucide::HandCoins)));
        }
    }
    items.push(None);
    let delete = h.delete.clone();
    items.push(Some(MenuItem::new("Delete", move |w, cx| delete(w, cx)).icon(Lucide::Trash2).danger()));

    let toggle = h.toggle_menu.clone();
    let mut menu_cell = div().relative().on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()).child(
        Button::icon_button(SharedString::from(format!("entry-menu-{id}")), Lucide::MoreHorizontal)
            .small()
            .selected(menu_open)
            .tooltip("More actions")
            .on_click(move |_, w, cx| {
                cx.stop_propagation();
                if !menu_open && crate::components::menu::just_closed(format!("entry-pop-{id}")) {
                    return;
                }
                toggle(w, cx);
            }),
    );
    if menu_open {
        let list = menu_list(&format!("entry-menu-{id}"), items, h.close_menu.clone(), cx);
        let close = h.close_menu.clone();
        menu_cell = menu_cell.child(popover(SharedString::from(format!("entry-pop-{id}")), Align::End, None, list, move |w, cx| close(w, cx), cx));
    }

    let edit = h.edit.clone();
    // Hidden until the row is hovered (or its menu is open, like focus-within)
    let mut actions = div()
        .w(px(60.))
        .flex_none()
        .flex()
        .items_center()
        .justify_end()
        .gap(r(0.5))
        .child(
            Button::icon_button(SharedString::from(format!("entry-edit-{id}")), Lucide::Pencil)
                .small()
                .tooltip("Edit")
                .on_click(move |_, w, cx| edit(w, cx)),
        )
        .child(menu_cell);
    if !menu_open {
        actions = actions.opacity(0.).group_hover(group.clone(), |s| s.opacity(1.));
    }

    let mut value_cell = num(if entry.entry.is_billable != 0 { money(value) } else { "—".into() }, 13.0, 500, if entry.entry.is_billable != 0 { t.fg3 } else { t.fg4 });
    value_cell = value_cell.w(px(84.)).flex_none().text_right().whitespace_nowrap();

    let hover = t.fg.opacity(0.025);
    let edit2 = h.edit.clone();
    let mut row = div()
        .id(ElementId::Name(format!("entry-row-{id}").into()))
        .group(group)
        .relative()
        .flex()
        .items_center()
        .gap(r(3.))
        .px(r(4.))
        .h(px(if show_project { 44. } else { 36. }))
        .hover(move |s| s.bg(hover))
        .on_click(move |e, w, cx| {
            if e.click_count() == 2 {
                edit2(w, cx);
            }
        })
        .child(fr(first, 1.3).min_w(px(130.)))
        .child(fr(div().min_w_0().child(description), 1.0))
        .child(num(range, 12.0, 500, t.fg3).line_height(px(16.)).whitespace_nowrap().text_right().flex_none())
        .child(div().flex().flex_none().justify_end().children(chip(&t, entry)))
        .child(num(duration_short(entry.entry.duration_minutes), 13.0, 500, t.fg).w(px(64.)).flex_none().text_right().whitespace_nowrap())
        .child(value_cell)
        .child(actions);
    if !last {
        row = row.child(separator(&t, if show_project { 32. } else { 16. }));
    }
    row.into_any_element()
}

/// RunningClock: the stopwatch dial, what's on the clock, and the live time
/// with what it has earned.
pub fn running_clock(t: &Theme, e: &TimerRow, paused: bool) -> Vec<AnyElement> {
    let seconds = timer::elapsed_seconds(e);
    let earned = timer::earned(e, seconds);
    let name = e.project_name.clone().unwrap_or_default();
    let what = if e.entry.description.is_empty() {
        rich(&[(&name, 600, t.fg)], 13.0, 19.5)
    } else {
        rich(&[(&name, 600, t.fg), (&format!(" · {}", e.entry.description), 500, t.fg3)], 13.0, 19.5)
    };
    let mut right = div()
        .flex()
        .flex_col()
        .items_end()
        .child(figures(timer::elapsed_text(seconds), 32.0, if paused { t.fg3 } else { t.fg }).line_height(px(32.)));
    if earned > 0.0 {
        right = right.child(
            div()
                .flex()
                .items_baseline()
                .mt(r(1.5))
                .child(figures(money(earned), 15.0, t.fg2).line_height(px(16.)))
                .child(text(format!(" at {}/hr", money(e.rate.unwrap_or(0.0))), 12.0, 500, t.fg3).line_height(px(16.))),
        );
    }
    vec![
        stopwatch_dial(t, seconds, 52.0, paused).into_any_element(),
        div()
            .min_w_0()
            .flex_1()
            .flex()
            .flex_col()
            .child(text(if paused { "Paused" } else { "On the clock" }, 12.0, 500, t.fg3).line_height(px(16.)))
            .child(what.truncate())
            .into_any_element(),
        right.into_any_element(),
    ]
}
