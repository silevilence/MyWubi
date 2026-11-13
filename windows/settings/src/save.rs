//! 将编辑会话的保存结果呈现给配置界面。
use crate::state::AppState;

pub fn save(state: &mut AppState) -> bool {
    if let Err(error) = core_engine::shortcuts::validate_structure(&state.config.shortcuts.rules) {
        state.active_panel = crate::state::Panel::SpecialInput;
        state.special_input.locate_rule_error(&error);
        state.dirty = true;
        state.status_msg = Some(format!("[ERR] {error}"));
        return false;
    }
    match state.tables.save(&state.config, &state.config_path) {
        Ok(()) => {
            if let Ok(canonical) = state.config.canonicalized() {
                state.config = canonical;
            }
            state.dirty = false;
            state.status_msg = Some("[OK] 已保存".into());
            log::info!("配置已保存到 {}", state.config_path.display());
            state.prepare_app_container_access();
            if state.config.personal.learn {
                if let Ok(exe) = std::env::current_exe() {
                    let _ = personal_input::background::start_desktop(&exe, &state.config_path);
                }
            }
            if let Err(error) = crate::model_catalog::prepare_saved_model(&state.config_path) {
                log::warn!("{error}");
                if state.config.prediction.model.is_some() {
                    state.status_msg = Some(format!("[OK] 配置已保存；{error}"));
                }
            }
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
