//! MergeClientsModal.tsx: fold one client into another (projects and
//! invoices move over, the duplicate is removed). Emits [`MergeEvent`].

use billable_core::models::Client;
use gpui::{div, prelude::*, px, Context, EventEmitter, IntoElement, ParentElement, Render, SharedString, Styled, Window};

use super::{select, toast_error, toast_success, Button, Modal};
use crate::{
    icons::Lucide,
    store,
    style::{icon, label, r, text},
    theme::theme,
};

pub enum MergeEvent {
    /// Merged; the id of the client that remains.
    Merged(i64),
    Closed,
}

pub struct MergeClientsModal {
    clients: Vec<Client>,
    from: i64,
    into: i64,
}

impl EventEmitter<MergeEvent> for MergeClientsModal {}

impl MergeClientsModal {
    pub fn new(clients: Vec<Client>, source: Option<i64>, target: Option<i64>, _cx: &mut Context<Self>) -> Self {
        let from = source.unwrap_or(0);
        let into = target.or_else(|| clients.iter().find(|c| Some(c.id) != source).map(|c| c.id)).unwrap_or(0);
        MergeClientsModal { clients, from, into }
    }

    /// Keep the record with more history; fold the other into it.
    pub fn suggest(clients: &[Client], ids: &[i64]) -> (i64, i64) {
        let weight = |id: i64| clients.iter().find(|c| c.id == id).map(|c| c.invoice_count * 10 + c.project_count).unwrap_or(0);
        let (a, b) = (ids.first().copied().unwrap_or(0), ids.get(1).copied().unwrap_or(0));
        if weight(a) >= weight(b) { (b, a) } else { (a, b) }
    }

    fn merge(&mut self, cx: &mut Context<Self>) {
        let (from, into) = (self.from, self.into);
        let target_name = self.clients.iter().find(|c| c.id == into).map(|c| c.name.clone()).unwrap_or_default();
        match store::write(cx, |db| db.merge_clients(from, into)) {
            Ok(r) => {
                let p = |n: i64| if n == 1 { "" } else { "s" };
                toast_success(
                    format!("Merged into {target_name}: {} project{}, {} invoice{} moved", r.moved_projects, p(r.moved_projects), r.moved_invoices, p(r.moved_invoices)),
                    cx,
                );
                cx.emit(MergeEvent::Merged(into));
            }
            Err(e) => toast_error(e.to_string(), cx),
        }
    }
}

impl Render for MergeClientsModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(cx);
        let (from, into) = (self.from, self.into);
        let source = self.clients.iter().find(|c| c.id == from).cloned();
        let target = self.clients.iter().find(|c| c.id == into).cloned();
        let valid = source.is_some() && target.is_some() && from != into;
        let mut from_opts: Vec<(i64, SharedString)> = vec![(0, "Choose…".into())];
        from_opts.extend(self.clients.iter().map(|c| (c.id, SharedString::from(c.name.clone()))));
        let mut into_opts: Vec<(i64, SharedString)> = vec![(0, "Choose…".into())];
        into_opts.extend(self.clients.iter().filter(|c| c.id != from).map(|c| (c.id, SharedString::from(c.name.clone()))));
        let w = cx.entity().downgrade();
        let (wa, wb, wc, wd, we) = (w.clone(), w.clone(), w.clone(), w.clone(), w);
        let mut body = div().flex().flex_col().gap(r(4.)).child(
            div()
                .flex()
                .items_end()
                .gap(r(3.))
                .child(div().flex_1().child(label(&t, "Merge this client")).child(select("merge-from", from_opts, &from, move |v, _, cx| {
                    let _ = wa.update(cx, |m, cx| {
                        m.from = v;
                        cx.notify();
                    });
                }, cx)))
                .child(div().mb(r(2.)).child(icon(Lucide::ArrowRight).size(r(4.)).text_color(t.fg3)))
                .child(div().flex_1().child(label(&t, "Into this one")).child(select("merge-into", into_opts, &into, move |v, _, cx| {
                    let _ = wb.update(cx, |m, cx| {
                        m.into = v;
                        cx.notify();
                    });
                }, cx))),
        );
        if let (true, Some(s), Some(g)) = (valid, source.as_ref(), target.as_ref()) {
            let plural = |n: i64| if n == 1 { "" } else { "s" };
            let b = |s: String| text(s, 13.0, 700, t.fg);
            let n = |s: String| text(s, 13.0, 500, t.fg2);
            body = body.child(
                div()
                    .rounded(px(8.))
                    .border_1()
                    .border_color(t.line)
                    .bg(t.panel2.opacity(0.6))
                    .p(r(3.))
                    .flex()
                    .flex_col()
                    .gap(r(1.))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .child(b(s.project_count.to_string()))
                            .child(n(format!(" project{} and ", plural(s.project_count))))
                            .child(b(s.invoice_count.to_string()))
                            .child(n(format!(" invoice{} move to ", plural(s.invoice_count))))
                            .child(b(g.name.clone()))
                            .child(n(".".into())),
                    )
                    .child(n(format!("{} is then removed. {}'s contact details and rate stay as they are.", s.name, g.name))),
            );
        }
        Modal::new("merge-clients", "Merge clients", move |_, cx| {
            let _ = wc.update(cx, |_, cx| cx.emit(MergeEvent::Closed));
        })
        .description("Use this when the same client was added twice.")
        .child(body)
        .footer(Button::secondary("merge-cancel", "Cancel").on_click(move |_, _, cx| {
            let _ = wd.update(cx, |_, cx| cx.emit(MergeEvent::Closed));
        }))
        .footer(Button::primary("merge-go", "Merge clients").disabled(!valid).on_click(move |_, _, cx| {
            let _ = we.update(cx, |m, cx| m.merge(cx));
        }))
    }
}
