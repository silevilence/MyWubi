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
    let (tx, rx) = mpsc::channel();
    let (reply, response) = mpsc::channel();
    endpoint.calls = rx;
    tx.send(Call {
        rpc: json!({"method":"tools/call","params":{"name":"acquire"}}),
        reply,
    })
    .unwrap();
    drop(endpoint);
    assert!(response.recv().is_err());
    editor.create("plain").unwrap();
    assert!(editor.has_draft());
}

#[test]
fn targeted_edit_is_atomic_and_round_trips_to_new_file() {
    let mut editor = SkinEditor::default();
    author::write("create_skin", &json!({"template":"bamboo"}), &mut editor).unwrap();
    let original = editor.draft().unwrap().package.files["windows/appearance.toml"].clone();
    let mut args = json!({"platform":"android","component":"jade","variant":"dark","fields":{"font_size":23},"images":{"space":{"resource":"shared:missing","mode":"stretch","opacity":1.0}}});
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
        let mut socket = TcpStream::connect(host).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let body = json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"status","arguments":{}}}).to_string();
        write!(socket, "POST /{path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",body.len()).unwrap();
        let mut response = String::new();
        socket.read_to_string(&mut response).unwrap();
        response
    });
    let until = Instant::now() + Duration::from_secs(5);
    while !client.is_finished() && Instant::now() < until {
        server.update(&mut editor);
        std::thread::sleep(Duration::from_millis(5));
    }
    let response = client.join().unwrap();
    assert!(response.starts_with("HTTP/1.1 200"));
    assert!(response.contains(editor.title()));
}
