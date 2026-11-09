//! Assets: Billable's Lucide icons (`billable/<Name>[@stroke|@fill].svg`),
//! then gpui-component's own icon set for its internals.

use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};

use crate::icons::Lucide;

pub struct Assets;

/// `Name`, `Name@3`, `Name@fill` → the icon's SVG, black on transparent
/// (GPUI's `svg()` uses it as a mask and paints it with `text_color`).
fn lucide_svg(spec: &str) -> Option<Vec<u8>> {
    let (name, option) = spec.split_once('@').unwrap_or((spec, ""));
    let icon = Lucide::from_name(name)?;
    let (stroke, fill) = match option {
        "" => ("2".to_string(), "none"),
        "fill" => ("2".to_string(), "#000"),
        w => (w.to_string(), "none"),
    };
    Some(
        format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="{fill}" stroke="#000" stroke-width="{stroke}" stroke-linecap="round" stroke-linejoin="round">{}</svg>"##,
            icon.body()
        )
        .into_bytes(),
    )
}

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some(rest) = path.strip_prefix("billable/") {
            let spec = rest.trim_end_matches(".svg");
            return Ok(lucide_svg(spec).map(Cow::Owned));
        }
        gpui_kit_assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        if path.starts_with("billable/") {
            return Ok(Lucide::ALL.iter().map(|i| format!("billable/{}.svg", i.name()).into()).collect());
        }
        gpui_kit_assets::Assets.list(path)
    }
}
