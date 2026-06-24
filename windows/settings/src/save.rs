//! 保存逻辑：调用 Config::save 原子写，更新 dirty 标志。

use crate::state::AppState;

/// 保存配置到磁盘。成功返回 true，失败返回 false 并设置状态消息。
pub fn save(state: &mut AppState) -> bool {
    match state.config.save(&state.config_path) {
        Ok(()) => {
            state.dirty = false;
            state.status_msg = Some("✅ 已保存".into());
            log::info!("配置已保存到 {}", state.config_path.display());
            true
        }
        Err(e) => {
            state.status_msg = Some(format!("❌ 保存失败: {e}"));
            log::error!("保存配置失败: {e}");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Panel;
    use std::path::PathBuf;

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
        }
    }

    #[test]
    fn save_success_clears_dirty() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = test_state(dir.path());
        assert!(state.dirty);
        assert!(save(&mut state));
        assert!(!state.dirty);
        assert_eq!(state.status_msg, Some("✅ 已保存".into()));
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
        assert!(state.status_msg.as_ref().unwrap().starts_with("❌ 保存失败"));
    }
}