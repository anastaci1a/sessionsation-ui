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
        secondary: rgb("subtext0")?,
        muted: rgb("overlay0")?,
        error: rgb("red")?,
    };
    let mut unsupported = false;
    if let Some(custom) = theme.and_then(|t| t.get("custom")) {
        for (key, token) in [
            ("accent", &mut palette.accent),
            ("panel_bg", &mut palette.panel),
            ("text", &mut palette.text),
            ("subtext0", &mut palette.secondary),
            ("overlay0", &mut palette.muted),
            ("red", &mut palette.error),
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
    fn popup_roles_match_dark_light_and_custom_host_tokens() {
        for (name, panel, text, secondary, muted) in [
            (
                "rose-pine",
                [25, 23, 36],
                [224, 222, 244],
                [200, 197, 220],
                [110, 106, 134],
            ),
            (
                "catppuccin-latte",
                [239, 241, 245],
                [76, 79, 105],
                [108, 111, 133],
                [156, 160, 176],
            ),
        ] {
            let (p, notice) = parse(&format!("[theme]\nname='{name}'")).unwrap();
            assert!(notice.is_none());
            assert_eq!(
                (p.panel, p.text, p.secondary, p.muted),
                (panel, text, secondary, muted)
            );
            let mut c = crate::render::Canvas::new(40, 12);
            crate::popup::paint(&mut c, "Roles", &p, 10, |local| {
                for (y, fg) in [
                    (0, crate::widgets::SELECTED),
                    (1, crate::widgets::SECONDARY),
                    (2, crate::widgets::MUTED),
                    (3, [215, 0, 0]),
                ] {
                    local.widget_text(
                        0,
                        y,
                        "X",
                        crate::render::Cell {
                            fg,
                            ..Default::default()
                        },
                        false,
                    );
                }
                crate::widgets::paint_hint(local, 0, 4, "Enter: Choose");
            });
            let colors: Vec<_> = c
                .cells
                .iter()
                .filter(|c| c.glyph == "X")
                .map(|c| c.fg)
                .collect();
            assert_eq!(colors, vec![p.text, p.secondary, p.muted, p.error]);
            assert!(
                c.popup_palette.is_none(),
                "popup roles must not leak into the menu"
            );
        }
        let (p, _) = parse("[theme]\nname='rose-pine'\n[theme.custom]\nsubtext0='#123456'\noverlay0='#234567'\nred='#345678'\ntext='#456789'").unwrap();
        assert_eq!(p.secondary, [18, 52, 86]);
        assert_eq!(p.muted, [35, 69, 103]);
        assert_eq!(p.error, [52, 86, 120]);
        assert_eq!(p.text, [69, 103, 137]);
    }
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
