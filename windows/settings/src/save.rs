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