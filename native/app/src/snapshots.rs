//! Dev only (`--features snapshots`): with BILLABLE_SNAPSHOTS=<dir> and
//! BILLABLE_SNAPSHOT_ROUTES="/,/clients,…", open each route, let it settle,
//! and save what the window draws as <dir>/<n>-<route>.png, then quit.
//! Renders offscreen from the frame GPUI built, so it needs no screen access.

#![cfg(feature = "snapshots")]

use std::time::Duration;

use gpui::App;

pub fn start(cx: &mut App) {
    let Ok(dir) = std::env::var("BILLABLE_SNAPSHOTS") else { return };
    let routes: Vec<String> = std::env::var("BILLABLE_SNAPSHOT_ROUTES")
        .unwrap_or_else(|_| "/".into())
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let settle: u64 = std::env::var("BILLABLE_SNAPSHOT_SETTLE_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(1600);
    let _ = std::fs::create_dir_all(&dir);
    cx.spawn(async move |cx| {
        for (i, route) in routes.iter().enumerate() {
            let r = route.clone();
            cx.update(|cx| crate::platform::with_main_window(cx, move |w, cx| crate::shell::navigate_path(&r, w, cx)));
            cx.background_executor().timer(Duration::from_millis(settle)).await;
            let name = route.trim_matches('/').replace(['/', '?', '=', '&'], "_");
            let file = format!("{dir}/{:02}-{}.png", i, if name.is_empty() { "dashboard".into() } else { name });
            cx.update(|cx| {
                crate::platform::with_main_window(cx, |w, _| match w.render_to_image() {
                    Ok(img) => {
                        if let Err(e) = img.save(&file) {
                            eprintln!("snapshot {file}: {e}");
                        }
                    }
                    Err(e) => eprintln!("snapshot {file}: {e}"),
                })
            });
        }
        cx.update(|cx| cx.quit());
    })
    .detach();
}
