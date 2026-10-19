//! Read-only HERDR theme adapter. The current server offers no resolved-palette API.
use crate::dialog::Palette;
use anyhow::{Context, Result};
fn color(v: &toml::Value) -> Option<[u8; 3]> {
    let s = v.as_str()?.strip_prefix('#')?;
    if s.len() != 6 || !s.is_ascii() {
        return None;
    }
    Some([
        u8::from_str_radix(&s[0..2], 16).ok()?,
        u8::from_str_radix(&s[2..4], 16).ok()?,
        u8::from_str_radix(&s[4..6], 16).ok()?,
    ])
}
pub fn parse(text: &str) -> Result<(Palette, Option<String>)> {
    let config: toml::Value = toml::from_str(text)?;
    let theme = config.get("theme");
    let name = theme
        .and_then(|t| t.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("catppuccin")
        .to_lowercase()
        .replace([' ', '_'], "-");
    let name = match name.as_str() {
        "light" | "latte" => "catppuccin-latte",
        "catppuccin-mocha" => "catppuccin",
        "tokyonight" => "tokyo-night",
        "tokyo-day" | "tokyonight-day" => "tokyo-night-day",
        "onedark" => "one-dark",
        "onelight" => "one-light",
        "gruvbox-dark" => "gruvbox",
        "solarized-dark" => "solarized",
        "lotus" => "kanagawa-lotus",
        "rosepine" => "rose-pine",
        "dawn" | "rosepine-dawn" => "rose-pine-dawn",
        other => other,
    };
    let table: serde_json::Value =
        serde_json::from_str(include_str!("../vendor/herdr/palettes.json"))?;
    let automatic = theme
        .and_then(|t| t.get("auto_switch"))
        .and_then(|v| v.as_bool())
        == Some(true);
    let unknown = table.get(name).is_none();
    let base = table.get(name).unwrap_or(&table["catppuccin"]);
    let rgb = |key| {
        serde_json::from_value::<[u8; 3]>(base[key].clone()).context("Invalid bundled theme token")
    };
    let mut palette = Palette {
        accent: rgb("accent")?,
        panel: rgb("panel_bg")?,
        text: rgb("text")?,
    };
    let mut unsupported = false;
    if let Some(custom) = theme.and_then(|t| t.get("custom")) {
        for (key, token) in [
            ("accent", &mut palette.accent),
            ("panel_bg", &mut palette.panel),
            ("text", &mut palette.text),
        ] {
            if let Some(value) = custom.get(key) {
                if let Some(value) = color(value) {
                    *token = value;
                } else {
                    unsupported = true;
                }
            }
        }
    }
    Ok((
        palette,
        if automatic || unknown || unsupported {
            Some("Dynamic HERDR theme unavailable; dialogs use configured/fallback colors".into())
        } else {
            None
        },
    ))
}
pub fn load() -> Result<(Palette, Option<String>)> {
    let path = crate::host_config::path();
    parse(&std::fs::read_to_string(path).context("HERDR theme could not be read")?)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selected_theme_custom_overrides_and_dynamic_fallback_are_explicit() {
        let (p, n) = parse("[theme]\nname='vesper'\n[theme.custom]\naccent='#123456'\n").unwrap();
        assert_eq!(p.accent, [18, 52, 86]);
        assert_eq!(p.panel, [26; 3]);
        assert!(n.is_none());
        assert!(
            parse("[theme]\nname='terminal'\nauto_switch=true")
                .unwrap()
                .1
                .is_some()
        );
    }
}
