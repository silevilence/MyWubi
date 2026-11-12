//! Settings resource work runs off the egui thread; selections remain unsaved drafts.
use core_engine::{
    expression_store::{self, ResourceInfo},
    expressions::*,
};
use eframe::egui;
use std::{
    path::{Path, PathBuf},
    sync::mpsc,
    time::Duration,
};

type Key = (PathBuf, Vec<Option<PathBuf>>);
fn key(path: &Path, config: &ExpressionsConfig) -> Key {
    (
        path.to_owned(),
        ExpressionKind::ALL
            .iter()
            .map(|kind| config.selection(*kind).resource.clone())
            .collect(),
    )
}
struct Loaded {
    entries: [Vec<ResourceInfo>; 6],
    imported: Option<(ExpressionKind, PathBuf)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completed_import_cannot_overwrite_a_reloaded_selection() {
        let path = Path::new("config.toml");
        let mut config = ExpressionsConfig::default();
        let mut catalog = ExpressionCatalog {
            key: Some(key(path, &config)),
            ..Default::default()
        };
        config.emoji.resource = Some("newer-choice.mwe".into());
        let changed = catalog.accept(
            path,
            &mut config,
            Loaded {
                entries: Default::default(),
                imported: Some((ExpressionKind::Emoji, "finished-import.mwe".into())),
            },
        );
        assert!(!changed);
        assert_eq!(config.emoji.resource, Some("newer-choice.mwe".into()));
        assert!(catalog.key.is_none());
    }
}
#[derive(Default)]
pub struct ExpressionCatalog {
    preview: crate::expression_preview::ExpressionPreview,
    entries: [Vec<ResourceInfo>; 6],
    key: Option<Key>,
    worker: Option<mpsc::Receiver<Result<Loaded, String>>>,
    message: Option<String>,
}

impl ExpressionCatalog {
    fn accept(&mut self, path: &Path, config: &mut ExpressionsConfig, loaded: Loaded) -> bool {
        let current = key(path, config);
        if self.key.as_ref() != Some(&current) {
            self.key = None;
            self.message =
                Some("设置已变化，保留当前选择；已导入文件可重新从词库列表选择。".into());
            return false;
        }
        self.entries = loaded.entries;
        if let Some((kind, resource)) = loaded.imported {
            config.selection_mut(kind).resource = Some(resource);
            self.message = Some("已导入并选中，点击底部“保存”生效。".into());
            self.key = Some(key(path, config));
            true
        } else {
            false
        }
    }
    fn start(&mut self, path: &Path, config: &ExpressionsConfig, import: Option<ExpressionKind>) {
        if self.worker.is_some() {
            return;
        }
        let path = path.to_owned();
        let mut config = config.clone();
        self.key = Some(key(&path, &config));
        let (tx, rx) = mpsc::channel();
        self.worker = Some(rx);
        self.message = None;
        std::thread::spawn(move || {
            let result = (|| {
                let imported = if let Some(kind) = import {
                    if let Some(source) = rfd::FileDialog::new()
                        .add_filter("MyWubi 表情词库", &["mwe"])
                        .pick_file()
                    {
                        let resource = expression_store::import(&path, kind, &source)?;
                        let resource_path = resource
                            .path
                            .strip_prefix(path.parent().unwrap_or(Path::new(".")))
                            .unwrap_or(&resource.path)
                            .to_owned();
                        config.selection_mut(kind).resource = Some(resource_path.clone());
                        Some((kind, resource_path))
                    } else {
                        None
                    }
                } else {
                    None
                };
                Ok(Loaded {
                    entries: [
                        expression_store::list(&path, ExpressionKind::Emoji, &config.emoji)?,
                        expression_store::list(&path, ExpressionKind::Kaomoji, &config.kaomoji)?,
                        expression_store::list(&path, ExpressionKind::Kana, &config.kana)?,
                        expression_store::list(&path, ExpressionKind::Math, &config.math)?,
                        expression_store::list(
                            &path,
                            ExpressionKind::Punctuation,
                            &config.punctuation,
                        )?,
                        expression_store::list(&path, ExpressionKind::Symbols, &config.symbols)?,
                    ],
                    imported,
                })
            })();
            let _ = tx.send(result);
        });
    }

