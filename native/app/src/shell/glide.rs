//! Moving between the two apps (SuiteGlide.tsx). The app showing eases back
//! towards the icon just chosen and fades, while the other eases in from the
//! same point, the icon lifting with a soft glow of its app's colour over a
//! tinted ground. Both apps stay in the window for the move, so it moves the
//! real screens, live.
//!
//! How each part is drawn here:
//! - Content HQ's page is a WKWebView, so it scales and fades with Core
//!   Animation on its own layer, around the icon, exactly as in Electron.
//! - Billable's screen and Content HQ's suite bar are drawn by GPUI, which
//!   can't scale a tree of elements: they cross-fade with the same timings,
//!   without the scale.
//! - The glow is a Core Animation radial gradient above everything (the web
//!   view is a native view on top of GPUI's drawing, so a GPUI glow would be
//!   hidden behind it). The mark and the ground are GPUI: the mark sits in the
//!   bar, above the web view's top edge, and scales by being drawn larger.
//!
//! The Core Animation parts run on their own once started; GPUI redraws each
//! frame only until the move is over (TOTAL_MS), then nothing keeps going.
//! Under Reduce Motion there is no glide (shouldGlide).

use std::{cell::RefCell, sync::Arc, time::Instant};

use gpui::{
    canvas, div, img, px, AnyElement, App, Bounds, Image, ImageFormat, IntoElement, ObjectFit, ParentElement,
    Pixels, Styled, StyledImage, Window,
};

