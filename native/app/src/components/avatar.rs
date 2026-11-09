//! ClientAvatar (Clients.tsx): a bright circle with the client's initials.

use gpui::{div, px, Div, ParentElement, Styled};

use crate::{format::{initials, js_name_hash}, style::text, theme::{hex, parse_hex}};

const COLORS: [&str; 8] = ["#FF9600", "#1CB0F6", "#58CC02", "#CE82FF", "#FF4B4B", "#2B70C9", "#FFC800", "#00CD9C"];

pub fn client_avatar(name: &str, size: f32) -> Div {
    let color = COLORS[(js_name_hash(name).unsigned_abs() as usize) % COLORS.len()];
    div()
        .size(px(size))
        .flex_none()
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(parse_hex(color))
        .child(text(initials(name), (size * 0.4).round(), 700, hex(0xffffff)).line_height(px(size)))
}
