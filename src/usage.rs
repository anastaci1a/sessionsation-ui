//! Shared, read-only account presentation. Missing observations are never limits.
use crate::{
    client::Account,
    render::{Accent, Canvas, Cell},
};
use unicode_width::UnicodeWidthStr;

pub fn short_name(id: &str) -> String {
    let name = id.split_once(':').map_or(id, |(_, n)| n);
    clipped_name(name, 12)
}
pub fn display_name(name: &str) -> String {
    clipped_name(name, 16)
}
fn clipped_name(name: &str, limit: usize) -> String {
    let clean: String = name.chars().filter(|c| !c.is_control()).collect();
    crate::widgets::ellipsize(&clean, limit)
}

#[derive(Clone, Debug)]
pub struct Window {
    pub label: &'static str,
    pub remaining: Option<f64>,
    pub limit: Option<f64>,
}
#[derive(Clone, Debug)]
pub struct Usage {
    pub windows: Vec<Window>,
    pub status: String,
    pub stale: bool,
}
impl Usage {
    pub fn account(a: &Account) -> Self {
        let stale = a.details.get("usage_fresh").and_then(|v| v.as_bool()) != Some(true);
        let status = if a.details.get("membership").and_then(|v| v.as_str()) == Some("removed") {
            "Deregistered"
        } else if a.details.get("enabled").and_then(|v| v.as_bool()) == Some(false) {
            "Disabled"
        } else if a.details.get("auth_required").and_then(|v| v.as_bool()) == Some(true) {
            "Login required"
        } else if a.usage["observed_at"].as_i64().is_none() {
            "Usage unavailable"
        } else if stale {
            "Usage stale"
        } else {
            "Ready"
        }
        .to_owned();
        let windows = {
            [
                ("short", "5h", "short_percent", 300),
                ("weekly", "wk", "weekly_percent", 10080),
            ]
            .into_iter()
            .filter(|(key, _, _, duration)| {
                a.usage[*key]["duration_minutes"] == *duration
                    || (*key == "weekly" && a.usage["short"]["duration_minutes"] == 300)
            })
            .map(|(key, label, threshold, duration)| {
                let remaining = (a.usage[key]["duration_minutes"] == duration)
                    .then(|| a.usage[key]["used_percent"].as_f64())
                    .flatten()
                    .filter(|n| n.is_finite() && (0. ..=100.).contains(n))
                    .map(|used| 100. - used);
                let limits = a.details.get("switch_thresholds");
                let limit = limits
                    .filter(|v| v["customized"] == true)
                    .and_then(|v| v[threshold].as_f64())
                    .filter(|n| n.is_finite() && (0. ..=100.).contains(n))
                    .map(|used| 100. - used);
                Window {
                    label,
                    remaining,
                    limit,
                }
            })
            .collect()
        };
        Self {
            windows,
            status,
            stale,
        }
    }
    pub fn status_only(&self) -> bool {
        matches!(
            self.status.as_str(),
            "Disabled" | "Login required" | "Deregistered"
        ) || self.windows.iter().all(|w| w.remaining.is_none())
    }
    pub fn height(&self) -> usize {
        if self.status_only() {
            1
        } else {
            self.windows.len().max(1)
        }
    }
    pub fn digits(&self) -> usize {
        if self
            .windows
            .iter()
            .any(|w| w.remaining.is_some_and(|p| p.round() == 100.))
        {
            3
        } else {
            2
        }
    }
    pub fn width(&self, bar: usize, digits: usize) -> usize {
        if self.status_only() {
            UnicodeWidthStr::width(self.status.as_str())
        } else {
            bar + digits + 5 + if self.stale { 6 } else { 0 }
        }
    }
    pub fn paint(&self, canvas: &mut Canvas, x: usize, y: usize, bar: usize, digits: usize) {
        self.paint_selected(canvas, x, y, bar, digits, false);
    }
    pub fn paint_focused(
        &self,
        canvas: &mut Canvas,
        x: usize,
        y: usize,
        bar: usize,
        digits: usize,
    ) {
        self.paint_selected(canvas, x, y, bar, digits, true);
        if self.status_only() {
            canvas.accent_text(
                x + self.width(bar, digits),
                y,
                " ◀",
                Accent {
                    bold: false,
                    flash: false,
                    ..Accent::default()
                },
            );
            return;
        }
        for row in 0..self.windows.len() {
            for col in x..(x + bar).min(canvas.width) {
                if y + row >= canvas.height {
                    break;
                }
                let cell = &mut canvas.cells[(y + row) * canvas.width + col];
                cell.bold = true;
            }
        }
        if let Some(row) = self.windows.iter().position(|w| w.remaining.is_some()) {
            canvas.text(
                x + self.width(bar, digits),
                y + row,
                " ◀",
                Cell {
                    fg: crate::widgets::SELECTED,
                    bold: true,
                    ..Cell::default()
                },
                false,
            );
        }
    }
    pub fn paint_deemphasized(
        &self,
        canvas: &mut Canvas,
        x: usize,
        y: usize,
        bar: usize,
        digits: usize,
    ) {
        if self.status_only() {
            self.paint_status(canvas, x, y, 600);
            return;
        }
        self.paint_selected(canvas, x, y, bar, digits, false);
        for (row, _) in self
            .windows
            .iter()
            .enumerate()
            .filter(|(_, w)| w.remaining.is_some())
        {
            for col in x..(x + bar).min(canvas.width) {
                if y + row >= canvas.height {
                    break;
                }
                let cell = &mut canvas.cells[(y + row) * canvas.width + col];
                cell.fg = cell.fg.map(|v| (u16::from(v) * 65 / 100) as u8);
                cell.bold = false;
            }
        }
    }
    pub fn paint_preview(
        &self,
        canvas: &mut Canvas,
        x: usize,
        y: usize,
        bar: usize,
        digits: usize,
    ) {
        self.paint_selected(canvas, x, y, bar, digits, true);
        for row in y..(y + self.height()).min(canvas.height) {
            for col in (x + bar).min(canvas.width)..(x + self.width(bar, digits)).min(canvas.width)
            {
                canvas.cells[row * canvas.width + col].bold = false;
            }
        }
    }
    fn paint_status(&self, canvas: &mut Canvas, x: usize, y: usize, brightness: i64) {
        canvas.accent_text(
            x,
            y,
            &self.status,
            Accent {
                brightness,
                bold: false,
                flash: false,
                ..Accent::default()
            },
        );
    }
    pub fn paint_selected(
        &self,
        canvas: &mut Canvas,
        x: usize,
        y: usize,
        bar: usize,
        digits: usize,
        selected: bool,
    ) {
        if self.status_only() {
            self.paint_status(canvas, x, y, 1000);
            return;
        }
        for (row, window) in self.windows.iter().enumerate() {
            let interior = bar.saturating_sub(2).max(1);
            let filled = window.remaining.map(|p| interior as f64 * p / 100.);
            let Some(filled) = filled else {
                canvas.accent_text(
                    x,
                    y,
                    &format!(
                        "{}: {}",
                        window.label,
                        if self.status == "Ready" {
                            "Usage unavailable"
                        } else {
                            &self.status
                        }
                    ),
                    Accent::default(),
                );
                continue;
            };
            let marker = window
                .limit
                .map(|p| 1 + (interior.saturating_sub(1) as f64 * p / 100.).round() as usize);
            for col in 0..bar {
                let glyph = bar_glyph(col, bar, filled, marker);
                if matches!(glyph, "╾" | "┽" | "┼" | "─" | "╶")
                    || (glyph == "╴" && filled < interior as f64)
                {
                    canvas.text(
                        x + col,
                        y + row,
                        glyph,
                        Cell {
                            fg: [144; 3],
                            ..Cell::default()
                        },
                        false,
                    );
                } else {
                    canvas.accent_text(
                        x + col,
                        y + row,
                        glyph,
                        Accent {
                            brightness: if self.stale { 300 } else { 1000 },
                            flash: false,
                            ..Accent::default()
                        },
                    );
                }
            }
            let percent = window
                .remaining
                .map(|p| format!("{p:.0}"))
                .unwrap_or_else(|| "?".into());
            let label = format!(
                "{:>digits$}% {}{}",
                percent,
                window.label,
                if self.stale { " stale" } else { "" }
            );
            canvas.text(
                x + bar + 1,
                y + row,
                &label,
                Cell {
                    fg: if selected {
                        crate::widgets::SELECTED
                    } else {
                        crate::widgets::SECONDARY
                    },
                    bold: selected,
                    ..Cell::default()
                },
                false,
            );
        }
    }
}

