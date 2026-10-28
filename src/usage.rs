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
/// Account name for lists and tags: the backend display name when set, else the
/// short name derived from the ID.
pub fn account_title(a: &Account) -> String {
    a.details
        .get("display_name")
        .and_then(|v| v.as_str())
        .map(|n| clipped_name(n, 12))
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| short_name(&a.id))
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
    /// Compact age of the observation (`42m ago`) once it is 60 s old or more.
    pub age: Option<String>,
}
/// Backend usage state (`fresh|partial|stale|missing|no_windows|error`), read from
/// the usage object or the account-level mirror.
pub fn usage_state(a: &Account) -> Option<&str> {
    a.usage["state"]
        .as_str()
        .or_else(|| a.details.get("usage_state").and_then(|v| v.as_str()))
}
static CLOCK_OVERRIDE: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);
/// Pin the wall clock (Unix seconds) used for usage ages; 0 restores the real clock.
pub fn set_clock(now: i64) {
    CLOCK_OVERRIDE.store(now, std::sync::atomic::Ordering::Relaxed);
}
fn clock() -> i64 {
    match CLOCK_OVERRIDE.load(std::sync::atomic::Ordering::Relaxed) {
        0 if cfg!(test) => 100,
        0 => std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64),
        pinned => pinned,
    }
}
/// A usable observation time in Unix seconds: null, 0, negative and future
/// values mean "never observed".
pub fn observed_at(a: &Account, now: i64) -> Option<i64> {
    let then = a.usage["observed_at"].as_i64()?;
    let then = if then > 100_000_000_000 {
        then / 1000
    } else {
        then
    };
    (then > 0 && then <= now).then_some(then)
}
/// `1m ago`, `2h ago`, `3d ago`; nothing below 60 s.
pub fn compact_age(age: i64) -> Option<String> {
    match age {
        i64::MIN..=59 => None,
        60..=3599 => Some(format!("{}m ago", age / 60)),
        3600..=86399 => Some(format!("{}h ago", age / 3600)),
        _ => Some(format!("{}d ago", age / 86400)),
    }
}
impl Usage {
    pub fn account(a: &Account) -> Self {
        Self::account_at(a, clock())
    }
    pub fn account_at(a: &Account, now: i64) -> Self {
        let state = usage_state(a);
        let fresh_flag = a.details.get("usage_fresh").and_then(|v| v.as_bool());
        let stale = match state {
            Some("stale") => true,
            Some("fresh" | "partial") => fresh_flag == Some(false),
            _ => fresh_flag != Some(true),
        };
        let status =
            if let Some(deletion) = a.details.get("deletion").filter(|v| v["id"].is_string()) {
                if deletion["phase"] == "failed" {
                    "Deletion failed"
                } else {
                    "Deleting"
                }
            } else if a.details.get("membership").and_then(|v| v.as_str()) == Some("removed") {
                "Deregistered"
            } else if a.details.get("enabled").and_then(|v| v.as_bool()) == Some(false) {
                "Disabled"
            } else if a.details.get("auth_required").and_then(|v| v.as_bool()) == Some(true)
                || a.details
                    .get("auth_detail")
                    .is_some_and(|v| v == "AUTH_REQUIRED")
            {
                "Login required"
            } else if state == Some("unsupported") {
                "Usage unsupported"
            } else if matches!(state, Some("missing" | "no_windows" | "error"))
                || observed_at(a, now).is_none()
            {
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
                    .filter(|_| !matches!(state, Some("missing" | "no_windows" | "error")))
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
            age: observed_at(a, now).and_then(|t| compact_age(now - t)),
        }
    }
    pub fn status_only(&self) -> bool {
        matches!(
            self.status.as_str(),
            "Disabled" | "Login required" | "Deregistered" | "Deleting" | "Deletion failed"
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
    /// Text shown in place of the bars: lowercase, in parentheses.
    pub fn status_label(&self) -> String {
        format!("({})", self.status.to_lowercase())
    }
    /// Left edge of the status text: one column right of the bar's first cell (a
    /// half-cell cap), never centred; longer text keeps this start and grows right.
    pub fn status_x(&self, x: usize, _bar: usize) -> usize {
        x + 1
    }
    pub fn width(&self, bar: usize, digits: usize) -> usize {
        if self.status_only() {
            (UnicodeWidthStr::width(self.status_label().as_str()) + 1).max(bar)
        } else {
            bar + digits + 5 + self.age.as_ref().map_or(0, |a| a.len() + 3)
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
            let sx = self.status_x(x, bar);
            let label = self.status_label();
            canvas.accent_text(
                sx,
                y,
                &label,
                Accent {
                    bold: true,
                    flash: false,
                    ..Accent::default()
                },
            );
            canvas.accent_text(
                sx + UnicodeWidthStr::width(label.as_str()),
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
            self.paint_status(canvas, x, y, bar, 600);
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
    fn paint_status(&self, canvas: &mut Canvas, x: usize, y: usize, bar: usize, brightness: i64) {
        canvas.accent_text(
            self.status_x(x, bar),
            y,
            &self.status_label(),
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
            self.paint_status(canvas, x, y, bar, 1000);
            return;
        }
        for (row, window) in self.windows.iter().enumerate() {
            // End cells contribute half a cell each: center-to-center length.
            let length = bar.saturating_sub(1) as f64;
            let filled = window.remaining.map(|p| length * p.clamp(0., 100.) / 100.);
            let Some(filled) = filled else {
                // A missing window is stated in its own row, never drawn as 0%.
                let text = format!(
                    "{} {}",
                    window.label,
                    if self.status == "Ready" {
                        "unknown"
                    } else {
                        &self.status
                    }
                );
                canvas.accent_text(
                    x,
                    y + row,
                    &crate::widgets::ellipsize(&text, bar),
                    Accent::default(),
                );
                continue;
            };
            let marker = window
                .limit
                .map(|p| (length * p.clamp(0., 100.) / 100.).round() as usize);
            for col in 0..bar {
                let glyph = bar_glyph(col, bar, filled, marker);
                if matches!(glyph, "╾" | "┽" | "┼" | "─" | "╶" | "╴" | "├" | "┤" | "│")
                {
                    canvas.text(
                        x + col,
                        y + row,
                        glyph,
                        Cell {
                            fg: [144; 3],
                            bold: true,
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
                            brightness: 1000,
                            bold: true,
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
                self.age
                    .as_ref()
                    .map_or(String::new(), |a| format!(" ({a})"))
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

// Full-width segments, with matching horizontal weights at every cell boundary.
// Character connections: https://www.unicode.org/charts/PDF/U2500.pdf
fn bar_glyph(col: usize, width: usize, filled: f64, marker: Option<usize>) -> &'static str {
    if width <= 1 {
        return "│";
    }
    // Coordinates start at the center of the first endcap. Neighboring cells
    // must agree on stroke weight at their shared edge, including mixed joints.
    let position = col as f64;
    let left_heavy = filled > position - 0.5;
    let right_heavy = filled > position + 0.5;
    if col == 0 {
        return match (marker == Some(col), right_heavy) {
            (true, true) => "┝",
            (true, false) => "├",
            (false, true) => "╺",
            (false, false) => "╶",
        };
    }
    if col == width - 1 {
        return match (marker == Some(col), left_heavy) {
            (true, true) => "┥",
            (true, false) => "┤",
            (false, true) => "╸",
            (false, false) => "╴",
        };
    }
    match (marker == Some(col), left_heavy, right_heavy) {
        (true, _, true) => "┿",
        (true, true, false) => "┽",
        (true, false, false) => "┼",
        (false, _, true) => "━",
        (false, true, false) => "╾",
        (false, false, false) => "─",
    }
}

/// Provider symbol drawn before every account name. One place to change them; each
/// is a single terminal cell wide (checked by a unit test).
pub fn provider_symbol(provider: &str) -> &'static str {
    match provider {
        "claude" => "✻",
        "gemini" => "✦",
        _ => "⬢",
    }
}
/// Smallest bar the home list shrinks to on narrow screens.
const MIN_BAR: usize = 6;
const MAX_BAR: usize = 24;
/// One layout calculation supplies the coordinates and the scroll window. Columns
/// from left: selection lane (1) + space, provider symbol (1) + space, names padded
/// to one width, space, bars (identical length for every account), values.
#[derive(Clone, Debug)]
pub struct AccountsLayout {
    pub left: usize,
    pub symbol_x: usize,
    pub name_x: usize,
    pub name_width: usize,
    pub bars: usize,
    pub bar_width: usize,
    pub digits: usize,
    pub height: usize,
}
impl AccountsLayout {
    pub fn fit(accounts: &[Account], width: usize) -> Self {
        let uses: Vec<_> = accounts.iter().map(Usage::account).collect();
        let height = uses
            .iter()
            .map(|u| u.height() + 1)
            .sum::<usize>()
            .saturating_sub(1);
        let mut names = accounts
            .iter()
            .map(|a| UnicodeWidthStr::width(account_title(a).as_str()))
            .max()
            .unwrap_or(0);
        let digits = uses.iter().map(Usage::digits).max().unwrap_or(2);
        // Lane + symbol columns, then names and one space before the bars.
        let right = |bar: usize| uses.iter().map(|u| u.width(bar, digits)).max().unwrap_or(0) + 2;
        let fixed = |names: usize| 4 + names + 1;
        let mut bar = MAX_BAR;
        while bar > MIN_BAR && fixed(names) + right(bar) + 2 > width {
            bar -= 1;
        }
        // Still too wide at the smallest bar: names give way last.
        while names > 3 && fixed(names) + right(bar) + 2 > width {
            names -= 1;
        }
        let total = fixed(names) + right(bar);
        let left = width.saturating_sub(total) / 2;
        Self {
            left,
            symbol_x: left + 2,
            name_x: left + 4,
            name_width: names,
            bars: left + fixed(names),
            bar_width: bar,
            digits,
            height,
        }
    }
    /// First account to draw so `selected` is visible in `rows` terminal rows.
    pub fn scroll_for(
        accounts: &[Account],
        first: usize,
        selected: Option<usize>,
        rows: usize,
    ) -> usize {
        let heights: Vec<usize> = accounts
            .iter()
            .map(|a| Usage::account(a).height() + 1)
            .collect();
        let mut first = first.min(accounts.len().saturating_sub(1));
        let Some(sel) = selected.filter(|s| *s < accounts.len()) else {
            return first;
        };
        first = first.min(sel);
        // Rows needed to show first..=sel (the last account has no trailing blank).
        while first < sel && heights[first..=sel].iter().sum::<usize>() - 1 > rows {
            first += 1;
        }
        first
    }
    #[allow(clippy::too_many_arguments)]
    pub fn paint(
        &self,
        canvas: &mut Canvas,
        accounts: &[Account],
        top: usize,
        bottom: usize,
        first: usize,
        selected: Option<usize>,
        back: bool,
        usage_focus: bool,
        moving: Option<usize>,
    ) {
        let depressed = back || selected.is_none_or(|i| i >= accounts.len());
        let name_focus = !depressed && !usage_focus;
        let mut y = top;
        for (index, account) in accounts.iter().enumerate().skip(first) {
            let usage = Usage::account(account);
            if y + usage.height() > bottom {
                break;
            }
            let active = selected == Some(index) && !back;
            if active && !usage_focus {
                canvas.accent_text(self.left, y, "▶", Accent::default());
            }
            let name_style = Accent {
                brightness: if depressed || name_focus && !active {
                    600
                } else {
                    1000
                },
                bold: active && !usage_focus,
                ..Accent::default()
            };
            let moving_row = moving == Some(index);
            canvas.accent_text(
                self.symbol_x,
                y,
                if moving_row {
                    "⇅"
                } else {
                    provider_symbol(account.provider())
                },
                name_style,
            );
            canvas.accent_text(
                self.name_x,
                y,
                &crate::widgets::ellipsize(&account_title(account), self.name_width),
                name_style,
            );
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
            if moving_row {
                canvas.accent_text(
                    self.bars + usage.width(self.bar_width, self.digits) + 1,
                    y,
                    "(moving)",
                    Accent::default(),
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
        let layout = AccountsLayout::fit(&accounts, 100);
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
                layout.paint(
                    &mut c,
                    &accounts,
                    3,
                    40,
                    0,
                    selected,
                    back,
                    usage_focus,
                    None,
                );
                let mut y = 3;
                for (i, a) in accounts.iter().enumerate() {
                    let active = selected == Some(i) && !back;
                    let dim_name = back || selected.is_none() || !usage_focus && !active;
                    let mut expected = base.clone();
                    expected.accent_text(
                        layout.name_x,
                        y,
                        &short_name(&a.id),
                        Accent {
                            brightness: if dim_name { 600 } else { 1000 },
                            bold: active && !usage_focus,
                            ..Accent::default()
                        },
                    );
                    let cell = y * 100 + layout.name_x;
                    assert_eq!(c.cells[cell], expected.cells[cell]);
                    let usage = Usage::account(a);
                    if usage.status_only() {
                        let brightness = if back || selected.is_none() || usage_focus && !active {
                            600
                        } else {
                            1000
                        };
                        usage.paint_status(
                            &mut expected,
                            layout.bars,
                            y,
                            layout.bar_width,
                            brightness,
                        );
                        let sx = usage.status_x(layout.bars, layout.bar_width);
                        for x in sx..sx + UnicodeWidthStr::width(usage.status_label().as_str()) {
                            expected.cells[y * 100 + x].bold = active && usage_focus;
                            assert_eq!(c.cells[y * 100 + x], expected.cells[y * 100 + x]);
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
            age: None,
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
            assert!(before.bold);
            assert!(after.bold);
            assert!(dim.cells[40 + x].bold);
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
                age: None,
            };
            let mut c = field.paint(4200);
            let mut expected = c.clone();
            u.paint_focused(&mut c, 2, 2, 20, 2);
            let sx = u.status_x(2, 20);
            let end = sx + UnicodeWidthStr::width(u.status_label().as_str());
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
            assert!(c.cells[120 + sx].bold);
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
        let sx = u.status_x(1, 20);
        expected.accent_text(
            sx,
            1,
            "(disabled)",
            Accent {
                bold: false,
                flash: false,
                ..Accent::default()
            },
        );
        assert_eq!(c.cells[60 + sx].fg, expected.cells[60 + sx].fg);
        assert!(!c.cells[60 + sx].bold);
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
            "╺" | "┝" => (0, 2),
            "╶" | "├" => (0, 1),
            "╸" | "┥" => (2, 0),
            "╴" | "┤" => (1, 0),
            "━" | "┿" => (2, 2),
            "╾" | "┽" => (2, 1),
            "─" | "┼" => (1, 1),
            _ => panic!("unexpected bar segment"),
        };
        for width in [2, 3, 7, 24] {
            for step in 0..=1000 {
                let filled = (width - 1) as f64 * step as f64 / 1000.;
                for marker in std::iter::once(None).chain((0..width).map(Some)) {
                    assert_eq!(weights(bar_glyph(0, width, filled, marker)).0, 0);
                    assert_eq!(weights(bar_glyph(width - 1, width, filled, marker)).1, 0);
                    if marker == Some(0) {
                        assert!(matches!(bar_glyph(0, width, filled, marker), "├" | "┝"));
                    }
                    if marker == Some(width - 1) {
                        assert!(matches!(
                            bar_glyph(width - 1, width, filled, marker),
                            "┤" | "┥"
                        ));
                    }
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
            assert!(text.contains(&format!("({})", expected.to_lowercase())));
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
        assert!(text.contains("(usage unavailable)"));
        assert!(!text.contains("Ready"));
    }
    #[test]
    fn half_endcaps_share_threshold_color_and_align_percentage_labels() {
        let mut a = account();
        a.usage["short"]["used_percent"] = json!(74.);
        a.details.remove("switch_thresholds");
        let u = Usage::account(&a);
        let mut c = Canvas::new(50, 5);
        u.paint(&mut c, 1, 1, 10, 2);
        assert_eq!(c.cells[51].glyph, "╺");
        assert_eq!(c.cells[52].glyph, "━");
        assert_eq!(c.cells[53].glyph, "╾");
        assert_eq!(c.cells[54].glyph, "─");
        assert_eq!(c.cells[54].fg, [144; 3]);
        assert_eq!(c.cells[55].glyph, "─");
        assert_eq!(c.cells[55].fg, [144; 3]);
        assert_eq!(c.cells[60].glyph, "╴");
        assert_eq!(c.cells[61].glyph, " ");
        assert_eq!(c.cells[62].glyph, "2");
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
        assert_eq!(c.cells[60 + 5].glyph, "┿");
        assert_eq!(c.cells[120 + 4].glyph, "┿");
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
    fn provider_symbols_are_one_cell_wide_and_distinct() {
        let all = ["codex", "claude", "gemini"].map(provider_symbol);
        assert_eq!(all, ["⬢", "✻", "✦"]);
        for symbol in all {
            assert_eq!(UnicodeWidthStr::width(symbol), 1, "{symbol}");
        }
        assert_eq!(provider_symbol("unknown"), "⬢");
    }
    #[test]
    fn mixed_providers_share_symbol_name_and_bar_columns_at_every_width() {
        let mk = |v: serde_json::Value| -> Account { serde_json::from_value(v).unwrap() };
        let both = json!({"state":"fresh","observed_at":100,"short":{"duration_minutes":300,"used_percent":35},"weekly":{"duration_minutes":10080,"used_percent":60}});
        let wk = json!({"state":"fresh","observed_at":100,"short":null,"weekly":{"duration_minutes":10080,"used_percent":8}});
        let accounts = vec![
            mk(
                json!({"id":"claude:z","provider":"claude","selectable":true,"enabled":true,"usage_fresh":true,"usage":both}),
            ),
            mk(
                json!({"id":"codex:a-rather-long-name","selectable":true,"enabled":true,"usage_fresh":true,"usage":wk}),
            ),
            mk(
                json!({"id":"gemini:g","provider":"gemini","selectable":true,"enabled":true,"usage_fresh":false,"usage_state":"unsupported","usage":{"state":"unsupported","observed_at":null}}),
            ),
            mk(
                json!({"id":"codex:mid","selectable":true,"enabled":true,"usage_fresh":true,"usage":both}),
            ),
        ];
        for width in [100, 60, 40, 30] {
            let l = AccountsLayout::fit(&accounts, width);
            let mut c = Canvas::new(width, 20);
            l.paint(&mut c, &accounts, 1, 20, 0, None, false, false, None);
            let cell = |x: usize, y: usize| c.cells[y * width + x].glyph.clone();
            let mut y = 1;
            for a in &accounts {
                let u = Usage::account(a);
                assert_eq!(cell(l.symbol_x, y), provider_symbol(a.provider()));
                assert_eq!(
                    cell(l.name_x, y),
                    account_title(a).chars().next().unwrap().to_string()
                );
                assert!(
                    l.name_x + l.name_width < l.bars,
                    "names end before the bars"
                );
                if !u.status_only() {
                    for row in y..y + u.height() {
                        // Every bar is exactly bar_width cells of bar glyphs ...
                        for x in l.bars..l.bars + l.bar_width {
                            assert!(
                                cell(x, row)
                                    .chars()
                                    .all(|g| ('\u{2500}'..='\u{257f}').contains(&g)),
                                "{width} {a:?} {x},{row}: {:?}",
                                cell(x, row)
                            );
                        }
                        // ... followed by one space and a right-aligned value column.
                        assert_eq!(cell(l.bars + l.bar_width, row), " ");
                        assert_eq!(cell(l.bars + l.bar_width + 1 + l.digits, row), "%");
                    }
                } else {
                    // Status text starts one column right of the bar's first cell.
                    let sx = u.status_x(l.bars, l.bar_width);
                    assert_eq!(cell(sx, y), "(");
                    assert_eq!(sx, l.bars + 1);
                }
                y += u.height() + 1;
            }
        }
    }
    #[test]
    fn layout_always_fits_by_shrinking_bars_and_scrolls_instead_of_dropping_to_a_menu() {
        let accounts = vec![account(), account()];
        let l = AccountsLayout::fit(&accounts, 80);
        assert_eq!(l.height, 5);
        assert_eq!(l.digits, 3);
        assert_eq!(l.bar_width, 24);
        let wide = AccountsLayout::fit(&accounts, 140);
        assert_eq!(wide.bar_width, l.bar_width);
        assert_eq!(wide.left, l.left + 30);
        let narrow = AccountsLayout::fit(&accounts, 40);
        assert!(narrow.bar_width < wide.bar_width && narrow.bar_width >= MIN_BAR);
        assert!(narrow.left >= 1);
        assert!(narrow.bars + narrow.bar_width + 6 <= 40, "{narrow:?}");
        let tiny = AccountsLayout::fit(&accounts, 24);
        assert_eq!(tiny.bar_width, MIN_BAR);
        // Scrolling keeps the selected account inside the window.
        let many: Vec<_> = (0..10).map(|_| account()).collect();
        assert_eq!(AccountsLayout::scroll_for(&many, 0, Some(0), 8), 0);
        let first = AccountsLayout::scroll_for(&many, 0, Some(9), 8);
        assert!(first > 0);
        let rows: usize = many[first..=9]
            .iter()
            .map(|a| Usage::account(a).height() + 1)
            .sum();
        assert!(rows - 1 <= 8);
        assert_eq!(
            AccountsLayout::scroll_for(&many, first, Some(first + 1), 8),
            first
        );
        assert_eq!(AccountsLayout::scroll_for(&many, first, Some(0), 8), 0);
    }
}

/// Shared right-edge picker columns. Padding is computed from displayed values,
/// not a fixed three-digit reservation; absent five-hour windows have no slash.
pub fn picker_labels<A: std::borrow::Borrow<Account>>(accounts: &[A]) -> Vec<String> {
    let values: Vec<_> = accounts
        .iter()
        .map(|a| {
            let a: &Account = a.borrow();
            let unavailable = matches!(usage_state(a), Some("missing" | "no_windows" | "error"));
            let remaining = |key: &str, duration: i64| {
                (a.usage[key]["duration_minutes"].as_i64() == Some(duration))
                    .then(|| a.usage[key]["used_percent"].as_f64())
                    .flatten()
                    .filter(|_| !unavailable)
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
    fn claude_account(usage: serde_json::Value) -> Account {
        serde_json::from_value(json!({"id":"claude:claude-main","provider":"claude","selectable":true,"usage_fresh":true,"enabled":true,"usage":usage})).unwrap()
    }
    #[test]
    fn unsupported_usage_is_a_status_never_zero_percent() {
        let a: Account = serde_json::from_value(json!({"id":"gemini:g","provider":"gemini","selectable":true,"enabled":true,"usage_fresh":false,"usage_state":"unsupported","usage":{"state":"unsupported","observed_at":null,"short":null,"weekly":null}})).unwrap();
        let u = Usage::account(&a);
        assert_eq!(u.status, "Usage unsupported");
        assert!(u.status_only());
        assert_eq!(u.height(), 1);
        assert_eq!(a.provider(), "gemini");
    }
    #[test]
    fn missing_error_and_empty_claude_usage_is_unavailable_never_zero_percent() {
        for state in ["missing", "no_windows", "error"] {
            // Even leftover percentages are not shown when the backend says unknown.
            let a = claude_account(json!({"state":state,"observed_at":100,
                "short":{"duration_minutes":300,"used_percent":0},"weekly":null}));
            let u = Usage::account(&a);
            assert_eq!(u.status, "Usage unavailable", "{state}");
            assert!(u.status_only());
            assert!(u.windows.iter().all(|w| w.remaining.is_none()));
            assert_eq!(picker_labels(&[a]), ["?% 5h / ?% wk"]);
        }
        let none = claude_account(
            json!({"state":"missing","observed_at":null,"short":null,"weekly":null}),
        );
        assert_eq!(Usage::account(&none).status, "Usage unavailable");
    }
    #[test]
    fn status_text_is_lowercase_parenthesised_and_starts_one_column_into_the_bar() {
        let mut a = account(Some(80.), 80.);
        a.details.insert("enabled".into(), json!(false));
        let u = Usage::account(&a);
        assert_eq!(u.status_label(), "(disabled)");
        // Always bar_x + 1, whatever the span; never centred.
        assert_eq!(u.status_x(5, 24), 6);
        assert_eq!(u.status_x(0, 13), 1);
        // Wider than the span: same start, growing right.
        assert_eq!(u.status_x(5, 8), 6);
        assert_eq!(u.width(8, 2), 11);
        assert_eq!(u.width(24, 2), 24);
        let mut c = Canvas::new(60, 2);
        u.paint(&mut c, 5, 0, 24, 2);
        let row: String = (0..60).map(|x| c.cells[x].glyph.as_str()).collect();
        assert_eq!(row.find("(disabled)"), Some(6));
        // Selector: exactly one space, then the triangle, right of the text.
        let mut c = Canvas::new(60, 2);
        u.paint_focused(&mut c, 5, 0, 24, 2);
        let row: String = (0..60).map(|x| c.cells[x].glyph.as_str()).collect();
        assert!(row.contains("(disabled) ◀"), "{row:?}");
        assert!(!row.contains("(disabled)  ◀"));
        // Overflow: the selector follows the grown text.
        let mut c = Canvas::new(60, 2);
        u.paint_focused(&mut c, 5, 0, 8, 2);
        let row: String = (0..60).map(|x| c.cells[x].glyph.as_str()).collect();
        assert_eq!(row.find("(disabled) ◀"), Some(6));
    }
    #[test]
    fn bars_show_compact_age_from_sixty_seconds_and_never_dim() {
        let a = claude_account(json!({"state":"stale","observed_at":100,
            "short":{"duration_minutes":300,"used_percent":40},"weekly":null}));
        for (now, age) in [
            (159, None),
            (160, Some("1m ago")),
            (100 + 42 * 60, Some("42m ago")),
            (100 + 2 * 3600, Some("2h ago")),
            (100 + 3 * 86400, Some("3d ago")),
        ] {
            let u = Usage::account_at(&a, now);
            assert_eq!(u.age.as_deref(), age);
            let mut c = Canvas::new(60, 3);
            u.paint(&mut c, 0, 0, 12, 2);
            let row: String = (0..60).map(|x| c.cells[x].glyph.as_str()).collect();
            assert!(!row.contains("stale"));
            assert_eq!(row.contains("ago"), age.is_some());
            if let Some(age) = age {
                assert!(row.contains(&format!("({age})")));
            }
        }
    }
    #[test]
    fn zero_null_negative_and_future_observations_have_no_age() {
        for bogus in [json!(0), json!(null), json!(-1), json!(10_000)] {
            let a = claude_account(json!({"state":"fresh","observed_at":bogus,
                "short":{"duration_minutes":300,"used_percent":40},"weekly":null}));
            assert_eq!(Usage::account_at(&a, 5000).age, None);
            assert_eq!(observed_at(&a, 5000), None);
        }
    }
    #[test]
    fn stale_and_partial_claude_usage_keep_known_windows_and_mark_the_rest() {
        let stale = claude_account(json!({"state":"stale","observed_at":100,
            "short":{"duration_minutes":300,"used_percent":25},"weekly":{"duration_minutes":10080,"used_percent":10}}));
        let u = Usage::account(&stale);
        assert!(u.stale);
        assert_eq!(u.status, "Usage stale");
        assert_eq!(u.windows[0].remaining, Some(75.));
        let partial = claude_account(json!({"state":"partial","observed_at":100,
            "short":{"duration_minutes":300,"used_percent":25},"weekly":null}));
        let u = Usage::account(&partial);
        assert!(!u.stale);
        assert_eq!(u.status, "Ready");
        assert_eq!(u.windows[0].remaining, Some(75.));
        assert_eq!(
            u.windows[1].remaining, None,
            "a missing window is not 100% or 0%"
        );
        assert!(!u.status_only());
        assert_eq!(picker_labels(&[partial]), ["75% 5h /  ?% wk"]);
    }
    #[test]
    fn claude_auth_detail_and_null_codex_windows_are_not_ready_or_zero() {
        let mut a =
            claude_account(json!({"state":"fresh","observed_at":100,"short":null,"weekly":null}));
        a.details
            .insert("auth_detail".into(), json!("AUTH_REQUIRED"));
        assert_eq!(Usage::account(&a).status, "Login required");
        let codex: Account = serde_json::from_value(json!({"id":"codex:x","selectable":true,"usage_fresh":true,
            "usage":{"observed_at":1,"short":null,"weekly":{"duration_minutes":10080,"used_percent":20}}})).unwrap();
        assert_eq!(Usage::account(&codex).windows.len(), 1);
        assert_eq!(picker_labels(&[codex]), ["80% wk"]);
        assert_eq!(a.provider(), "claude");
    }
    #[test]
    fn missing_window_row_is_painted_in_its_own_row_with_a_word_not_a_bar() {
        let a = claude_account(json!({"state":"partial","observed_at":100,"short":null,
            "weekly":{"duration_minutes":10080,"used_percent":50}}));
        // Short absent: only the weekly row exists and it is a real bar.
        assert_eq!(Usage::account(&a).windows.len(), 1);
        let b = claude_account(json!({"state":"partial","observed_at":100,
            "short":{"duration_minutes":300,"used_percent":50},"weekly":null}));
        let mut canvas = Canvas::new(40, 4);
        Usage::account(&b).paint(&mut canvas, 0, 0, 12, 2);
        let row = |y: usize| -> String {
            (0..40)
                .map(|x| canvas.cells[y * 40 + x].glyph.as_str())
                .collect()
        };
        assert!(row(0).contains("5h"));
        assert!(row(1).starts_with("wk unknown"));
    }
}
