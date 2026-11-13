//! Shared-rule draft editor; the unified special-input panel also uses this component.
use crate::state::AppState;
use core_engine::shortcuts::{self, ShortcutKind, ShortcutRule};
use eframe::egui::{self, Ui};

pub fn show(ui: &mut Ui, state: &mut AppState) {
    ui.heading("日期时间快捷输入");
    ui.label("编码只接受英文字母；保存后显示小写。位置是整轮候选中的全局位置，从 1 开始。");
    let environment = (&state.tables.editor.config).into();
    let mut changed = false;
    let mut remove = None;
    for (index, rule) in state.config.shortcuts.rules.iter_mut().enumerate() {
        ui.push_id(index, |ui| {
            ui.group(|ui| {
                ui.horizontal_wrapped(|ui| {
                    changed |= ui.checkbox(&mut rule.enabled, "用户启用").changed();
                    ui.label("编码");
                    changed |= ui
                        .add(egui::TextEdit::singleline(&mut rule.code).desired_width(100.0))
                        .changed();
                    ui.label("全局位置");
                    changed |= ui
                        .add(egui::DragValue::new(&mut rule.position).speed(1))
                        .changed();
                    if ui.button("删除").clicked() {
                        remove = Some(index);
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label("日期时间格式");
                    changed |= ui.text_edit_singleline(&mut rule.format).changed();
                });
                if state.tables.editor.load_error.is_some() {
                    ui.label("主表不可用，尚不能判断键位兼容性");
                } else {
                    let status = shortcuts::availability(rule, environment);
                    ui.small(status.reason.as_deref().unwrap_or(if status.effective {
                        "当前可用"
                    } else {
                        "用户未启用"
                    }));
                }
            });
        });
    }
    if let Some(index) = remove {
        state.config.shortcuts.rules.remove(index);
        changed = true;
    }
    if ui.button("新增格式").clicked() {
        let position = (1..=shortcuts::MAX_POSITION)
            .find(|position| {
                !state.config.shortcuts.rules.iter().any(|rule| {
                    rule.code.eq_ignore_ascii_case("jjad") && rule.position == *position
                })
            })
            .unwrap_or(1);
        state.config.shortcuts.rules.push(ShortcutRule {
            code: "jjad".into(),
            position,
            kind: ShortcutKind::DateTime,
            format: "yyyy年MM月dd日".into(),
            enabled: true,
        });
        changed = true;
    }
    if let Err(error) = shortcuts::validate_structure(&state.config.shortcuts.rules) {
        ui.colored_label(ui.visuals().error_fg_color, error.to_string());
    }
    ui.collapsing("格式帮助", |ui| {
        ui.label("yyyy 年 · MM 月 · dd 日 · HH 时（24 小时）· mm 分 · ss 秒");
        ui.label("英文字面量放在单引号内，如 yyyy-MM-dd'T'HH:mm:ss；两个单引号表示一个单引号。");
    });
    if changed {
        state.mark_dirty();
    }
}
