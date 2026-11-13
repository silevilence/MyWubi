use crate::state::AppState;
use core_engine::{expressions::ExpressionsConfig, shortcuts::ShortcutConfig, Config};
use eframe::egui;
use std::{path::PathBuf, sync::mpsc};

type DraftKey = (PathBuf, ExpressionsConfig, ShortcutConfig);
enum Completed {
    Restored(Box<Config>),
    Exported(PathBuf),
    Cancelled,
}
type WorkResult = Result<Completed, String>;
#[derive(Default)]
pub struct BackupUi {
    worker: Option<(DraftKey, mpsc::Receiver<WorkResult>)>,
    message: String,
}
fn key(state: &AppState) -> DraftKey {
    (
        state.config_path.clone(),
        state.config.expressions.clone(),
        state.config.shortcuts.clone(),
    )
}
impl BackupUi {
    pub fn show(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        if let Some((expected, receiver)) = &self.worker {
            match receiver.try_recv() {
                Ok(result) => {
                    self.message = match result {
                        Ok(Completed::Restored(draft)) if *expected == key(state) => {
                            state.config.expressions = draft.expressions;
                            state.config.shortcuts = draft.shortcuts;
                            state.mark_dirty();
                            "备份已恢复到草稿；请检查后保存，输入法重载后应用。".into()
                        }
                        Ok(Completed::Restored(_)) => {
                            "特殊输入草稿已变化，保留当前编辑；请重新恢复备份。".into()
                        }
                        Ok(Completed::Exported(target)) => {
                            format!("已导出特殊输入草稿与资源：{}", target.display())
                        }
                        Ok(Completed::Cancelled) => "已取消；当前设置未改变。".into(),
                        Err(error) => format!("备份操作失败，当前设置未改变：{error}"),
                    };
                    self.worker = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.message = "备份工作线程已结束，当前设置未改变。".into();
                    self.worker = None;
                }
                Err(mpsc::TryRecvError::Empty) => ui
                    .ctx()
                    .request_repaint_after(std::time::Duration::from_millis(100)),
            }
        }
        ui.collapsing("特殊输入备份与恢复", |ui| {
            ui.label("包含当前草稿的六类所选资源、开关和日期规则；不包含主码表、个人词库或其他设置。恢复会替换特殊输入草稿，检查后再保存。");
            ui.horizontal_wrapped(|ui| {
                for (restore, label) in [(false,"导出特殊输入备份"),(true,"恢复备份到草稿")] {
                    if ui.add_enabled(self.worker.is_none(), egui::Button::new(label)).clicked() {
                        self.start(state, restore);
                    }
                }
            });
            if !self.message.is_empty() {ui.label(&self.message);}
            if self.worker.is_some() {ui.spinner();}
        });
    }
    fn start(&mut self, state: &AppState, restore: bool) {
        let expected = key(state);
        let path = state.config_path.clone();
        let config = state.config.clone();
        let (tx, rx) = mpsc::channel();
        self.worker = Some((expected, rx));
        self.message.clear();
        std::thread::spawn(move || {
            let dialog = rfd::FileDialog::new().add_filter("MyWubi 特殊输入备份", &["mwsi"]);
            let result = if restore {
                dialog.pick_file().map(|source| {
                    core_engine::special_backup::restore(&path, &config, &source)
                        .map(|config| Completed::Restored(Box::new(config)))
                })
            } else {
                dialog
                    .set_file_name("MyWubi-special-input.mwsi")
                    .save_file()
                    .map(|target| {
                        core_engine::special_backup::export(&path, &config, &target)
                            .map(|()| Completed::Exported(target))
                    })
            }
            .unwrap_or(Ok(Completed::Cancelled));
            let _ = tx.send(result);
        });
    }
}
