//! Stage one immutable skin for independently observable OS-host checks.
use super::{author::string, tool};
use core_engine::skin::{archive::MAX_ARCHIVE, Package};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Cursor, Read},
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::Command,
};
use uuid::Uuid;

pub fn definitions() -> Vec<Value> {
    vec![tool("test","Run host acceptance against an exported skin in Windows Sandbox and/or a designated Android test device. Poll test_status with the returned job.",json!({"lease":{"type":"string"},"path":{"type":"string"},"platform":{"enum":["windows","android","both"]},"serial":{"type":"string","description":"Exact adb serial of the test device"},"windowsComponent":{"type":"string"},"androidComponent":{"type":"string"}}),&["lease","path","platform"]),
    tool("test_status","Read per-platform evidence and outcome of a host test",json!({"job":{"type":"string","format":"uuid"}}),&["job"])]
}
fn base() -> PathBuf {
    std::env::temp_dir().join("MyWubi-skin-tests")
}
fn ps(path: &Path, script: &str) -> std::io::Result<()> {
    fs::write(path, format!("\u{feff}{script}"))
}
fn read_json(path: &Path) -> Result<Value, String> {
    let s = fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(s.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())
}
pub fn status(job: &str) -> Result<Value, String> {
    let id = Uuid::parse_str(job).map_err(|e| e.to_string())?;
    let root = base().join(id.to_string());
    let job = read_json(&root.join("job.json"))?;
    let mut reports = serde_json::Map::new();
    for p in ["windows", "android"]
        .into_iter()
        .filter(|p| job["platform"] == "both" || job["platform"] == *p)
    {
        let report = root.join(p).join("report.json");
        reports.insert(
            p.into(),
            if report.is_file() {
                read_json(&report)?
            } else {
                json!({"status":"running"})
            },
        );
    }
    Ok(
        json!({"job":id.to_string(),"evidence":root,"skin":job["file"],"sha256":job["sha256"],"results":reports}),
    )
}
pub fn start(args: &Value) -> Result<Value, String> {
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    stage(args, executable.parent().ok_or("编辑器路径不可用")?)
}
fn stage(args: &Value, bin: &Path) -> Result<Value, String> {
    let platform = string(args, "platform")?;
    if !matches!(platform, "windows" | "android" | "both") {
        return Err("未知平台".into());
    }
    let serial = args["serial"].as_str().unwrap_or("");
    if !serial
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"-.:_".contains(&b))
    {
        return Err("无效的 adb 设备序列号".into());
    }
    let path = Path::new(string(args, "path")?);
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|f| f.take(MAX_ARCHIVE + 1).read_to_end(&mut bytes))
        .map_err(|e| e.to_string())?;
    let package = Package::read(Cursor::new(&bytes)).map_err(|e| e.to_string())?;
    let file = format!(
        "{}@{}.mywubi-skin",
        package.metadata.id, package.metadata.version
    );
    let job = Uuid::new_v4().to_string();
    let root = base().join(&job);
    let app = root.join("app");
    let scripts = root.join("scripts");
    let mut manifest = json!({"job":job,"platform":platform,"serial":serial,"file":file,"sha256":Sha256::digest(&bytes).iter().map(|b|format!("{b:02x}")).collect::<String>()});
    let mut selected = Vec::new();
    for p in ["windows", "android"]
        .into_iter()
        .filter(|p| platform == "both" || platform == *p)
    {
        let field = format!("{p}Component");
        let component = package.platforms.get(p).and_then(|v| {
            v.components.iter().find(|c| {
                c.kind == "appearance" && args[&field].as_str().is_none_or(|id| id == c.id)
            })
        });
        let failure = match component {
            None => Some(("not_run", "皮肤未提供指定平台的外观组件".to_owned())),
            Some(c) => match crate::skin_editor::resolve_view(&package, p, &c.id, true) {
                Ok(_) => {
                    manifest[&field] = c.id.clone().into();
                    None
                }
                Err(error) => Some(("failed", error)),
            },
        };
        selected.push((p, failure));
    }
    let prepare = || -> std::io::Result<()> {
        fs::create_dir_all(app.join("skins"))?;
        fs::create_dir_all(app.join("tables"))?;
        fs::create_dir_all(&scripts)?;
        fs::write(app.join("skins").join(&file), bytes)?;
        fs::write(root.join("job.json"), manifest.to_string())?;
        ps(
            &root.join("run-host.ps1"),
            include_str!("../../../../scripts/skin-mcp/run-host.ps1"),
        )?;
        if let Some(id) = manifest["windowsComponent"].as_str() {
            for dll in ["im_engine.dll", "core_engine.dll"] {
                if bin.join(dll).is_file() {
                    fs::copy(bin.join(dll), app.join(dll))?;
                }
            }
            fs::write(app.join("tables/main.dict"), "ab\t以及\t10\nab\t已经\t9\n")?;
            fs::write(app.join("config.toml"),format!("[basic]\nauto_commit_unique=false\n[dictionary]\nsystem_table=\"tables/main.dict\"\nenable_user_dict=false\n[skins.windows]\nmode=\"dark\"\nuse_skin_metrics=true\n[skins.windows.selection]\nid={:?}\nversion={:?}\ncomponent={id:?}\n",package.metadata.id,package.metadata.version))?;
            ps(
                &scripts.join("start.ps1"),
                include_str!("../../../../scripts/windows-expression-browser/start.ps1"),
            )?;
            ps(
                &scripts.join("host.ps1"),
                include_str!("../../../../scripts/windows-expression-browser/host.ps1"),
            )?;
            ps(
                &scripts.join("verify.ps1"),
                include_str!("../../../../scripts/skin-mcp/verify-windows.ps1"),
            )?;
            fs::write(
                scripts.join("interop.cs"),
                include_bytes!("../../../../scripts/windows-expression-browser/interop.cs"),
            )?;
            ps(
                &scripts.join("guest.ps1"),
                "try { & C:\\Browser-Source\\scripts\\start.ps1 } finally { shutdown.exe /s /t 1 }",
            )?;
        }
        Ok(())
    };
    prepare().map_err(|e| e.to_string())?;
    for (p, failure) in selected {
        let report = root.join(p).join("report.json");
        if let Some((state, reason)) = failure {
            record(&report, state, &reason)?;
            continue;
        }
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(base().join(format!("{p}.lock")))
            .map_err(|e| e.to_string())?;
        if lock.try_lock().is_err() {
            record(&report, "not_run", "该平台已有宿主验收正在执行")?;
            continue;
        }
        let script = root.join("run-host.ps1");
        std::thread::Builder::new()
            .name(format!("skin-test-{p}"))
            .spawn(move || {
                let _lock = lock;
                let powershell = PathBuf::from(
                    std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into()),
                )
                .join("System32/WindowsPowerShell/v1.0/powershell.exe");
                let result = Command::new(powershell)
                    .args([
                        "-NoProfile",
                        "-NonInteractive",
                        "-ExecutionPolicy",
                        "RemoteSigned",
                        "-File",
                    ])
                    .arg(script)
                    .arg(p)
                    .creation_flags(0x08000000)
                    .output();
                if !report.is_file() {
                    let message = match result {
                        Ok(o) => String::from_utf8_lossy(&o.stderr).into_owned(),
                        Err(e) => e.to_string(),
                    };
                    let _ = record(&report, "failed", &message);
                }
            })
            .map_err(|e| e.to_string())?;
    }
    status(&job)
}
fn record(path: &Path, state: &str, reason: &str) -> Result<(), String> {
    fs::create_dir_all(path.parent().ok_or("缺少报告目录")?)
        .and_then(|_| fs::write(path, json!({"status":state,"reason":reason}).to_string()))
        .map_err(|e| e.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn host_status_rejects_paths_and_separates_absent_components() {
        assert!(status("../job.json").is_err());
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/skins/bamboo-android.mywubi-skin");
        let result = start(&json!({"path":path,"platform":"windows"})).unwrap();
        assert_eq!(result["results"]["windows"]["status"], "not_run");
        assert_eq!(result["sha256"].as_str().unwrap().len(), 64);
    }
}
