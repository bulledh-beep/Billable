//! Billing.tsx's invoice list: status filter, year and search, the `.tbl`
//! table with selection checkboxes and row menus, and the floating bulk
//! action bar.

use std::{collections::BTreeSet, rc::Rc, time::Duration};

use billable_core::models::{BillingOverview, Invoice};
use chrono::{Datelike, Local};
use gpui::{
    anchored, deferred, div, point, prelude::*, px, Animation, AnimationExt as _, AnyElement, App, Context, Div, ElementId,
    IntoElement, MouseButton, ParentElement, SharedString, Styled, Window,
};

use super::BillingPage;
use crate::{
    components::{
        badge::status_tone,
        fields::{checkbox, search_field},
        menu_list, popover, segmented, select,
        table::{layout, Column, Row},
        Align, Button, EmptyState, MenuItem, Segment,
    },
    export,
    fonts::rounded_family,
    format::{format_day, hours_short, money},
    icons::Lucide,
    motion,
    shell::{app::navigate, route::Route},
    style::{card, num_font, page_width, r, section_title, text},
    theme::Theme,
};

const TABLE_GROUP: &str = "bill-tbl";

use crate::components::table::measure;

impl BillingPage {
    pub(super) fn invoices_section(&mut self, t: &Theme, data: &BillingOverview, visible: &[Invoice], window: &mut Window, cx: &mut Context<Self>) -> Div {
        let by_year = self.by_year();
        let count = |s: &str| by_year.iter().filter(|i| i.status == s).count();
        let mut years: BTreeSet<i32> = BTreeSet::new();
        years.insert(Local::now().year());
        for i in &data.invoices {
            if let Some(y) = i.issue_date.get(..4).and_then(|y| y.parse::<i32>().ok()) {
                years.insert(y);
            }
        }
        let mut year_opts: Vec<(Option<i32>, SharedString)> = vec![(None, "All years".into())];
        year_opts.extend(years.iter().rev().filter(|y| **y != 0).map(|y| (Some(*y), SharedString::from(y.to_string()))));

        let this = cx.weak_entity();
        let this2 = this.clone();
        let head = div()
            .flex()
            .items_center()
            .gap(r(3.))
            .mb(r(2.))
            .child(section_title(t, "Invoices"))
            .child(segmented(
                "bill-filter",
                &self.filter,
                vec![
                    Segment::new("all".to_string(), "All").count(by_year.len()),
                    Segment::new("draft".to_string(), "Drafts").count(count("draft")),
                    Segment::new("sent".to_string(), "Awaiting").count(count("sent")),
                    Segment::new("overdue".to_string(), "Overdue").count(count("overdue")),
                    Segment::new("paid".to_string(), "Paid").count(count("paid")),
                ],
                move |v, _, cx| {
                    let _ = this.update(cx, |p, cx| {
                        p.filter = v;
                        cx.notify();
                    });
                },
                cx,
            ))
            .child(
                div()
                    .ml_auto()
                    .flex()
                    .items_center()
                    .gap(r(2.))
                    .child(div().w(r(28.)).child(select(
                        "bill-year",
                        year_opts,
                        &self.year,
                        move |v, _, cx| {
                            let _ = this2.update(cx, |p, cx| {
                                p.year = v;
                                cx.notify();
                            });
                        },
                        cx,
                    )))
                    .child(div().w(r(60.)).child(search_field("bill-search-clear", &self.search, cx))),
            );

        let body = if data.invoices.is_empty() {
            card(t).child(
                EmptyState::new("bill-empty", "No invoices yet", "Create one from your unbilled time. The time you pick is linked to the invoice.")
                    .action("New invoice", |_, w, cx| navigate(Route::InvoiceNew { client_id: None, project_id: None, project_ids: vec![] }, w, cx)),
            )
        } else {
            card(t).overflow_hidden().child(self.table(t, visible, window, cx))
        };
        div().flex().flex_col().child(head).child(body)
    }

