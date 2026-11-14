//! Dedicated appearance watcher. All package reads happen here, never on key dispatch.
use core_engine::{
    skin::runtime::{self},
    Config,
};
use notify::Watcher;
use std::{
    path::PathBuf,
    sync::{Arc, OnceLock},
    time::Duration,
};
use windows::{
    core::w,
    Win32::{
        System::Registry::*,
        UI::{Accessibility::*, WindowsAndMessaging::*},
    },
};

pub struct Snapshot {
    pub appearance: runtime::Snapshot,
    pub images: Arc<text_render::skin_images::Cache>,
}
impl std::ops::Deref for Snapshot {
    type Target = runtime::Snapshot;
    fn deref(&self) -> &Self::Target {
        &self.appearance
    }
}
fn load(
    path: &std::path::Path,
    config: &Config,
    dark: bool,
    contrast: bool,
) -> Result<Snapshot, core_engine::skin::Error> {
    let appearance = runtime::load(path, config, "windows", dark, contrast)?;
    let images = match &appearance.package {
        Some(package) => text_render::skin_images::decode(package, &appearance.images)?,
        None => Default::default(),
    };
    Ok(Snapshot {
        appearance,
        images: Arc::new(images),
    })
}
static SKIN: OnceLock<Arc<arc_swap::ArcSwap<Snapshot>>> = OnceLock::new();

pub fn current() -> Option<Arc<Snapshot>> {
    SKIN.get().map(|s| s.load_full())
}

pub fn start(path: PathBuf, config: &Config) {
    if SKIN.get().is_some() {
        return;
    }
    let (dark, contrast) = system_state();
    let initial = load(&path, config, dark, contrast).or_else(|error| {
        log::error!("[skin] 初始皮肤加载失败，保留旧外观：{error}");
        let mut fallback = config.clone();
        fallback.skins.windows = Default::default();
        load(&path, &fallback, dark, contrast)
    });
    let initial = match initial {
        Ok(value) => value,
        Err(error) => {
            log::error!("[skin] {error}");
            return;
        }
    };
    let skin = Arc::new(arc_swap::ArcSwap::from_pointee(initial));
    if SKIN.set(skin.clone()).is_err() {
        return;
    }
    if let Err(error) = std::thread::Builder::new()
        .name("MyWubiSkinLoader".into())
        .spawn(move || {
            let (tx, rx) = std::sync::mpsc::channel();
            let watcher = notify::recommended_watcher(move |event| {
                let _ = tx.send(event);
            });
            let mut watcher = match watcher {
                Ok(value) => value,
                Err(error) => {
                    log::error!("[skin] {error}");
                    return;
                }
            };
            let Some(parent) = path.parent() else {
                return;
            };
            if let Err(error) = watcher.watch(parent, notify::RecursiveMode::Recursive) {
                log::error!("[skin] {error}");
                return;
            }
            let mut last_system = (dark, contrast);
            loop {
                let event = rx.recv_timeout(Duration::from_secs(1));
                let next_system = system_state();
                let changed = match event {
                    Ok(Ok(event)) => {
                        !matches!(event.kind, notify::EventKind::Access(_))
                            && event
                                .paths
                                .iter()
                                .any(|p| p == &path || p.starts_with(runtime::store_path(&path)))
                    }
                    Ok(Err(error)) => {
                        log::warn!("[skin] 监听失败：{error}");
                        false
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => false,
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                };
                if !changed && next_system == last_system {
                    continue;
                }
                last_system = next_system;
                match Config::load(&path)
                    .map_err(|e| e.to_string())
                    .and_then(|config| {
                        load(&path, &config, next_system.0, next_system.1)
                            .map_err(|e| e.to_string())
                    }) {
                    Ok(next) => {
                        skin.store(Arc::new(next));
                        log::info!("[skin] 已发布完整外观快照");
                    }
                    Err(error) => log::error!("[skin] 保留上次可用皮肤：{error}"),
                }
            }
        })
    {
        log::error!("[skin] 加载线程启动失败：{error}");
    }
}

fn system_state() -> (bool, bool) {
    let mut light = 1u32;
    let mut size = 4u32;
    let mut contrast = HIGHCONTRASTW {
        cbSize: std::mem::size_of::<HIGHCONTRASTW>() as u32,
        ..Default::default()
    };
    unsafe {
        let _ = RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some((&mut light as *mut u32).cast()),
            Some(&mut size),
        );
        let _ = SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            contrast.cbSize,
            Some((&mut contrast as *mut HIGHCONTRASTW).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        );
    }
    (light == 0, contrast.dwFlags.contains(HCF_HIGHCONTRASTON))
}
