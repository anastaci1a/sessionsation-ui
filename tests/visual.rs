//! Reviewed cell snapshots cover motion, common controls and terminal-bottom masks.
//! UPDATE_GOLDENS=1 is an explicit developer operation, never enabled by CI.
use sessionsation_ui::{
    render::{Accent, Field},
    widgets::{Options, Row, TextInput},
};

#[test]
fn fixed_seed_time_and_geometry_match_cell_goldens() {
    for (width, height, time) in [(80, 24, 0), (80, 24, 4060), (80, 24, 4700), (40, 16, 8000)] {
        let mut field = Field::new(width, height, 1967);
        let mut options = Options::default();
        let rows = vec![
            Row::yes("Account: codex:fixture"),
            Row::yes("Space: Example"),
            Row::yes("Name: 界e\u{301}"),
            Row::gate("Start Session", false),
        ];
        let mut frame = None;
        for tick in (0..=time).step_by(20) {
            let mut c = field.paint(tick);
            let (left, _) = options.paint(&mut c, &rows, 5, height - 5);
            TextInput::new("界e\u{301}".into()).paint(&mut c, 2, 3, width - 4, true);
            options.paint_back(&mut c, left, 5, "Back");
            c.accent_text(
                2,
                height - 1,
                "Esc: Back",
                Accent {
                    soft: true,
                    brightness: 550,
                    flash: false,
                    ..Accent::default()
                },
            );
            field.apply_masks(&c);
            frame = Some(c);
        }
        let c = frame.unwrap();
        let cells: Vec<_> = c
            .cells
            .iter()
            .enumerate()
            .filter(|(_, c)| c.glyph != " " || c.bg.is_some() || c.strike)
            .map(|(i, c)| {
                serde_json::json!([i % width, i / width, c.glyph, c.fg, c.bg, c.bold, c.strike])
            })
            .collect();
        let actual = serde_json::to_string_pretty(
            &serde_json::json!({"width":width,"height":height,"time":time,"cells":cells}),
        )
        .unwrap()
            + "\n";
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("tests/golden/{width}x{height}-{time}.json"));
        if std::env::var_os("UPDATE_GOLDENS").is_some() {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, &actual).unwrap();
        }
        assert_eq!(std::fs::read_to_string(path).unwrap(), actual);
    }
}

#[test]
fn management_geometry_usage_markers_and_picker_columns() {
    use sessionsation_ui::{
        client::Account,
        dialog::Dialog,
        management::{Manager, Page},
        usage::picker_labels,
    };
    let accounts:Vec<Account>=(0..3).map(|n|serde_json::from_value(serde_json::json!({"id":format!("codex:fixture{n}"),"selectable":true,"usage_fresh":true,"enabled":true,"usage":{"observed_at":100,"short":if n==2{serde_json::Value::Null}else{serde_json::json!({"duration_minutes":300,"used_percent":if n==0{95}else{0}})},"weekly":{"duration_minutes":10080,"used_percent":20}},"switch_thresholds":{"customized":true,"short_percent":80,"weekly_percent":90}})).unwrap()).collect();
    for (name, width, height) in [
        ("inline", 100, 30),
        ("compact", 60, 16),
        ("actions", 80, 24),
        ("picker", 60, 20),
        ("modal", 60, 24),
        ("operations-wide", 120, 24),
        ("operations-compact", 50, 20),
        ("operations-tiny", 40, 10),
    ] {
        let mut m = Manager::default();
        m.inventory(accounts.clone());
        m.capabilities = serde_json::json!({"providers":[{"id":"codex","account_limits":true,"enrollment":true}]});
        let mut f = Field::new(width, height, 1967);
        let mut c = f.paint(4060);
        match name {
            "operations-wide" | "operations-compact" | "operations-tiny" => {
                m.page = Page::Operations;
                m.operations = vec![
                    serde_json::json!({"id":"11111111-1111-4111-8111-111111111111","kind":"account_delete","phase":"preserving_history"}),
                    serde_json::json!({"id":"22222222-2222-4222-8222-222222222222","kind":"switch","phase":"complete"}),
                ];
            }
            "compact" => {
                m.page = Page::Accounts;
                m.options.selected = 1;
            }
            "actions" => m.page = Page::Account("codex:fixture0".into()),
            "modal" => {
                m.dialog = Some(Dialog::new(
                    "Deregister account",
                    vec![
                        "Credentials and conversation history remain recoverable.".into(),
                        "Account: codex:fixture0".into(),
                    ],
                    &["Confirm", "Cancel"],
                ))
            }
            _ => {}
        }
        if name == "picker" {
            let rows: Vec<_> = accounts
                .iter()
                .zip(picker_labels(&accounts))
                .map(|(a, label)| Row::yes(&a.id).with_right_preview(label))
                .collect();
            let mut options = Options::default();
            let (left, _) = options.paint(&mut c, &rows, 3, height - 3);
            options.paint_back(&mut c, left, 3, "Back");
        } else {
            m.paint(&mut c, height - 1);
        }
        let cells: Vec<_> = c
            .cells
            .iter()
            .enumerate()
            .filter(|(_, c)| c.glyph != " " || c.bg.is_some() || c.strike)
            .map(|(i, c)| {
                serde_json::json!([i % width, i / width, c.glyph, c.fg, c.bg, c.bold, c.strike])
            })
            .collect();
        let actual = serde_json::to_string_pretty(
            &serde_json::json!({"width":width,"height":height,"cells":cells}),
        )
        .unwrap()
            + "\n";
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("tests/golden/management-{name}.json"));
        if std::env::var_os("UPDATE_GOLDENS").is_some() {
            std::fs::write(&path, &actual).unwrap();
        }
        assert_eq!(std::fs::read_to_string(path).unwrap(), actual);
    }
}
