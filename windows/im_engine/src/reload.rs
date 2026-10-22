use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use arc_swap::ArcSwap;
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};

use crate::RuntimeSnapshot;

pub(crate) fn spawn(runtime: Arc<ArcSwap<RuntimeSnapshot>>) {
    std::thread::spawn(move || {
        if let Err(err) = watch_loop(runtime) {
            log::error!("[reload] watcher 退出: {err}");
        }
    });
}

fn paths(runtime: &RuntimeSnapshot) -> Vec<PathBuf> {
    let mut paths = vec![
        runtime.config_path.clone(),
        runtime.system_table_path.clone(),
        runtime.user_table_path.clone(),
    ];
    paths.extend(runtime.temporary.paths.iter().cloned());
    paths
}

fn watch_loop(runtime: Arc<ArcSwap<RuntimeSnapshot>>) -> notify::Result<()> {
    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher: RecommendedWatcher = notify::recommended_watcher(move |result| {
        let _ = tx.send(result);
    })?;
    let mut targets = paths(&runtime.load());
    let mut watched = BTreeSet::new();
    update_watches(&mut watcher, &mut watched, &targets);
    while let Ok(result) = rx.recv() {
        let event = match result {
            Ok(event) => event,
            Err(err) => {
                log::warn!("[reload] watcher 事件错误: {err}");
                continue;
            }
        };
        if !event_matches(&event, &targets) {
            continue;
        }
        match crate::load_runtime_snapshot() {
            Ok(snapshot) => {
                let attempted = paths(&snapshot);
                if snapshot.temporary.load_errors.is_empty() {
                    targets = attempted;
                    let revision = snapshot.revision;
                    runtime.store(Arc::new(snapshot));
                    log::info!("[reload] 已发布新的运行时快照 revision={revision}");
                } else {
                    // 新引用尚不可用时也监听它；文件修复后无需再保存主配置。
                    targets = paths(&runtime.load());
                    targets.extend(attempted);
                    log::error!(
                        "[reload] 辅助码表重载失败，保留旧快照: {}",
                        snapshot.temporary.load_errors.join("; ")
                    );
                }
            }
            Err(err) => log::error!("[reload] 重载失败，继续保留旧快照: {err}"),
        }
        update_watches(&mut watcher, &mut watched, &targets);
    }
    Ok(())
}

fn update_watches(
    watcher: &mut RecommendedWatcher,
    watched: &mut BTreeSet<PathBuf>,
    targets: &[PathBuf],
) {
    let desired: BTreeSet<_> = targets
        .iter()
        .filter_map(|target| {
            target
                .parent()?
                .ancestors()
                .find(|parent| parent.is_dir())
                .map(Path::to_path_buf)
        })
        .collect();
    for path in desired.difference(watched).cloned().collect::<Vec<_>>() {
        match watcher.watch(&path, RecursiveMode::Recursive) {
            Ok(()) => {
                watched.insert(path);
            }
            Err(error) => log::warn!("[reload] 无法监听 {}: {error}", path.display()),
        }
    }
    for path in watched.difference(&desired).cloned().collect::<Vec<_>>() {
        if let Err(error) = watcher.unwatch(&path) {
            log::warn!("[reload] 取消监听失败: {error}");
        }
        watched.remove(&path);
    }
}

fn normalized(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for part in path.components() {
        match part {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                result.pop();
            }
            part => result.push(part.as_os_str()),
        }
    }
    PathBuf::from(result.to_string_lossy().to_lowercase())
}

fn event_matches(event: &Event, targets: &[PathBuf]) -> bool {
    if matches!(event.kind, notify::EventKind::Access(_)) {
        return false;
    }
    event.paths.iter().any(|path| {
        let path = normalized(path);
        targets
            .iter()
            .any(|target| normalized(target).starts_with(&path))
    })
}

#[cfg(test)]
fn event_requires_reload(event: &Event, runtime: &RuntimeSnapshot) -> bool {
    event_matches(event, &paths(runtime))
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_engine::{Config, Dictionary};
    use std::path::PathBuf;

    fn runtime_snapshot() -> RuntimeSnapshot {
        RuntimeSnapshot {
            temporary: Arc::default(),
            revision: 1,
            dict: Dictionary::from_entries(Vec::new(), None, Default::default()).unwrap(),
            config: Config::default(),
            config_path: PathBuf::from(r"C:\Users\test\AppData\Roaming\MyWubi\config.toml"),
            system_table_path: PathBuf::from(
                r"C:\Users\test\AppData\Roaming\MyWubi\tables\wubi86.dict",
            ),
            user_table_path: PathBuf::from(
                r"C:\Users\test\AppData\Roaming\MyWubi\tables\user.dict",
            ),
        }
    }

    #[test]
    fn config_file_event_triggers_reload() {
        let runtime = runtime_snapshot();
        let event = Event {
            kind: notify::EventKind::Modify(notify::event::ModifyKind::Data(
                notify::event::DataChange::Content,
            )),
            paths: vec![runtime.config_path.clone()],
            attrs: Default::default(),
        };

        assert!(event_requires_reload(&event, &runtime));
    }

    #[test]
    fn active_table_event_triggers_reload() {
        let runtime = runtime_snapshot();
        let event = Event {
            kind: notify::EventKind::Modify(notify::event::ModifyKind::Data(
                notify::event::DataChange::Content,
            )),
            paths: vec![runtime.system_table_path.clone()],
            attrs: Default::default(),
        };

        assert!(event_requires_reload(&event, &runtime));
    }

    #[test]
    fn user_table_event_triggers_reload() {
        let runtime = runtime_snapshot();
        let event = Event {
            kind: notify::EventKind::Modify(notify::event::ModifyKind::Data(
                notify::event::DataChange::Content,
            )),
            paths: vec![runtime.user_table_path.clone()],
            attrs: Default::default(),
        };

        assert!(event_requires_reload(&event, &runtime));
    }

    #[test]
    fn unrelated_file_does_not_trigger_reload() {
        let runtime = runtime_snapshot();
        let event = Event {
            kind: notify::EventKind::Modify(notify::event::ModifyKind::Data(
                notify::event::DataChange::Content,
            )),
            paths: vec![PathBuf::from(
                r"C:\Users\test\AppData\Roaming\MyWubi\debug.log",
            )],
            attrs: Default::default(),
        };

        assert!(!event_requires_reload(&event, &runtime));
    }

    #[test]
    fn auxiliary_rename_delete_and_parent_creation_trigger_reload() {
        let mut runtime = runtime_snapshot();
        runtime.temporary = Arc::new(core_engine::TemporaryTables {
            paths: vec![PathBuf::from(r"D:\bundle\new\pinyin.dict")],
            ..Default::default()
        });
        for path in [
            r"D:\bundle\new\pinyin.dict",
            r"D:\bundle\new",
            r"d:\BUNDLE\new\pinyin.dict",
        ] {
            let event = Event {
                kind: notify::EventKind::Remove(notify::event::RemoveKind::Any),
                paths: vec![path.into()],
                attrs: Default::default(),
            };
            assert!(event_requires_reload(&event, &runtime), "{path}");
        }
        let event = Event {
            kind: notify::EventKind::Modify(notify::event::ModifyKind::Name(
                notify::event::RenameMode::Both,
            )),
            paths: vec![
                r"D:\bundle\new\temp.dict".into(),
                r"D:\bundle\new\pinyin.dict".into(),
            ],
            attrs: Default::default(),
        };
        assert!(event_requires_reload(&event, &runtime));
    }
}