    /// The `.tbl` of invoices. Laid out with the shared table's column
    /// algorithm, drawn here because the header holds a checkbox.
    fn table(&mut self, t: &Theme, visible: &[Invoice], window: &mut Window, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let all_selected = !visible.is_empty() && visible.iter().all(|i| self.selected.contains(&i.id));
        let any_selected = !self.selected.is_empty();
        let columns = vec![
            Column::new("").fixed(r(8.).into()),
            Column::new("Invoice"),
            Column::new("Client"),
            Column::new("Issued"),
            Column::new("Due / paid"),
            Column::new("Status"),
            Column::new("Amount").right(),
            Column::new("").fixed(r(10.).into()),
        ];
        let n = columns.len();

        // Measure every cell (max-content), then build the rows.
        struct Line {
            inv: Invoice,
            cells: Vec<(AnyElement, f32)>,
        }
        let mut lines: Vec<Line> = Vec::new();
        for inv in visible {
            let id = inv.id;
            let is_sel = self.selected.contains(&id);
            let g = SharedString::from(format!("bill-row-{id}"));
            let mut cells: Vec<(AnyElement, f32)> = Vec::new();

            // Checkbox: hidden until the row is hovered, unless selected
            let mut cb = checkbox(t, SharedString::from(format!("bill-cb-{id}")), is_sel, false).on_click(cx.listener(move |p, _, _, cx| {
                if !p.selected.remove(&id) {
                    p.selected.insert(id);
                }
                cx.notify();
            }));
            if !is_sel {
                cb = cb.opacity(0.).group_hover(g.clone(), |s| s.opacity(1.));
            }
            cells.push((div().on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()).child(cb).into_any_element(), 17.0));

            // Invoice: number over project · hours
            let sub = {
                let base = match (&inv.project_name, inv.project_count) {
                    (Some(p), _) if !p.is_empty() => p.clone(),
                    (_, c) if c > 1 => format!("{c} projects"),
                    _ => "No project".to_string(),
                };
                if inv.entry_hours > 0.0 { format!("{base} · {}", hours_short(inv.entry_hours)) } else { base }
            };
            let w = measure(window, &inv.invoice_number, 13.0, 500, false).max(measure(window, &sub, 12.0, 500, false).min(220.0));
            cells.push((
                div()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(text(inv.invoice_number.clone(), 13.0, 500, t.fg).line_height(px(19.5)).whitespace_nowrap())
                    .child(text(sub, 12.0, 500, t.fg3).line_height(px(16.)).max_w(px(220.)).truncate())
                    .into_any_element(),
                w,
            ));

            // Client, issued
            let client = inv.client_name.clone().unwrap_or_default();
            cells.push((text(client.clone(), 14.0, 500, t.fg2).truncate().into_any_element(), measure(window, &client, 14.0, 500, false)));
            let issued = format_day(&inv.issue_date);
            cells.push((text(issued.clone(), 14.0, 500, t.fg2).whitespace_nowrap().into_any_element(), measure(window, &issued, 14.0, 500, false)));

            // Due / paid
            let days = inv.days_past_due.unwrap_or(0);
            let (due, color) = match inv.status.as_str() {
                "paid" => (format!("Paid {}", format_day(inv.payment_date.as_deref().unwrap_or(&inv.issue_date))), t.fg2),
                "overdue" => (format!("{days} day{} late", if days == 1 { "" } else { "s" }), t.red),
                "draft" => ("Not sent".to_string(), t.fg3),
                _ => (format!("Due {}", format_day(&inv.due_date)), t.fg2),
            };
            cells.push((text(due.clone(), 14.0, 500, color).whitespace_nowrap().into_any_element(), measure(window, &due, 14.0, 500, false)));

            // Status
            let (_, label) = status_tone(&inv.status);
            let label = if label.is_empty() { inv.status.clone() } else { label.to_string() };
            cells.push((crate::components::status_badge(t, &inv.status).into_any_element(), measure(window, &label, 12.0, 600, false) + 2.0 * 8.125));

            // Amount
            let amount = money(inv.total);
            cells.push((
                div().font(num_font(500)).text_size(px(14.)).text_color(t.fg).whitespace_nowrap().child(amount.clone()).into_any_element(),
                measure(window, &amount, 14.0, 500, true),
            ));

            // Menu
            cells.push((self.row_menu(inv, cx).into_any_element(), 27.0));
            lines.push(Line { inv: inv.clone(), cells });
        }

