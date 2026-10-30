use core_engine::{ffi::*, session::HostContext, Config};
use serde_json::{json, Value};
use std::path::PathBuf;

fn invoke(request: Value) -> Value {
    serde_json::from_slice(&call(
        &serde_json::to_vec(&json!({"version":1,"request":request})).unwrap(),
    ))
    .unwrap()
}
fn ok(request: Value) -> Value {
    let response = invoke(request);
    assert_eq!(response["ok"], true, "{response}");
    response["value"].clone()
}
fn fixture() -> (PathBuf, u64, u64) {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "mywubi-ffi-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main.dict"), "a\t一\t100\nab\t丁\t80\n").unwrap();
    let mut config = Config::default();
    config.dictionary.system_table = "main.dict".into();
    config.dictionary.user_table = "user.dict".into();
    config.save(dir.join("config.toml")).unwrap();
    let runtime = ok(json!({"op":"BuildRuntime","path":dir.join("config.toml")}))["handle"]
        .as_u64()
        .unwrap();
    let session =
        ok(json!({"op":"CreateSession","runtime":runtime,"context":HostContext::default()}))
            ["handle"]
            .as_u64()
            .unwrap();
    (dir, runtime, session)
}
fn release(ids: &[u64]) {
    for id in ids {
        ok(json!({"op":"Destroy","handle":id}));
    }
}

#[test]
fn editing_is_two_phase_and_handles_are_isolated() {
    let (dir, runtime, s) = fixture();
    let p = ok(json!({"op":"Prepare","session":s,"action":{"InputCode":"a"}}));
    assert_eq!(
        ok(json!({"op":"State","session":s}))["view"]["spelling"],
        ""
    );
    assert_eq!(
        invoke(json!({"op":"Prepare","session":s,"action":"Space"}))["error"]["code"],
        4
    );
    let other =
        ok(json!({"op":"CreateSession","runtime":runtime,"context":HostContext::default()}))
            ["handle"]
            .as_u64()
            .unwrap();
    assert_eq!(
        ok(
            json!({"op":"Ack","session":other,"token":p["token"],"outcome":"Applied","owns_preedit":true})
        )["status"],
        "Stale"
    );
    ok(json!({"op":"Ack","session":s,"token":p["token"],"outcome":"Applied","owns_preedit":true}));
    let window = ok(json!({"op":"Candidates","session":s,"offset":0,"count":256}));
    assert_eq!(window["candidates"][0]["text"], "一");
    let p = ok(
        json!({"op":"Prepare","session":s,"action":{"SelectCandidate":[window["query_revision"],0]}}),
    );
    assert_eq!(p["op"]["CommitAndReplace"]["text"], "一");
    ok(
        json!({"op":"Ack","session":s,"token":p["token"],"outcome":"OutcomeUnknown","owns_preedit":true}),
    );
    let cleanup = ok(json!({"op":"Recover","session":s}));
    assert!(cleanup["op"].get("CommitAndReplace").is_none());
    ok(json!({"op":"Close","session":s}));
    assert_eq!(
        ok(json!({"op":"IsCurrent","session":s,"token":p["token"]})),
        false
    );
    release(&[s, other, runtime]);
    assert_eq!(
        invoke(json!({"op":"Destroy","handle":s}))["error"]["code"],
        3
    );
    assert_eq!(
        invoke(json!({"op":"State","session":runtime}))["error"]["code"],
        3
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn resources_preserve_versions_and_unicode() {
    let (dir, runtime, s) = fixture();
    let path = dir.join("config.toml");
    let read = ok(json!({"op":"ReadConfig","path":path}));
    let saved = ok(
        json!({"op":"SaveConfig","path":path,"config":read["config"],"expected":read["version"]}),
    );
    std::fs::write(&path, "# external change").unwrap();
    assert_eq!(
        invoke(
            json!({"op":"SaveConfig","path":path,"config":read["config"],"expected":saved["version"]})
        )["error"]["code"],
        6
    );
    let user = dir.join("user.dict");
    let v = ok(json!({"op":"ReadUser","path":user}))["version"]
        .as_u64()
        .unwrap();
    let edited = ok(
        json!({"op":"EditUser","path":user,"expected":v,"edit":{"Add":{"code":"abc","word":"词😀","weight":10}}}),
    );
    let read_user = ok(json!({"op":"ReadUser","path":user}));
    assert_eq!(read_user["entries"][0]["word"], "词😀");
    let p = ok(json!({"op":"Prepare","session":s,"action":{"InsertLiteral":"😀\u{0}e\u{301}"}}));
    assert_eq!(p["op"]["CommitAndReplace"]["text"], "😀\u{0}e\u{301}");
    release(&[
        s,
        runtime,
        v,
        edited["version"].as_u64().unwrap(),
        read_user["version"].as_u64().unwrap(),
        read["version"].as_u64().unwrap(),
        saved["version"].as_u64().unwrap(),
    ]);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn c_transport_matches_rust_and_rejects_invalid_buffers() {
    let fixtures: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/protocol/errors.json")).unwrap();
    for fixture in fixtures {
        let bytes = serde_json::to_vec(&fixture["request"]).unwrap();
        let response: Value = serde_json::from_slice(&call(&bytes)).unwrap();
        assert_eq!(response["error"]["code"], fixture["code"]);
        let mut buffer = Buffer {
            data: std::ptr::null(),
            len: 0,
            allocation: 0,
        };
        // SAFETY: both allocations live through the call; result is released after comparison.
        unsafe {
            assert_eq!(
                mywubi_v1_call(bytes.as_ptr(), bytes.len() as u64, &mut buffer),
                0
            );
            assert_eq!(
                std::slice::from_raw_parts(buffer.data, buffer.len as usize),
                call(&bytes)
            );
        }
        assert_eq!(mywubi_v1_buffer_free(buffer.allocation), 0);
    }
    let cases = [
        b"{\"version\":2,\"request\":{\"op\":\"Destroy\",\"handle\":0}}".as_slice(),
        b"invalid",
        b"\xff",
        b"{\"version\":1,\"request\":{\"op\":\"Unknown\"}}",
    ];
    for bytes in cases {
        let mut buffer = Buffer {
            data: std::ptr::null(),
            len: 0,
            allocation: 0,
        };
        // SAFETY: input/output are live allocations and output is freed after copying.
        unsafe {
            assert_eq!(
                mywubi_v1_call(bytes.as_ptr(), bytes.len() as u64, &mut buffer),
                0
            );
            assert_eq!(
                std::slice::from_raw_parts(buffer.data, buffer.len as usize),
                call(bytes)
            );
        }
        assert_eq!(mywubi_v1_buffer_free(buffer.allocation), 0);
        assert_eq!(mywubi_v1_buffer_free(buffer.allocation), 3);
    }
    // SAFETY: these invalid null/oversized combinations must be rejected before dereference.
    unsafe {
        assert_eq!(mywubi_v1_call(std::ptr::null(), 1, std::ptr::null_mut()), 2);
    }
    assert_eq!(mywubi_v1_version(), 1);
}
