//! Money.tsx: a figure with the cents set back, optionally counting up.

use std::time::Duration;

use gpui::{div, Animation, AnimationExt as _, AnyElement, ElementId, Hsla, IntoElement, ParentElement, Styled};

use crate::{format::money_parts, style::figures};

/// `$1,153.75` as `$1,153` + a quieter `.75`.
pub fn money(amount: f64, size: f32, color: Hsla) -> AnyElement {
    let (whole, cents) = money_parts(amount);
    div()
        .flex()
        .flex_none()
        .whitespace_nowrap()
        .child(figures(whole, size, color))
        .child(figures(cents, size, color.opacity(0.45)))
        .into_any_element()
}

/// `<Money animate>`: counts up from 0 over 700ms (ease-out cubic) the first
/// time it appears under this id.
pub fn money_counting(id: impl Into<ElementId>, amount: f64, size: f32, color: Hsla) -> AnyElement {
    if crate::motion::reduced() {
        return money(amount, size, color);
    }
    div()
        .with_animation(id, Animation::new(Duration::from_millis(700)), move |d, delta| {
            let eased = 1.0 - (1.0 - delta).powi(3);
            d.child(money(amount * eased as f64, size, color))
        })
        .into_any_element()
}
