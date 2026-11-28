use super::*;
use std::{
    io::{Read, Write},
    net::TcpStream,
    time::Instant,
};

#[test]
fn settings_controls_address_the_existing_window_and_both_endpoints_observe_shutdown() {
    let root = tempfile::tempdir().unwrap();
    let config = root.path().join("config.toml");
    let window = control::Controls::register(&config).unwrap().unwrap();
    assert!(control::Controls::register(&config).unwrap().is_none());
    assert!(control::signal(&config, "enable"));
    assert_eq!(window.poll(), (true, false, false));
    window.publish(true).unwrap();
    assert!(control::enabled(&config));
    assert!(control::signal(&config, "disable"));
    assert!(window.poll().1);
    window.publish(false).unwrap();
    assert!(!control::enabled(&config));
}

#[test]
fn release_expiry_and_shutdown_revoke_writer_ownership() {
    let mut editor = SkinEditor::default();
    let mut endpoint = Endpoint::start(egui::Context::default()).unwrap();
    let first = endpoint.call("acquire", &Value::Null, &mut editor).unwrap();
    let token: Value = serde_json::from_str(first[0]["text"].as_str().unwrap()).unwrap();
    assert!(endpoint.call("acquire", &Value::Null, &mut editor).is_err());
    assert!(endpoint
        .call("release", &json!({"lease":"other"}), &mut editor)
        .is_err());
    endpoint.last_write -= Duration::from_secs(301);
    endpoint.update(&mut editor);
    assert!(endpoint.call("release", &token, &mut editor).is_err());
    author::write("create_skin", &json!({"template":"bamboo"}), &mut editor).unwrap();
    let lease = endpoint.call("acquire", &Value::Null, &mut editor).unwrap();
    let lease: Value = serde_json::from_str(lease[0]["text"].as_str().unwrap()).unwrap();
    let mut args = json!({"platform":"android","component":"jade","variant":"dark","fields":{"key_text_size":29},"lease":lease["lease"]});
    let (tx, rx) = mpsc::channel();
    let (reply, response) = mpsc::channel();
    endpoint.calls = rx;
    tx.send(Call {
        rpc: json!({"method":"tools/call","params":{"name":"edit_appearance","arguments":args}}),
        reply,
    })
    .unwrap();
    drop(endpoint);
    args["fields"]["key_text_size"] = 31.into();
    author::write("edit_appearance", &args, &mut editor).unwrap();
    assert!(response.recv().is_err());
    assert_eq!(
        author::read("inspect", &args, &editor).unwrap()["fields"]["key_text_size"].as_f64(),
        Some(31.0)
    );
}

#[test]
fn targeted_edit_is_atomic_and_round_trips_to_new_file() {
    let mut editor = SkinEditor::default();
    author::write("create_skin", &json!({"template":"bamboo"}), &mut editor).unwrap();
    let original = editor.draft().unwrap().package.files["windows/appearance.toml"].clone();
    let mut args = json!({"platform":"android","component":"jade","variant":"dark","fields":{"font_size":23},"images":{"space":{"resource":"shared:missing","mode":"stretch","opacity":1.0}}});
    let mut light = args.clone();
    light["variant"] = "light".into();
    let before_light = author::read("inspect", &light, &editor).unwrap();
    let before = author::read("inspect", &args, &editor).unwrap();
    assert!(author::write("edit_appearance", &args, &mut editor)
        .unwrap_err()
        .contains("images.dark.space.resource"));
    assert_eq!(author::read("inspect", &args, &editor).unwrap(), before);
    args["images"]["space"]["resource"] = "shared:bamboo".into();
    author::write("edit_appearance", &args, &mut editor).unwrap();
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("皮肤.mywubi-skin");
    author::write("export", &json!({"path":path}), &mut editor).unwrap();
    assert!(author::write("export", &json!({"path":path}), &mut editor).is_err());
    let mut editor = SkinEditor::default();
    author::write("open_skin", &json!({"path":path}), &mut editor).unwrap();
    assert_eq!(
        author::read("inspect", &light, &editor).unwrap(),
        before_light
    );
    assert_eq!(
        editor.draft().unwrap().package.files["windows/appearance.toml"],
        original
    );
    assert_eq!(
        author::read("inspect", &args, &editor).unwrap()["images"]["space"]["resource"],
        "shared:bamboo"
    );
}