// Match horizontal weights at every cell boundary, including 0%/100% caps.
// Character connections: https://www.unicode.org/charts/PDF/U2500.pdf
fn bar_glyph(col: usize, width: usize, filled: f64, marker: Option<usize>) -> &'static str {
    let interior = width.saturating_sub(2).max(1) as f64;
    if col == 0 {
        return if filled > 0. { "╺" } else { "╶" };
    }
    if col + 1 == width {
        return if filled >= interior { "╸" } else { "╴" };
    }
    let position = (col - 1) as f64;
    let left_heavy = filled > position;
    let right_heavy = filled >= interior || filled > position + 1.;
    match (marker == Some(col), left_heavy, right_heavy) {
        (true, _, true) => "┿",
        (true, true, false) => "┽",
        (true, false, false) => "┼",
        (false, _, true) => "━",
        (false, true, false) => "╾",
        (false, false, false) => "─",
    }
}

/// One layout calculation supplies the fit decision and actual coordinates.
#[derive(Clone, Debug)]
pub struct AccountsLayout {
    pub left: usize,
    pub bars: usize,
    pub bar_width: usize,
    pub digits: usize,
    pub height: usize,
}
impl AccountsLayout {
    pub fn fit(accounts: &[Account], width: usize, available_height: usize) -> Option<Self> {
        let uses: Vec<_> = accounts.iter().map(Usage::account).collect();
        let height = uses
            .iter()
            .map(|u| u.height() + 1)
            .sum::<usize>()
            .saturating_sub(1);
        if height > available_height {
            return None;
        }
        let names = accounts
            .iter()
            .map(|a| UnicodeWidthStr::width(short_name(&a.id).as_str()))
            .max()
            .unwrap_or(0);
        let digits = uses.iter().map(Usage::digits).max().unwrap_or(2);
        let right = uses.iter().map(|u| u.width(12, digits)).max().unwrap_or(0) + 2;
        // Two-column selection lane, then one space between the name and bar.
        let prefix = names + 3;
        let min_width = prefix + right;
        if min_width + 2 > width {
            return None;
        }
        let extra = (width - 2 - min_width).min(12);
        let left = width.saturating_sub(min_width + extra) / 2;
        Some(Self {
            left,
            bars: left + prefix,
            bar_width: 12 + extra,
            digits,
            height,
        })
    }
    pub fn paint(
        &self,
        canvas: &mut Canvas,
        accounts: &[Account],
        top: usize,
        selected: Option<usize>,
        back: bool,
        usage_focus: bool,
    ) {
        let depressed = back || selected.is_none_or(|i| i >= accounts.len());
        let name_focus = !depressed && !usage_focus;
        let mut y = top;
        for (index, account) in accounts.iter().enumerate() {
            let active = selected == Some(index) && !back;
            if active && !usage_focus {
                canvas.accent_text(self.left, y, "▶", Accent::default());
            }
            canvas.accent_text(
                self.left + 2,
                y,
                &short_name(&account.id),
                Accent {
                    brightness: if depressed || name_focus && !active {
                        600
                    } else {
                        1000
                    },
                    bold: active && !usage_focus,
                    ..Accent::default()
                },
            );
            let usage = Usage::account(account);
            if active && usage_focus {
                usage.paint_focused(canvas, self.bars, y, self.bar_width, self.digits);
            } else if depressed || usage_focus {
                usage.paint_deemphasized(canvas, self.bars, y, self.bar_width, self.digits);
            } else {
                usage.paint_selected(
                    canvas,
                    self.bars,
                    y,
                    self.bar_width,
                    self.digits,
                    active && usage.status == "Login required",
                );
            }
            y += usage.height() + 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn home_focus_dims_only_other_names_or_statuses_and_restores_on_exit() {
        let mut accounts = vec![account(), account(), account()];
        accounts[0].id = "codex:personal".into();
        accounts[1].id = "codex:work".into();
        accounts[1].details.insert("enabled".into(), json!(false));
        accounts[2].id = "codex:other".into();
        accounts[2]
            .details
            .insert("auth_required".into(), json!(true));
        let layout = AccountsLayout::fit(&accounts, 100, 30).unwrap();
        let mut field = crate::render::Field::new(100, 40, 1967);
        for time in [0, 4200, 8300] {
            let base = field.paint(time);
            for (selected, back, usage_focus) in [
                (Some(0), false, false),
                (Some(1), false, false),
                (Some(0), false, true),
                (Some(1), false, true),
                (Some(2), false, true),
                (Some(0), true, false),
                (Some(0), true, true),
                (None, false, false),
            ] {
                let mut c = base.clone();
                layout.paint(&mut c, &accounts, 3, selected, back, usage_focus);
                let mut y = 3;
                for (i, a) in accounts.iter().enumerate() {
                    let active = selected == Some(i) && !back;
                    let dim_name = back || selected.is_none() || !usage_focus && !active;
                    let mut expected = base.clone();
                    expected.accent_text(
                        layout.left + 2,
                        y,
                        &short_name(&a.id),
                        Accent {
                            brightness: if dim_name { 600 } else { 1000 },
                            bold: active && !usage_focus,
                            ..Accent::default()
                        },
                    );
                    let cell = y * 100 + layout.left + 2;
                    assert_eq!(c.cells[cell], expected.cells[cell]);
                    let usage = Usage::account(a);
                    if usage.status_only() {
                        let brightness = if back || selected.is_none() || usage_focus && !active {
                            600
                        } else {
                            1000
                        };
                        usage.paint_status(&mut expected, layout.bars, y, brightness);
                        for x in
                            layout.bars..layout.bars + usage.width(layout.bar_width, layout.digits)
                        {
                            assert_eq!(c.cells[y * 100 + x], expected.cells[y * 100 + x]);
                            assert!(!c.cells[y * 100 + x].bold);
                        }
                    }
                    if (back || selected.is_none()) && !usage.status_only() {
                        let mut plain = base.clone();
                        usage.paint(&mut plain, layout.bars, y, layout.bar_width, layout.digits);
                        for row in y..y + usage.height() {
                            for x in layout.bars + layout.bar_width + 1
                                ..layout.bars + usage.width(layout.bar_width, layout.digits)
                            {
                                assert_eq!(c.cells[row * 100 + x], plain.cells[row * 100 + x]);
                                assert_eq!(c.cells[row * 100 + x].fg, crate::widgets::SECONDARY);
                                assert!(!c.cells[row * 100 + x].bold);
                            }
                            for x in layout.bars..layout.bars + layout.bar_width {
                                assert_eq!(
                                    c.cells[row * 100 + x].fg,
                                    plain.cells[row * 100 + x]
                                        .fg
                                        .map(|v| (u16::from(v) * 65 / 100) as u8)
                                );
                            }
                        }
                    }
                    y += usage.height() + 1;
                }
            }
        }
    }
    #[test]
    fn focused_bar_keeps_colors_and_dims_other_bars() {
        let u = Usage {
            windows: vec![Window {
                label: "wk",
                remaining: Some(50.),
                limit: Some(20.),
            }],
            status: "Ready".into(),
            stale: false,
        };
        let mut plain = Canvas::new(40, 4);
        let mut focus = Canvas::new(40, 4);
        let mut dim = Canvas::new(40, 4);
        u.paint_deemphasized(&mut dim, 1, 1, 20, 2);
        u.paint_selected(&mut plain, 1, 1, 20, 2, true);
        u.paint_focused(&mut focus, 1, 1, 20, 2);
        let arrow = &focus.cells[40 + 1 + u.width(20, 2) + 1];
        assert_eq!(arrow.glyph, "◀");
        assert_eq!(arrow.fg, crate::widgets::SELECTED);
        assert_eq!(focus.cells[40 + 1 + u.width(20, 2)].glyph, " ");
        for x in 1..21 {
            let before = &plain.cells[40 + x];
            let after = &focus.cells[40 + x];
            assert_eq!(before.glyph, after.glyph);
            assert!(after.bold);
            assert_eq!(before.fg, after.fg);
            assert_eq!(
                dim.cells[40 + x].fg,
                before.fg.map(|n| (u16::from(n) * 65 / 100) as u8)
            );
        }
    }
    #[test]
    fn status_focus_has_matching_accent_triangle_without_bars_or_white_text() {
        let mut field = crate::render::Field::new(60, 8, 1967);
        for status in [
            "Disabled",
            "Login required",
            "Deregistered",
            "Usage unavailable",
        ] {
            let u = Usage {
                status: status.into(),
                windows: vec![],
                stale: false,
            };
            let mut c = field.paint(4200);
            let mut expected = c.clone();
            u.paint_focused(&mut c, 2, 2, 20, 2);
            let end = 2 + UnicodeWidthStr::width(status);
            assert_eq!(c.cells[120 + end].glyph, " ");
            assert_eq!(c.cells[120 + end + 1].glyph, "◀");
            expected.accent_text(
                end,
                2,
                " ◀",
                Accent {
                    bold: false,
                    flash: false,
                    ..Accent::default()
                },
            );
            assert_eq!(expected.cells[120 + end + 1], c.cells[120 + end + 1]);
            assert!(!c.cells[122].bold);
        }
    }
    #[test]
    fn selected_usage_labels_and_account_status_colors_have_distinct_rules() {
        let mut a = account();
        let u = Usage::account(&a);
        let mut c = Canvas::new(60, 5);
        u.paint_selected(&mut c, 1, 1, 20, 3, true);
        assert_eq!(c.cells[60 + 22].fg, crate::widgets::SELECTED);
        assert!(c.cells[60 + 22].bold);
        a.details.insert("enabled".into(), json!(false));
        let u = Usage::account(&a);
        assert_eq!(u.height(), 1);
        let mut c = Canvas::new(60, 5);
        u.paint_selected(&mut c, 1, 1, 20, 3, true);
        let mut expected = Canvas::new(60, 5);
        expected.accent_text(
            1,
            1,
            "Disabled",
            Accent {
                bold: false,
                flash: false,
                ..Accent::default()
            },
        );
        assert_eq!(c.cells[61].fg, expected.cells[61].fg);
        assert!(!c.cells[61].bold);
        assert!(c.cells.iter().all(|c| c.glyph != "━"));
        assert_eq!(display_name("abcdefghijklmnopqrstuvwx"), "abcdefghijklmno…");
        assert_eq!(short_name("codex:abcdefghijklmnopqrstuvwx"), "abcdefghijk…");
        assert_eq!(display_name("my:session"), "my:session");
    }
    fn account() -> Account {
        serde_json::from_value(json!({"id":"codex:long-account-name", "selectable":true,
        "usage_fresh":true,"usage":{"observed_at":100,"short":{"duration_minutes":300,"used_percent":20},"weekly":{"duration_minutes":10080,"used_percent":0}},
        "switch_thresholds":{"customized":true,"short_percent":80,"weekly_percent":90}
    })).unwrap()
    }
    #[test]
    fn every_bar_join_is_continuous_at_all_percentages_and_threshold_positions() {
        let weights = |glyph| match glyph {
            "╺" => (0, 2),
            "╶" => (0, 1),
            "╸" => (2, 0),
            "╴" => (1, 0),
            "━" | "┿" => (2, 2),
            "╾" | "┽" => (2, 1),
            "─" | "┼" => (1, 1),
            _ => panic!("unexpected bar segment"),
        };
        for width in [3, 7, 24] {
            for step in 0..=1000 {
                let filled = (width - 2) as f64 * step as f64 / 1000.;
                for marker in std::iter::once(None).chain((1..width - 1).map(Some)) {
                    for col in 0..width - 1 {
                        assert_eq!(
                            weights(bar_glyph(col, width, filled, marker)).1,
                            weights(bar_glyph(col + 1, width, filled, marker)).0,
                            "width={width}, percentage={step}/10, marker={marker:?}, col={col}"
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn disabled_and_unconfigured_accounts_never_wait_for_impossible_usage() {
        for (details, expected) in [
            (json!({"enabled":false}), "Disabled"),
            (
                json!({"enabled":true,"auth_required":true}),
                "Login required",
            ),
        ] {
            let mut a = account();
            a.usage = json!({});
            for (key, value) in details.as_object().unwrap() {
                a.details.insert(key.clone(), value.clone());
            }
            let mut c = Canvas::new(50, 4);
            Usage::account(&a).paint(&mut c, 1, 1, 20, 2);
            let text = c.cells.iter().map(|c| c.glyph.as_str()).collect::<String>();
            assert!(text.contains(expected));
            assert!(!text.contains("Loading"));
        }
    }
    #[test]
    fn weekly_only_and_missing_data_never_replace_usage_with_ready() {
        let mut a = account();
        a.usage["short"] = serde_json::Value::Null;
        let u = Usage::account(&a);
        assert_eq!(u.windows.len(), 1);
        assert_eq!(u.windows[0].label, "wk");
        a.usage = json!({});
        let mut c = Canvas::new(40, 4);
        Usage::account(&a).paint(&mut c, 1, 1, 20, 2);
        let text = c.cells.iter().map(|c| c.glyph.as_str()).collect::<String>();
        assert!(text.contains("Usage unavailable"));
        assert!(!text.contains("Ready"));
    }
    #[test]
    fn capped_line_bars_share_threshold_color_and_align_percentage_labels() {
        let mut a = account();
        a.usage["short"]["used_percent"] = json!(74.);
        a.details.remove("switch_thresholds");
        let u = Usage::account(&a);
        let mut c = Canvas::new(50, 5);
        u.paint(&mut c, 1, 1, 10, 2);
        assert_eq!(c.cells[51].glyph, "╺");
        assert_eq!(c.cells[52].glyph, "━");
        assert_eq!(c.cells[53].glyph, "━");
        assert_eq!(c.cells[54].glyph, "╾");
        assert_eq!(c.cells[54].fg, [144; 3]);
        assert_eq!(c.cells[55].glyph, "─");
        assert_eq!(c.cells[55].fg, [144; 3]);
        assert_eq!(c.cells[60].glyph, "╴");
        let u = Usage::account(&account());
        u.paint(&mut c, 1, 1, 10, 3);
        let marker = &c.cells[53];
        assert_eq!(marker.glyph, "┿");
        assert_eq!(marker.fg, c.cells[52].fg);
        let text = c.cells[50..100]
            .iter()
            .map(|c| c.glyph.as_str())
            .collect::<String>();
        assert!(text.contains(" 80% 5h"));
        let mut a = account();
        a.usage["short"]["used_percent"] = json!(75.);
        Usage::account(&a).paint(&mut c, 1, 1, 10, 3);
        assert_eq!(c.cells[53].glyph, "┽");
        assert_eq!(c.cells[53].fg, [144; 3]);
        a.usage["short"]["used_percent"] = json!(90.);
        Usage::account(&a).paint(&mut c, 1, 1, 10, 3);
        assert_eq!(c.cells[53].glyph, "┼");
        assert_eq!(c.cells[53].fg, [144; 3]);
    }
    #[test]
    fn markers_convert_used_thresholds_and_names_only_shorten_display() {
        let a = account();
        let u = Usage::account(&a);
        assert_eq!(short_name(&a.id), "long-accoun…");
        assert_eq!(a.id, "codex:long-account-name");
        assert_eq!(u.windows[0].remaining, Some(80.));
        assert_eq!(u.windows[0].limit, Some(20.));
        assert_eq!(u.digits(), 3);
        let mut c = Canvas::new(60, 6);
        u.paint(&mut c, 3, 1, 11, 3);
        assert_eq!(c.cells[60 + 6].glyph, "┿");
        assert_eq!(c.cells[120 + 5].glyph, "┿");
    }
    #[test]
    fn no_five_hour_window_means_status_and_missing_weekly_is_unknown() {
        let mut a = account();
        a.usage["weekly"] = serde_json::Value::Null;
        assert_eq!(Usage::account(&a).windows[1].remaining, None);
        a.usage["short"]["duration_minutes"] = json!(60);
        assert_eq!(Usage::account(&a).windows.len(), 0);
        a.usage = json!({});
        assert_eq!(Usage::account(&a).status, "Usage unavailable");
    }
    #[test]
    fn fit_is_all_or_nothing_and_shares_columns() {
        let accounts = vec![account(), account()];
        assert!(AccountsLayout::fit(&accounts, 80, 4).is_none());
        let l = AccountsLayout::fit(&accounts, 80, 5).unwrap();
        assert_eq!(l.height, 5);
        assert_eq!(l.digits, 3);
        assert!(AccountsLayout::fit(&accounts, 30, 40).is_none());
        assert!(l.bars + l.bar_width + 10 < 80);
        assert_eq!(l.bar_width, 24);
        let wide = AccountsLayout::fit(&accounts, 140, 5).unwrap();
        assert_eq!(wide.bar_width, l.bar_width);
        assert_eq!(wide.left, l.left + 30);
        let narrow = AccountsLayout::fit(&accounts, 40, 5).unwrap();
        assert!(narrow.bar_width < wide.bar_width);
        assert!(narrow.left >= 1);
    }
}

/// Shared right-edge picker columns. Padding is computed from displayed values,
/// not a fixed three-digit reservation; absent five-hour windows have no slash.
pub fn picker_labels(accounts: &[Account]) -> Vec<String> {
    let values: Vec<_> = accounts
        .iter()
        .map(|a| {
            let remaining = |key: &str, duration: i64| {
                (a.usage[key]["duration_minutes"].as_i64() == Some(duration))
                    .then(|| a.usage[key]["used_percent"].as_f64())
                    .flatten()
                    .filter(|n| n.is_finite() && (0. ..=100.).contains(n))
                    .map(|n| format!("{:.0}", 100. - n))
                    .unwrap_or_else(|| "?".into())
            };
            (
                (a.usage["short"]["duration_minutes"] == 300).then(|| remaining("short", 300)),
                remaining("weekly", 10080),
            )
        })
        .collect();
    let digits = values
        .iter()
        .flat_map(|(short, weekly)| short.iter().chain(std::iter::once(weekly)))
        .map(String::len)
        .max()
        .unwrap_or(1);
    values
        .into_iter()
        .map(|(short, weekly)| match short {
            Some(short) => format!("{short:>digits$}% 5h / {weekly:>digits$}% wk"),
            None => format!("{weekly:>digits$}% wk"),
        })
        .collect()
}

#[cfg(test)]
mod picker_tests {
    use super::*;
    use serde_json::json;
    fn account(short: Option<f64>, weekly: f64) -> Account {
        serde_json::from_value(json!({"id":"codex:fixture","selectable":true,"usage":{"short":short.map(|used|json!({"duration_minutes":300,"used_percent":used})),"weekly":{"duration_minutes":10080,"used_percent":weekly}}})).unwrap()
    }
    #[test]
    fn picker_padding_is_minimal_and_all_usage_columns_align() {
        let mut accounts = vec![
            account(Some(95.), 20.),
            account(Some(1.), 94.),
            account(None, 55.),
        ];
        let labels = picker_labels(&accounts);
        assert_eq!(labels, vec![" 5% 5h / 80% wk", "99% 5h /  6% wk", "45% wk"]);
        assert_eq!(labels[0].find('/'), labels[1].find('/'));
        assert_eq!(labels[0].find('%'), labels[1].find('%'));
        accounts[1].usage["short"]["used_percent"] = json!(0);
        assert_eq!(picker_labels(&accounts)[0], "  5% 5h /  80% wk");
        assert_eq!(
            picker_labels(&[account(Some(95.), 94.)])[0],
            "5% 5h / 6% wk"
        );
        accounts[2].usage["short"] = json!({"duration_minutes":60,"used_percent":0});
        assert!(!picker_labels(&accounts)[2].contains('/'));
    }
}