        let header_widths: Vec<f32> = columns.iter().map(|c| measure(window, &c.header, 12.0, 600, false)).collect();
        let sizing: Vec<Row> = lines
            .iter()
            .map(|l| Row::new(ElementId::Name("m".into()), l.cells.iter().map(|(_, w)| crate::components::table::Cell::new(div(), *w)).collect()))
            .collect();
        let total = page_width(window) - 2.0;
        let widths = layout(&columns, &header_widths, &sizing, total);
        let pads = |i: usize| (if i == 0 { 13.0 } else { 9.75 }, if i + 1 == n { 13.0 } else { 9.75 });
        let cell = |i: usize, height: f32, align: crate::components::table::Align| {
            let (l, rr) = pads(i);
            let d = div().w(px(widths[i])).flex_none().h(px(height)).pl(px(l)).pr(px(rr)).flex().items_center().min_w_0().overflow_hidden();
            match align {
                crate::components::table::Align::Right => d.justify_end(),
                crate::components::table::Align::Center => d.justify_center(),
                crate::components::table::Align::Left => d,
            }
        };

        // Header, with select-all (shown on hover over the table, or while anything is selected)
        let mut head = div().flex().border_b_1().border_color(t.line);
        let visible_ids: Vec<i64> = visible.iter().map(|i| i.id).collect();
        for (i, c) in columns.iter().enumerate() {
            let mut h = cell(i, 35.0, c.align);
            if i == 0 {
                let ids = visible_ids.clone();
                let mut cb = checkbox(t, "bill-cb-all", all_selected, any_selected && !all_selected).on_click(cx.listener(move |p, _, _, cx| {
                    let all = !ids.is_empty() && ids.iter().all(|id| p.selected.contains(id));
                    p.selected = if all { Default::default() } else { ids.iter().copied().collect() };
                    cx.notify();
                }));
                if !any_selected {
                    cb = cb.opacity(0.).group_hover(TABLE_GROUP, |s| s.opacity(1.));
                }
                h = h.child(cb);
            } else {
                h = h.child(div().font_family(rounded_family(600)).text_size(px(12.)).text_color(t.fg3).whitespace_nowrap().child(c.header.clone()));
            }
            head = head.child(h);
        }

