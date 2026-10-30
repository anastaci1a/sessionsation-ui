//! Shared, read-only account presentation. Missing observations are never limits.
use crate::{
    client::Account,
    render::{Accent, Canvas, Cell},
};
use unicode_width::UnicodeWidthStr;

/// Cells an account name gets in lists and tags; longer names are shortened.
pub const ACCOUNT_TITLE_CELLS: usize = 12;
pub fn short_name(id: &str) -> String {
    clipped_name(full_short_name(id), ACCOUNT_TITLE_CELLS)
}
fn full_short_name(id: &str) -> &str {
    id.split_once(':').map_or(id, |(_, n)| n)
}
/// Account name for lists and tags: the backend display name when set, else the
/// short name derived from the ID, shortened to [`ACCOUNT_TITLE_CELLS`].
pub fn account_title(a: &Account) -> String {
    crate::widgets::ellipsize(&account_title_full(a), ACCOUNT_TITLE_CELLS)
}
/// [`account_title`] before shortening (control characters removed).
pub fn account_title_full(a: &Account) -> String {
    a.details
        .get("display_name")
        .and_then(|v| v.as_str())
        .map(|n| n.chars().filter(|c| !c.is_control()).collect::<String>())
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| {
            full_short_name(&a.id)
                .chars()
                .filter(|c| !c.is_control())
                .collect()
        })
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
    /// Draw the age after the first value (false: the layout puts it under the name).
    pub age_inline: bool,
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
/// Wall clock in Unix seconds, honouring `set_clock` (tests and demos).
pub fn clock() -> i64 {
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
            age_inline: true,
        }
    }
    /// Account-level states that replace the bars with their parenthetical: the
    /// account needs an action first (login, enable, restore, deletion recovery).
    pub fn action_required(&self) -> bool {
        matches!(
            self.status.as_str(),
            "Disabled" | "Login required" | "Deregistered" | "Deletion failed"
        )
    }
    /// Usage itself is unavailable (unsupported, unknown or not yet reported) for
    /// an account that needs no action: one stub bar stands in for the bars.
    pub fn stub(&self) -> bool {
        !self.action_required() && self.windows.iter().all(|w| w.remaining.is_none())
    }
    /// One row without values or age: an action-required parenthetical or a stub bar.
    pub fn status_only(&self) -> bool {
        self.action_required() || self.stub()
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
    /// Parenthetical shown in place of the bars: the lowercase action-required
    /// status, otherwise [`UNAVAILABLE`] (revealed while a stub is hovered).
    pub fn status_label(&self) -> String {
        if self.action_required() {
            format!("({})", self.status.to_lowercase())
        } else {
            UNAVAILABLE.into()
        }
    }
    /// Left edge of the status text: one column right of the bar's first cell (a
    /// half-cell cap), never centred; longer text keeps this start and grows right.
    /// Age suffix colour: the dimmest secondary grey, one level up while the row
    /// is selected (the plain unselected menu-item grey); never bold.
    pub fn age_color(selected: bool) -> [u8; 3] {
        if selected {
            crate::widgets::SECONDARY
        } else {
            crate::widgets::MUTED
        }
    }
    /// Recolour an already painted age suffix for a selected account row.
    pub fn recolor_age(
        &self,
        canvas: &mut Canvas,
        x: usize,
        y: usize,
        bar: usize,
        digits: usize,
        selected: bool,
    ) {
        let Some(age) = &self.age else { return };
        if self.status_only() {
            return;
        }
        for (row, window) in self.windows.iter().enumerate() {
            if row > 0 || !self.age_inline {
                break;
            }
            let main =
                UnicodeWidthStr::width(format!("{:>digits$}% {}", "0", window.label).as_str());
            let start = x + bar + 1 + main;
            for col in start..start + age.len() + 3 {
                if y + row < canvas.height && col < canvas.width {
                    let cell = &mut canvas.cells[(y + row) * canvas.width + col];
                    cell.fg = Self::age_color(selected);
                    cell.bold = false;
                }
            }
        }
    }
    pub fn status_x(&self, x: usize, _bar: usize) -> usize {
        x + 1
    }
    pub fn width(&self, bar: usize, digits: usize) -> usize {
        let label = UnicodeWidthStr::width(self.status_label().as_str()) + 1;
        if self.status_only() {
            label.max(bar)
        } else if self.windows.iter().any(|w| w.remaining.is_none()) {
            // A missing window's stub reveals the parenthetical while hovered.
            self.values_width(bar, digits).max(label)
        } else {
            self.values_width(bar, digits)
        }
    }
    /// Bars and their percentage column, without anything a hover may reveal.
    pub fn values_width(&self, bar: usize, digits: usize) -> usize {
        bar + digits + 5
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
        // Hover reveal: each stub gives way to the parenthetical, from bar_x + 1.
        for (row, window) in self.windows.iter().enumerate() {
            if window.remaining.is_none() && !self.status_only() {
                self.reveal(canvas, x, y + row, bar, digits);
            }
        }
        if self.status_only() {
            let sx = self.status_x(x, bar);
            let label = self.status_label();
            if self.stub() {
                self.reveal(canvas, x, y, bar, digits);
            } else {
                self.paint_status(canvas, x, y, bar, true);
            }
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
            let age = self
                .age
                .as_ref()
                .filter(|_| row == 0 && self.age_inline)
                .map_or(0, |a| a.len() + 3);
            canvas.text(
                x + self.values_width(bar, digits) + age,
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
            self.paint_selected(canvas, x, y, bar, digits, false);
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
    /// A parenthetical status takes the usage bars' hue level; bold only while
    /// the cursor is on it.
    /// Hide the stub (and a missing window's unknown value) on row `y` and show
    /// [`Usage::status_label`] in its place, bold with the cursor on it.
    fn reveal(&self, canvas: &mut Canvas, x: usize, y: usize, bar: usize, digits: usize) {
        let span = self.width(bar, digits).max(bar);
        canvas.text(x, y, &" ".repeat(span), Cell::default(), false);
        self.paint_status(canvas, x, y, bar, true);
    }
    /// The stub bar standing in for an unavailable usage bar: the bar's unfilled
    /// grey, never hue-shifted or bold, with no marker.
    fn paint_stub(canvas: &mut Canvas, x: usize, y: usize, bar: usize) {
        canvas.text(
            x,
            y,
            &crate::stub_bar::stub_bar(bar),
            Cell {
                fg: UNFILLED,
                bold: false,
                ..Cell::default()
            },
            false,
        );
    }
    fn paint_status(&self, canvas: &mut Canvas, x: usize, y: usize, bar: usize, bold: bool) {
        canvas.accent_text(
            self.status_x(x, bar),
            y,
            &self.status_label(),
            Accent {
                brightness: BAR_LEVEL,
                bold,
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
        if self.action_required() {
            self.paint_status(canvas, x, y, bar, false);
            return;
        }
        if self.stub() {
            Self::paint_stub(canvas, x, y, bar);
            return;
        }
        for (row, window) in self.windows.iter().enumerate() {
            // End cells contribute half a cell each: center-to-center length.
            let length = bar.saturating_sub(1) as f64;
            let filled = window.remaining.map(|p| length * p.clamp(0., 100.) / 100.);
            let Some(filled) = filled else {
                // A missing window keeps its row and column: a stub bar and `?`,
                // never drawn as 0%.
                let muted = Cell {
                    fg: crate::widgets::MUTED,
                    ..Cell::default()
                };
                Self::paint_stub(canvas, x, y + row, bar);
                canvas.text(
                    x + bar + 1,
                    y + row,
                    &format!("{:>digits$}% {}", "?", window.label),
                    muted,
                    false,
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
                            fg: UNFILLED,
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
                            brightness: BAR_LEVEL,
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
            let label = format!("{:>digits$}% {}", percent, window.label);
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
            if let Some(age) = self.age.as_ref().filter(|_| row == 0 && self.age_inline) {
                canvas.text(
                    x + bar + 1 + UnicodeWidthStr::width(label.as_str()),
                    y + row,
                    &format!(" ({age})"),
                    Cell {
                        fg: Self::age_color(selected),
                        ..Cell::default()
                    },
                    false,
                );
            }
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
        "antigravity" => "◬",
        _ => "⏣",
    }
}
/// Hue level of the usage bars, shared by parenthetical statuses.
const BAR_LEVEL: i64 = 1000;
/// Grey of a bar's unfilled part, also the whole stub bar.
pub const UNFILLED: [u8; 3] = [144; 3];
/// Parenthetical a hovered stub bar reveals.
pub const UNAVAILABLE: &str = "(usage unavailable)";
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
    /// Observation ages go under the names instead of after the values.
    pub age_below: bool,
}
impl AccountsLayout {
    /// Columns are centred without the observation age, which hangs to the right.
    /// If any account's age cannot fit inline, every account shows it on its
    /// secondary line, under the name.
    pub fn fit(accounts: &[Account], width: usize) -> Self {
        let uses: Vec<_> = accounts.iter().map(Usage::account).collect();
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
        let bars = left + fixed(names);
        let age_below = uses.iter().any(|u| {
            !u.status_only()
                && u.age.as_ref().is_some_and(|age| {
                    bars + u.values_width(bar, digits) + 3 + age.len() + 1 > width
                })
        });
        let mut layout = Self {
            left,
            symbol_x: left + 2,
            name_x: left + 4,
            name_width: names,
            bars,
            bar_width: bar,
            digits,
            height: 0,
            age_below,
        };
        layout.height = uses
            .iter()
            .map(|u| layout.row_height(u) + 1)
            .sum::<usize>()
            .saturating_sub(1);
        layout
    }
    /// Terminal rows one account occupies (without the blank after it).
    pub fn row_height(&self, usage: &Usage) -> usize {
        if self.age_below && usage.age.is_some() && !usage.status_only() {
            usage.height().max(2)
        } else {
            usage.height()
        }
    }
    /// Terminal rows each account takes, with the blank after it.
    fn heights(&self, accounts: &[Account]) -> Vec<usize> {
        accounts
            .iter()
            .map(|a| self.row_height(&Usage::account(a)) + 1)
            .collect()
    }
    /// `…` rows need room for one on each side of the tallest account; shorter
    /// areas (such as a single row) scroll plainly without them.
    fn ellipses(heights: &[usize], rows: usize) -> bool {
        heights
            .iter()
            .max()
            .is_some_and(|h| h + 2 * ELLIPSIS_ROWS <= rows + 1)
    }
    /// What `rows` terminal rows show from account `first` on: a `…` row stands
    /// for the accounts above it, and for those below when they do not all fit.
    pub fn window(&self, accounts: &[Account], first: usize, rows: usize) -> ListWindow {
        Self::window_of(&self.heights(accounts), first, rows)
    }
    fn window_of(heights: &[usize], first: usize, rows: usize) -> ListWindow {
        let count = heights.len();
        let first = first.min(count.saturating_sub(1));
        let ellipses = Self::ellipses(heights, rows);
        // The blank after the last row may fall on the row below the area.
        let room = rows + 1;
        let mut used = usize::from(ellipses && first > 0) * ELLIPSIS_ROWS;
        let rest: usize = heights[first..].iter().sum();
        let reserve = if ellipses && used + rest > room {
            ELLIPSIS_ROWS
        } else {
            0
        };
        let mut end = first;
        while end < count && used + heights[end] + reserve <= room {
            used += heights[end];
            end += 1;
        }
        ListWindow {
            first,
            end,
            above: ellipses && first > 0,
            below: ellipses && end < count,
        }
    }
    /// First account to draw so `selected` is visible in `rows` terminal rows,
    /// moving the scroll as little as possible. A `…` row hides two or more
    /// accounts; a single hidden account is shown instead. From [`MARGIN_ROWS`]
    /// visible rows, the cursor keeps one account between itself and a `…` row,
    /// so the list scrolls one account before reaching it.
    pub fn scroll(
        &self,
        accounts: &[Account],
        first: usize,
        selected: Option<usize>,
        rows: usize,
    ) -> usize {
        let heights = self.heights(accounts);
        let count = accounts.len();
        let first = first.min(count.saturating_sub(1));
        let selected = selected.filter(|s| *s < count);
        if !Self::ellipses(&heights, rows) {
            let Some(sel) = selected else {
                return first;
            };
            let mut first = first.min(sel);
            // Rows needed to show first..=sel (the last account has no trailing blank).
            while first < sel && heights[first..=sel].iter().sum::<usize>() - 1 > rows {
                first += 1;
            }
            return first;
        }
        let windows: Vec<ListWindow> = (0..count)
            .map(|first| Self::window_of(&heights, first, rows))
            .collect();
        // Strictest first; the relaxed passes only serve accounts taller than a `…` row.
        for (margin, single) in [(true, false), (true, true), (false, false), (false, true)] {
            let valid: Vec<&ListWindow> = windows
                .iter()
                .filter(|w| single || w.first != 1 && !(w.below && w.end + 1 == count))
                .filter(|w| {
                    selected.is_none_or(|s| {
                        (w.first..w.end).contains(&s)
                            && !(margin
                                && w.rows() >= MARGIN_ROWS
                                && (w.above && s == w.first || w.below && s + 1 == w.end))
                    })
                })
                .collect();
            let Some(best) = valid
                .iter()
                .min_by_key(|w| (w.first.abs_diff(first), w.first))
            else {
                continue;
            };
            // A window reaching the last account starts as early as it can.
            return valid
                .iter()
                .filter(|w| best.end == count && w.end == count)
                .map(|w| w.first)
                .min()
                .unwrap_or(best.first);
        }
        first
    }
    /// Scroll for a layout whose ages fit inline.
    pub fn scroll_for(
        accounts: &[Account],
        first: usize,
        selected: Option<usize>,
        rows: usize,
    ) -> usize {
        Self::fit(accounts, usize::MAX / 4).scroll(accounts, first, selected, rows)
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
        let cursor_off = back || selected.is_none_or(|i| i >= accounts.len());
        let name_focus = !cursor_off && !usage_focus;
        let window = self.window(accounts, first, bottom.saturating_sub(top));
        // `…` rows take the names' hue at their position, at the level of a name
        // without the cursor (never dimmed by usage focus); they are not selectable.
        let ellipsis = Accent {
            brightness: if cursor_off || moving.is_some() || name_focus {
                600
            } else {
                1000
            },
            ..Accent::default()
        };
        let mut y = top;
        if window.above {
            canvas.accent_text(self.name_x, y, "…", ellipsis);
            y += ELLIPSIS_ROWS;
        }
        for (index, account) in accounts
            .iter()
            .enumerate()
            .take(window.end)
            .skip(window.first)
        {
            let mut usage = Usage::account(account);
            usage.age_inline = !self.age_below;
            let height = self.row_height(&usage);
            if y + height > bottom {
                break;
            }
            let active = selected == Some(index) && !back;
            let moving_row = moving == Some(index);
            // While an account is moved, every other account looks exactly as it
            // does with the cursor on the menu options above the accounts.
            let depressed = cursor_off || moving.is_some() && !moving_row;
            let before = moving_row.then(|| canvas.cells.clone());
            if active && !usage_focus {
                canvas.accent_text(self.left, y, "▷", Accent::default());
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
            // The gap after the icon and the name's padding are part of the label:
            // they cover stars like its letters do.
            canvas.accent_text(self.name_x.saturating_sub(1), y, " ", name_style);
            canvas.accent_text(
                self.name_x,
                y,
                &pad(
                    &crate::widgets::fit_selectable(
                        &account_title_full(account),
                        self.name_width,
                        crate::widgets::frame_time(canvas),
                        active,
                    ),
                    self.name_width,
                ),
                name_style,
            );
            if active && usage_focus {
                usage.paint_focused(canvas, self.bars, y, self.bar_width, self.digits);
            } else if depressed || usage_focus || usage.status_only() {
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
            // The age brightens only while the cursor is on the usage itself.
            if active && usage_focus {
                usage.recolor_age(canvas, self.bars, y, self.bar_width, self.digits, true);
            }
            if self.age_below
                && !usage.status_only()
                && let Some(age) = &usage.age
            {
                canvas.text(
                    self.name_x,
                    y + 1,
                    &pad(
                        &crate::widgets::fit_scrolling(
                            &format!("({age})"),
                            self.name_width,
                            crate::widgets::frame_time(canvas),
                        ),
                        self.name_width,
                    ),
                    Cell {
                        fg: Usage::age_color(active && usage_focus),
                        ..Cell::default()
                    },
                    false,
                );
            }
            if moving_row {
                canvas.accent_text(
                    self.bars + usage.width(self.bar_width, self.digits) + 1,
                    y,
                    "(moving)",
                    Accent {
                        brightness: BAR_LEVEL,
                        bold: active && usage_focus,
                        ..Accent::default()
                    },
                );
            }
            if let (Some(before), Some(theme)) = (before, canvas.theme.clone()) {
                pulse_row(canvas, &before, &theme, y, y + height);
            }
            y += height + 1;
        }
        if window.below {
            canvas.accent_text(self.name_x, y, "…", ellipsis);
        }
    }
}
/// Accounts one home list scroll position shows, and whether a `…` row marks
/// hidden accounts above or below them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ListWindow {
    pub first: usize,
    /// One past the last visible account.
    pub end: usize,
    pub above: bool,
    pub below: bool,
}
impl ListWindow {
    /// Visible rows: accounts and `…` rows.
    pub fn rows(&self) -> usize {
        self.end - self.first + usize::from(self.above) + usize::from(self.below)
    }
}
/// Terminal rows of a `…` row with its blank line.
const ELLIPSIS_ROWS: usize = 2;
/// From this many visible rows, the cursor keeps an account between itself and a `…` row.
const MARGIN_ROWS: usize = 5;
/// `text` padded with spaces to `width` cells.
fn pad(text: &str, width: usize) -> String {
    let used = unicode_width::UnicodeWidthStr::width(text);
    format!("{text}{}", " ".repeat(width.saturating_sub(used)))
}

/// The moving account's hue-shifted cells (bullet or icon, name, bars, statuses)
/// pulse between the selected-bullet level and its dim level, sampled per cell,
/// with the grabbed picker arrow's oscillation. Grey text is left alone.
fn pulse_row(
    canvas: &mut Canvas,
    before: &[Cell],
    theme: &crate::render::Theme,
    top: usize,
    bottom: usize,
) {
    for y in top..bottom.min(canvas.height) {
        for x in 0..canvas.width {
            let i = y * canvas.width + x;
            let cell = &canvas.cells[i];
            let [r, g, b] = cell.fg;
            if *cell == before[i] || cell.glyph.trim().is_empty() || (r == g && g == b) {
                continue;
            }
            canvas.cells[i].fg = theme.accent_pulse(x, y);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn moving_row_pulses_between_accent_levels_and_others_look_inactive() {
        let mut accounts = vec![account(), account(), account()];
        accounts[0].id = "codex:one".into();
        accounts[1].id = "codex:two".into();
        accounts[2].id = "codex:three".into();
        let layout = AccountsLayout::fit(&accounts, 100);
        let mut field = crate::render::Field::new(100, 40, 1967);
        let rows = |c: &Canvas, y: usize| c.cells[y * 100..(y + 1) * 100].to_vec();
        let row_y = |i: usize| 3 + i * (layout.row_height(&Usage::account(&accounts[i])) + 1);
        for time in [5000, 5250, 5500, 5750] {
            let base = field.paint(time);
            let theme = base.theme.clone().unwrap();
            let mut moving = base.clone();
            layout.paint(
                &mut moving,
                &accounts,
                3,
                40,
                0,
                Some(1),
                false,
                false,
                Some(1),
            );
            // Other accounts: exactly the state with the cursor above the accounts.
            let mut above = base.clone();
            layout.paint(&mut above, &accounts, 3, 40, 0, None, false, false, None);
            for i in [0, 2] {
                assert_eq!(rows(&moving, row_y(i)), rows(&above, row_y(i)), "row {i}");
            }
            // The moving row: every hue-shifted cell is the pulse at its cell, and
            // stays within [dark, light] on every channel.
            let y = row_y(1);
            let mut pulsed = 0;
            for (x, cell) in rows(&moving, y).iter().enumerate() {
                let [r, g, b] = cell.fg;
                if cell.glyph.trim().is_empty()
                    || (r == g && g == b)
                    || cell == &base.cells[y * 100 + x]
                {
                    continue;
                }
                assert_eq!(cell.fg, theme.accent_pulse(x, y), "x={x} t={time}");
                let level = |brightness| {
                    theme
                        .accent(
                            x,
                            y,
                            &Accent {
                                brightness,
                                ..Accent::default()
                            },
                        )
                        .fg
                };
                let (light, dark) = (level(1000), level(600));
                for c in 0..3 {
                    let (lo, hi) = (light[c].min(dark[c]), light[c].max(dark[c]));
                    assert!((lo..=hi).contains(&cell.fg[c]), "x={x} t={time}");
                }
                pulsed += 1;
            }
            assert!(pulsed > 10, "icon, name and bars pulse ({pulsed})");
        }
        // Leaving move mode restores the normal selected styling.
        let base = field.paint(5250);
        let mut normal = base.clone();
        layout.paint(
            &mut normal,
            &accounts,
            3,
            40,
            0,
            Some(1),
            false,
            false,
            None,
        );
        let mut plain = base.clone();
        layout.paint(&mut plain, &accounts, 3, 40, 0, Some(1), false, false, None);
        assert_eq!(normal.cells, plain.cells);
        let theme = base.theme.clone().unwrap();
        let y = row_y(1);
        let name = rows(&normal, y)
            .iter()
            .position(|c| c.glyph == "t")
            .unwrap();
        assert_eq!(
            normal.cells[y * 100 + name].fg,
            theme
                .accent(
                    name,
                    y,
                    &Accent {
                        bold: true,
                        ..Accent::default()
                    }
                )
                .fg
        );
    }
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
                        usage.paint_status(
                            &mut expected,
                            layout.bars,
                            y,
                            layout.bar_width,
                            active && usage_focus,
                        );
                        let sx = usage.status_x(layout.bars, layout.bar_width);
                        for x in sx..sx + UnicodeWidthStr::width(usage.status_label().as_str()) {
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
    fn parenthetical_status_takes_the_bar_hue_and_is_bold_only_with_the_cursor_on_it() {
        let mut a = account();
        a.id = "codex:personal".into();
        a.details.insert("enabled".into(), json!(false));
        let usage = Usage::account(&a);
        assert!(usage.status_only());
        let mut bars = account();
        bars.id = "codex:bars".into();
        let layout = AccountsLayout::fit(&[a.clone(), bars.clone()], 100);
        let base = crate::render::Field::new(100, 10, 1967).paint(4200);
        let sx = usage.status_x(layout.bars, layout.bar_width);
        let len = UnicodeWidthStr::width(usage.status_label().as_str());
        // The usage bars' own colour, sampled at each cell.
        let bar = |x: usize, y: usize| {
            let mut c = base.clone();
            Usage::account(&bars).paint(&mut c, x, y, layout.bar_width, layout.digits);
            c.cells[y * 100 + x].fg
        };
        // (selected, back, usage_focus, cursor on the parenthetical)
        for (selected, back, focus, on) in [
            (Some(0), false, false, false),
            (Some(0), false, true, true),
            (Some(1), false, false, false),
            (Some(1), false, true, false),
            (None, false, false, false),
            (Some(0), true, true, false),
        ] {
            let mut c = base.clone();
            layout.paint(
                &mut c,
                &[a.clone(), bars.clone()],
                3,
                10,
                0,
                selected,
                back,
                focus,
                None,
            );
            for x in sx..sx + len {
                let cell = &c.cells[3 * 100 + x];
                assert_eq!(cell.bold, on, "{selected:?} {back} {focus}");
                assert_eq!(cell.fg, bar(x, 3), "{selected:?} {back} {focus} x={x}");
            }
        }
        // `(moving)` keeps the bold rule and pulses with the rest of its row.
        for (focus, on) in [(false, false), (true, true)] {
            let mut c = base.clone();
            let accounts = [bars.clone()];
            layout.paint(&mut c, &accounts, 3, 10, 0, Some(0), false, focus, Some(0));
            let line: String = c.cells[300..400].iter().map(|c| c.glyph.as_str()).collect();
            let mx = line
                .find("(moving)")
                .map(|b| line[..b].chars().count())
                .unwrap();
            for x in mx..mx + "(moving)".len() {
                assert_eq!(c.cells[300 + x].bold, on, "moving focus={focus}");
                assert_eq!(
                    c.cells[300 + x].fg,
                    c.theme.as_ref().unwrap().accent_pulse(x, 3),
                    "moving x={x}"
                );
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
            age_inline: true,
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
                age_inline: true,
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
    #[test]
    fn values_read_as_remaining_and_ages_hang_right_or_wrap_together() {
        let mut a = account();
        a.usage["observed_at"] = json!(40);
        let accounts = vec![a.clone(), account()];
        let u = Usage::account(&accounts[0]);
        assert_eq!(u.age.as_deref(), Some("1m ago"));
        // The age is outside the centred block.
        let with = AccountsLayout::fit(&accounts, 120);
        let without = AccountsLayout::fit(&[account(), account()], 120);
        assert_eq!(with.left, without.left);
        assert!(!with.age_below);
        let mut c = Canvas::new(120, 12);
        with.paint(&mut c, &accounts, 1, 12, 0, None, false, false, None);
        let row = |c: &Canvas, y: usize| -> String {
            (0..c.width)
                .map(|x| c.cells[y * c.width + x].glyph.as_str())
                .collect()
        };
        assert!(row(&c, 1).contains("% 5h (1m ago)"), "{}", row(&c, 1));
        assert!(!row(&c, 2).contains("ago"), "shown once per account");
        // Too narrow for any inline age: every account shows it under its name.
        let tight = AccountsLayout::fit(&accounts, 52);
        assert!(tight.age_below);
        let mut c = Canvas::new(52, 12);
        tight.paint(&mut c, &accounts, 1, 12, 0, None, false, false, None);
        assert!(!row(&c, 1).contains("ago"));
        let under = row(&c, 2);
        assert_eq!(under.find("(1m"), Some(tight.name_x));
        assert_eq!(c.cells[2 * 52 + tight.name_x].fg, crate::widgets::MUTED);
        // Selecting the account name (the left cursor) keeps the age dark; only
        // usage focus brightens it.
        for (usage_focus, colour) in [
            (false, crate::widgets::MUTED),
            (true, crate::widgets::SECONDARY),
        ] {
            let mut c = Canvas::new(120, 12);
            with.paint(
                &mut c,
                &accounts,
                1,
                12,
                0,
                Some(0),
                false,
                usage_focus,
                None,
            );
            let line = row(&c, 1);
            let x = line[..line.find("(1m").unwrap()].chars().count();
            assert_eq!(c.cells[120 + x].fg, colour, "usage focus {usage_focus}");
        }
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
        assert!(text.contains(&crate::stub_bar::stub_bar(20)));
        assert!(!text.contains("Ready"));
        // The cursor on it reveals the parenthetical instead.
        let mut c = Canvas::new(40, 4);
        Usage::account(&a).paint_focused(&mut c, 1, 1, 20, 2);
        let text = c.cells.iter().map(|c| c.glyph.as_str()).collect::<String>();
        assert!(text.contains("(usage unavailable)"));
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
    fn account_cursor_is_an_outline_triangle_without_shifting_the_row() {
        assert_eq!(
            UnicodeWidthStr::width("▷"),
            UnicodeWidthStr::width("▶"),
            "outline cursor keeps the filled cursor's width"
        );
        let a: Account = serde_json::from_value(
            json!({"id":"claude:z","provider":"claude","selectable":true,"enabled":true}),
        )
        .unwrap();
        let accounts = vec![a];
        let l = AccountsLayout::fit(&accounts, 60);
        let mut idle = Canvas::new(60, 6);
        let mut active = Canvas::new(60, 6);
        l.paint(&mut idle, &accounts, 1, 6, 0, None, false, false, None);
        l.paint(&mut active, &accounts, 1, 6, 0, Some(0), false, false, None);
        assert_eq!(active.cells[60 + l.left].glyph, "▷");
        assert!(active.cells.iter().all(|c| c.glyph != "▶"));
        for i in (0..60 * 6).filter(|&i| i != 60 + l.left) {
            assert_eq!(idle.cells[i].glyph, active.cells[i].glyph, "cell {i} moved");
        }
    }
    #[test]
    fn provider_symbols_are_one_cell_wide_and_distinct() {
        let all = ["codex", "claude", "gemini", "antigravity"].map(provider_symbol);
        assert_eq!(all, ["⏣", "✻", "✦", "◬"]);
        for symbol in all {
            assert_eq!(UnicodeWidthStr::width(symbol), 1, "{symbol}");
        }
        assert_eq!(provider_symbol("unknown"), "⏣");
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
                } else if u.stub() {
                    // Unavailable usage: the stub bar at the bar's position and width.
                    let row: String = (l.bars..l.bars + l.bar_width).map(|x| cell(x, y)).collect();
                    assert_eq!(row, crate::stub_bar::stub_bar(l.bar_width), "{width}");
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
    /// One-row accounts `a0`, `a1`, ...: each account or `…` row is one row
    /// plus its blank, so N visible rows take 2N - 1 terminal rows.
    fn one_row_accounts(count: usize) -> Vec<Account> {
        (0..count)
            .map(|i| {
                serde_json::from_value(
                    json!({"id":format!("codex:a{i}"),"selectable":true,"enabled":true}),
                )
                .unwrap()
            })
            .collect()
    }
    /// What `rows` terminal rows show from `first`: names, `…`, or "" for a blank.
    fn shown(l: &AccountsLayout, accounts: &[Account], first: usize, rows: usize) -> Vec<String> {
        let mut c = Canvas::new(60, rows + 3);
        l.paint(
            &mut c,
            accounts,
            1,
            1 + rows,
            first,
            None,
            false,
            false,
            None,
        );
        let text = |y: usize| -> String {
            c.cells[y * 60..(y + 1) * 60]
                .iter()
                .map(|c| c.glyph.as_str())
                .collect()
        };
        // At least one blank row always follows the list, `…` included.
        assert!(text(1 + rows).trim().is_empty());
        (1..1 + rows)
            .map(|y| {
                let name: String = c.cells[y * 60 + l.name_x..y * 60 + l.name_x + l.name_width]
                    .iter()
                    .map(|c| c.glyph.as_str())
                    .collect();
                name.trim().to_owned()
            })
            .collect()
    }
    /// Visible rows separated by single blank rows.
    fn spaced(items: &[&str]) -> Vec<String> {
        let mut out: Vec<String> = items
            .iter()
            .flat_map(|s| [s.to_string(), String::new()])
            .collect();
        out.pop();
        out
    }
    #[test]
    fn home_list_scrolls_one_account_early_past_ellipsis_rows_hiding_two_or_more() {
        let accounts = one_row_accounts(6);
        let l = AccountsLayout::fit(&accounts, 60);
        // Five visible rows, six accounts: four accounts, then `…` for two.
        let area = 9;
        let mut first = 0;
        let mut expect = |sel: usize, scroll: usize, items: &[&str]| {
            first = l.scroll(&accounts, first, Some(sel), area);
            assert_eq!(first, scroll, "cursor on a{sel}");
            assert_eq!(shown(&l, &accounts, first, area), spaced(items), "a{sel}");
        };
        let top = ["a0", "a1", "a2", "a3", "…"];
        let end = ["…", "a2", "a3", "a4", "a5"];
        expect(0, 0, &top);
        expect(1, 0, &top);
        // The third row is the last before the margin: still no scroll.
        expect(2, 0, &top);
        // Down from it: the list advances and the cursor (a3) stays on the third
        // row; the bottom `…` gives way to the last accounts and a top `…` hides two.
        expect(3, 2, &end);
        expect(4, 2, &end);
        expect(5, 2, &end);
        expect(4, 2, &end);
        expect(3, 2, &end);
        // The reverse upward: from the second account below the top `…`.
        expect(2, 0, &top);
        expect(0, 0, &top);
        // Exactly one hidden account is shown instead of a `…` row.
        assert_eq!(l.scroll(&accounts, 1, None, area), 0);
        for sel in 0..6 {
            for from in 0..6 {
                let w = l.window(&accounts, l.scroll(&accounts, from, Some(sel), area), area);
                assert!(w.first != 1 && w.end + 1 != 6, "{sel} from {from}: {w:?}");
            }
        }
        // Wrapping lands at the extremes at once: the end window for the last account.
        assert_eq!(l.scroll(&accounts, 6, Some(5), area), 2);
        assert_eq!(l.scroll(&accounts, 0, Some(5), area), 2);
        assert_eq!(l.scroll(&accounts, 2, Some(0), area), 0);
        // A spare terminal row leaves two blanks under the bottom `…`, never none.
        let mut spare = spaced(&top);
        spare.push(String::new());
        assert_eq!(shown(&l, &accounts, 0, area + 1), spare);
    }
    #[test]
    fn short_home_lists_drop_the_margin_and_one_row_scrolls_plainly() {
        let accounts = one_row_accounts(6);
        let l = AccountsLayout::fit(&accounts, 60);
        // Four visible rows: the cursor may sit right next to a `…` row.
        let area = 7;
        let mut first = 0;
        let mut expect = |sel: usize, scroll: usize, items: &[&str]| {
            first = l.scroll(&accounts, first, Some(sel), area);
            assert_eq!(first, scroll, "cursor on a{sel}");
            assert_eq!(shown(&l, &accounts, first, area), spaced(items), "a{sel}");
        };
        expect(0, 0, &["a0", "a1", "a2", "…"]);
        expect(2, 0, &["a0", "a1", "a2", "…"]);
        expect(3, 2, &["…", "a2", "a3", "…"]);
        expect(4, 3, &["…", "a3", "a4", "a5"]);
        expect(3, 3, &["…", "a3", "a4", "a5"]);
        expect(2, 2, &["…", "a2", "a3", "…"]);
        expect(1, 0, &["a0", "a1", "a2", "…"]);
        // One row (or too few for a `…` row on each side of an account): no `…`
        // rows; the list scrolls plainly to keep the cursor's account visible.
        assert_eq!(l.scroll(&accounts, 0, Some(3), 1), 3);
        assert_eq!(shown(&l, &accounts, 3, 1), ["a3"]);
        assert_eq!(l.scroll(&accounts, 3, Some(0), 1), 0);
        assert_eq!(shown(&l, &accounts, 0, 3), spaced(&["a0", "a1"]));
    }
    #[test]
    fn ellipsis_rows_align_with_names_and_use_the_name_hue_for_each_state() {
        let accounts = one_row_accounts(9);
        let l = AccountsLayout::fit(&accounts, 60);
        let mut field = crate::render::Field::new(60, 12, 1967);
        let base = field.paint(4200);
        let theme = base.theme.clone().unwrap();
        // From a3 in nine rows: `…` (row 1), a3, a4, a5, `…` (row 9).
        assert_eq!(
            l.window(&accounts, 3, 9),
            ListWindow {
                first: 3,
                end: 6,
                above: true,
                below: true
            }
        );
        for (selected, back, usage_focus, moving, brightness) in [
            (Some(4), false, false, None, 600),
            // Usage focus keeps the brighter level.
            (Some(4), false, true, None, 1000),
            (None, false, false, None, 600),
            (Some(4), true, false, None, 600),
            (Some(4), false, false, Some(4), 600),
        ] {
            let mut c = base.clone();
            l.paint(
                &mut c,
                &accounts,
                1,
                10,
                3,
                selected,
                back,
                usage_focus,
                moving,
            );
            let level = |y: usize| {
                theme.accent(
                    l.name_x,
                    y,
                    &Accent {
                        brightness,
                        ..Accent::default()
                    },
                )
            };
            for y in [1, 9] {
                let cell = &c.cells[y * 60 + l.name_x];
                assert_eq!(cell.glyph, "…");
                assert_eq!((cell.fg, cell.bold), (level(y).fg, level(y).bold), "{y}");
                // Left of the name column (cursor lane, icon) and the rest stay as they were.
                for x in (0..60).filter(|&x| x != l.name_x) {
                    assert_eq!(c.cells[y * 60 + x], base.cells[y * 60 + x], "{x},{y}");
                }
                // Exactly one blank row on each side inside the list.
                let blank = if y == 1 { 2 } else { 8 };
                assert_eq!(
                    c.cells[blank * 60..(blank + 1) * 60],
                    base.cells[blank * 60..(blank + 1) * 60]
                );
            }
            // The same level as a name the cursor is not on (a5, row 7).
            let name = &c.cells[7 * 60 + l.name_x];
            assert_eq!(name.glyph, "a");
            assert_eq!(name.fg, level(7).fg);
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
            Some(short) => format!("{short:>digits$}% 5h · {weekly:>digits$}% wk"),
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
        assert_eq!(labels, vec![" 5% 5h · 80% wk", "99% 5h ·  6% wk", "45% wk"]);
        assert_eq!(labels[0].find('/'), labels[1].find('/'));
        assert_eq!(labels[0].find('%'), labels[1].find('%'));
        accounts[1].usage["short"]["used_percent"] = json!(0);
        assert_eq!(picker_labels(&accounts)[0], "  5% 5h ·  80% wk");
        assert_eq!(
            picker_labels(&[account(Some(95.), 94.)])[0],
            "5% 5h · 6% wk"
        );
        accounts[2].usage["short"] = json!({"duration_minutes":60,"used_percent":0});
        assert!(!picker_labels(&accounts)[2].contains('/'));
    }
    fn claude_account(usage: serde_json::Value) -> Account {
        serde_json::from_value(json!({"id":"claude:claude-main","provider":"claude","selectable":true,"usage_fresh":true,"enabled":true,"usage":usage})).unwrap()
    }
    #[test]
    fn antigravity_unsupported_usage_is_a_status_never_zero_percent() {
        let a: Account = serde_json::from_value(json!({"id":"antigravity:agy","provider":"antigravity","selectable":true,"enabled":true,"usage_fresh":false,"usage_state":"unsupported","usage":{"state":"unsupported","observed_at":null,"short":null,"weekly":null}})).unwrap();
        let u = Usage::account(&a);
        assert_eq!(u.status, "Usage unsupported");
        assert!(u.status_only());
        assert_eq!(a.provider(), "antigravity");
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
            assert_eq!(picker_labels(&[a]), ["?% 5h · ?% wk"]);
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
    fn age_suffix_is_muted_grey_unselected_and_menu_grey_unbold_when_selected() {
        let a = claude_account(json!({"state":"fresh","observed_at":100,
            "short":{"duration_minutes":300,"used_percent":40},"weekly":null}));
        let u = Usage::account_at(&a, 100 + 42 * 60);
        let age_at = |c: &Canvas| {
            let row: String = (0..60).map(|x| c.cells[x].glyph.as_str()).collect();
            let byte = row.find("(42m ago)").unwrap();
            row[..byte].chars().count()
        };
        let mut c = Canvas::new(60, 2);
        u.paint(&mut c, 0, 0, 12, 2);
        let x = age_at(&c);
        assert_eq!(c.cells[x].fg, crate::widgets::MUTED);
        assert!(!c.cells[x].bold);
        // The value text before it keeps its own colour.
        assert_eq!(c.cells[x - 6].fg, crate::widgets::SECONDARY);
        // Selected (focused row): the unselected menu-item grey, still not bold.
        let mut c = Canvas::new(60, 2);
        u.paint_selected(&mut c, 0, 0, 12, 2, true);
        assert_eq!(c.cells[x].fg, crate::widgets::SECONDARY);
        assert!(!c.cells[x].bold);
        // A selected home row recolours an already painted suffix.
        let mut c = Canvas::new(60, 2);
        u.paint(&mut c, 0, 0, 12, 2);
        u.recolor_age(&mut c, 0, 0, 12, 2, true);
        assert_eq!(c.cells[x].fg, crate::widgets::SECONDARY);
        assert!(!c.cells[x].bold);
        assert_eq!(c.cells[x - 1].fg, crate::widgets::SECONDARY);
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
        assert_eq!(picker_labels(&[partial]), ["75% 5h ·  ?% wk"]);
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
    fn missing_window_row_keeps_its_columns_with_a_stub_bar_and_unknown_value() {
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
        // A stub bar keeps the column; its value is unknown, never 0%.
        assert!(row(1).starts_with("╶╶─╼━━━━╾─╴╴  ?% wk"), "{}", row(1));
        assert_eq!(canvas.cells[40].fg, UNFILLED);
        assert!(!canvas.cells[40].bold);
        assert_eq!(canvas.cells[40 + 14].fg, crate::widgets::MUTED);
    }
    fn with_details(mut a: Account, details: serde_json::Value) -> Account {
        for (k, v) in details.as_object().unwrap() {
            a.details.insert(k.clone(), v.clone());
        }
        a
    }
    fn fresh_usage() -> serde_json::Value {
        json!({"state":"fresh","observed_at":100,
            "short":{"duration_minutes":300,"used_percent":20},
            "weekly":{"duration_minutes":10080,"used_percent":0}})
    }
    /// Stub where usage is unavailable, a parenthetical only for action-required
    /// (and disabled) accounts, real bars otherwise.
    #[test]
    fn state_selects_stub_parenthetical_or_real_bar() {
        #[derive(Debug, PartialEq)]
        enum Shown {
            Bars,
            Stub,
            Status(&'static str),
        }
        let unsupported: Account = serde_json::from_value(json!({"id":"gemini:g","provider":"gemini","selectable":true,"enabled":true,"usage_fresh":false,"usage_state":"unsupported","usage":{"state":"unsupported","observed_at":null,"short":null,"weekly":null}})).unwrap();
        let cases: Vec<(&str, Account, Shown)> = vec![
            ("fresh", claude_account(fresh_usage()), Shown::Bars),
            (
                "stale",
                claude_account(json!({"state":"stale","observed_at":100,
                    "short":{"duration_minutes":300,"used_percent":20},"weekly":null})),
                Shown::Bars,
            ),
            ("unsupported", unsupported, Shown::Stub),
            (
                "missing",
                claude_account(
                    json!({"state":"missing","observed_at":null,"short":null,"weekly":null}),
                ),
                Shown::Stub,
            ),
            (
                "error",
                claude_account(json!({"state":"error","observed_at":100,
                    "short":{"duration_minutes":300,"used_percent":0},"weekly":null})),
                Shown::Stub,
            ),
            ("not yet reported", claude_account(json!({})), Shown::Stub),
            (
                "deleting",
                with_details(
                    claude_account(fresh_usage()),
                    json!({"deletion":{"id":"d1","phase":"running"}}),
                ),
                Shown::Bars,
            ),
            (
                "deleting without usage",
                with_details(
                    claude_account(json!({})),
                    json!({"deletion":{"id":"d1","phase":"running"}}),
                ),
                Shown::Stub,
            ),
            (
                "login",
                with_details(claude_account(fresh_usage()), json!({"auth_required":true})),
                Shown::Status("(login required)"),
            ),
            (
                "auth detail",
                with_details(
                    claude_account(json!({})),
                    json!({"auth_detail":"AUTH_REQUIRED"}),
                ),
                Shown::Status("(login required)"),
            ),
            (
                "disabled",
                with_details(claude_account(fresh_usage()), json!({"enabled":false})),
                Shown::Status("(disabled)"),
            ),
            (
                "deregistered",
                with_details(claude_account(json!({})), json!({"membership":"removed"})),
                Shown::Status("(deregistered)"),
            ),
            (
                "deletion failed",
                with_details(
                    claude_account(fresh_usage()),
                    json!({"deletion":{"id":"d1","phase":"failed"}}),
                ),
                Shown::Status("(deletion failed)"),
            ),
        ];
        for (name, a, expected) in cases {
            let u = Usage::account(&a);
            let mut c = Canvas::new(60, 3);
            u.paint(&mut c, 2, 0, 20, 2);
            let row: String = (0..60).map(|x| c.cells[x].glyph.as_str()).collect();
            let bar: String = (2..22).map(|x| c.cells[x].glyph.as_str()).collect();
            let shown = if u.action_required() {
                assert!(u.status_only() && !u.stub(), "{name}");
                assert_eq!(row.find('('), Some(3), "{name}: at bar_x + 1");
                Shown::Status(match u.status_label().as_str() {
                    "(login required)" => "(login required)",
                    "(disabled)" => "(disabled)",
                    "(deregistered)" => "(deregistered)",
                    "(deletion failed)" => "(deletion failed)",
                    other => panic!("{name}: {other}"),
                })
            } else if bar == crate::stub_bar::stub_bar(20) {
                assert!(u.stub() && u.status_only(), "{name}");
                assert!(!row.contains('('), "{name}: no parenthetical until hovered");
                Shown::Stub
            } else {
                assert!(!u.status_only(), "{name}");
                assert!(row.contains("% 5h"), "{name}: {row}");
                Shown::Bars
            };
            assert_eq!(shown, expected, "{name}: {row:?}");
            if let Shown::Status(label) = expected {
                assert!(row.contains(label), "{name}: {row}");
            }
        }
    }
    /// The stub's colour is the bars' unfilled grey: no hue, no marker, never bold.
    #[test]
    fn stub_is_unfilled_grey_never_bold_in_every_paint_mode() {
        let a = claude_account(json!({}));
        let u = Usage::account(&a);
        let base = crate::render::Field::new(60, 4, 1967).paint(4200);
        type Paint = fn(&Usage, &mut Canvas, usize, usize, usize, usize);
        let modes: [Paint; 3] = [
            |u, c, x, y, b, d| u.paint(c, x, y, b, d),
            |u, c, x, y, b, d| u.paint_selected(c, x, y, b, d, true),
            |u, c, x, y, b, d| u.paint_deemphasized(c, x, y, b, d),
        ];
        for paint in modes {
            let mut c = base.clone();
            paint(&u, &mut c, 4, 1, 17, 2);
            let row: String = (4..21).map(|x| c.cells[60 + x].glyph.as_str()).collect();
            assert_eq!(row, "╶╶╴──━━━━━━━──╶╴╴");
            for x in 4..21 {
                assert_eq!(c.cells[60 + x].fg, UNFILLED, "x={x}");
                assert!(!c.cells[60 + x].bold, "x={x}");
            }
        }
        // The grey is the one the normal bars use for their unfilled part.
        let mut c = base.clone();
        let empty = claude_account(json!({"state":"fresh","observed_at":100,
            "short":{"duration_minutes":300,"used_percent":100},"weekly":null}));
        Usage::account(&empty).paint(&mut c, 4, 1, 17, 2);
        assert_eq!(c.cells[60 + 10].glyph, "─");
        assert_eq!(c.cells[60 + 10].fg, UNFILLED);
    }
    fn home_row(c: &Canvas, y: usize) -> String {
        (0..c.width)
            .map(|x| c.cells[y * c.width + x].glyph.as_str())
            .collect()
    }
    /// Only the cursor on that account's usage hides its stub and shows
    /// `(usage unavailable)` at bar_x + 1; moving away shows the stub again.
    #[test]
    fn hovering_a_stub_reveals_usage_unavailable_in_its_place() {
        let accounts = vec![
            claude_account(fresh_usage()),
            claude_account(
                json!({"state":"missing","observed_at":null,"short":null,"weekly":null}),
            ),
            claude_account(json!({"state":"partial","observed_at":100,
                "short":{"duration_minutes":300,"used_percent":20},"weekly":null})),
        ];
        let width = 90;
        let l = AccountsLayout::fit(&accounts, width);
        let stub = crate::stub_bar::stub_bar(l.bar_width);
        // Rows from 1: fresh 1-2, blank, stub 4, blank, partial 6-7 (weekly missing).
        let (stub_y, partial_y) = (4, 7);
        let base = crate::render::Field::new(width, 12, 1967).paint(4200);
        // (selected, usage focus, stub hovered, partial hovered)
        for (selected, focus, on_stub, on_partial) in [
            (None, false, false, false),
            (Some(1), false, false, false),
            (Some(1), true, true, false),
            (Some(0), true, false, false),
            (Some(2), false, false, false),
            (Some(2), true, false, true),
        ] {
            let mut c = base.clone();
            l.paint(&mut c, &accounts, 1, 12, 0, selected, false, focus, None);
            let bar = |y: usize| -> String {
                (l.bars..l.bars + l.bar_width)
                    .map(|x| c.cells[y * width + x].glyph.as_str())
                    .collect()
            };
            for (y, hovered) in [(stub_y, on_stub), (partial_y, on_partial)] {
                let row = home_row(&c, y);
                let at = row.find(UNAVAILABLE).map(|b| row[..b].chars().count());
                if hovered {
                    assert_eq!(at, Some(l.bars + 1), "{selected:?} {focus} y={y}: {row}");
                    assert_eq!(c.cells[y * width + l.bars].glyph, " ");
                    assert!(!row.contains('━'), "stub hidden: {row}");
                    assert!(!row.contains("?%"), "unknown value hidden: {row}");
                    for x in l.bars + 1..l.bars + 1 + UNAVAILABLE.len() {
                        assert!(c.cells[y * width + x].bold, "cursor on it: x={x}");
                    }
                } else {
                    assert_eq!(at, None, "{selected:?} {focus} y={y}: {row}");
                    assert_eq!(bar(y), stub, "{selected:?} {focus} y={y}");
                }
            }
            // The stub-only account keeps the selector right of the revealed text.
            if on_stub {
                assert!(home_row(&c, stub_y).contains("(usage unavailable) ◀"));
            }
            // The partial account's known window keeps its bar and selector.
            assert!(
                home_row(&c, partial_y - 1).contains("80% 5h"),
                "{selected:?}"
            );
        }
    }
    /// Action-required parentheticals take exactly the filled bar's colour, cell
    /// by cell (same hueshift and level); bold only with the cursor on them.
    #[test]
    fn action_required_parenthetical_matches_the_filled_bar_colour() {
        let full = claude_account(json!({"state":"fresh","observed_at":100,
            "short":{"duration_minutes":300,"used_percent":0},"weekly":null}));
        for details in [
            json!({"auth_required":true}),
            json!({"auth_detail":"AUTH_REQUIRED"}),
            json!({"enabled":false}),
            json!({"membership":"removed"}),
        ] {
            let a = with_details(claude_account(fresh_usage()), details.clone());
            let accounts = vec![a.clone(), full.clone()];
            let width = 90;
            let l = AccountsLayout::fit(&accounts, width);
            let base = crate::render::Field::new(width, 12, 1967).paint(4200);
            let u = Usage::account(&a);
            assert!(u.action_required(), "{details}");
            let label = u.status_label();
            let sx = l.bars + 1;
            // A full bar's filled part painted at the same cells.
            let mut filled = base.clone();
            Usage::account(&full).paint(&mut filled, sx - 1, 1, label.len() + 2, l.digits);
            for (selected, focus, bold) in [
                (None, false, false),
                (Some(0), false, false),
                (Some(1), true, false),
                (Some(0), true, true),
            ] {
                let mut c = base.clone();
                l.paint(&mut c, &accounts, 1, 12, 0, selected, false, focus, None);
                let row = home_row(&c, 1);
                assert_eq!(row.find(&label).map(|b| row[..b].chars().count()), Some(sx));
                for x in sx..sx + label.len() {
                    let cell = &c.cells[width + x];
                    let bar = &filled.cells[width + x];
                    assert_eq!(bar.glyph, "━", "filled at x={x}");
                    assert_eq!(cell.fg, bar.fg, "{details} {selected:?} {focus} x={x}");
                    assert_eq!(cell.bold, bold, "{details} {selected:?} {focus} x={x}");
                }
            }
        }
    }
}
