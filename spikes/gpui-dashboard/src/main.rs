//! Billable's Dashboard, drawn natively with GPUI (Zed's UI toolkit), to see
//! how close a native Rust port can get to the Electron app's look.
//!
//! It reads the active profile's real database, read-only. Nothing here
//! writes anything: the buttons look and hover like the app's but do nothing.

mod art;
mod data;
mod fonts;
mod format;
mod theme;
mod ui;

use gpui::{point, prelude::*, px, size, App, Application, Bounds, TitlebarOptions, WindowBounds, WindowOptions};

fn main() {
    Application::new().run(|cx: &mut App| {
        match fonts::rounded_instances() {
            Ok(fonts) => {
                if let Err(e) = cx.text_system().add_fonts(fonts) {
                    eprintln!("SF Rounded copies were not accepted: {e}");
                }
            }
            Err(e) => eprintln!("Could not make SF Rounded weights: {e}"),
        }

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
        cx.open_window(options, |window, cx| cx.new(|cx| ui::BillableWindow::new(window, cx)))
            .expect("open window");
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
    });
}
