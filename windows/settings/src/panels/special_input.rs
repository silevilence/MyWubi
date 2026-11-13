use crate::state::AppState;
use core_engine::{
    expressions::ExpressionKind,
    shortcuts::{ShortcutKind, ShortcutRule},
};
use eframe::egui::{self, Ui};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Item {
    Table(ExpressionKind),
    Shortcut(usize),
}
pub struct SpecialInputUi {
    filter: String,
    selected: Item,
    detail: bool,
    pub preview: super::shortcuts::PreviewUi,
    trials: [super::expression_trial::TrialUi; 6],
    backup: super::special_backup::BackupUi,
}
impl Default for SpecialInputUi {
    fn default() -> Self {
        Self {
            filter: String::new(),
            selected: Item::Table(ExpressionKind::Emoji),
            detail: false,
            preview: Default::default(),
            trials: Default::default(),
            backup: Default::default(),
        }
    }
}
pub fn show(ui: &mut Ui, state: &mut AppState) {
    ui.heading("特殊输入");
    ui.label("选择特殊码表或快捷编码，在详情中调整；所有更改统一保存。");
    if state
        .expressions
        .poll(ui, &state.config_path, &mut state.config.expressions)
    {
        state.mark_dirty();
    }
    let mut view = std::mem::take(&mut state.special_input);
    view.backup.show(ui, state);
    let detail_id = format!("special-detail-{:?}", view.selected);
    if ui.available_width() >= 760.0 {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(220.0);
                egui::ScrollArea::vertical()
                    .id_source("special-list")
                    .show(ui, |ui| list(ui, state, &mut view));
            });
            ui.separator();
            ui.vertical(|ui| {
                ui.set_width(ui.available_width());
                egui::ScrollArea::vertical()
                    .id_source(&detail_id)
                    .show(ui, |ui| detail(ui, state, &mut view));
            });
        });
    } else if view.detail {
        if ui.button("返回项目列表").clicked() {
            view.detail = false;
        }
        egui::ScrollArea::vertical()
            .id_source(&detail_id)
            .show(ui, |ui| detail(ui, state, &mut view));
    } else {
        egui::ScrollArea::vertical()
            .id_source("special-list")
            .show(ui, |ui| list(ui, state, &mut view));
    }
    state.special_input = view;
}
fn list(ui: &mut Ui, state: &mut AppState, view: &mut SpecialInputUi) {
    ui.label("搜索类别或编码");
    ui.add(egui::TextEdit::singleline(&mut view.filter).desired_width(f32::INFINITY));
    let filter = view.filter.to_ascii_lowercase();
    ui.strong("特殊码表");
    for kind in ExpressionKind::ALL {
        if !kind.label().to_ascii_lowercase().contains(&filter) {
            continue;
        }
        let summary =
            state
                .expressions
                .summary(&state.config_path, &state.config.expressions, kind);
        if ui
            .selectable_label(
                view.selected == Item::Table(kind),
                format!("{}\n{summary}", kind.label()),
            )
            .clicked()
        {
            view.selected = Item::Table(kind);
            view.detail = true;
        }
    }
    ui.separator();
    ui.strong("快捷输入");
    let mut seen = std::collections::BTreeSet::new();
    for (index, rule) in state.config.shortcuts.rules.iter().enumerate() {
        let code = rule.code.to_ascii_lowercase();
        if !seen.insert(code.clone()) || !code.contains(&filter) {
            continue;
        }
        let rows: Vec<_> = state
            .config
            .shortcuts
            .rules
            .iter()
            .filter(|r| r.code.eq_ignore_ascii_case(&code))
            .collect();
        let count = rows.iter().filter(|r| r.enabled).count();
        if ui
            .selectable_label(
                view.selected == Item::Shortcut(index),
                format!("{code}\n{} 个格式 · {count} 个用户启用", rows.len()),
            )
            .clicked()
        {
            view.selected = Item::Shortcut(index);
            view.detail = true;
        }
    }
    if ui
        .add_enabled(
            state.config.shortcuts.rules.len() < 1024,
            egui::Button::new("新增快捷编码"),
        )
        .clicked()
    {
        let mut code = "jjad".to_owned();
        let mut suffix = 0;
        while state
            .config
            .shortcuts
            .rules
            .iter()
            .any(|r| r.code.eq_ignore_ascii_case(&code))
        {
            code = format!(
                "jjad{}{}{}",
                char::from(b'a' + (suffix / 676 % 26) as u8),
                char::from(b'a' + (suffix / 26 % 26) as u8),
                char::from(b'a' + (suffix % 26) as u8)
            );
            suffix += 1;
        }
        view.selected = Item::Shortcut(state.config.shortcuts.rules.len());
        view.detail = true;
        state.config.shortcuts.rules.push(ShortcutRule {
            code,
            position: 2,
            format: "yyyy年MM月dd日".into(),
            kind: ShortcutKind::DateTime,
            enabled: true,
        });
        state.mark_dirty();
    }
}
fn detail(ui: &mut Ui, state: &mut AppState, view: &mut SpecialInputUi) {
    match view.selected {
        Item::Table(kind) => {
            ui.heading(kind.label());
            if state
                .expressions
                .show(ui, &state.config_path, &mut state.config.expressions, kind)
            {
                state.mark_dirty();
            }
            let index = ExpressionKind::ALL.iter().position(|k| *k == kind).unwrap();
            view.trials[index].show(ui, &state.config_path, &state.config, kind);
        }
        Item::Shortcut(index) => {
            if index >= state.config.shortcuts.rules.len() {
                ui.label("请选择或新增一个快捷编码。");
                view.detail = false;
                return;
            }
            if let Some(next) = super::shortcuts::show(ui, state, index, &mut view.preview) {
                view.selected = Item::Shortcut(next);
            } else {
                view.detail = false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn desktop_and_narrow_frames_keep_the_same_invalid_rule_draft_and_selection() {
        // Missing config deliberately skips AppContainer ACL preparation. No window is created.
        let path = std::env::temp_dir()
            .join(format!("si-headless-{}", std::process::id()))
            .join("missing.toml");
        let mut state = AppState::load(path, true);
        state.config.shortcuts.rules = vec![ShortcutRule {
            code: "JJAD".into(),
            position: 0,
            format: "yyyy-QQ".into(),
            kind: ShortcutKind::DateTime,
            enabled: true,
        }];
        state.special_input.selected = Item::Shortcut(0);
        state.special_input.detail = true;
        state.special_input.filter = "jj".into();
        let original = state.config.shortcuts.rules.clone();
        let ctx = egui::Context::default();
        for width in [1100.0, 480.0, 760.0] {
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 700.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| show(ui, &mut state));
                },
            );
            assert!(!output.shapes.is_empty());
            assert_eq!(state.config.shortcuts.rules, original);
            assert_eq!(state.special_input.selected, Item::Shortcut(0));
            assert_eq!(state.special_input.filter, "jj");
        }
    }
}
