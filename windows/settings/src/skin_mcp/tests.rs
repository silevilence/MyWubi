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
