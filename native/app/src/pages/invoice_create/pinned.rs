//! `position: fixed` for the builder's action bar. GPUI pages scroll inside
//! the window's page area, so this element lays its child out normally and
//! then moves it to a spot in window coordinates while prepainting, the way
//! a scroll offset moves content. It still clips to the page area, which
//! keeps toasts (drawn above the page) on top of it as in the web app.

use std::panic::Location;

use gpui::{
    AnyElement, App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement, LayoutId, Pixels,
    Point, Window,
};

pub struct Pinned {
    origin: Point<Pixels>,
    child: AnyElement,
}

/// Draw `child` (which should be `absolute`, so it takes no room) with its
/// top-left corner at `origin` in the window.
pub fn pinned(origin: Point<Pixels>, child: impl IntoElement) -> Pinned {
    Pinned { origin, child: child.into_any_element() }
}

impl IntoElement for Pinned {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for Pinned {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static Location<'static>> {
        None
    }

    fn request_layout(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, window: &mut Window, cx: &mut App) -> (LayoutId, ()) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, bounds: Bounds<Pixels>, _: &mut (), window: &mut Window, cx: &mut App) {
        let offset = self.origin - bounds.origin;
        window.with_element_offset(offset, |window| {
            self.child.prepaint(window, cx);
        });
    }

    fn paint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, _: &mut (), _: &mut (), window: &mut Window, cx: &mut App) {
        self.child.paint(window, cx);
    }
}
