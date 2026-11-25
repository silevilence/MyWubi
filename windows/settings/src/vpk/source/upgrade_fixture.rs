//! Exercise the real Velopack download and installer inside the disposable Windows guest.
use super::*;
use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use velopack::locator::VelopackLocatorConfig;
use velopack::{ApplyWaitMode, UpdateCheck, UpdateManager};

#[test]
#[ignore = "requires a disposable guest with MyWubi installed and cached fixture packages"]
fn installed_copy_downloads_pinned_release_then_upgrades() {
    let root = PathBuf::from(env::var_os("MYWUBI_TEST_INSTALL").unwrap());
    let fixture = PathBuf::from(env::var_os("MYWUBI_TEST_RELEASE").unwrap());
    let tag = env::var("MYWUBI_TEST_TAG").unwrap();
    let filename = format!("MyWubi-{}-full.nupkg", &tag[1..]);
    let installed = root.join("current");
    let locator = VelopackLocatorConfig {
        UpdateExePath: root.join("Update.exe"),
        ManifestPath: installed.join("sq.version"),
        PackagesDir: root.join("packages"),
        RootAppDir: root,
        CurrentBinaryDir: installed,
        IsPortable: false,
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let source = WindowsSource {
        api: format!("{base}/releases"),
        downloads: base,
        tag: Arc::default(),
    };
    let checked = Arc::new(AtomicBool::new(false));
    let served = checked.clone();
    let expected_tag = tag.clone();
    let server = thread::spawn(move || {
        let android_only = serde_json::json!({"tag_name":"V5.0.0","draft":false,"prerelease":false,
            "assets":[{"name":"release-scope-android.json"}]});
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let request = BufReader::new(&stream).lines().next().unwrap().unwrap();
            let url = request.split_whitespace().nth(1).unwrap();
            let path = url.split('?').next().unwrap();
            let package = path == format!("/{tag}/{filename}");
            let bytes = if path == "/releases" {
                assert!(
                    !served.load(Ordering::SeqCst),
                    "Download rescanned a changed release list"
                );
                if url.ends_with("page=1") {
                    serde_json::to_vec(&vec![android_only.clone(); PAGE_SIZE]).unwrap()
                } else {
                    serde_json::to_vec(&serde_json::json!([android_only, {"tag_name":tag,"draft":false,
                        "prerelease":false,"assets":[{"name":"releases.win.json"},{"name":filename}]}])).unwrap()
                }
            } else if path == format!("/{tag}/releases.win.json") {
                fs::read(fixture.join("releases.win.json")).unwrap()
            } else {
                assert!(package, "Unexpected download: {url}");
                assert!(served.load(Ordering::SeqCst));
                fs::read(fixture.join(&filename)).unwrap()
            };
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                bytes.len()
            )
            .unwrap();
            stream.write_all(&bytes).unwrap();
            if package {
                break;
            }
        }
    });
    let manager = UpdateManager::new(source.clone(), None, Some(locator.clone())).unwrap();
    let UpdateCheck::UpdateAvailable(info) = manager.check_for_updates().unwrap() else {
        panic!("Expected newer Windows release")
    };
    let selected = source.tag.get().unwrap();
    assert_eq!(selected, &expected_tag);
    assert_eq!(info.TargetFullRelease.Version, expected_tag[1..]);
    checked.store(true, Ordering::SeqCst);
    let manager = UpdateManager::new(source.package_source(selected), None, Some(locator)).unwrap();
    manager.download_updates(&info, None).unwrap();
    server.join().unwrap();
    manager
        .unsafe_apply_updates(
            &info.TargetFullRelease,
            true,
            ApplyWaitMode::NoWait,
            false,
            Vec::<String>::new(),
        )
        .unwrap();
}