        let mut body = div().flex().flex_col();
        let count = lines.len();
        let hover = t.fg.opacity(0.03);
        for (ri, line) in lines.into_iter().enumerate() {
            let id = line.inv.id;
            let last = ri + 1 == count;
            let height = if last { 50.0 } else { 49.0 };
            let mut row = div()
                .id(SharedString::from(format!("bill-tr-{id}")))
                .group(SharedString::from(format!("bill-row-{id}")))
                .flex()
                .cursor_pointer()
                .hover(move |s| s.bg(hover))
                .on_click(move |_, w, cx| navigate(Route::Invoice(id), w, cx));
            if self.selected.contains(&id) {
                row = row.bg(t.accent.opacity(0.05));
            }
            if !last {
                row = row.border_b_1().border_color(t.line);
            }
            for (i, (content, _)) in line.cells.into_iter().enumerate() {
                row = row.child(cell(i, height, columns[i].align).child(content));
            }
            body = body.child(row);
        }
        if count == 0 {
            body = body.child(div().h(r(20.)).flex().items_center().justify_center().child(text("No invoices match these filters", 14.0, 500, t.fg3)));
        }
        div().id("bill-table").group(TABLE_GROUP).w(px(total)).text_size(px(14.)).child(head).child(body)
    }

    fn row_menu(&self, inv: &Invoice, cx: &mut Context<Self>) -> Div {
        let id = inv.id;
        let open = self.menu == Some(id);
        let mut wrap = div().relative().on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()).child(
            Button::icon_button(SharedString::from(format!("bill-menu-{id}")), Lucide::MoreHorizontal)
                .small()
                .selected(open)
                .tooltip("More actions")
                .on_click(cx.listener(move |p, _, _, cx| {
                    cx.stop_propagation();
                    if p.menu != Some(id) && crate::components::menu::just_closed(format!("bill-pop-{id}")) {
                        return;
                    }
                    p.menu = if p.menu == Some(id) { None } else { Some(id) };
                    cx.notify();
                })),
        );
        if open {
            let this = cx.weak_entity();
            let close: Rc<dyn Fn(&mut Window, &mut App)> = {
                let this = this.clone();
                Rc::new(move |_, cx| {
                    let _ = this.update(cx, |p, cx| {
                        p.menu = None;
                        cx.notify();
                    });
                })
            };
            let mut items: Vec<Option<MenuItem>> = vec![
                Some(MenuItem::new("Open", move |w, cx| navigate(Route::Invoice(id), w, cx)).icon(Lucide::ExternalLink)),
                Some(MenuItem::new("Download PDF", move |w, cx| export::invoice_pdf(id, w, cx)).icon(Lucide::Download)),
            ];
            if inv.status == "draft" {
                let ts = this.clone();
                items.push(Some(
                    MenuItem::new("Mark as sent", move |_, cx| {
                        let _ = ts.update(cx, |p, cx| p.mark_sent(vec![id], cx));
                    })
                    .icon(Lucide::Send),
                ));
            }
            if inv.status != "paid" {
                let tp = this.clone();
                let one = inv.clone();
                items.push(Some(
                    MenuItem::new("Record payment…", move |w, cx| {
                        let one = one.clone();
                        let _ = tp.update(cx, |p, cx| p.open_payment(vec![one], w, cx));
                    })
                    .icon(Lucide::CreditCard),
                ));
            }
            items.push(None);
            let td = this.clone();
            let del = inv.clone();
            items.push(Some(
                MenuItem::new("Delete", move |_, cx| {
                    let del = del.clone();
                    let _ = td.update(cx, |p, cx| {
                        p.deleting = Some(del);
                        cx.notify();
                    });
                })
                .icon(Lucide::Trash2)
                .danger(),
            ));
            let list = menu_list(&format!("bill-menu-{id}"), items, close.clone(), cx);
            wrap = wrap.child(popover(SharedString::from(format!("bill-pop-{id}")), Align::End, None, list, move |w, cx| close(w, cx), cx));
        }
        wrap
    }

    /// The floating bar for the selection: count, total, mark sent, record
    /// payment, clear. Fixed `bottom-5`, centred at `50% + 110px`.
    pub(super) fn bulk_bar(&self, t: &Theme, selected: &[Invoice], window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let total: f64 = selected.iter().map(|i| i.total).sum();
        let drafts: Vec<i64> = selected.iter().filter(|i| i.status == "draft").map(|i| i.id).collect();
        let unpaid: Vec<Invoice> = selected.iter().filter(|i| i.status != "paid").cloned().collect();
        let mut bar = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(r(3.))
            .pl(r(4.))
            .pr(r(2.))
            .h(r(11.))
            .rounded(px(10.))
            .bg(t.panel)
            .shadow(t.shadow_pop())
            .child(text(format!("{} selected", selected.len()), 13.0, 500, t.fg).whitespace_nowrap())
            .child(div().font(num_font(500)).text_size(px(13.)).text_color(t.fg3).whitespace_nowrap().child(money(total)))
            .child(div().w(px(1.)).h(r(5.)).bg(t.line));
        if !drafts.is_empty() {
            bar = bar.child(Button::secondary("bulk-sent", "Mark as sent").small().on_click(cx.listener(move |p, _, _, cx| p.mark_sent(drafts.clone(), cx))));
        }
        if !unpaid.is_empty() {
            bar = bar.child(Button::primary("bulk-pay", "Record payment").small().on_click(cx.listener(move |p, _, w, cx| p.open_payment(unpaid.clone(), w, cx))));
        }
        bar = bar.child(
            Button::icon_button("bulk-clear", Lucide::X).small().tooltip("Clear selection").on_click(cx.listener(|p, _, _, cx| {
                p.selected.clear();
                cx.notify();
            })),
        );
        let bar: AnyElement = if motion::reduced() {
            bar.into_any_element()
        } else {
            bar.relative()
                .with_animation("bulk-in", Animation::new(Duration::from_millis(150)), |b, d| b.opacity(d).top(px(12.0 * (1.0 - d))))
                .into_any_element()
        };
        let viewport = window.viewport_size();
        let x = f32::from(viewport.width) / 2.0 + 110.0;
        let bottom = f32::from(viewport.height) - f32::from(r(5.));
        // A zero-width box at the centre line; the bar overflows it evenly.
        deferred(
            anchored()
                .position(point(px(x), px(bottom) - r(11.)))
                .child(div().w(px(0.)).h(r(11.)).flex().justify_center().child(bar)),
        )
        .with_priority(5)
        .into_any_element()
    }
}
