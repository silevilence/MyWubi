use crate::state::AppState;
use core_engine::{
    shortcut_preview::{self, ShortcutPreview},
    shortcuts::{self, ShortcutKind, ShortcutRule},
    BehaviorProfile, Config,
};
use eframe::egui::{self, Ui};
use std::{
    path::Path,
    sync::mpsc,
    time::{Duration, Instant},
};

type PreviewResult = (String, Result<ShortcutPreview, String>);
pub struct PreviewUi {
    key: String,
    changed_at: Instant,
    done_key: Option<String>,
    worker: Option<mpsc::Receiver<PreviewResult>>,
    result: Option<Result<ShortcutPreview, String>>,
}
impl Default for PreviewUi {
    fn default() -> Self {
        Self {
            key: String::new(),
            changed_at: Instant::now(),
            done_key: None,
            worker: None,
            result: None,
        }
    }
}
impl PreviewUi {
    fn update(&mut self, ui: &mut Ui, path: &Path, config: &Config, code: &str) {
        let key = serde_json::to_string(&(path, config, code)).unwrap_or_default();
        if key != self.key {
            self.key = key;
            self.changed_at = Instant::now();
            self.result = None;
        }
        if let Some(result) = self.worker.as_ref().and_then(|rx| match rx.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Disconnected) => {
                Some((self.key.clone(), Err("预览线程结束，请重新预览".into())))
            }
            Err(mpsc::TryRecvError::Empty) => None,
        }) {
            self.worker = None;
            if result.0 == self.key {
                self.done_key = Some(result.0);
                self.result = Some(result.1);
            }
        }
        if self.worker.is_none()
            && self.done_key.as_ref() != Some(&self.key)
            && self.changed_at.elapsed() >= Duration::from_millis(250)
        {
            let path = path.to_owned();
            let config = config.clone();
            let code = code.to_owned();
            let key = self.key.clone();
            let (tx, rx) = mpsc::channel();
            self.worker = Some(rx);
            std::thread::spawn(move || {
                let result = shortcut_preview::build(
                    path,
                    config,
                    &code,
                    shortcut_preview::example_time(),
                    BehaviorProfile::LegacyV1,
                );
                let _ = tx.send((key, result));
            });
        }
        if self.result.is_none() {
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
    }
    fn show(&mut self, ui: &mut Ui, indices: &[usize]) {
        ui.separator();
        ui.strong("候选预览 · 当前设置草稿");
        ui.small("固定示例：2026-11-12 07:08:09，Asia/Shanghai（UTC+08:00）。预览不会写入宿主。");
        match &self.result {
            None => {
                ui.spinner();
                ui.label("正在读取所选资源并计算预览…");
            }
            Some(Err(error)) => {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            Some(Ok(preview)) => {
                for &index in indices {
                    if let Some(status) = preview.states.get(index) {
                        ui.label(format!(
                            "格式 {}：{}",
                            index + 1,
                            status.reason.as_deref().unwrap_or(if status.effective {
                                "当前可用"
                            } else {
                                "用户未启用"
                            })
                        ));
                    }
                }
                ui.label(format!(
                    "共 {} 项 · 本轮每页最多 {} 项；全局位置跨页连续编号。",
                    preview.total, preview.effective_capacity
                ));
                if preview.effective_capacity == 9 {
                    ui.small("动态命中时配置容量 10 也按 9 分页，全局第 10 项在第二页按 1 选择。");
                }
                for warning in &preview.warnings {
                    ui.colored_label(ui.visuals().warn_fg_color, warning);
                }
                if preview.rows.len() < preview.total {
                    ui.small("展示前 256 项和全部日期规则；省略的普通候选保留其原位置。");
                }
                for row in &preview.rows {
                    ui.horizontal_wrapped(|ui| {
                        ui.strong(format!("{}.", row.actual));
                        ui.label(&row.candidate.text);
                        ui.small(format!(
                            "{} · 目标 {} · 第 {} 页第 {} 项",
                            row.source,
                            row.target
                                .map(|p| p.to_string())
                                .unwrap_or_else(|| "—".into()),
                            row.page,
                            row.slot
                        ));
                    });
                }
                ui.collapsing("本次预览的资源快照", |ui| {
                    for resource in &preview.resources {
                        ui.small(resource);
                    }
                });
            }
        }
        if ui
            .add_enabled(
                self.worker.is_none(),
                egui::Button::new("重新读取资源并预览"),
            )
            .clicked()
        {
            self.done_key = None;
            self.result = None;
        }
    }
}
fn next_position(rules: &[ShortcutRule], code: &str) -> u32 {
    if let Some(position) = rules
        .iter()
        .filter(|r| r.code.eq_ignore_ascii_case(code))
        .map(|r| r.position)
        .max()
        .and_then(|p| p.checked_add(1))
        .filter(|p| *p <= shortcuts::MAX_POSITION)
    {
        return position;
    }
    (1..=shortcuts::MAX_POSITION)
        .find(|position| {
            !rules
                .iter()
                .any(|r| r.code.eq_ignore_ascii_case(code) && r.position == *position)
        })
        .unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn copied_format_follows_existing_global_positions_before_using_a_gap() {
        let mut rules = vec![ShortcutRule {
            code: "JJAD".into(),
            position: 2,
            format: "yyyy".into(),
            kind: ShortcutKind::DateTime,
            enabled: true,
        }];
        assert_eq!(next_position(&rules, "jjad"), 3);
        rules[0].position = 10000;
        assert_eq!(next_position(&rules, "jjad"), 1);
    }
    #[test]
    fn late_preview_of_an_older_draft_cannot_publish_into_the_current_editor() {
        let mut preview = PreviewUi::default();
        let ctx = egui::Context::default();
        let (tx, rx) = mpsc::channel();
        preview.worker = Some(rx);
        tx.send(("old draft".into(), Err("stale error".into())))
            .unwrap();
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                preview.update(ui, Path::new("current.toml"), &Config::default(), "jjad")
            });
        });
        assert!(preview.result.is_none());
        assert!(preview.worker.is_none());
        assert!(preview.done_key.is_none());
    }
}
pub fn show(
    ui: &mut Ui,
    state: &mut AppState,
    selected: usize,
    preview: &mut PreviewUi,
) -> Option<usize> {
    let mut code = state.config.shortcuts.rules[selected].code.clone();
    let indices: Vec<_> = state
        .config
        .shortcuts
        .rules
        .iter()
        .enumerate()
        .filter(|(_, r)| r.code.eq_ignore_ascii_case(&code))
        .map(|(i, _)| i)
        .collect();
    let mut changed = false;
    let mut remove = None;
    let mut copy = None;
    ui.heading("日期时间快捷输入");
    ui.label("编码（1～32 个英文字母）");
    if ui.text_edit_singleline(&mut code).changed() {
        for &index in &indices {
            state.config.shortcuts.rules[index].code = code.clone();
        }
        changed = true;
    }
    ui.small(format!("保存后的规范编码：{}", code.to_ascii_lowercase()));
    ui.label("全局候选位置从 1 开始；下一页接着编号。候选不足时靠后排列，不留空项。");
    let error = shortcuts::validate_structure(&state.config.shortcuts.rules).err();
    let can_add = indices.len() < 32 && state.config.shortcuts.rules.len() < 1024;
    for &index in &indices {
        let rule = &mut state.config.shortcuts.rules[index];
        ui.push_id(index, |ui| {
            ui.group(|ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.strong(format!("格式 {} · 日期时间", index + 1));
                    changed |= ui.checkbox(&mut rule.enabled, "用户启用").changed();
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label("全局候选位置");
                    changed |= ui
                        .add(egui::DragValue::new(&mut rule.position).speed(1))
                        .changed();
                });
                ui.label("日期时间格式");
                changed |= ui
                    .add(egui::TextEdit::singleline(&mut rule.format).desired_width(f32::INFINITY))
                    .changed();
                if let Ok(parts) = core_engine::shortcut_format::compile(&rule.format) {
                    if let Ok(text) = core_engine::shortcut_time::render(
                        &parts,
                        &shortcut_preview::example_time(),
                    ) {
                        ui.label(format!("示例输出：{text}"));
                    }
                }
                if let Some(error) = error.as_ref().filter(|e| e.rule == Some(index)) {
                    ui.colored_label(
                        ui.visuals().error_fg_color,
                        format!("{}：{}", error.field_label(), error.message),
                    );
                }
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .add_enabled(can_add, egui::Button::new("复制格式"))
                        .clicked()
                    {
                        copy = Some(rule.clone());
                    }
                    if ui.button("删除格式").clicked() {
                        remove = Some(index);
                    }
                });
            });
        });
    }
    if ui
        .add_enabled(
            indices.len() < 32 && state.config.shortcuts.rules.len() < 1024,
            egui::Button::new("新增格式"),
        )
        .clicked()
    {
        copy = Some(ShortcutRule {
            code: code.clone(),
            position: 1,
            kind: ShortcutKind::DateTime,
            format: "yyyy-MM-dd".into(),
            enabled: true,
        });
    }
    if let Some(mut rule) = copy {
        rule.position = next_position(&state.config.shortcuts.rules, &code);
        state.config.shortcuts.rules.push(rule);
        changed = true;
    }
    if let Some(index) = remove {
        state.config.shortcuts.rules.remove(index);
        changed = true;
    }
    if let Err(error) = shortcuts::validate_structure(&state.config.shortcuts.rules) {
        ui.colored_label(
            ui.visuals().error_fg_color,
            format!("保存前请修正：{error}"),
        );
    }
    ui.collapsing("格式标记与转义",|ui| {
        ui.label("yyyy 年 · MM 月 · dd 日 · HH 时（24 小时）· mm 分 · ss 秒");
        ui.label("中文和标点直接输入；英文字面量放在单引号内，如 yyyy-MM-dd'T'HH:mm:ss；两个单引号表示单引号。");
    });
    if changed {
        state.mark_dirty();
    }
    preview.update(ui, &state.config_path, &state.config, &code);
    if remove.is_none() {
        preview.show(ui, &indices);
    }
    state
        .config
        .shortcuts
        .rules
        .iter()
        .position(|r| r.code.eq_ignore_ascii_case(&code))
}