    pub fn show(&mut self, ui: &mut egui::Ui, path: &Path, config: &mut ExpressionsConfig) -> bool {
        let mut changed = false;
        let result = self
            .worker
            .as_ref()
            .and_then(|worker| match worker.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some(Err("词库检查线程意外结束，请重试".into()))
                }
                Err(mpsc::TryRecvError::Empty) => None,
            });
        if let Some(result) = result {
            self.worker = None;
            match result {
                Ok(loaded) => changed = self.accept(path, config, loaded),
                Err(error) => self.message = Some(format!("操作失败：{error}；原选择与配置保留。")),
            }
        }
        let key = key(path, config);
        if self.key.as_ref() != Some(&key) && self.worker.is_none() {
            self.start(path, config, None);
        }
        if self.worker.is_some() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("正在检查词库…");
            });
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
        if let Some(message) = &self.message {
            ui.label(message);
        }
        let mut import = None;
        let mut refresh = false;
        ui.add_enabled_ui(self.worker.is_none(), |ui| {
            for (index, kind) in ExpressionKind::ALL.into_iter().enumerate() {
                let selection = config.selection_mut(kind);
                ui.push_id(kind.label(), |ui| {
                    ui.strong(kind.label());
                    changed |= ui
                        .checkbox(&mut selection.append, "普通输入时追加")
                        .changed();
                    changed |= ui.checkbox(&mut selection.search, "专用模式可用").changed();
                    let selected = selection.path(path, kind);
                    let current = self.entries[index]
                        .iter()
                        .find(|info| info.path == selected);
                    let label = current
                        .map(|info| info.name.as_str())
                        .unwrap_or("尚未检查所选词库");
                    egui::ComboBox::from_id_source("resource")
                        .selected_text(label)
                        .show_ui(ui, |ui| {
                            for info in &self.entries[index] {
                                let label = format!(
                                    "{}{}{}",
                                    if info.builtin { "内置 · " } else { "" },
                                    info.name,
                                    if info.error.is_some() {
                                        "（不可用）"
                                    } else {
                                        ""
                                    }
                                );
                                let value = (!info.builtin).then(|| {
                                    info.path
                                        .strip_prefix(path.parent().unwrap_or(Path::new(".")))
                                        .unwrap_or(&info.path)
                                        .to_owned()
                                });
                                changed |= ui
                                    .selectable_value(&mut selection.resource, value, label)
                                    .changed();
                            }
                        });
                    ui.horizontal(|ui| {
                        if ui.button("导入词库…").clicked() {
                            import = Some(kind);
                        }
                        if ui.button("使用内置词库").clicked()
                            && selection.resource.take().is_some()
                        {
                            changed = true;
                        }
                    });
                    if let Some(info) = current {
                        ui.small(info.path.display().to_string());
                        if let Some(error) = &info.error {
                            ui.colored_label(egui::Color32::LIGHT_RED, error);
                        } else {
                            ui.label(format!(
                                "校验通过 · {} 条 · 版本 {}",
                                info.entries, info.version
                            ));
                            ui.small(if selection.needed() {
                                "保存后由输入法加载；校验通过不代表当前宿主已采用。"
                            } else {
                                "两个用途均关闭，输入法不加载此词库。"
                            });
                            ui.collapsing("来源与许可", |ui| {
                                for source in &info.sources {
                                    ui.label(format!(
                                        "{} · {} · {}",
                                        source.id, source.revision, source.license
                                    ));
                                    ui.hyperlink(&source.url);
                                    ui.collapsing(format!("完整声明：{}", source.id), |ui| {
                                        ui.label(&source.notice);
                                    });
                                }
                            });
                        }
                    }
                    ui.separator();
                });
            }
            refresh = ui.button("重新检查词库").clicked();
        });
        if refresh || import.is_some() {
            self.start(path, config, import);
        }
        self.preview.ui(ui);
        ui.small("配置独立于主码表。导入会复制到配置目录；选择和开关需保存后生效。中文输入时右键点击语言栏中/英按钮可打开辅助输入菜单。");
        changed
    }
}
