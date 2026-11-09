//! The alarm-clock mascot (Illustrations.tsx's `Mascot`): moods, a blink
//! every 4.6s, and the bob / hop motions.

use std::time::Duration;

use gpui::{div, img, px, Animation, AnimationExt as _, AnyElement, App, ElementId, IntoElement, ParentElement, RenderOnce, SharedString, Styled, Window};

pub use crate::art::Mood;
use crate::{art::{Art, Gallery}, motion};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    None,
    /// mascot-bob: up 3px and back, every 3.2s.
    Bob,
    /// mascot-hop: two hops, then still.
    Hop,
}

#[derive(IntoElement)]
pub struct Mascot {
    id: SharedString,
    mood: Mood,
    size: f32,
    motion: Motion,
}

impl Mascot {
    pub fn new(id: impl Into<SharedString>, mood: Mood, size: f32) -> Self {
        Mascot { id: id.into(), mood, size, motion: Motion::None }
    }
    pub fn motion(mut self, m: Motion) -> Self {
        self.motion = m;
        self
    }
}

impl RenderOnce for Mascot {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let size = self.size;
        let gallery = cx.global_mut::<Gallery>();
        let open = gallery.get(Art::Mascot { mood: self.mood, blink: false }, size);
        if motion::reduced() {
            return img(open).size(px(size)).flex_none().into_any_element();
        }
        let shut = gallery.get(Art::Mascot { mood: self.mood, blink: true }, size);
        let face = div().size(px(size)).flex_none().with_animation(
            ElementId::Name(format!("{}-blink", self.id).into()),
            Animation::new(Duration::from_millis(4600)).repeat(),
            move |d, delta| d.child(img(if (0.925..0.975).contains(&delta) { shut.clone() } else { open.clone() }).size(px(size))),
        );
        let holder = div().relative().flex_none().child(face);
        let el: AnyElement = match self.motion {
            Motion::None => holder.into_any_element(),
            Motion::Hop => holder
                .with_animation(ElementId::Name(format!("{}-hop", self.id).into()), Animation::new(Duration::from_millis(1800)), |d, delta| {
                    let p = (delta * 2.0).fract();
                    let y = if p < 0.3 {
                        -10.0 * p / 0.3
                    } else if p < 0.55 {
                        -10.0 + 10.0 * (p - 0.3) / 0.25
                    } else if p < 0.7 {
                        -3.0 * (p - 0.55) / 0.15
                    } else {
                        -3.0 + 3.0 * (p - 0.7) / 0.3
                    };
                    d.top(px(if delta >= 1.0 { 0.0 } else { y }))
                })
                .into_any_element(),
            Motion::Bob => holder
                .with_animation(
                    ElementId::Name(format!("{}-bob", self.id).into()),
                    Animation::new(Duration::from_millis(3200)).repeat(),
                    |d, delta| d.top(px(-3.0 * (0.5 - 0.5 * (delta * std::f32::consts::TAU).cos()))),
                )
                .into_any_element(),
        };
        el
    }
}
