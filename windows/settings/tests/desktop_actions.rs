use core_engine::Config;
use settings::{
    automation::{self, Action, Application, Request},
    skin_editor::SkinEditor,
    state::AppState,
};
use std::path::{Path, PathBuf};

fn fixture(root: &Path) -> PathBuf {
    let table = root.join("test.dict");
    std::fs::write(&table, "---\ndisplay_name: Test\ntype: shape\ncharset: abcdefghijklmnopqrstuvwxyz\nmax_code_len: 4\n---\na\t工\t1\n").unwrap();
    let mut config = Config::default();
    config.dictionary.system_table = table;
    config.dictionary.user_table = root.join("user.dict");
    assert!(core_engine::read_table(&config.dictionary.system_table).is_ok());
    let path = root.join("config.toml");
    config.save(&path).unwrap();
    path
}

#[test]
fn settings_actions_save_reload_and_preserve_navigation() {
    let directory = tempfile::tempdir().unwrap();
    let config = fixture(directory.path());
    let report = automation::execute(
        Request {
            config: config.clone(),
            actions: vec![
                Action::Navigate {
                    page: "appearance".into(),
                },
                Action::Set {
                    field: "basic.candidate_count".into(),
                    value: 7.into(),
                },
                Action::Save,
                Action::Reload { discard: false },
            ],
        },
        Application::Settings,
    );
    assert!(report.ok, "{:?}", report.error);
    assert_eq!(Config::load(&config).unwrap().basic.candidate_count, 7);
    assert_eq!(report.results.last().unwrap()["page"], "Appearance");
    assert_eq!(report.results.last().unwrap()["dirty"], false);
}

#[test]
fn invalid_values_and_unknown_fields_do_not_mutate_a_session() {
    let directory = tempfile::tempdir().unwrap();
    let config = fixture(directory.path());
    let mut state = AppState::load_isolated(config.clone());
    let before = std::fs::read(config).unwrap();
    assert!(automation::set_field(&mut state, "basic.candidate_count", 0.into()).is_err());
    assert!(automation::set_field(&mut state, "basic.candidate_typo", 5.into()).is_err());
    assert!(!state.has_unsaved_changes());
    assert_eq!(std::fs::read(&state.config_path).unwrap(), before);
}

#[test]
fn dirty_reload_requires_explicit_discard_and_keeps_page() {
    let directory = tempfile::tempdir().unwrap();
    let mut state = AppState::load_isolated(fixture(directory.path()));
    state.active_panel = settings::state::Panel::Dictionary;
    automation::set_field(&mut state, "basic.candidate_count", 9.into()).unwrap();
    assert!(state.reload(false).is_err());
    assert_eq!(state.config.basic.candidate_count, 9);
    state.reload(true).unwrap();
    assert!(!state.has_unsaved_changes());
    assert_eq!(state.active_panel, settings::state::Panel::Dictionary);
}

#[test]
fn another_process_change_cannot_be_overwritten_by_settings() {
    let directory = tempfile::tempdir().unwrap();
    let config = fixture(directory.path());
    let mut state = AppState::load_isolated(config.clone());
    automation::set_field(&mut state, "basic.candidate_count", 9.into()).unwrap();
    let mut external = Config::load(&config).unwrap();
    external.appearance.font_size = 22;
    external.save(&config).unwrap();
    assert!(!settings::save::save(&mut state));
    assert!(state.status_msg.as_ref().unwrap().contains("其他窗口"));
    assert_eq!(Config::load(&config).unwrap().appearance.font_size, 22);
    assert!(state.has_unsaved_changes());
}

#[test]
fn skin_edit_export_reopen_and_apply_preserve_other_settings() {
    let directory = tempfile::tempdir().unwrap();
    let config = fixture(directory.path());
    let output = directory.path().join("new.mywubi-skin");
    let mut editor = SkinEditor::default();
    editor.create("jade").unwrap();
    editor
        .set_parameter("windows", "jade", false, "font_size", 22.into())
        .unwrap();
    editor.export_file(&output).unwrap();
    assert!(!editor.is_dirty());
    let mut latest = Config::load(&config).unwrap();
    latest.basic.candidate_count = 8;
    latest.save(&config).unwrap();
    editor.apply(&config).unwrap();
    let saved = Config::load(&config).unwrap();
    assert_eq!(saved.basic.candidate_count, 8);
    assert_eq!(saved.skins.windows.selection.unwrap().component, "jade");
    let draft = core_engine::skin::editor::Draft::open(&output).unwrap();
    let file = std::str::from_utf8(&draft.package.files["windows/appearance.toml"]).unwrap();
    let appearance: core_engine::skin::parameters::Appearance = toml::from_str(file).unwrap();
    assert_eq!(
        appearance.variants["light"]["font_size"].as_integer(),
        Some(22)
    );
}

