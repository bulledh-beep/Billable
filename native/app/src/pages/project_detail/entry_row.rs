//! TimeTracking.tsx's `EntryRow` as ProjectDetail shows it (`showProject`
//! off): date, note, times, where the money is, duration, value, and the
//! edit button and "…" menu that appear on hover.

use billable_core::models::EntryRow;
use gpui::{div, prelude::*, px, AnyElement, Context, ElementId, IntoElement, ParentElement, SharedString, Styled, Window};
use gpui_component::tooltip::Tooltip;

use super::{is_locked, menu::menu_trigger, ProjectDetailPage};
use crate::{
    components::{billing_chip, Button, MenuItem},
    format::{self, duration_short, format_time},
    icons::Lucide,
    shell::{app::navigate, route::Route},
    style::{num_font, r, separator, text},
    theme::Theme,
    timer,
};

/// What the entry is worth (0 when it isn't billable).
fn entry_value(e: &EntryRow) -> f64 {
    if e.entry.is_billable != 0 { e.entry.duration_minutes / 60.0 * e.rate.unwrap_or(0.0) } else { 0.0 }
}

impl ProjectDetailPage {
    pub(super) fn entry_row(&self, t: &Theme, e: &EntryRow, last: bool, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let id = e.entry.id;
        let group: SharedString = format!("project-entry-{id}").into();
        let locked = is_locked(e);
        let unbilled = e.billing_state == "unbilled";
        let menu_open = self.entry_menu.is_open(&id);

        // Date (`toLocaleDateString('en-US', { weekday: 'short', month: 'short', day: 'numeric' })`)
        let date = format::parse_time(&e.entry.start_time).map(|d| d.format("%a, %b %-d").to_string()).unwrap_or_default();
        // minmax(130px, 1.3fr) beside minmax(0, 1fr): grow 1.3 : 1 from nothing
        let mut date_col = div().min_w(px(130.)).flex_basis(px(0.));
        date_col.style().flex_grow = Some(1.3);
        date_col = date_col.child(div().font(num_font(500)).text_size(px(13.)).line_height(px(18.)).text_color(t.fg2).whitespace_nowrap().child(date));

        let mut note = div()
            .id(ElementId::Name(format!("project-entry-note-{id}").into()))
            .flex_basis(px(0.))
            .flex_grow(1.)
            .min_w_0()
            .child(text(e.entry.description.clone(), 13.0, 500, t.fg2).line_height(px(18.)).truncate());
        if !e.entry.description.is_empty() {
            let tip = e.entry.description.clone();
            note = note.tooltip(move |w, cx| Tooltip::new(tip.clone()).build(w, cx));
        }

        let times = format!(
            "{} – {}",
            format_time(&e.entry.start_time),
            e.entry.end_time.as_deref().map(format_time).unwrap_or_else(|| "…".into())
        );

        // BillingChip: a link to the invoice when there is one
        let chip: AnyElement = match (billing_chip(t, &e.billing_state, e.invoice_number.as_deref()), e.entry.invoice_id.filter(|i| *i != 0)) {
            (Some(chip), Some(inv)) => {
                let tip = format!("Open {}", e.invoice_number.clone().unwrap_or_default());
                div()
                    .id(ElementId::Name(format!("project-entry-inv-{id}").into()))
                    .cursor_pointer()
                    .hover(|s| s.underline())
                    .tooltip(move |w, cx| Tooltip::new(tip.clone()).build(w, cx))
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(move |_, w, cx| {
                        cx.stop_propagation();
                        navigate(Route::Invoice(inv), w, cx)
                    })
                    .child(chip)
                    .into_any_element()
            }
            (Some(chip), None) => chip.into_any_element(),
            (None, _) => div().into_any_element(),
        };

        let value = entry_value(e);
        let billable = e.entry.is_billable != 0;

        // The "…" menu
        let this = cx.weak_entity();
        let mut items = Vec::new();
        {
            let (pid, desc) = (e.entry.project_id, e.entry.description.clone());
            items.push(Some(MenuItem::new("Start timer for this", move |_, cx| timer::start(pid, Some(desc.clone()), cx)).icon(Lucide::RotateCcw)));
        }
        if !locked {
            items.push(None);
            for (label, minutes) in [("Add 15 minutes", 15.0), ("Add 30 minutes", 30.0), ("Add 1 hour", 60.0)] {
                let (w, entry) = (this.clone(), e.clone());
                items.push(Some(
                    MenuItem::new(label, move |_, cx| {
                        let _ = w.update(cx, |p, cx| p.add_time(&entry, minutes, cx));
                    })
                    .icon(Lucide::Plus),
                ));
            }
        }
        if unbilled || e.billing_state == "nonbillable" {
            items.push(None);
        }
        if unbilled {
            let w = this.clone();
            items.push(Some(
                MenuItem::new("Don't bill this", move |_, cx| {
                    let _ = w.update(cx, |p, cx| p.set_billable(id, false, cx));
                })
                .icon(Lucide::Ban),
            ));
        }
        if e.billing_state == "nonbillable" {
            let w = this.clone();
            // CircleDollarSign isn't in the icon set yet; HandCoins stands in.
            items.push(Some(
                MenuItem::new("Make billable", move |_, cx| {
                    let _ = w.update(cx, |p, cx| p.set_billable(id, true, cx));
                })
                .icon(Lucide::HandCoins),
            ));
        }
        items.push(None);
        {
            let (w, entry) = (this.clone(), e.clone());
            items.push(Some(
                MenuItem::new("Delete", move |_, cx| {
                    let entry = entry.clone();
                    let _ = w.update(cx, |p, cx| {
                        p.deleting = Some(entry);
                        cx.notify();
                    });
                })
                .icon(Lucide::Trash2)
                .danger(),
            ));
        }

        let edit_entry = e.clone();
        let mut actions = div()
            .w(px(60.))
            .flex_none()
            .flex()
            .items_center()
            .justify_end()
            .gap(r(0.5))
            .child(
                Button::icon_button(SharedString::from(format!("project-entry-edit-{id}")), Lucide::Pencil)
                    .small()
                    .tooltip("Edit")
                    .on_click(cx.listener(move |p, _, w, cx| {
                        cx.stop_propagation();
                        p.open_editor(Some(edit_entry.clone()), w, cx)
                    })),
            )
            .child(menu_trigger(format!("project-entry-menu-{id}"), id, menu_open, |p: &mut Self| &mut p.entry_menu, items, cx));
        // opacity-0 group-hover:opacity-100 focus-within:opacity-100 (the
        // open menu keeps its trigger focused)
        if !menu_open {
            actions = actions.opacity(0.).group_hover(group.clone(), |s| s.opacity(1.));
        }

        let hover = t.fg.opacity(0.025);
        let dbl_entry = e.clone();
        let mut row = div()
            .id(ElementId::Name(format!("project-entry-row-{id}").into()))
            .group(group)
            .relative()
            .flex()
            .items_center()
            .gap(r(3.))
            .px(r(4.))
            .h(px(36.))
            .hover(move |s| s.bg(hover))
            .on_click(cx.listener(move |p, ev: &gpui::ClickEvent, w, cx| {
                if ev.click_count() == 2 {
                    p.open_editor(Some(dbl_entry.clone()), w, cx);
                }
            }))
            .child(date_col)
            .child(note)
            .child(text(times, 12.0, 500, t.fg3).font(num_font(500)).line_height(px(16.)).whitespace_nowrap().flex_none())
            .child(div().flex_none().flex().justify_end().child(chip))
            .child(div().w(px(64.)).flex_none().flex().justify_end().font(num_font(500)).text_size(px(13.)).text_color(t.fg).whitespace_nowrap().child(duration_short(e.entry.duration_minutes)))
            .child(
                div()
                    .w(px(84.))
                    .flex_none()
                    .flex()
                    .justify_end()
                    .font(num_font(500))
                    .text_size(px(13.))
                    .text_color(if billable { t.fg3 } else { t.fg4 })
                    .whitespace_nowrap()
                    .child(if billable { format::money(value) } else { "—".into() }),
            )
            .child(actions);
        if !last {
            row = row.child(separator(t, 16.0));
        }
        row.into_any_element()
    }
}
