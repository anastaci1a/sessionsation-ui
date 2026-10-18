//! Shared, read-only account presentation. Missing observations are never limits.
use crate::{
    client::Account,
    render::{Accent, Canvas, Cell},
    widgets::Row,
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub fn short_name(id: &str) -> String {
    let name = id.split_once(':').map_or(id, |(_, n)| n);
    let clean: String = name.chars().filter(|c| !c.is_control()).collect();
    if UnicodeWidthStr::width(clean.as_str()) <= 12 {
        return clean;
    }
    let mut out = String::new();
    for g in clean.graphemes(true) {
        if UnicodeWidthStr::width(out.as_str()) + UnicodeWidthStr::width(g) > 11 {
            break;
        }
        out.push_str(g);
    }
    out.push('…');
    out
}

#[derive(Clone, Debug)]
pub struct Window {
    pub label: &'static str,
    pub remaining: Option<u8>,
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
            "Removed"
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
        let windows = if a.usage["short"]["duration_minutes"] == 300 {
            [
                ("short", "5h", "short_percent", 300),
                ("weekly", "wkly", "weekly_percent", 10080),
            ]
            .into_iter()
            .map(|(key, label, threshold, duration)| {
                let remaining = (a.usage[key]["duration_minutes"] == duration)
                    .then(|| a.usage[key]["used_percent"].as_f64())
                    .flatten()
                    .filter(|n| n.is_finite() && (0. ..=100.).contains(n))
                    .map(|used| (100. - used).round() as u8);
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
        } else {
            vec![]
        };
        Self {
            windows,
            status,
            stale,
        }
    }
    pub fn height(&self) -> usize {
        self.windows.len().max(1)
    }
    pub fn digits(&self) -> usize {
        if self.windows.iter().any(|w| w.remaining == Some(100)) {
            3
        } else {
            2
        }
    }
    pub fn paint(&self, canvas: &mut Canvas, x: usize, y: usize, bar: usize, digits: usize) {
        if self.windows.is_empty() {
            canvas.text(x, y, &self.status, Cell::default(), false);
            return;
        }
        for (row, window) in self.windows.iter().enumerate() {
            let filled = window.remaining.map(|p| (bar * p as usize + 50) / 100);
            let marker = window
                .limit
                .map(|p| ((bar.saturating_sub(1)) as f64 * p / 100.).round() as usize);
            for col in 0..bar {
                let glyph = if marker == Some(col) {
                    "│"
                } else if filled.is_some_and(|n| col < n) {
                    "━"
                } else {
                    "─"
                };
                if marker == Some(col) {
                    canvas.text(
                        x + col,
                        y + row,
                        glyph,
                        Cell {
                            fg: [255, 196, 110],
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
                            brightness: if self.stale || filled.is_none_or(|n| col >= n) {
                                300
                            } else {
                                1000
                            },
                            flash: false,
                            ..Accent::default()
                        },
                    );
                }
            }
            let percent = window
                .remaining
                .map(|p| p.to_string())
                .unwrap_or_else(|| "?".into());
            let label = format!(
                "{:<4} {:>digits$}%{}",
                window.label,
                percent,
                if self.stale { " stale" } else { "" }
            );
            canvas.text(x + bar + 1, y + row, &label, Cell::default(), false);
        }
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
        let right = uses
            .iter()
            .map(|u| {
                if u.windows.is_empty() {
                    u.status.len()
                } else {
                    12 + 1 + 4 + 1 + digits + 1 + if u.stale { 6 } else { 0 }
                }
            })
            .max()
            .unwrap_or(0);
        let prefix = 2 + names + 2;
        let min_width = (prefix + right).max(17); // longest top option + marker
        if min_width + 8 > width {
            return None;
        }
        let extra = (width - 8 - min_width).min(12);
        let block = min_width + extra;
        let left = (width - block) / 2;
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
    ) {
        let mut y = top;
        for (index, account) in accounts.iter().enumerate() {
            Row::yes(short_name(&account.id)).paint(
                canvas,
                self.left,
                y,
                selected == Some(index) && !back,
                back,
            );
            let usage = Usage::account(account);
            usage.paint(canvas, self.bars, y, self.bar_width, self.digits);
            y += usage.height() + 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn account() -> Account {
        serde_json::from_value(json!({"id":"codex:long-account-name", "selectable":true,
        "usage_fresh":true,"usage":{"observed_at":100,"short":{"duration_minutes":300,"used_percent":20},"weekly":{"duration_minutes":10080,"used_percent":0}},
        "switch_thresholds":{"customized":true,"short_percent":80,"weekly_percent":90}
    })).unwrap()
    }
    #[test]
    fn markers_convert_used_thresholds_and_names_only_shorten_display() {
        let a = account();
        let u = Usage::account(&a);
        assert_eq!(short_name(&a.id), "long-accoun…");
        assert_eq!(a.id, "codex:long-account-name");
        assert_eq!(u.windows[0].remaining, Some(80));
        assert_eq!(u.windows[0].limit, Some(20.));
        assert_eq!(u.digits(), 3);
        let mut c = Canvas::new(60, 6);
        u.paint(&mut c, 3, 1, 11, 3);
        assert_eq!(c.cells[60 + 5].glyph, "│");
        assert_eq!(c.cells[120 + 4].glyph, "│");
    }
    #[test]
    fn no_five_hour_window_means_status_and_missing_weekly_is_unknown() {
        let mut a = account();
        a.usage["weekly"] = serde_json::Value::Null;
        assert_eq!(Usage::account(&a).windows[1].remaining, None);
        a.usage["short"]["duration_minutes"] = json!(60);
        assert!(Usage::account(&a).windows.is_empty());
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
    }
}
