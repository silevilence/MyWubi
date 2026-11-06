//! Exercises the release archive through the installer, without installing a system TIP.
#[test]
#[ignore = "requires compiled release catalog, MI_COMPONENT_ARCHIVE, MI_GGUF_PACKAGE and MI_ONNX_PACKAGE"]
fn release_component_cache_install_and_both_native_backends_work_without_developer_path() {
    use std::{fs, io::Write, process::{Command, Stdio}, sync::atomic::AtomicBool};
    let root = tempfile::tempdir().unwrap();
    let config = root.path().join("config.toml");
    core_engine::Config::default().save(&config).unwrap();
    let before = fs::read(&config).unwrap();
    let catalog = runtime_components::catalog().unwrap();
    let component = &catalog[0];
    let downloads = root.path().join("components/.downloads");
    fs::create_dir_all(&downloads).unwrap();
    fs::copy(std::env::var_os("MI_COMPONENT_ARCHIVE").unwrap(), downloads.join(format!("{}-{}.part", component.key(), component.archive.sha256))).unwrap();
    model_input::components::install(&config, "llama", &AtomicBool::new(false), |_| {}).unwrap();
    for (capability, variable) in [("llama", "MI_GGUF_PACKAGE"), ("onnx", "MI_ONNX_PACKAGE")] {
        let program = runtime_components::worker(&config, capability).unwrap();
        let mut command = Command::new(&program);
        command.arg("--trial").current_dir(program.parent().unwrap()).env_clear()
            .env("SystemRoot", std::env::var_os("SystemRoot").unwrap_or_default())
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        #[cfg(windows)] {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command.spawn().unwrap();
        let request = serde_json::json!({"package":std::env::var(variable).unwrap(),"prefix":"北京","wait_ms":1000,"threads":2});
        child.stdin.take().unwrap().write_all(request.to_string().as_bytes()).unwrap();
        let start = std::time::Instant::now();
        while child.try_wait().unwrap().is_none() {
            if start.elapsed() > std::time::Duration::from_secs(60) { child.kill().unwrap(); panic!("native trial timed out"); }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        eprintln!("{capability}: {result}");
        assert_eq!(result["ok"], true);
    }
    assert_eq!(fs::read(&config).unwrap(), before);
    eprintln!("archive_sha256={}; prediction remains disabled", component.archive.sha256);
}
