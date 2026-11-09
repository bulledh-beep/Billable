//! Animation helpers matching index.css's keyframes and framer-motion's
//! springs. Everything checks the Mac's Reduce Motion setting first, as the
//! CSS does with `prefers-reduced-motion`.

use std::{cell::Cell, time::Duration};

use gpui::{div, px, Animation, AnimationExt as _, AnyElement, App, IntoElement, ParentElement, Styled};

thread_local! {
    static REDUCED: Cell<bool> = const { Cell::new(false) };
}

/// Refresh the cached Reduce Motion flag (call once per frame from the root).
pub fn sync(cx: &App) {
    REDUCED.with(|r| r.set(cx.reduce_motion()));
}

/// Whether to skip decorative motion.
pub fn reduced() -> bool {
    REDUCED.with(|r| r.get())
}

/// CSS `cubic-bezier()`.
pub fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32) -> impl Fn(f32) -> f32 + Clone {
    move |x: f32| {
        let bx = |t: f32| 3.0 * (1.0 - t).powi(2) * t * x1 + 3.0 * (1.0 - t) * t * t * x2 + t * t * t;
        let by = |t: f32| 3.0 * (1.0 - t).powi(2) * t * y1 + 3.0 * (1.0 - t) * t * t * y2 + t * t * t;
        let (mut lo, mut hi) = (0.0f32, 1.0f32);
        for _ in 0..24 {
            let mid = (lo + hi) / 2.0;
            if bx(mid) < x { lo = mid } else { hi = mid }
        }
        by((lo + hi) / 2.0)
    }
}

/// Progress of an animation that waits `delay` ms, then runs `dur` ms,
/// given the progress of the whole (delay + dur) run.
pub fn after(delta: f32, delay: f32, dur: f32) -> f32 {
    ((delta * (delay + dur) - delay) / dur).clamp(0.0, 1.0)
}

/// A damped spring's progress at time `t` seconds (framer-motion's
/// `type: 'spring'`), overshooting like the real thing.
pub fn spring(t: f32, stiffness: f32, damping: f32, mass: f32) -> f32 {
    let w0 = (stiffness / mass).sqrt();
    let zeta = damping / (2.0 * (stiffness * mass).sqrt());
    if zeta < 1.0 {
        let wd = w0 * (1.0 - zeta * zeta).sqrt();
        1.0 - (-zeta * w0 * t).exp() * ((wd * t).cos() + (zeta * w0 / wd) * (wd * t).sin())
    } else {
        1.0 - (1.0 + w0 * t) * (-w0 * t).exp()
    }
}

/// `.page > *`: each block rises 10px and fades in, 40ms after the last.
pub fn rise_in(id: impl Into<gpui::ElementId>, index: usize, el: impl IntoElement) -> AnyElement {
    if reduced() {
        return el.into_any_element();
    }
    let delay = (index as f32 * 40.0).min(240.0);
    let ease = cubic_bezier(0.2, 0.8, 0.2, 1.0);
    div()
        .relative()
        .child(el)
        .with_animation(id, Animation::new(Duration::from_millis((delay + 420.0) as u64)), move |d, delta| {
            let p = ease(after(delta, delay, 420.0));
            d.opacity(p).top(px(10.0 * (1.0 - p)))
        })
        .into_any_element()
}
