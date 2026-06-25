//! 构建脚本：
//! 1. 嵌入 requireAdministrator 清单，使 settings.exe 始终以管理员身份运行
//! 2. 将 workspace 根目录 `assets/tables/*.dict` 复制到目标输出目录 `tables/`，
//!    以便 settings.exe 运行时可以通过"初始化码表"功能从 exe 同目录 `tables/` 拷贝模板。

use std::path::Path;

fn main() {
    // 嵌入 requireAdministrator 清单
    if std::env::var("CARGO_CFG_WINDOWS").is_ok() {
        let mut res = winres::WindowsResource::new();
        res.set_manifest(
            r#"<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
<trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
        <requestedPrivileges>
            <requestedExecutionLevel level="requireAdministrator" uiAccess="false"/>
        </requestedPrivileges>
    </security>
</trustInfo>
</assembly>
"#,
        );
        res.compile().unwrap();
    }

    println!("cargo:rerun-if-changed=../../assets/tables");

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    let ws_root = Path::new(&manifest_dir).parent()
        .and_then(|p| p.parent())
        .expect("workspace root");
    let target_root = ws_root.join("target");

    let src = ws_root.join("assets").join("tables");
    let dst = target_root.join(&profile).join("tables");

    if !src.is_dir() {
        println!("cargo:warning=assets/tables/ 不存在，跳过码表复制");
        return;
    }

    std::fs::create_dir_all(&dst).ok();
    let mut count = 0u32;

    if let Ok(entries) = std::fs::read_dir(&src) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "dict") {
                if let Some(fname) = path.file_name() {
                    if std::fs::copy(&path, dst.join(fname)).is_ok() {
                        count += 1;
                    }
                }
            }
        }
    }

    if count > 0 {
        println!("cargo:warning=已复制 {count} 个码表到 {}", dst.display());
    }
}
