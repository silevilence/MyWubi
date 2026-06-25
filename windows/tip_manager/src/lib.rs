//! Windows TIP（Text Input Processor）管理器。
//!
//! 封装 TIP 全生命周期管理：注册表写入 + `ITfInputProcessorProfileMgr` COM 调用。

pub mod detect;
pub mod error;
pub mod guids;
pub mod profile;
pub mod registrar;

pub use error::TipManagerError;
pub use profile::TipProfileManager;

/// TIP 当前安装与启用状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipStatus {
    /// 系统中未找到本 TIP 的注册痕迹。
    NotInstalled,
    /// TIP 已注册但当前被禁用。
    InstalledDisabled,
    /// TIP 已注册且已启用。
    InstalledEnabled,
    /// 检测到异常状态（注册表残缺、COM 不可用等）。
    Unknown,
}

/// 安装 TIP：注册表写入 + COM RegisterProfile + EnableProfile。
///
/// `dll_path` 为 `im_engine.dll` 的绝对路径。
pub fn install(dll_path: &str) -> Result<(), TipManagerError> {
    log::info!("[tip_manager] 开始安装 TIP，dll_path={dll_path}");

    // 1. 注册表写入
    registrar::register_tip(dll_path)?;

    // 2. COM 注册与启用 Profile
    let mgr = profile::ComProfileManager::new()?;

    // 3. 通过 ITfCategoryMgr 注册 TSF 类别（替代手动注册表写入）
    profile::register_categories()?;

    // 4. COM RegisterProfile + Enable
    mgr.register_profile()?;
    mgr.enable()?;

    log::info!("[tip_manager] TIP 安装完成");
    Ok(())
}

/// 卸载 TIP：COM DisableProfile + UnregisterProfile + 注册表清理。
pub fn uninstall() -> Result<(), TipManagerError> {
    log::info!("[tip_manager] 开始卸载 TIP");

    // 1. COM 禁用与反注册
    if let Ok(mgr) = profile::ComProfileManager::new() {
        if let Ok(true) = mgr.is_enabled() {
            let _ = mgr.disable();
        }
        let _ = mgr.unregister_profile();
        // 清理 COM 类别注册
        let _ = profile::unregister_categories();
    } else {
        log::warn!("[tip_manager] COM 不可用，跳过 Profile 清理，仅清理注册表");
    }

    // 2. 注册表清理
    registrar::unregister_tip()?;

    log::info!("[tip_manager] TIP 卸载完成");
    Ok(())
}

/// 启用已安装但禁用中的 TIP。
pub fn enable() -> Result<(), TipManagerError> {
    let mgr = profile::ComProfileManager::new()?;
    mgr.enable()?;
    log::info!("[tip_manager] TIP 已启用");
    Ok(())
}

/// 禁用已安装且启用中的 TIP。
pub fn disable() -> Result<(), TipManagerError> {
    let mgr = profile::ComProfileManager::new()?;
    mgr.disable()?;
    log::info!("[tip_manager] TIP 已禁用");
    Ok(())
}

/// 检测当前 TIP 的安装与启用状态。
pub fn detect_status() -> TipStatus {
    detect::detect_status_impl(None)
}
