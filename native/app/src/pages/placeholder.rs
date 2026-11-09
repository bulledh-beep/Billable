//! Stand-in for screens not ported yet.

use gpui::{div, prelude::*, px, Context, IntoElement, ParentElement, Render, Styled, Window};

use crate::{
    shell::{app::{Header, Page}, route::Route},
    style::{r, text},
    theme::theme,
};

pub struct Placeholder {
    route: Route,
}

impl Placeholder {
    pub fn new(route: Route) -> Self {
        Placeholder { route }
    }
}

impl Page for Placeholder {
    fn header(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> Header {
        Header::new(format!("{:?}", self.route.section()))
    }
}

impl Render for Placeholder {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(cx);
        div().p(r(7.)).child(text(format!("{} is being ported.", self.route.path()), 14.0, 600, t.fg3).line_height(px(20.)))
    }
}
