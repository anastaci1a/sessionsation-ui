//! Sign-in links printed by a provider CLI inside the HERDR login pane.
//!
//! Only the pane's visible text is read, and only to find an `http(s)` link;
//! nothing here touches credential files or tokens.

/// Label HERDR shows for the plugin's login pane (`herdr-plugin.toml`).
pub const LOGIN_PANE_LABEL: &str = "Provider login";

/// Providers whose CLI prints its OAuth link inside the login pane.
pub fn pane_provider(provider: &str) -> bool {
    matches!(provider, "claude" | "gemini" | "antigravity")
}

/// The most recent `http(s)` link on `screen`, rebuilt across terminal line wraps.
///
/// A terminal breaks a long link at its last column, so every row of the link but
/// the final one is exactly as wide as the screen's widest row and the next row
/// continues it with no space or other break. Such rows are joined exactly; no
/// character is dropped or added. Surrounding sentence punctuation is trimmed.
pub fn extract(screen: &str) -> Option<String> {
    let rows: Vec<&str> = screen.lines().map(|l| l.trim_end()).collect();
    let width = rows.iter().map(|r| r.chars().count()).max()?;
    let mut found = None;
    let mut i = 0;
    while i < rows.len() {
        let Some(start) = link_start(rows[i]) else {
            i += 1;
            continue;
        };
        let mut text = rows[i][start..].to_owned();
        let mut last = i;
        // Only a row that ends in the link and fills the screen can have wrapped.
        let mut full = link_tail(rows[i]) && rows[i].chars().count() == width;
        while full {
            let Some(next) = rows.get(last + 1).filter(|r| continues(r)) else {
                break;
            };
            text.push_str(next);
            last += 1;
            full = next.chars().count() == width;
        }
        if let Some(link) = crate::dialog::links_in(&text).into_iter().next() {
            found = Some(link);
        }
        i = last + 1;
    }
    found
}

/// Byte offset of the last `http(s)://` on the row.
fn link_start(row: &str) -> Option<usize> {
    match (row.rfind("https://"), row.rfind("http://")) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    }
}

/// Whether the row ends in the link itself.
fn link_tail(row: &str) -> bool {
    link_start(row).is_some_and(|s| !row[s..].contains(char::is_whitespace))
}

/// A row that can continue a wrapped link: non-empty and one unbroken word.
fn continues(row: &str) -> bool {
    !row.is_empty() && !row.contains(char::is_whitespace)
}
