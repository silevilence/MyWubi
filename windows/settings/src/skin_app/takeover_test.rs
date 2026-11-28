use super::*;
use egui::{Event, FullOutput, Pos2, RawInput, Rect};

fn frame(app: &mut SkinApp, ctx: &egui::Context, events: Vec<Event>) -> FullOutput {
    ctx.run(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1180., 800.))),
            modifiers: events
                .iter()
                .find_map(|event| match event {
                    Event::Key { modifiers, .. } => Some(*modifiers),
                    _ => None,
                })
                .unwrap_or_default(),
            events,
            ..Default::default()
        },
        |ctx| app.show(ctx),
    )
}
fn label(output: &FullOutput, text: &str) -> Pos2 {
    output
        .shapes
        .iter()
        .find_map(|shape| {
            if let egui::Shape::Text(t) = &shape.shape {
                if t.galley.job.text.contains(text) {
                    return Some(t.visual_bounding_rect().center());
                }
            }
            None
        })
        .unwrap_or_else(|| panic!("Missing editor label {text}"))
}
#[test]
fn existing_window_observes_both_switches_and_ctrl_s_is_readonly() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("config.toml");
    core_engine::Config::default().save(&path).unwrap();
    let original = std::fs::read(&path).unwrap();
    let mut editor = SkinEditor::default();
    editor.create("jade").unwrap();
    let title = editor.title().to_owned();
    let mut app = SkinApp {
        controls: crate::skin_mcp::control::Controls::register(&path).unwrap(),
        editor,
        config: Default::default(),
        path: path.clone(),
        close_confirm: false,
        allow_close: false,
        error: None,
        mcp: None,
    };
    let ctx = egui::Context::default();
    toggle_mcp(&path, true).unwrap();
    let first = frame(&mut app, &ctx, vec![]);
    label(&first, "等待 Agent 接管");
    let restore = label(&first, "关闭 MCP，恢复手动编辑");
    assert_eq!(app.editor.title(), title);
    assert!(crate::skin_mcp::control::enabled(&path));
    frame(
        &mut app,
        &ctx,
        vec![Event::Key {
            key: egui::Key::S,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::CTRL,
        }],
    );
    assert!(!app.editor.is_busy());
    for down in [true, false] {
        frame(
            &mut app,
            &ctx,
            vec![
                Event::PointerMoved(restore),
                Event::PointerButton {
                    pos: restore,
                    button: egui::PointerButton::Primary,
                    pressed: down,
                    modifiers: Default::default(),
                },
            ],
        );
    }
    assert!(app.mcp.is_none());
    toggle_mcp(&path, true).unwrap();
    frame(&mut app, &ctx, vec![]);
    assert!(app.mcp.is_some());
    toggle_mcp(&path, false).unwrap();
    frame(&mut app, &ctx, vec![]);
    assert!(app.mcp.is_none());
    assert_eq!(app.editor.title(), title);
    assert_eq!(std::fs::read(path).unwrap(), original);
}
