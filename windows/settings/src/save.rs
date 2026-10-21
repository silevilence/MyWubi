//! 将编辑会话的保存结果呈现给配置界面。
use crate::state::AppState;

pub fn save(state: &mut AppState) -> bool {
    match state.tables.save(&state.config, &state.config_path) {
        Ok(()) => {
            state.dirty = false;
            state.status_msg = Some("[OK] 已保存".into());
            log::info!("配置已保存到 {}", state.config_path.display());
            true
        }
        Err(error) => {
            // 即使码表部分成功，配置仍需保存；关闭窗口必须继续提示。
            state.dirty = true;
            state.status_msg = Some(format!("[ERR] {error}"));
            log::error!("{error}");
            false
        }
    }
}
