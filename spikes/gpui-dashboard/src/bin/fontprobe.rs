//! Which font names reach SF Rounded, and do its weights work?
#[path = "../fonts.rs"]
mod fonts;
use gpui::{div, prelude::*, px, rgb, size, App, Application, Bounds, Context, FontWeight, Window, WindowBounds, WindowOptions};

struct Probe;

const FAMILIES: [&str; 4] = ["Billable Rounded 500", "Billable Rounded 600", "Billable Rounded 700", ".SystemUIFont"];

impl Render for Probe {
    fn render(&mut self, _w: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let mut col = div().flex().flex_col().gap_2().p_4().size_full().bg(rgb(0x18181b)).text_color(rgb(0xf4f4f6));
        for fam in FAMILIES {
            let mut row = div().flex().flex_col().child(div().text_size(px(11.)).text_color(rgb(0x9696a1)).child(fam));
            for w in [400.0] {
                row = row.child(
                    div().font_family(fam).font_weight(FontWeight(w)).text_size(px(26.)).child(format!("{w} Fresh rt14 $5")),
                );
            }
            col = col.child(row);
        }
        col
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let t = std::time::Instant::now();
        let fonts = fonts::rounded_instances().expect("instances");
        eprintln!("instanced in {:?}, sizes {:?}", t.elapsed(), fonts.iter().map(|f| f.len()).collect::<Vec<_>>());
        cx.text_system().add_fonts(fonts).expect("add fonts");
        let bounds = Bounds::centered(None, size(px(560.), px(760.)), cx);
        cx.open_window(WindowOptions { window_bounds: Some(WindowBounds::Windowed(bounds)), ..Default::default() }, |_, cx| cx.new(|_| Probe)).unwrap();
    });
}
