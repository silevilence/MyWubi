//! 管理员权限检测与按需提升重启。
//!
//! settings.exe 不再嵌入 `requireAdministrator` 清单（否则 Velopack 安装器
//! 在非提升上下文启动它时会报 `ERROR_ELEVATION_REQUIRED`）。改为 `asInvoker`
//! 运行，仅在需要管理输入法时由用户主动触发「以管理员身份重启」。

/// 当前进程是否以管理员身份运行。
pub fn is_admin() -> bool {
    #[cfg(windows)]
    {
        unsafe { windows::Win32::UI::Shell::IsUserAnAdmin() }.as_bool()
    }
    #[cfg(not(windows))]
    {
        true
    }
}

/// 以管理员身份重启当前程序（通过 ShellExecute "runas"）。
///
/// 调用后当前进程应尽快退出。返回是否成功发起重启请求。
pub fn relaunch_elevated() -> bool {
    #[cfg(windows)]
    {
        use windows::core::HSTRING;
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

        let exe = match std::env::current_exe() {
            Ok(p) => p,
            Err(_) => return false,
        };
        let exe_str = match exe.to_str() {
            Some(s) => s.to_string(),
            None => return false,
        };

        let operation = HSTRING::from("runas");
        let file = HSTRING::from(exe_str);
        // 借用 HSTRING 以满足 Param<PCWSTR, CopyType> 约束。
        let result = unsafe { ShellExecuteW(None, &operation, &file, None, None, SW_SHOWNORMAL) };
        // ShellExecuteW 返回 HINSTANCE，<= 32 表示失败。
        result.0 as isize > 32
    }
    #[cfg(not(windows))]
    {
        false
    }
}
