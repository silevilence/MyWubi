use core_engine::{
    candidates::{ExpressionInput, ExpressionPhase},
    expression_trial::{self, TrialAction, TrialResult},
    expressions::ExpressionKind,
    Config,
};
use eframe::egui::{self, Ui};
use std::{
    path::Path,
    sync::mpsc,
    time::{Duration, Instant},
};
pub struct TrialUi {
    input: ExpressionInput,
    text: String,
    actions: Vec<TrialAction>,
    key: String,
    started: Option<String>,
    changed: Instant,
    worker: Option<mpsc::Receiver<(String, Result<TrialResult, String>)>>,
    result: Option<Result<TrialResult, String>>,
}
impl Default for TrialUi {
    fn default() -> Self {
        Self {
            input: ExpressionInput::Words,
            text: String::new(),
            actions: vec![],
            key: String::new(),
            started: None,
            changed: Instant::now(),
            worker: None,
            result: None,
        }
    }
}
impl TrialUi {
    fn accept_text(&mut self) {
        if !self.text.is_empty() {
            self.actions
                .push(TrialAction::Input(std::mem::take(&mut self.text)));
        }
    }
    pub fn show(&mut self, ui: &mut Ui, path: &Path, config: &Config, kind: ExpressionKind) {
        ui.separator();
        ui.strong("检索试用");
        ui.small("使用当前草稿及所选资源，只在设置中预览，不写入宿主。");
        let empty_query = self.text.is_empty()
            && (self.actions.is_empty()
                || self.result.as_ref().is_some_and(|result| {
                    result.as_ref().is_ok_and(|result| {
                        result.view.spelling.is_empty()
                            && result.view.expression.as_ref().is_some_and(|query| {
                                query.phase == ExpressionPhase::Editing && query.text.is_empty()
                            })
                    })
                }));
        let previous_input = self.input;
        ui.add_enabled_ui(empty_query, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.selectable_value(&mut self.input, ExpressionInput::Words, "主码表组词");
                ui.selectable_value(&mut self.input, ExpressionInput::Literal, "直接输入别名");
            });
        });
        if self.input != previous_input {
            // A fully erased query may still have a replay history in the old mode.
            self.actions.clear();
        }
        if !empty_query {
            ui.small("切换查询方式前请先清空。");
        }
        let editing = self
            .result
            .as_ref()
            .and_then(|r| r.as_ref().ok())
            .is_none_or(|r| {
                r.selected.is_none()
                    && r.view
                        .expression
                        .as_ref()
                        .is_none_or(|q| q.phase == ExpressionPhase::Editing)
            });
        ui.label(if self.input == ExpressionInput::Literal {
            "别名（如 shi，数字也是查询文本）"
        } else {
            "主码表编码（选择下方候选组成查询）"
        });
        ui.add_enabled(
            editing,
            egui::TextEdit::singleline(&mut self.text).desired_width(f32::INFINITY),
        );
        let mut actions = self.actions.clone();
        if !self.text.is_empty() {
            actions.push(TrialAction::Input(self.text.clone()));
        }
        let key =
            serde_json::to_string(&(path, config, kind, self.input, &actions)).unwrap_or_default();
        if self.key != key {
            self.key = key;
            self.result = None;
            self.started = None;
            self.changed = Instant::now();
        }
        if let Some((key, result)) = self.worker.as_ref().and_then(|rx| match rx.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Disconnected) => {
                Some((self.key.clone(), Err("试用线程结束，请清空后重试".into())))
            }
            Err(mpsc::TryRecvError::Empty) => None,
        }) {
            self.worker = None;
            if key == self.key {
                self.started = Some(key);
                self.result = Some(result);
            }
        }
        if self.worker.is_none()
            && self.started.as_ref() != Some(&self.key)
            && self.changed.elapsed() >= Duration::from_millis(250)
        {
            let path = path.to_owned();
            let config = config.clone();
            let input = self.input;
            let key = self.key.clone();
            self.started = Some(key.clone());
            let (tx, rx) = mpsc::channel();
            self.worker = Some(rx);
            std::thread::spawn(move || {
                let result = expression_trial::build(path, config, kind, input, &actions);
                let _ = tx.send((key, result));
            });
        }
        if self.result.is_none() {
            ui.spinner();
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
        let mut select = None;
        match &self.result {
            Some(Err(error)) => {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            Some(Ok(result)) => {
                if let Some(query) = &result.view.expression {
                    ui.label(format!("查询：{}", query.text));
                    if let Some(message) = &query.message {
                        ui.label(message);
                    }
                }
                if let Some(text) = &result.selected {
                    ui.strong(format!("选中结果：{text}"));
                }
                if result.window.total > result.window.candidates.len() {
                    ui.small(format!(
                        "共 {} 项，试用展示前 256 项。",
                        result.window.total
                    ));
                }
                ui.horizontal_wrapped(|ui| {
                    for candidate in &result.window.candidates {
                        if ui.button(&candidate.text).clicked() {
                            select = Some(candidate.id.0);
                        }
                    }
                });
                ui.collapsing("试用资源", |ui| {
                    for resource in &result.resources {
                        ui.small(resource);
                    }
                });
            }
            None => {}
        }
        if let Some(index) = select {
            self.accept_text();
            self.actions.push(TrialAction::Select(index));
        }
        if ui
            .add_enabled(
                editing
                    && self.result.as_ref().is_some_and(|result| {
                        result.as_ref().is_ok_and(|result| {
                            !result.view.spelling.is_empty()
                                || result
                                    .view
                                    .expression
                                    .as_ref()
                                    .is_some_and(|query| !query.text.is_empty())
                        })
                    }),
                egui::Button::new("查询退格"),
            )
            .clicked()
        {
            self.accept_text();
            self.actions.push(TrialAction::Backspace);
        }
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    editing && self.result.as_ref().is_some_and(|r| r.is_ok()),
                    egui::Button::new("查找"),
                )
                .clicked()
            {
                self.accept_text();
                self.actions.push(TrialAction::Find);
            }
            if ui
                .add_enabled(
                    !editing
                        && self
                            .result
                            .as_ref()
                            .is_some_and(|r| r.as_ref().is_ok_and(|r| r.selected.is_none())),
                    egui::Button::new("修改查询"),
                )
                .clicked()
            {
                self.actions.push(TrialAction::Edit);
            }
            if ui.button("清空试用").clicked() {
                self.text.clear();
                self.actions.clear();
                self.started = None;
                self.result = None;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returning_to_completed_input_schedules_a_new_trial() {
        let mut trial = TrialUi::default();
        let ctx = egui::Context::default();
        let frame = |trial: &mut TrialUi| {
            let _ = ctx.run(Default::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    trial.show(
                        ui,
                        Path::new("missing.toml"),
                        &Config::default(),
                        ExpressionKind::Kana,
                    );
                });
            });
        };
        trial.text = "shi".into();
        frame(&mut trial);
        trial.started = Some(trial.key.clone());
        trial.result = Some(Err("previous result".into()));
        trial.text.push('x');
        frame(&mut trial);
        trial.text.pop();
        frame(&mut trial);
        trial.changed = Instant::now() - Duration::from_secs(1);
        frame(&mut trial);
        assert!(
            trial.worker.is_some(),
            "the discarded result must be recomputed"
        );
    }
}
