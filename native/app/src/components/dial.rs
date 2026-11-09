//! Dial.tsx's StopwatchDial: a small stopwatch face for the running timer.
//! The amber hand steps each second; the short hand counts minutes.

use gpui::{canvas, div, point, px, Bounds, Hsla, IntoElement, ParentElement, PathBuilder, Pixels, Styled};

use crate::theme::{hex, Theme};

const AMBER: u32 = 0xF5A623;

fn line(bounds: Bounds<Pixels>, scale: f32, angle_deg: f32, from: f32, to: f32, width: f32) -> Option<gpui::Path<Pixels>> {
    // In the 32×32 drawing, a hand from (16, from) to (16, to) rotated about (16, 16).
    let a = angle_deg.to_radians();
    let at = |y: f32| {
        let (dx, dy) = (0.0f32, y - 16.0);
        let (rx, ry) = (dx * a.cos() - dy * a.sin(), dx * a.sin() + dy * a.cos());
        point(bounds.origin.x + px((16.0 + rx) * scale), bounds.origin.y + px((16.0 + ry) * scale))
    };
    let mut b = PathBuilder::stroke(px(width * scale));
    b.move_to(at(from));
    b.line_to(at(to));
    b.build().ok()
}

/// `size` 22 in the toolbar. `seconds` is the elapsed time.
pub fn stopwatch_dial(t: &Theme, seconds: f64, size: f32, paused: bool) -> impl IntoElement {
    let scale = size / 32.0;
    let hand: Hsla = if paused { t.fg3 } else { hex(AMBER) };
    let fg = t.fg;
    let second_angle = (seconds * 6.0) as f32;
    let minute_angle = ((seconds / 60.0) * 6.0) as f32;
    let ring = 14.6 * 2.0 * scale;
    div()
        .relative()
        .size(px(size))
        .flex_none()
        .child(
            div()
                .absolute()
                .left(px((size - ring) / 2.0))
                .top(px((size - ring) / 2.0))
                .size(px(ring))
                .rounded_full()
                .border(px(1.4 * scale))
                .border_color(fg.opacity(0.28)),
        )
        .child(
            canvas(
                |_, _, _| (),
                move |bounds, _, window, _| {
                    for i in 0..4 {
                        if let Some(p) = line(bounds, scale, i as f32 * 90.0, 3.4, 6.2, 1.5) {
                            window.paint_path(p, fg.opacity(0.55));
                        }
                    }
                    if let Some(p) = line(bounds, scale, minute_angle, 16.0, 9.0, 2.2) {
                        window.paint_path(p, fg.opacity(0.75));
                    }
                    if let Some(p) = line(bounds, scale, second_angle, 18.0, 5.0, 1.6) {
                        window.paint_path(p, hand);
                    }
                },
            )
            .absolute()
            .size_full(),
        )
        .child(
            div()
                .absolute()
                .left(px(size / 2.0 - 2.1 * scale))
                .top(px(size / 2.0 - 2.1 * scale))
                .size(px(4.2 * scale))
                .rounded_full()
                .bg(hand),
        )
}