#[test]
fn skin_dirty_replacement_and_overwrite_are_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("skin.mywubi-skin");
    let mut editor = SkinEditor::default();
    editor.create("plain").unwrap();
    assert!(editor.create("jade").is_err());
    assert!(editor
        .set_parameter("windows", "plain", false, "font_size", 500.into())
        .is_err());
    editor.export_file(&output).unwrap();
    let original = std::fs::read(&output).unwrap();
    editor
        .set_parameter("windows", "plain", false, "font_size", 24.into())
        .unwrap();
    assert!(editor.export_file(&output).is_err());
    assert_eq!(std::fs::read(output).unwrap(), original);
}

#[test]
fn both_executables_support_file_automation_without_a_window() {
    let directory = tempfile::tempdir().unwrap();
    let config = fixture(directory.path());
    for (index, exe, actions) in [
        (
            0,
            env!("CARGO_BIN_EXE_settings"),
            serde_json::json!([{"action":"set","field":"appearance.font_size","value":20},{"action":"save"}]),
        ),
        (
            1,
            env!("CARGO_BIN_EXE_skin_editor"),
            serde_json::json!([{"action":"skin_new","preset":"contrast"},{"action":"skin_export","path":directory.path().join("cli.mywubi-skin")}]),
        ),
    ] {
        let request = directory.path().join(format!("request-{index}.json"));
        let report = directory.path().join(format!("report-{index}.json"));
        std::fs::write(
            &request,
            serde_json::to_vec(&serde_json::json!({"config":config,"actions":actions})).unwrap(),
        )
        .unwrap();
        let status = std::process::Command::new(exe)
            .arg("--automation")
            .arg(&request)
            .arg("--output")
            .arg(&report)
            .status()
            .unwrap();
        let report: serde_json::Value =
            serde_json::from_slice(&std::fs::read(report).unwrap()).unwrap();
        assert!(status.success(), "{report}");
        assert_eq!(report["ok"], true);
    }
}

#[test]
fn actual_commit_dropdown_writes_selection_and_marks_dirty() {
    use eframe::egui::{self, epaint::Shape};
    let directory = tempfile::tempdir().unwrap();
    let mut state = AppState::load_isolated(fixture(directory.path()));
    let ctx = egui::Context::default();
    settings::ui_style::install(&ctx);
    settings::fonts::load_chinese_fonts(&ctx);
    let mut render = |events| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100.0, 900.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default()
                    .show(ctx, |ui| settings::panels::basic::show(ui, &mut state));
            },
        )
    };
    fn text_position(output: &egui::FullOutput, label: &str) -> egui::Pos2 {
        output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                Shape::Text(text) if text.galley.job.text == label => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("Missing visible control: {label}"))
    }
    fn click(pos: egui::Pos2) -> Vec<egui::Event> {
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
        ]
    }
    let _ = render(vec![]);
    let output = render(vec![]);
    let _ = render(click(text_position(&output, "空格首选上屏")));
    let opened = render(vec![]);
    let _ = render(click(text_position(&opened, "回车上屏")));
    assert_eq!(
        state.config.basic.commit_mode,
        core_engine::config::CommitMode::EnterCommit
    );
    assert!(state.has_unsaved_changes());
}

#[test]
fn unfinished_text_survives_frames_and_external_changes_refresh_it() {
    use eframe::egui;
    let ctx = egui::Context::default();
    let mut text = String::new();
    let mut edit_id = egui::Id::NULL;
    let mut render = |events, current| {
        let _ = ctx.run(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let (value, response) = settings::ui_style::text_buffer(ui, "color", current);
                    text = value;
                    edit_id = response.id;
                    response.request_focus();
                });
            },
        );
        text.clone()
    };
    render(vec![], "#FFFFFF");
    let edited = render(
        vec![
            egui::Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers {
                    ctrl: true,
                    command: true,
                    ..Default::default()
                },
            },
            egui::Event::Text("#1".into()),
        ],
        "#FFFFFF",
    );
    assert_eq!(edited, "#1");
    assert_eq!(render(vec![], "#FFFFFF"), "#1");
    assert_eq!(render(vec![], "#123456"), "#123456");
}