#[test]
fn bamboo_preview_changes_only_android_dark_space_and_exports_the_final_revision() {
    use base64::{engine::general_purpose::STANDARD, Engine};
    let temp = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MYWUBI_SKIN_PREVIEW_EVIDENCE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| temp.path().into());
    std::fs::create_dir_all(&root).unwrap();
    let mut editor = SkinEditor::default();
    author::write("create_skin", &json!({"template":"bamboo"}), &mut editor).unwrap();
    author::write(
        "set_identity",
        &json!({"id":"agent.bamboo.space","name":"竹纸 · 深色空格","version":"1"}),
        &mut editor,
    )
    .unwrap();
    let render = |e: &SkinEditor, platform: &str, revision: &str| {
        let content=preview::run(&json!({"platform":platform,"component":"jade","variant":"dark","scenes":[{"state":"ordinary","scale":1.0}]}),e).unwrap();
        let pixels = STANDARD
            .decode(content[1]["data"].as_str().unwrap())
            .unwrap();
        std::fs::write(root.join(format!("{platform}-{revision}.png")), &pixels).unwrap();
        image::load_from_memory(&pixels).unwrap().to_rgba8()
    };
    let android = render(&editor, "android", "before");
    let windows = render(&editor, "windows", "before");
    let mut patch = json!({"platform":"android","component":"jade","variant":"dark","images":{"space":{"resource":"shared:bamboo","mode":"stretch","opacity":1.0}}});
    author::write("edit_appearance", &patch, &mut editor).unwrap();
    let first = render(&editor, "android", "first");
    assert_ne!(android, first);
    // Keep the pale paper subtle under the light lettering of the dark keyboard.
    patch["images"]["space"]["opacity"] = 0.20.into();
    author::write("edit_appearance", &patch, &mut editor).unwrap();
    let after = render(&editor, "android", "after");
    assert_ne!(first, after);
    assert_eq!(windows, render(&editor, "windows", "after"));
    let diff: Vec<_> = android
        .enumerate_pixels()
        .filter(|(x, y, p)| *p != after.get_pixel(*x, *y))
        .collect();
    assert!(!diff.is_empty());
    assert!(diff.iter().all(|(x, y, _)| *x > android.width() * 2 / 5
        && *x < android.width() * 3 / 5
        && *y > android.height() * 3 / 4));
    let path = root.join("agent.bamboo.space.mywubi-skin");
    author::write("export", &json!({"path":path}), &mut editor).unwrap();
    let mut combined = image::RgbaImage::new(android.width() * 2, android.height());
    image::imageops::overlay(&mut combined, &android, 0, 0);
    image::imageops::overlay(&mut combined, &after, i64::from(android.width()), 0);
    combined.save(root.join("android-comparison.png")).unwrap();
}

#[test]
#[ignore = "requires an explicitly selected OS host and exported skin"]
fn run_designated_host_acceptance() {
    let mut editor = SkinEditor::default();
    let mut endpoint = Endpoint::start(egui::Context::default()).unwrap();
    let acquired = endpoint.call("acquire", &Value::Null, &mut editor).unwrap();
    let mut args: Value = serde_json::from_str(acquired[0]["text"].as_str().unwrap()).unwrap();
    args["platform"] = std::env::var("MYWUBI_TEST_PLATFORM").unwrap().into();
    args["path"] = std::env::var("MYWUBI_TEST_SKIN").unwrap().into();
    args["serial"] = std::env::var("MYWUBI_TEST_SERIAL")
        .unwrap_or_default()
        .into();
    let response = endpoint.call("test", &args, &mut editor).unwrap();
    let mut report: Value = serde_json::from_str(response[0]["text"].as_str().unwrap()).unwrap();
    println!("Host evidence: {}", report["evidence"]);
    let end = Instant::now() + Duration::from_secs(310);
    while report["results"]
        .as_object()
        .unwrap()
        .values()
        .any(|v| v["status"] == "running")
    {
        assert!(Instant::now() < end, "{report}");
        std::thread::sleep(Duration::from_millis(500));
        let result = endpoint.call("test_status", &report, &mut editor).unwrap();
        report = serde_json::from_str(result[0]["text"].as_str().unwrap()).unwrap();
    }
    println!("{report}");
    assert!(
        report["results"]
            .as_object()
            .unwrap()
            .iter()
            .all(|(platform, r)| r["status"] == "passed"
                || (platform == "android"
                    && args["serial"] == "offline-test"
                    && r["status"] == "not_run")),
        "{report}"
    );
}

