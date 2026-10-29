//! Sign-in links in captured login-pane screens, wrapped as a terminal wraps them.

use sessionsation_ui::login_link::extract;

fn screen(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/login-screens/{name}.txt",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn url(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/login-screens/{name}.url",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
    .trim_end_matches('\n')
    .to_owned()
}

#[test]
fn wrapped_links_are_rebuilt_exactly_at_both_widths() {
    for (provider, width) in [
        ("claude", 80),
        ("claude", 120),
        ("gemini", 80),
        ("gemini", 120),
    ] {
        let text = screen(&format!("{provider}-{width}"));
        let rows: Vec<_> = text.lines().collect();
        assert!(rows.iter().all(|r| r.chars().count() <= width));
        assert!(
            rows.iter().filter(|r| r.chars().count() == width).count() >= 2,
            "{provider} at {width} must really wrap"
        );
        assert_eq!(extract(&text), Some(url(provider)), "{provider} at {width}");
    }
}

#[test]
fn a_link_ending_exactly_at_the_last_column_is_complete() {
    assert_eq!(extract(&screen("exact-80")), Some(url("exact")));
}

#[test]
fn the_most_recent_link_wins() {
    assert_eq!(extract(&screen("replaced-80")), Some(url("gemini")));
}

#[test]
fn no_link_yet_is_none() {
    assert_eq!(extract(&screen("waiting-80")), None);
    assert_eq!(extract(""), None);
}

#[test]
fn unwrapped_and_padded_screens_work_too() {
    let link = "https://example.invalid/a?b=c%20d#e";
    assert_eq!(
        extract(&format!("Visit {link}.\nPaste code >")),
        Some(link.into())
    );
    assert_eq!(
        extract(&format!("Visit {link}      \n      \n")),
        Some(link.into())
    );
}
