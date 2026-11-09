//! Billable for macOS, native.

mod art;
mod assets;
mod components;
mod export;
mod fonts;
mod format;
mod icons;
mod motion;
mod pages;
mod platform;
mod services;
mod shell;
#[cfg(feature = "snapshots")]
mod snapshots;
mod store;
mod style;
mod theme;
mod timer;

use gpui::{point, prelude::*, px, size, App, Bounds, TitlebarOptions, WindowBounds, WindowOptions};
use gpui_component::Root;

/// index.css sets `-webkit-font-smoothing: antialiased` on the body, which
/// turns off macOS's stroke thickening. GPUI thickens glyphs unless the app's
/// own AppleFontSmoothing preference is 0, so set it for Billable only.
fn match_css_font_smoothing() {
    use objc2_foundation::{NSString, NSUserDefaults};
    let defaults = NSUserDefaults::standardUserDefaults();
    defaults.setInteger_forKey(0, &NSString::from_str("AppleFontSmoothing"));
}

fn main() {
    match_css_font_smoothing();
    let app = gpui_platform::application().with_assets(assets::Assets);
    app.on_reopen(platform::show_main_window);
    app.on_open_urls(|urls| {
        for url in urls {
            platform::queue_link(url);
        }
    });
    app.run(|cx: &mut App| {
        gpui_component::init(cx);
        cx.set_global(art::Gallery::default());
        cx.set_global(theme::Theme::dark());
        match fonts::rounded_instances() {
            Ok(fonts) => {
                if let Err(e) = cx.text_system().add_fonts(fonts) {
                    eprintln!("SF Rounded copies were not accepted: {e}");
                }
            }
            Err(e) => eprintln!("Could not make SF Rounded weights: {e}"),
        }
        match store::Store::open() {
            Ok(s) => cx.set_global(s),
            Err(e) => {
                eprintln!("Couldn't open Billable's data: {e:#}");
                cx.quit();
                return;
            }
        }

        platform::init(cx);
        services::phone::start(cx);
        open_main_window(cx);
        #[cfg(feature = "snapshots")]
        snapshots::start(cx);
        cx.activate(true);
    });
}

/// Open Billable's window (at launch, and again from the Dock or the menu
/// bar after it was closed).
pub fn open_main_window(cx: &mut App) {
    let bounds = Bounds::centered(None, size(px(1280.), px(820.)), cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some("Billable".into()),
            appears_transparent: true,
            traffic_light_position: Some(point(px(16.), px(18.))),
        }),
        window_min_size: Some(size(px(960.), px(600.))),
        ..Default::default()
    };
    match cx.open_window(options, |window, cx| {
        let view = shell::app::root(window, cx);
        cx.new(|cx| Root::new(view, window, cx))
    }) {
        Ok(handle) => platform::set_main_window(handle.into()),
        Err(e) => eprintln!("Couldn't open the window: {e}"),
    }
}
