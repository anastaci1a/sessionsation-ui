//! Reviewed cell snapshots cover motion, common controls and terminal-bottom masks.
//! UPDATE_GOLDENS=1 is an explicit developer operation, never enabled by CI.
use sessionsation_ui::{
    render::{Accent, Cell, Field},
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
            options.paint(&mut c, &rows, 5, height - 5);
            TextInput::new("界e\u{301}".into()).paint(&mut c, 2, 3, width - 4, true);
            c.text(2, height - 2, "◀— Back", Cell::default(), false);
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
