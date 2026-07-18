//! 保存逻辑：调用 Config::save 原子写，更新 dirty 标志。

use crate::state::AppState;

/// 保存配置到磁盘。成功返回 true，失败返回 false 并设置状态消息。
pub fn save(state: &mut AppState) -> bool {
    if state.table_editor.dirty {
        let path = state.table_editor.path.clone();
        if let Err(error) = state.table_editor.save_to(path) {
            state.status_msg = Some(format!("[ERR] 保存码表失败: {error}"));
            log::error!("保存码表失败: {error}");
            return false;
        }
    }
    match state.config.save(&state.config_path) {
        Ok(()) => {
            state.dirty = false;
            state.status_msg = Some("[OK] 已保存".into());
            log::info!("配置已保存到 {}", state.config_path.display());
            true
        }
        Err(e) => {
            state.status_msg = Some(format!("[ERR] 保存失败: {e}"));
            log::error!("保存配置失败: {e}");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Panel;

    /// 构造一个测试用 AppState，config_path 指向临时目录。
    fn test_state(tmp: &std::path::Path) -> AppState {
        let config = core_engine::Config::default();
        AppState {
            config,
            dirty: true,
            active_panel: Panel::Basic,
            config_path: tmp.join("config.toml"),
            status_msg: None,
            portable: false,
            load_error: None,
            pending_pick: None,
            table_dir: tmp.to_path_buf(),
            scanned_tables: Vec::new(),
            tip_status: tip_manager::detect_status(),
            uninstall_confirm: false,
            tip_operation_pending: false,
            update_state: crate::vpk::UpdateState::Idle,
            update_worker: None,
            user_dictionary_editor: Default::default(),
            table_editor: Default::default(),
        }
    }

    #[test]
    fn save_success_clears_dirty() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = test_state(dir.path());
        assert!(state.dirty);
        assert!(save(&mut state));
        assert!(!state.dirty);
        assert_eq!(state.status_msg, Some("[OK] 已保存".into()));
    }

    #[test]
    fn save_failure_keeps_dirty() {
        // 写入一个只读目录应触发保存失败
        let dir = tempfile::tempdir().unwrap();
        let mut state = test_state(dir.path());
        // 将路径指向一个不存在目录中的文件，让其父目录也不存在 → 保存失败
        state.config_path = dir.path().join("nonexistent").join("config.toml");
        assert!(state.dirty);
        assert!(!save(&mut state));
        assert!(state.dirty);
        assert!(state.status_msg.as_ref().unwrap().starts_with("[ERR] 保存失败"));
    }

    #[test]
    fn save_writes_dirty_table_editor() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = test_state(dir.path());
        state.table_editor.path = dir.path().join("edited.dict");
        state.table_editor.entries = vec![core_engine::Entry {
            code: "a".into(),
            word: "工".into(),
            weight: 999,
        }];
        state.table_editor.dirty = true;

        assert!(save(&mut state));
        let table = core_engine::Dictionary::load(&state.table_editor.path).unwrap();

        assert_eq!(table.exact("a")[0].word, "工");
    }

    #[test]
    fn save_rejects_conflicting_hotkeys() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = test_state(dir.path());
        state.config.hotkey.page_prev = state.config.hotkey.page_next.clone();

        assert!(!save(&mut state));
        assert!(state.dirty);
        assert!(state.status_msg.as_deref().is_some_and(|msg| msg.contains("冲突")));
    }
}
