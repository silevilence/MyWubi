#![cfg(windows)]
use std::{
    path::PathBuf,
    process::Command,
    time::{Duration, Instant},
};

#[test]
#[ignore = "real per-user service; set MYWUBI_TEST_CONFIG and MYWUBI_TEST_SETTINGS; install trusted components first"]
fn saved_preload_outlives_prepare_helper_exit() {
    let path = PathBuf::from(std::env::var("MYWUBI_TEST_CONFIG").unwrap());
    assert!(
        model_ipc::exchange(br#"{"op":"Status"}"#, Duration::from_millis(100)).is_err(),
        "run with no pre-existing service"
    );
    let cold = Instant::now();
    save_and_exit();
    let pid = wait_phase("Ready");
    let mut service_guard = TestService {
        pid: Some(pid),
        path: path.clone(),
        original: std::fs::read(&path).unwrap(),
    };
    eprintln!(
        "prepare helper exit -> model Ready: {:?}; pid={pid}",
        cold.elapsed()
    );
    // A normal host resolves the same saved path without a Win32 verbatim
    // prefix. Its first request must reuse the already prepared instance.
    let config = core_engine::Config::load(&path).unwrap();
    let prepare = serde_json::json!({"op":"Prepare", "root":core_engine::config_path::model_store_path(&path), "config":config.prediction});
    let reply = model_ipc::exchange(
        &serde_json::to_vec(&prepare).unwrap(),
        Duration::from_secs(1),
    )
    .unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&reply).unwrap()["ok"],
        true
    );
    let mut request: serde_json::Value =
        serde_json::from_str(include_str!("../../../model_input/evaluation/request.json")).unwrap();
    request["task"] = "Predict".into();
    request["prefix"] = "北京".into();
    request["code"] = "".into();
    request["candidates"] = serde_json::json!([]);
    let started = Instant::now();
    model_ipc::exchange(
        &serde_json::to_vec(&serde_json::json!({"op":"Submit","request":request})).unwrap(),
        Duration::from_secs(1),
    )
    .unwrap();
    let mut response = serde_json::Value::Null;
    while started.elapsed() < Duration::from_millis(1000) {
        let bytes = model_ipc::exchange(
            &serde_json::to_vec(&serde_json::json!({"op":"Poll","key":request["key"]})).unwrap(),
            Duration::from_millis(100),
        )
        .unwrap();
        response = serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["value"].clone();
        if !response.is_null() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    eprintln!(
        "first ordinary host after settings preload: {:?}; response={response}",
        started.elapsed()
    );
    assert_eq!(response["Ok"]["words"], serde_json::json!(["市"]));
    // Do not poll during this interval: ordinary service idle expiry is 60 s.
    std::thread::sleep(Duration::from_secs(61));
    assert_eq!(
        wait_phase("Ready"),
        pid,
        "saved keep policy lost its resident model"
    );
    let original = std::fs::read(&path).unwrap();
    let mut config = core_engine::Config::load(&path).unwrap();
    config.prediction.enabled = false;
    config.save(&path).unwrap();
    save_and_exit();
    assert_eq!(wait_phase("Idle"), pid);
    std::fs::write(&path, original).unwrap();
    save_and_exit();
    assert_eq!(wait_phase("Ready"), pid, "save started a duplicate service");
    use std::os::windows::process::CommandExt;
    assert!(Command::new("taskkill.exe")
        .args(["/PID", &pid.to_string(), "/F"])
        .creation_flags(0x08000000)
        .status()
        .unwrap()
        .success());
    service_guard.pid = None;
}

// Restore this test-owned configuration and service even after an assertion.
struct TestService {
    pid: Option<u32>,
    path: PathBuf,
    original: Vec<u8>,
}
impl Drop for TestService {
    fn drop(&mut self) {
        use std::os::windows::process::CommandExt;
        let _ = std::fs::write(&self.path, &self.original);
        if let Some(pid) = self.pid {
            let _ = Command::new("taskkill.exe")
                .args(["/PID", &pid.to_string(), "/F"])
                .creation_flags(0x08000000)
                .output();
        }
    }
}

fn save_and_exit() {
    use std::os::windows::process::CommandExt;
    // Exercise the real release helper with its compiled trust catalog.
    assert!(
        Command::new(std::env::var_os("MYWUBI_TEST_SETTINGS").unwrap())
            .arg("--prepare-model")
            .arg(std::env::var_os("MYWUBI_TEST_CONFIG").unwrap())
            .creation_flags(0x08000000)
            .status()
            .unwrap()
            .success()
    );
}
fn wait_phase(phase: &str) -> u32 {
    let start = Instant::now();
    loop {
        if let Ok(bytes) = model_ipc::exchange(br#"{"op":"Status"}"#, Duration::from_millis(100)) {
            let response: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            if response["value"] == phase {
                return response["process_id"].as_u64().unwrap() as u32;
            }
        }
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "service did not enter {phase}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}
