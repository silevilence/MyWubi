//! 让搜索等 AppContainer 宿主只读访问输入法正在使用的数据。
//!
//! 不给容器写权限，不向兄弟文件继承；原子替换文件后须重新调用。
use std::path::Path;

pub(crate) fn prepare(config_path: &Path, config: &core_engine::Config) -> Result<(), String> {
    let system = core_engine::config_path::resolve_resource_path(
        config_path,
        &config.dictionary.system_table,
    );
    let user =
        core_engine::config_path::resolve_resource_path(config_path, &config.dictionary.user_table);
    for path in [config_path, system.as_path(), user.as_path()] {
        // 用户词库尚未创建时仍需允许监视其目录。
        for target in path.parent().into_iter().chain(Some(path)) {
            if target.exists() {
                tip_manager::app_container::grant_read(target)?;
            }
        }
    }
    Ok(())
}
