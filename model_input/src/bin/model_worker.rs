#![cfg_attr(windows, windows_subsystem = "windows")]
#[cfg(windows)]
fn main() {
    if std::env::args().any(|arg| arg == "--check-backends") {
        #[cfg(all(feature = "llama-cpp-2", feature = "ort"))]
        let ready = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.join("onnxruntime.dll")))
            .is_some_and(|p| model_input::onnx::initialize_runtime(&p).is_ok());
        #[cfg(not(all(feature = "llama-cpp-2", feature = "ort")))]
        let ready = false;
        std::process::exit(if ready { 0 } else { 1 });
    }
    use std::time::{Duration, Instant};
    let Ok(mut server) = model_ipc::Server::new() else {
        return;
    };
    let endpoint = model_input::service::Endpoint::default();
    let mut last = Instant::now();
    let mut owner = 0u32;
    loop {
        let served = server
            .poll(|pid, bytes| {
                let Ok(mut request) = serde_json::from_slice::<serde_json::Value>(bytes) else {
                    return br#"{"ok":false,"error":"invalid request"}"#.to_vec();
                };
                let op = request["op"].as_str().unwrap_or("").to_owned();
                if matches!(op.as_str(), "Poll" | "Invalidate" | "Pause" | "Close") && owner != pid
                {
                    return br#"{"ok":true,"value":null}"#.to_vec();
                }
                if op == "Configure" || op == "Submit" {
                    if owner != pid {
                        endpoint.exchange(br#"{"op":"Invalidate"}"#);
                    }
                    owner = pid;
                }
                let pointer = if op == "Submit" {
                    "/request/key/edit/session"
                } else {
                    "/key/edit/session"
                };
                if let Some(value) = request.pointer_mut(pointer) {
                    if let Some(session) = value.as_u64() {
                        *value = serde_json::json!(session ^ (u64::from(pid) << 32));
                    }
                }
                let mut response: serde_json::Value = serde_json::from_slice(
                    &endpoint.exchange(&serde_json::to_vec(&request).unwrap_or_default()),
                )
                .unwrap_or_default();
                if let Some(value) = response.pointer_mut("/value/Ok/key/edit/session") {
                    if let Some(session) = value.as_u64() {
                        *value = serde_json::json!(session ^ (u64::from(pid) << 32));
                    }
                }
                serde_json::to_vec(&response).unwrap_or_default()
            })
            .unwrap_or(false);
        if served {
            last = Instant::now();
        }
        if last.elapsed() > Duration::from_secs(60) {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}
#[cfg(not(windows))]
fn main() {}