use crate::{
    art::{Art, Gallery},
    motion::cubic_bezier,
    theme::{hex, mix, rgba4, shadow, Theme},
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SuiteApp {
    Billable,
    Content,
}

impl SuiteApp {
    fn ix(self) -> usize {
        self as usize
    }
    /// #ff9a1a / #1c9a8b
    fn color(self) -> (u8, u8, u8) {
        match self {
            SuiteApp::Billable => (0xff, 0x9a, 0x1a),
            SuiteApp::Content => (0x1c, 0x9a, 0x8b),
        }
    }
    fn mark(self) -> Art {
        match self {
            SuiteApp::Billable => Art::BillableMark,
            SuiteApp::Content => Art::ContentMark,
        }
    }
}

/// The app showing eases back and is gone in this long...
const LEAVE_MS: f32 = 240.0;
/// ...and the other starts in this far into it, so the two cross.
const ENTER_FROM_MS: f32 = 110.0;
const ENTER_MS: f32 = 420.0;
/// The whole move (the glow is done at 520, the mark at 480).
pub const TOTAL_MS: f32 = ENTER_FROM_MS + ENTER_MS;

fn ease_leave(x: f32) -> f32 {
    cubic_bezier(0.4, 0.0, 1.0, 1.0)(x.clamp(0.0, 1.0))
}
fn ease_enter(x: f32) -> f32 {
    cubic_bezier(0.16, 1.0, 0.3, 1.0)(x.clamp(0.0, 1.0))
}
fn ease_out(x: f32) -> f32 {
    cubic_bezier(0.0, 0.0, 0.58, 1.0)(x.clamp(0.0, 1.0))
}

/// The leaving app at `t` ms: (opacity, scale).
fn leave_at(t: f32) -> (f32, f32) {
    let p = ease_leave(t / LEAVE_MS);
    (1.0 - p, 1.0 - 0.08 * p)
}

/// The entering app at `t` ms: (opacity, scale). Opacity reaches 1 at 40%
/// of the eased move; before it starts it holds its first frame (fill both).
fn enter_at(t: f32) -> (f32, f32) {
    let p = ease_enter((t - ENTER_FROM_MS) / ENTER_MS);
    ((p / 0.4).min(1.0), 0.92 + 0.08 * p)
}

/// The mark at `t` ms: (opacity, scale, shadow strength).
fn mark_at(t: f32) -> (f32, f32, f32) {
    if t < 220.0 {
        let p = cubic_bezier(0.3, 0.7, 0.4, 1.0)((t / 200.0).clamp(0.0, 1.0));
        (1.0, 0.75 + 0.25 * p, p)
    } else {
        let q = ease_out((t - 220.0) / 260.0);
        (1.0 - q, 1.0 - 0.25 * q, 1.0 - q)
    }
}

/// The glow at `t` ms: (opacity, scale).
fn glow_at(t: f32) -> (f32, f32) {
    if t < 200.0 {
        let p = ease_out(t / 220.0);
        (0.45 * p, 0.3 + 0.5 * p)
    } else {
        let q = ease_out((t - 200.0) / 320.0);
        (0.45 * (1.0 - q), 0.8 + 0.6 * q)
    }
}

thread_local! {
    /// Where each app's icon in the switch on screen was last painted (the
    /// Billable icon sits in Content HQ's bar, Content HQ's in the sidebar).
    static ICONS: RefCell<[Option<Bounds<Pixels>>; 2]> = const { RefCell::new([None, None]) };
    static RUNNING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// The last ground image, kept for the next move the same size.
    static GROUND: RefCell<Option<(String, Arc<Image>)>> = const { RefCell::new(None) };
}

/// Whether a move is under way (Content HQ leaves its page area clear).
pub fn running() -> bool {
    RUNNING.with(|r| r.get())
}

fn log_enabled() -> bool {
    std::env::var_os("BILLABLE_GLIDE_LOG").is_some()
}

/// An app's icon in the switch, noting where it lands (data-suite-icon).
pub fn icon(app: SuiteApp, size: f32, cx: &mut App) -> impl IntoElement {
    let image = cx.global_mut::<Gallery>().get(app.mark(), size);
    div()
        .relative()
        .size(px(size))
        .flex_none()
        .child(img(image).size(px(size)))
        .child(
            canvas(move |bounds, _, _| ICONS.with(|i| i.borrow_mut()[app.ix()] = Some(bounds)), |_, _, _, _| {})
                .absolute()
                .inset_0(),
        )
}

pub struct Glide {
    pub from: SuiteApp,
    pub to: SuiteApp,
    x: f32,
    y: f32,
    /// The mark's size: the icon's, a third larger.
    size: f32,
    start: Instant,
    ground: Arc<Image>,
    native: Option<native::Native>,
    frames: u32,
}

impl Glide {
    /// Start a move to `to`, from its icon in the switch showing now. `None`
    /// under Reduce Motion or when the icon hasn't been laid out.
    /// `web` is Content HQ's web view (already made and shown), if any.
    pub fn start(from: SuiteApp, to: SuiteApp, web: Option<native::Object>, t: &Theme, window: &mut Window, cx: &mut App) -> Option<Glide> {
        if from == to || cx.reduce_motion() {
            return None;
        }
        let icon = ICONS.with(|i| i.borrow()[to.ix()])?;
        if icon.size.width <= px(0.) {
            return None;
        }
        let center = icon.center();
        let (x, y) = (f32::from(center.x), f32::from(center.y));
        let size = (f32::from(icon.size.width) * 4.0 / 3.0).round();
        let viewport = window.viewport_size();
        let ground = ground_image(to, x, y, f32::from(viewport.width), f32::from(viewport.height), t, cx);
        let native = native::Native::start(window, web, to, x, y, from == SuiteApp::Content);
        RUNNING.with(|r| r.set(true));
        if log_enabled() {
            eprintln!("glide start {from:?} -> {to:?} at ({x:.1}, {y:.1}) mark {size}px, web {}", native.as_ref().is_some_and(|n| n.has_web()));
        }
        Some(Glide { from, to, x, y, size, start: Instant::now(), ground, native, frames: 0 })
    }

    pub fn elapsed_ms(&self) -> f32 {
        self.start.elapsed().as_secs_f32() * 1000.0
    }

    pub fn finished(&self) -> bool {
        self.elapsed_ms() >= TOTAL_MS
    }

    /// How opaque an app's GPUI-drawn layer is now.
    pub fn opacity(&self, app: SuiteApp) -> f32 {
        let t = self.elapsed_ms();
        if app == self.from { leave_at(t).0 } else { enter_at(t).0 }
    }

    /// Called once per frame while the move runs: asks for the next frame
    /// and, with BILLABLE_GLIDE_LOG set, notes where everything is.
    pub fn tick(&mut self, window: &mut Window) {
        window.request_animation_frame();
        self.frames += 1;
        if log_enabled() {
            let t = self.elapsed_ms();
            let (mo, ms, _) = mark_at(t);
            let (go, gs) = glow_at(t);
            let native = self.native.as_ref().map(|n| n.describe(t, self.from == SuiteApp::Content)).unwrap_or_default();
            eprintln!(
                "glide t={t:6.1}ms billable={:.3} content={:.3} mark=({mo:.2}, x{ms:.3}) glow=({go:.3}, x{gs:.2}){native}",
                self.opacity(SuiteApp::Billable),
                self.opacity(SuiteApp::Content)
            );
        }
    }

    /// Take down the Core Animation parts (the move is over or replaced).
    pub fn finish(self) {
        if log_enabled() {
            eprintln!("glide done after {:.1}ms, {} GPUI frames", self.elapsed_ms(), self.frames);
        }
        if let Some(n) = self.native {
            n.finish();
        }
        RUNNING.with(|r| r.set(false));
    }

    /// The ground of the app being opened, a shade darker, tinted with its
    /// colour around the icon. Under both apps.
    pub fn ground(&self) -> AnyElement {
        img(self.ground.clone()).absolute().top_0().left_0().size_full().object_fit(ObjectFit::Fill).into_any_element()
    }

    /// The chosen app's mark, lifting with a coloured shadow, then giving way.
    pub fn mark(&self, cx: &mut App) -> AnyElement {
        let (opacity, scale, lift) = mark_at(self.elapsed_ms());
        if opacity <= 0.0 {
            return div().into_any_element();
        }
        let size = self.size * scale;
        let (r, g, b) = self.to.color();
        let image = cx.global_mut::<Gallery>().get(self.to.mark(), self.size);
        div()
            .absolute()
            .left(px(self.x - size / 2.0))
            .top(px(self.y - size / 2.0))
            .size(px(size))
            .opacity(opacity)
            // drop-shadow(0 3px 10px color 50%) on the rounded tile (rx 120/512)
            .rounded(px(size * 120.0 / 512.0))
            .shadow(vec![shadow(0.0, 3.0 * lift, 10.0 * lift, 0.0, rgba4(r, g, b, 0.5 * lift))])
            .child(img(image).size_full())
            .into_any_element()
    }
}

/// The ground as an image: the app's background a shade darker (94% with
/// black) under radial-gradient(75% 55% at the icon, colour 14%, transparent
/// 72%). Drawn at a quarter size; it's soft enough to stretch.
fn ground_image(to: SuiteApp, x: f32, y: f32, w: f32, h: f32, t: &Theme, cx: &mut App) -> Arc<Image> {
    let base = match to {
        SuiteApp::Billable => t.bg,
        SuiteApp::Content if t.dark => hex(0x0b1415),
        SuiteApp::Content => hex(0xeef4f2),
    };
    let ground = crate::theme::css_hex(mix(base, hex(0x000000), 0.94));
    let (r, g, b) = to.color();
    let (w, h) = (w.max(1.0).round(), h.max(1.0).round());
    let key = format!("{ground}|{r},{g},{b}|{x:.0},{y:.0}|{w}x{h}");
    if let Some(image) = GROUND.with(|c| c.borrow().as_ref().filter(|(k, _)| *k == key).map(|(_, i)| i.clone())) {
        return image;
    }
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{qw}" height="{qh}" viewBox="0 0 {w} {h}" preserveAspectRatio="none"><defs><radialGradient id="g" gradientUnits="userSpaceOnUse" cx="0" cy="0" r="1" gradientTransform="translate({x} {y}) scale({rx} {ry})"><stop offset="0" stop-color="rgb({r},{g},{b})" stop-opacity="0.14"/><stop offset="0.72" stop-color="rgb({r},{g},{b})" stop-opacity="0"/></radialGradient></defs><rect width="{w}" height="{h}" fill="{ground}"/><rect width="{w}" height="{h}" fill="url(#g)"/></svg>"#,
        qw = (w / 4.0).ceil(),
        qh = (h / 4.0).ceil(),
        rx = w * 0.75,
        ry = h * 0.55,
    );
    let image = Arc::new(Image::from_bytes(ImageFormat::Svg, svg.into_bytes()));
    if let Some((_, old)) = GROUND.with(|c| c.borrow_mut().replace((key, image.clone()))) {
        old.remove_asset(cx);
    }
    image
}

/// The parts Core Animation moves: Content HQ's web view and the glow.
pub mod native {
    use std::ffi::c_void;

    use gpui::Window;
    use objc2::{class, msg_send, rc::Retained, runtime::{AnyObject, Bool}, Encode, Encoding};
    use objc2_core_foundation::{CGPoint, CGRect, CGSize};
    use objc2_foundation::NSString;
    use wry::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    use super::{enter_at, glow_at, leave_at, SuiteApp, ENTER_FROM_MS, ENTER_MS, LEAVE_MS};

    pub type Object = Retained<AnyObject>;

    #[link(name = "QuartzCore", kind = "framework")]
    extern "C" {
        fn CACurrentMediaTime() -> f64;
    }
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGColorCreateSRGB(r: f64, g: f64, b: f64, a: f64) -> *mut c_void;
        fn CGColorRelease(color: *mut c_void);
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CATransform3D([f64; 16]);

    const D: Encoding = Encoding::Double;
    unsafe impl Encode for CATransform3D {
        const ENCODING: Encoding = Encoding::Struct("CATransform3D", &[D; 16]);
    }

    impl CATransform3D {
        /// Scale `s`, then move by (tx, ty).
        fn scale_move(s: f64, tx: f64, ty: f64) -> Self {
            let mut m = [0.0; 16];
            m[0] = s;
            m[5] = s;
            m[10] = 1.0;
            m[15] = 1.0;
            m[12] = tx;
            m[13] = ty;
            CATransform3D(m)
        }
    }

    /// Keyframes sampled this finely; Core Animation joins them linearly.
    const SAMPLES: usize = 48;
    const WEB_OPACITY: &str = "billable-glide-opacity";
    const WEB_TRANSFORM: &str = "billable-glide-transform";

    fn ns(s: &str) -> Retained<NSString> {
        NSString::from_str(s)
    }

    unsafe fn number(v: f64) -> Object {
        msg_send![class!(NSNumber), numberWithDouble: v]
    }

    unsafe fn transform(t: CATransform3D) -> Object {
        msg_send![class!(NSValue), valueWithCATransform3D: t]
    }

    unsafe fn array(items: &[*mut AnyObject]) -> Object {
        msg_send![class!(NSArray), arrayWithObjects: items.as_ptr(), count: items.len()]
    }

    unsafe fn array_of(items: &[Object]) -> Object {
        let ptrs: Vec<*mut AnyObject> = items.iter().map(|o| Retained::as_ptr(o) as *mut AnyObject).collect();
        array(&ptrs)
    }

    /// A linear keyframe animation through `values`, evenly spaced.
    unsafe fn keyframes(layer: &AnyObject, path: &str, key: &str, values: Vec<Object>, begin: f64, duration: f64, fill: &str, remove: bool) {
        let n = values.len();
        let times: Vec<Object> = (0..n).map(|i| number(i as f64 / (n - 1) as f64)).collect();
        let a: Object = msg_send![class!(CAKeyframeAnimation), animationWithKeyPath: &*ns(path)];
        let values = array_of(&values);
        let times = array_of(&times);
        let _: () = msg_send![&*a, setValues: &*values];
        let _: () = msg_send![&*a, setKeyTimes: &*times];
        let _: () = msg_send![&*a, setBeginTime: begin];
        let _: () = msg_send![&*a, setDuration: duration];
        let _: () = msg_send![&*a, setFillMode: &*ns(fill)];
        let _: () = msg_send![&*a, setRemovedOnCompletion: Bool::new(remove)];
        let _: () = msg_send![layer, addAnimation: &*a, forKey: &*ns(key)];
    }

    /// Sample `f` over `duration_ms` at SAMPLES+1 evenly spaced times.
    fn sample<T>(duration_ms: f32, f: impl Fn(f32) -> T) -> Vec<T> {
        (0..=SAMPLES).map(|i| f(duration_ms * i as f32 / SAMPLES as f32)).collect()
    }

    unsafe fn transaction(f: impl FnOnce()) {
        let _: () = msg_send![class!(CATransaction), begin];
        let _: () = msg_send![class!(CATransaction), setDisableActions: Bool::YES];
        f();
        let _: () = msg_send![class!(CATransaction), commit];
    }

    struct Web {
        view: Object,
        layer: Object,
        radius: f64,
        masks: bool,
        model: CGRect,
        icon: CGPoint,
    }

    pub struct Native {
        glow: Option<Object>,
        web: Option<Web>,
    }

    /// GPUI's own NSView, under everything Billable draws.
    fn host_view(window: &Window, web: Option<&Object>) -> Option<Object> {
        if let Some(web) = web {
            let sup: Option<Object> = unsafe { msg_send![&**web, superview] };
            if sup.is_some() {
                return sup;
            }
        }
        let handle = HasWindowHandle::window_handle(window).ok()?;
        let RawWindowHandle::AppKit(h) = handle.as_raw() else { return None };
        unsafe { Retained::retain(h.ns_view.as_ptr() as *mut AnyObject) }
    }

    impl Native {
        pub fn has_web(&self) -> bool {
            self.web.is_some()
        }

        /// Start the glow at the icon (x, y, window points from the top left)
        /// and move the web view: out if `web_leaves`, else in.
        pub fn start(window: &Window, web: Option<Object>, to: SuiteApp, x: f32, y: f32, web_leaves: bool) -> Option<Native> {
            let host = host_view(window, web.as_ref())?;
            unsafe {
                let host_layer: Option<Object> = msg_send![&*host, layer];
                let host_layer = host_layer?;
                let flipped: Bool = msg_send![&*host, isFlipped];
                let bounds: CGRect = msg_send![&*host, bounds];
                let in_view = CGPoint::new(x as f64, if flipped.as_bool() { y as f64 } else { bounds.size.height - y as f64 });
                let icon: CGPoint = msg_send![&*host, convertPointToLayer: in_view];
                let scale: f64 = {
                    let w: Option<Object> = msg_send![&*host, window];
                    w.map(|w| msg_send![&*w, backingScaleFactor]).unwrap_or(2.0)
                };
                let begin = CACurrentMediaTime();
                let mut native = Native { glow: None, web: None };
                transaction(|| {
                    native.glow = Some(glow(&host_layer, icon, to, scale, begin));
                    if let Some(view) = web {
                        native.web = web_move(view, icon, begin, web_leaves);
                    }
                });
                Some(native)
            }
        }

        /// For the log: the web view's opacity and scale as Core Animation
        /// shows them now, and how far the icon point drifted (should be ~0).
        pub fn describe(&self, t: f32, web_leaves: bool) -> String {
            let Some(web) = &self.web else { return String::new() };
            unsafe {
                let p: Option<Object> = msg_send![&*web.layer, presentationLayer];
                let Some(p) = p else { return String::new() };
                let o: f32 = msg_send![&*p, opacity];
                let f: CGRect = msg_send![&*p, frame];
                let s = f.size.width / web.model.size.width;
                let ex = web.icon.x + s * (web.model.origin.x - web.icon.x);
                let ey = web.icon.y + s * (web.model.origin.y - web.icon.y);
                let (want_o, want_s) = if web_leaves { leave_at(t) } else { enter_at(t) };
                format!(
                    " web(ca opacity={o:.3} scale={s:.4} | expected {want_o:.3}, {want_s:.4} | icon drift {:.2},{:.2}pt)",
                    f.origin.x - ex,
                    f.origin.y - ey
                )
            }
        }

        pub fn finish(self) {
            unsafe {
                transaction(|| {
                    if let Some(glow) = &self.glow {
                        let _: () = msg_send![&**glow, removeAllAnimations];
                        let _: () = msg_send![&**glow, removeFromSuperlayer];
                    }
                    if let Some(web) = &self.web {
                        let _: () = msg_send![&*web.layer, removeAnimationForKey: &*ns(WEB_OPACITY)];
                        let _: () = msg_send![&*web.layer, removeAnimationForKey: &*ns(WEB_TRANSFORM)];
                        let _: () = msg_send![&*web.layer, setCornerRadius: web.radius];
                        let _: () = msg_send![&*web.layer, setMasksToBounds: Bool::new(web.masks)];
                        let _: () = msg_send![&*web.view, setAlphaValue: 1.0f64];
                    }
                });
            }
        }
    }

    /// radial-gradient(circle, colour 50% white 0%, colour 32% 20%,
    /// transparent 64%) in a 360pt circle at the icon, lifting (scale 0.3 to
    /// 0.8, to 45%) then spreading out and fading (to 1.4).
    unsafe fn glow(host: &AnyObject, icon: CGPoint, to: SuiteApp, scale: f64, begin: f64) -> Object {
        let (r, g, b) = to.color();
        let (r, g, b) = (r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0);
        let colors = [
            CGColorCreateSRGB((r + 1.0) / 2.0, (g + 1.0) / 2.0, (b + 1.0) / 2.0, 1.0),
            CGColorCreateSRGB(r, g, b, 0.32),
            CGColorCreateSRGB(r, g, b, 0.0),
            CGColorCreateSRGB(r, g, b, 0.0),
        ];
        let layer: Object = msg_send![class!(CAGradientLayer), layer];
        let color_array = array(&colors.map(|c| c as *mut AnyObject));
        for c in colors {
            CGColorRelease(c);
        }
        let locations = array_of(&[number(0.0), number(0.2), number(0.64), number(1.0)]);
        // A circle's gradient runs to its farthest corner: 180√2 from the middle.
        let reach = 0.5 + 0.5 * std::f64::consts::SQRT_2;
        let _: () = msg_send![&*layer, setType: &*ns("radial")];
        let _: () = msg_send![&*layer, setColors: &*color_array];
        let _: () = msg_send![&*layer, setLocations: &*locations];
        let _: () = msg_send![&*layer, setStartPoint: CGPoint::new(0.5, 0.5)];
        let _: () = msg_send![&*layer, setEndPoint: CGPoint::new(reach, reach)];
        let _: () = msg_send![&*layer, setFrame: CGRect::new(CGPoint::new(icon.x - 180.0, icon.y - 180.0), CGSize::new(360.0, 360.0))];
        let _: () = msg_send![&*layer, setContentsScale: scale];
        let _: () = msg_send![&*layer, setZPosition: 1000.0f64];
        let _: () = msg_send![&*layer, setOpacity: 0.0f32];
        let _: () = msg_send![host, addSublayer: &*layer];
        let total = 520.0;
        let opacity = sample(total, |t| number(glow_at(t).0 as f64));
        let transform_values = sample(total, |t| transform(CATransform3D::scale_move(glow_at(t).1 as f64, 0.0, 0.0)));
        keyframes(&layer, "opacity", "glow-opacity", opacity, begin, total as f64 / 1000.0, "forwards", false);
        keyframes(&layer, "transform", "glow-transform", transform_values, begin, total as f64 / 1000.0, "forwards", false);
        layer
    }

    /// Scale and fade the web view around the icon, its corners rounded for
    /// the move: out (240ms, to 0.92 and gone) or in (from 0.92 and nothing,
    /// 110ms later, over 420ms).
    unsafe fn web_move(view: Object, icon: CGPoint, begin: f64, leaves: bool) -> Option<Web> {
        let _: () = msg_send![&*view, setWantsLayer: Bool::YES];
        let layer: Option<Object> = msg_send![&*view, layer];
        let layer = layer?;
        let radius: f64 = msg_send![&*layer, cornerRadius];
        let masks: Bool = msg_send![&*layer, masksToBounds];
        let position: CGPoint = msg_send![&*layer, position];
        let model: CGRect = msg_send![&*layer, frame];
        let flipped: Bool = msg_send![&*layer, isGeometryFlipped];
        // Where the icon is from the layer's anchor, in the layer's own
        // orientation: scaling by s about the icon is scaling about the
        // anchor and moving by (1 - s) × this.
        let dy = icon.y - position.y;
        let to_icon = (icon.x - position.x, if flipped.as_bool() { -dy } else { dy });
        let _: () = msg_send![&*layer, setCornerRadius: 12.0f64];
        let _: () = msg_send![&*layer, setMasksToBounds: Bool::YES];
        let _: () = msg_send![&*view, setAlphaValue: 1.0f64];
        let at = |s: f32| {
            let k = 1.0 - s as f64;
            transform(CATransform3D::scale_move(s as f64, k * to_icon.0, k * to_icon.1))
        };
        if leaves {
            let d = LEAVE_MS;
            keyframes(&layer, "opacity", WEB_OPACITY, sample(d, |t| number(leave_at(t).0 as f64)), begin, d as f64 / 1000.0, "forwards", false);
            keyframes(&layer, "transform", WEB_TRANSFORM, sample(d, |t| at(leave_at(t).1)), begin, d as f64 / 1000.0, "forwards", false);
        } else {
            let d = ENTER_MS;
            let from = ENTER_FROM_MS;
            let begin = begin + from as f64 / 1000.0;
            keyframes(&layer, "opacity", WEB_OPACITY, sample(d, |t| number(enter_at(t + from).0 as f64)), begin, d as f64 / 1000.0, "both", false);
            keyframes(&layer, "transform", WEB_TRANSFORM, sample(d, |t| at(enter_at(t + from).1)), begin, d as f64 / 1000.0, "both", false);
        }
        Some(Web { view, layer, radius, masks: masks.as_bool(), model, icon })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timings() {
        // The leaving app is gone at 240ms, the entering one fully there by
        // 40% of its eased move and settled at the end.
        assert!((leave_at(0.0).0 - 1.0).abs() < 1e-4 && leave_at(240.0).0.abs() < 1e-4);
        assert!((leave_at(240.0).1 - 0.92).abs() < 1e-4);
        assert_eq!(enter_at(0.0), (0.0, 0.92));
        assert!(enter_at(110.0).0.abs() < 1e-4);
        assert!((enter_at(TOTAL_MS).0 - 1.0).abs() < 1e-4 && (enter_at(TOTAL_MS).1 - 1.0).abs() < 1e-4);
        assert!(mark_at(480.0).0.abs() < 1e-3);
        assert!(glow_at(520.0).0.abs() < 1e-3);
    }
}
