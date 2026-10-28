//! Reviewed cell snapshots cover motion, common controls and terminal-bottom masks.
//! UPDATE_GOLDENS=1 is an explicit developer operation, never enabled by CI.
use sessionsation_ui::{
    render::Field,
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
            sessionsation_ui::widgets::paint_hint(&mut c, 2, height - 1, "Esc: Back");
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
            &serde_json::json!({"width":width,"height":height,"time":time,"cursor":c.cursor,"cells":cells}),
        )
        .unwrap()
            + "\n";
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("tests/golden/{width}x{height}-{time}.json"));
        if std::env::var_os("UPDATE_GOLDENS").is_some() {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, &actual).unwrap();
        }
        assert!(
            std::fs::read_to_string(&path).unwrap() == actual,
            "cell snapshot differs: {}",
            path.display()
        );
    }
}

#[test]
fn management_geometry_usage_markers_and_picker_columns() {
    use sessionsation_ui::{
        client::Account,
        dialog::Dialog,
        management::{Manager, Page},
    };
    let accounts:Vec<Account>=(0..3).map(|n|serde_json::from_value(serde_json::json!({"id":format!("codex:fixture{n}"),"selectable":true,"usage_fresh":true,"enabled":true,"usage":{"observed_at":100,"short":if n==2{serde_json::Value::Null}else{serde_json::json!({"duration_minutes":300,"used_percent":if n==0{95}else{0}})},"weekly":{"duration_minutes":10080,"used_percent":20}},"switch_thresholds":{"customized":true,"short_percent":80,"weekly_percent":90}})).unwrap()).collect();
    for (name, width, height) in [
        ("inline", 100, 30),
        ("inline-selected", 100, 30),
        ("inline-usage-focus", 100, 30),
        ("attention", 100, 30),
        ("sessions-wide", 110, 24),
        ("sessions-claude-busy", 80, 20),
        ("archives-wide", 110, 24),
        ("archives-compact", 40, 18),
        ("sessions-loading", 60, 18),
        ("sessions-compact", 40, 18),
        ("settings-wide", 140, 24),
        ("settings-compact", 50, 18),
        ("roots-compact", 50, 18),
        ("directory-loading", 80, 26),
        ("directory-scroll", 80, 26),
        ("compact", 60, 16),
        ("actions", 80, 24),
        ("picker", 60, 20),
        ("providers", 60, 16),
        ("claude-session", 80, 24),
        ("add-to-herdr", 80, 24),
        ("modal", 60, 24),
        ("login-codex", 70, 20),
        ("login-claude-wait", 70, 20),
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
            "inline-selected" => m.options.selected = 5,
            "inline-usage-focus" => {
                m.options.selected = 5;
                m.usage_focus = true;
            }
            "attention" => {
                m.accounts[0]
                    .details
                    .insert("enabled".into(), serde_json::json!(false));
                m.accounts[1]
                    .details
                    .insert("auth_required".into(), serde_json::json!(true));
                m.options.selected = 6;
            }
            "archives-wide" | "archives-compact" => {
                m.page = Page::Archives;
                m.archives = vec![
                    serde_json::json!({"id":"11111111-1111-4111-8111-111111111111","name":"Saved notes"}),
                    serde_json::json!({"id":"22222222-2222-4222-8222-222222222222","name":"Earlier draft"}),
                ];
            }
            "sessions-loading" => {
                m.page = Page::Sessions;
            }
            "sessions-wide" | "sessions-compact" => {
                m.page = Page::Sessions;
                m.sessions_known = true;
                m.sessions = vec![
                    serde_json::json!({"name":"A name longer than twelve","id":"a","account_id":"codex:personal","availability":"running","activity":"busy","active_since":1}),
                    serde_json::json!({"name":"Saved","id":"b","account_id":"codex:work","availability":"saved"}),
                ];
                m.options.selected = sessionsation_ui::session_list::session_row(&m.sessions, 1);
            }
            "sessions-claude-busy" => {
                m.page = Page::Sessions;
                m.sessions_known = true;
                m.sessions = vec![
                    serde_json::json!({"name":"Codex busy","id":"a","account_id":"codex:personal","availability":"running","activity":"busy","active_since":1}),
                    serde_json::json!({"name":"Claude busy","id":"c","provider":"claude","account_id":"claude:main","availability":"running","native_status":"busy","activity":"busy"}),
                    serde_json::json!({"name":"Claude idle","id":"d","provider":"claude","account_id":"claude:main","availability":"running","native_status":"idle","activity":"idle"}),
                ];
            }
            "operations-wide" | "operations-compact" | "operations-tiny" => {
                m.page = Page::Operations;
                m.operations = vec![
                    serde_json::json!({"id":"11111111-1111-4111-8111-111111111111","kind":"account_delete","phase":"preserving_history"}),
                    serde_json::json!({"id":"22222222-2222-4222-8222-222222222222","kind":"switch","phase":"complete"}),
                ];
            }
            "compact" => {
                m.page = Page::Accounts;
                m.options.selected = 2;
            }
            "actions" => m.page = Page::Account("codex:fixture0".into()),
            "providers" => {
                let mut all = accounts.clone();
                all.extend([
                    serde_json::from_value(serde_json::json!({"id":"claude:claude-main","provider":"claude","selectable":true,"usage_fresh":true,"enabled":true,"usage":{"state":"partial","source":"claude-code-statusline","observed_at":100,"short":{"duration_minutes":300,"used_percent":40,"resets_at":1},"weekly":null}})).unwrap(),
                    serde_json::from_value(serde_json::json!({"id":"claude:claude-new","provider":"claude","selectable":true,"enabled":true,"usage_fresh":false,"usage":{"state":"missing","source":"claude-code-statusline","observed_at":null,"short":null,"weekly":null}})).unwrap(),
                ]);
                m.inventory(all);
                m.page = Page::Accounts;
                m.options.selected = 6;
            }
            "add-to-herdr" => {
                m.page = Page::Session("c".into());
                m.sessions = vec![
                    serde_json::json!({"id":"c","name":"Claude work","provider":"claude","account_id":"claude:claude-main","availability":"running","attachment":{"host":"tmux"},"capabilities":{"focus":false,"rename":true,"resume":false,"add_to_herdr":false},"capability_reasons":{"add_to_herdr":"INTERACTIVE_ELSEWHERE: Open in another terminal"}}),
                ];
            }
            "claude-session" => {
                m.page = Page::Session("c".into());
                m.sessions = vec![
                    serde_json::json!({"id":"c","name":"Claude work","provider":"claude","account_id":"claude:claude-main","availability":"running","attachment":null,"capabilities":{"focus":false,"rename":true,"resume":false}}),
                ];
            }
            "login-codex" => {
                m.dialog = Some(Dialog::new(
                    "Complete provider login",
                    vec![
                        "https://example.invalid/device".into(),
                        "Code: ABCD-EFGH".into(),
                        "Complete login in your browser. Closing this dialog does not cancel enrollment.".into(),
                    ],
                    &["Continue in background"],
                ))
            }
            "login-claude-wait" => {
                m.dialog = Some(Dialog::new(
                    "Complete provider login",
                    vec![
                        "Complete the Claude Code login in the opened pane…".into(),
                        "This dialog closes when sign-in succeeds. Cancel only stops waiting; it does not cancel the login.".into(),
                    ],
                    &["Cancel"],
                ))
            }
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
        if name.starts_with("settings-")
            || name == "roots-compact"
            || name.starts_with("directory-")
        {
            use sessionsation_ui::{
                app::{App, Page as AppPage},
                client::Workspace,
                preferences::{Mode, Preferences},
            };
            let prefs = Preferences {
                global_root: Some("/projects/a/long/path/that/needs/a/detached/preview".into()),
                ..Preferences::default()
            };
            let mut app = App::new(
                prefs,
                "/tmp".into(),
                "fixture".into(),
                serde_json::json!({"session":"fixture","socket":"/tmp/fixture"}),
                None,
            );
            app.page = AppPage::Config;
            app.options.selected = 1;
            if name == "roots-compact" {
                app.page = AppPage::Roots;
                app.options.selected = 0;
                app.prefs.mode = Mode::PerSpace;
                app.spaces = vec![
                    Workspace {
                        workspace_id: "w1".into(),
                        label: "Alpha".into(),
                        focused: true,
                    },
                    Workspace {
                        workspace_id: "w2".into(),
                        label: "Beta".into(),
                        focused: false,
                    },
                ];
                app.prefs
                    .space_roots
                    .insert(Preferences::key("fixture", "w1"), "/short".into());
                app.prefs.space_roots.insert(
                    Preferences::key("fixture", "w2"),
                    "/projects/a/very/long/path/that/cannot/fit".into(),
                );
            }
            if name.starts_with("directory-") {
                use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
                use sessionsation_ui::{app::Target, directory::Listing, widgets::TextInput};
                app.page = AppPage::Directory(Target::Global);
                app.input = TextInput::new("/projects".into());
                app.listing = Listing {
                    path: Some("/projects".into()),
                    existing: true,
                    children: (0..36)
                        .map(|i| format!("/projects/project{i:02}").into())
                        .collect(),
                    ..Listing::default()
                };
                app.options.selected = app.rows().len() - 3;
                if name == "directory-loading" {
                    app.key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
                }
            }
            c = app.paint(&mut f, 4060, width, height);
            c.cells[..width].fill(sessionsation_ui::render::Cell::default());
        } else if name == "picker" {
            let mut app = sessionsation_ui::app::App::new(
                sessionsation_ui::preferences::Preferences::default(),
                "/tmp".into(),
                "fixture".into(),
                serde_json::json!({}),
                None,
            );
            app.page = sessionsation_ui::app::Page::Accounts;
            app.options.selected = 0;
            app.accounts = accounts.clone();
            c = app.paint(&mut f, 4060, width, height);
            c.cells[..width].fill(sessionsation_ui::render::Cell::default());
        } else {
            m.paint_at(&mut c, height - 1, 90062);
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