#[test]
fn preview_batch_labels_native_scaling_and_schematic_states() {
    let mut editor = SkinEditor::default();
    editor.create("jade").unwrap();
    let mut request = json!({"platform":"windows","component":"jade","variant":"light","scenes":[{"state":"long"},{"state":"long","scale":2}]});
    let native = preview::run(&request, &editor).unwrap();
    let metadata = |v: &Value| serde_json::from_str::<Value>(v["text"].as_str().unwrap()).unwrap();
    assert_eq!(metadata(&native[0])["source"], "windows-native");
    assert!(metadata(&native[2])["height"].as_u64() > metadata(&native[0])["height"].as_u64());
    request["platform"] = "android".into();
    request["scenes"] = json!([{"state":"pressed"},{"state":"disabled"},{"state":"bubble","landscape":true,"scale":2}]);
    let android = preview::run(&request, &editor).unwrap();
    assert_eq!(metadata(&android[0])["source"], "android-desktop-schematic");
    assert_ne!(android[1]["data"], android[3]["data"]);
    assert_eq!(metadata(&android[4])["width"], 720);
}

#[test]
fn http_client_inspects_the_open_draft() {
    let mut server = Endpoint::start(egui::Context::default()).unwrap();
    let url = server.url.clone();
    let mut editor = SkinEditor::default();
    editor.create("jade").unwrap();
    let client = std::thread::spawn(move || {
        let (host, path) = url
            .strip_prefix("http://")
            .unwrap()
            .split_once('/')
            .unwrap();
        let rpc = |method: &str, params: Value| -> Value {
            let mut socket = TcpStream::connect(host).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(15)))
                .unwrap();
            let body = json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}).to_string();
            write!(socket,"POST /{path} HTTP/1.0\r\nHost: {host}\r\nConnection: close\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",body.len()).unwrap();
            let mut response = String::new();
            socket.read_to_string(&mut response).unwrap();
            let (headers, body) = response.split_once("\r\n\r\n").unwrap();
            assert!(headers.lines().next().unwrap().contains(" 200 "));
            serde_json::from_str::<Value>(body).unwrap()["result"].clone()
        };
        let invoke = |name: &str, arguments: Value| {
            rpc("tools/call", json!({"name":name,"arguments":arguments}))
        };
        assert_eq!(
            rpc(
                "initialize",
                json!({"protocolVersion":VERSION,"capabilities":{},"clientInfo":{"name":"acceptance","version":"1"}})
            )["protocolVersion"],
            VERSION
        );
        for version in ["2025-03-26", "2025-06-18"] {
            assert_eq!(
                rpc(
                    "initialize",
                    json!({"protocolVersion":version,"capabilities":{},"clientInfo":{"name":"acceptance","version":"1"}})
                )["protocolVersion"],
                version
            );
        }
        assert!(rpc("resources/list", json!({}))["resources"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["uri"] == "mywubi://skin/android-space"));
        let lease: Value = serde_json::from_str(
            invoke("acquire", json!({}))["content"][0]["text"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        let mut edit = json!({"platform":"android","component":"jade","variant":"dark","fields":{"key_text_size":36}});
        edit["lease"] = lease["lease"].clone();
        assert!(invoke("edit_appearance", edit).get("isError").is_none());
        let result = invoke(
            "preview",
            json!({"platform":"android","component":"jade","variant":"dark","scenes":[{"state":"bubble","landscape":true,"scale":2}]}),
        );
        assert_eq!(result["content"][1]["mimeType"], "image/png");
        assert!(result["content"][1]["data"]
            .as_str()
            .unwrap()
            .starts_with("iVBOR"));
        assert!(invoke("release", lease).get("isError").is_none());
        invoke("status", json!({}))["content"][0]["text"]
            .as_str()
            .unwrap()
            .to_owned()
    });
    let until = Instant::now() + Duration::from_secs(20);
    while !client.is_finished() && Instant::now() < until {
        server.update(&mut editor);
        std::thread::sleep(Duration::from_millis(5));
    }
    let response = client.join().unwrap();
    assert!(response.contains(editor.title()));
}
