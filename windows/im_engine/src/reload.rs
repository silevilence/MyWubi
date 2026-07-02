use std::path::Path;
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

fn watch_loop(runtime: Arc<ArcSwap<RuntimeSnapshot>>) -> notify::Result<()> {
    let watch_dir = runtime
        .load()
        .config_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher: RecommendedWatcher = notify::recommended_watcher(move |result| {
        let _ = tx.send(result);
    })?;
    watcher.watch(&watch_dir, RecursiveMode::Recursive)?;
    let snapshot = runtime.load();
    for path in [&snapshot.system_table_path, &snapshot.user_table_path] {
        if let Some(parent) = path.parent().filter(|parent| !parent.starts_with(&watch_dir)) {
            watcher.watch(parent, RecursiveMode::NonRecursive)?;
        }
    }
    drop(snapshot);
    log::info!("[reload] 开始监听 {}", watch_dir.display());

    while let Ok(result) = rx.recv() {
        let event = match result {
            Ok(event) => event,
            Err(err) => {
                log::warn!("[reload] watcher 事件错误: {err}");
                continue;
            }
        };

        let current = runtime.load();
        if !event_requires_reload(&event, &current) {
            continue;
        }
        drop(current);

        match crate::load_runtime_snapshot() {
            Ok(snapshot) => {
                let revision = snapshot.revision;
                runtime.store(Arc::new(snapshot));
                log::info!("[reload] 已发布新的运行时快照 revision={revision}");
            }
            Err(err) => {
                log::error!("[reload] 重载失败，继续保留旧快照: {err}");
            }
        }
    }

    Ok(())
}

fn event_requires_reload(event: &Event, runtime: &RuntimeSnapshot) -> bool {
    event.paths.iter().any(|path| {
        same_path(path, &runtime.config_path)
            || same_file_in_dir(path, &runtime.config_path)
            || same_path(path, &runtime.system_table_path)
            || same_path(path, &runtime.user_table_path)
    })
}

fn same_path(path: &Path, target: &Path) -> bool {
    path == target
}

fn same_file_in_dir(path: &Path, target: &Path) -> bool {
    path.parent() == target.parent() && path.file_name() == target.file_name()
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_engine::{Config, Dictionary};
    use std::path::PathBuf;

    fn runtime_snapshot() -> RuntimeSnapshot {
        RuntimeSnapshot {
            revision: 1,
            dict: Dictionary::from_entries(Vec::new(), None, Default::default()).unwrap(),
            config: Config::default(),
            config_path: PathBuf::from(r"C:\Users\test\AppData\Roaming\MyWubi\config.toml"),
            system_table_path: PathBuf::from(r"C:\Users\test\AppData\Roaming\MyWubi\tables\wubi86.dict"),
            user_table_path: PathBuf::from(r"C:\Users\test\AppData\Roaming\MyWubi\tables\user.dict"),
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
            paths: vec![PathBuf::from(r"C:\Users\test\AppData\Roaming\MyWubi\debug.log")],
            attrs: Default::default(),
        };

        assert!(!event_requires_reload(&event, &runtime));
    }
}
