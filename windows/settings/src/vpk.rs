//! Velopack 集成：生命周期钩子（TIP 自动注册/反注册）+ 按需更新检查。
//!
//! - `init_velopack()` 必须在 `main()` 最开始调用一次，处理安装/更新/卸载钩子参数。
//! - `UpdateWorker` 在后台线程执行更新检查/下载，通过 mpsc 通道回报进度。
//! - 便携版采用被动更新策略（重定向至发布页）；安装版调用 Velopack 静默增量更新。

use std::sync::mpsc::{self, Receiver};
use std::thread;
use velopack::sources::GithubSource;
use velopack::{UpdateCheck, UpdateManager, VelopackAsset};

/// GitHub 发布源 URL（与 workspace `repository` 一致）。
const RELEASE_REPO: &str = "https://github.com/silevilence/MyWubi";
/// 发布页 URL（便携版被动更新时打开）。
pub const RELEASES_PAGE_URL: &str = "https://github.com/silevilence/MyWubi/releases";

/// 后台更新检查线程向 UI 回报的事件。
#[derive(Debug)]
pub enum UpdateEvent {
    /// 当前已是最新版本。
    NoUpdate,
    /// 发现新版本（版本号、更新说明、是否便携模式）。
    Available {
        version: String,
        notes: String,
        portable: bool,
        asset: VelopackAsset,
    },
    /// 下载进度（0–100）。
    DownloadProgress(i16),
    /// 下载完成，可应用并重启。
    DownloadDone { asset: VelopackAsset },
    /// 当前并非 Velopack 安装（如开发期直接运行 target/ 下的 exe）。
    NotInstalled,
    /// 出错。
    Error(String),
}

/// 后台更新 worker 句柄（UI 侧持有 receiver 轮询）。
pub struct UpdateWorker {
    pub rx: Receiver<UpdateEvent>,
}

/// UI 侧的更新状态机。
#[derive(Debug)]
pub enum UpdateState {
    Idle,
    Checking,
    NoUpdate,
    NotInstalled,
    Available {
        version: String,
        notes: String,
        portable: bool,
        asset: VelopackAsset,
    },
    Downloading { progress: i16 },
    Ready { asset: VelopackAsset },
    Error(String),
}

/// 在 `main()` 最开始调用一次：运行 Velopack 启动逻辑。
///
/// Velopack 以特殊命令行参数触发本进程时，本函数处理其内部事务后可能直接退出。
/// 正常启动时本函数直接返回，继续执行后续 GUI 逻辑。
///
/// 注：不在此注册任何 TIP 安装/卸载钩子。Velopack 安装器在非提升上下文运行，
/// 而 TIP 注册需要管理员权限——若由安装器自动调用会触发提升冲突。TIP 注册
/// 改由用户安装完成后手动运行 settings.exe，在「输入法管理」面板完成。
pub fn init_velopack() {
    velopack::VelopackApp::build().run();
}

/// 启动后台更新检查线程，返回 worker 句柄。
pub fn start_check() -> UpdateWorker {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mgr = match UpdateManager::new(
            GithubSource::new(RELEASE_REPO, None, false),
            None,
            None,
        ) {
            Ok(m) => m,
            Err(e) => {
                log::info!("[vpk] UpdateManager 不可用（非 Velopack 安装）: {e}");
                let _ = tx.send(UpdateEvent::NotInstalled);
                return;
            }
        };

        let portable = mgr.get_is_portable();

        let check = match mgr.check_for_updates() {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(UpdateEvent::Error(format!("检查更新失败: {e}")));
                return;
            }
        };

        match check {
            UpdateCheck::NoUpdateAvailable | UpdateCheck::RemoteIsEmpty => {
                let _ = tx.send(UpdateEvent::NoUpdate);
            }
            UpdateCheck::UpdateAvailable(info) => {
                let asset = info.TargetFullRelease.clone();
                let version = asset.Version.clone();
                let notes = asset.NotesMarkdown.clone();
                let _ = tx.send(UpdateEvent::Available {
                    version,
                    notes,
                    portable,
                    asset,
                });
            }
        }
    });
    UpdateWorker { rx }
}

/// 启动后台下载线程。`asset` 为检查阶段返回的目标资产。
pub fn start_download(asset: VelopackAsset) -> UpdateWorker {
    let (tx, rx) = mpsc::channel();
    let asset_clone = asset.clone();
    thread::spawn(move || {
        let mgr = match UpdateManager::new(
            GithubSource::new(RELEASE_REPO, None, false),
            None,
            None,
        ) {
            Ok(m) => m,
            Err(e) => {
                let _ = tx.send(UpdateEvent::Error(format!("初始化更新管理器失败: {e}")));
                return;
            }
        };

        let (ptx, prx) = mpsc::channel::<i16>();
        // 转发进度到 UI
        let tx2 = tx.clone();
        thread::spawn(move || {
            while let Ok(p) = prx.recv() {
                let _ = tx2.send(UpdateEvent::DownloadProgress(p));
            }
        });

        let info = velopack::UpdateInfo {
            TargetFullRelease: asset_clone,
            BaseRelease: None,
            DeltasToTarget: vec![],
            IsDowngrade: false,
        };
        if let Err(e) = mgr.download_updates(&info, Some(ptx)) {
            let _ = tx.send(UpdateEvent::Error(format!("下载更新失败: {e}")));
            return;
        }
        let _ = tx.send(UpdateEvent::DownloadDone { asset });
    });
    UpdateWorker { rx }
}

/// 应用已下载的更新并重启进程。调用后进程会立即退出。
pub fn apply_and_restart(asset: VelopackAsset) {
    thread::spawn(move || {
        if let Ok(mgr) = UpdateManager::new(
            GithubSource::new(RELEASE_REPO, None, false),
            None,
            None,
        ) {
            if let Err(e) = mgr.apply_updates_and_restart(&asset) {
                log::error!("[vpk] 应用更新失败: {e}");
            }
        }
    });
}

/// 在系统默认浏览器中打开发布页（便携版被动更新策略）。
pub fn open_releases_page() {
    #[cfg(windows)]
    {
        // 通过 shell 的 start 命令打开 URL，避免直接调用 ShellExecuteW 的类型绑定问题。
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "", RELEASES_PAGE_URL])
            .spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("xdg-open").arg(RELEASES_PAGE_URL).spawn();
    }
}