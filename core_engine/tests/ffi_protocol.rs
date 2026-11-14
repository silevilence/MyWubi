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
fn skin_resolution_leaves_pending_input_and_candidate_identity_unchanged() {
    let (dir, runtime, session) = fixture();
    let plan=ok(json!({"op":"Prepare","session":session,"action":{"InputCode":"a"}}));
    let before=ok(json!({"op":"State","session":session}));
    let config=Config::load(dir.join("config.toml")).unwrap();
    for dark in [false,true] {
        let result=ok(json!({"op":"ResolveSkin","path":dir.join("config.toml"),"config":config,"platform":"android","dark":dark,"high_contrast":false}));
        assert!(result["parameters"].is_object());
    }
    assert_eq!(before,ok(json!({"op":"State","session":session})));
    assert!(!plan.is_null());
    release(&[session,runtime]);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn draft_shortcut_preview_uses_selected_resources_without_saving_or_touching_live_sessions() {
    let (dir, runtime, session) = fixture();
    let path = dir.join("config.toml");
    let bytes = std::fs::read(&path).unwrap();
    let before = ok(json!({"op":"State","session":session}));
    let mut config = Config::load(&path).unwrap();
    config.shortcuts.rules = vec![core_engine::shortcuts::ShortcutRule {
        code: "JJAD".into(),
        position: 2,
        format: "yyyy年MM月dd日".into(),
        kind: core_engine::shortcuts::ShortcutKind::DateTime,
        enabled: true,
    }];
    let request = json!({"op":"PreviewShortcuts","path":path,"config":config,"code":"JJAD","time":core_engine::shortcut_preview::example_time()});
    let first = ok(request.clone());
    assert_eq!(first["code"], "jjad");
    assert_eq!(first["rows"][0]["actual"], 1);
    assert_eq!(first["rows"][0]["target"], 2);
    std::fs::write(dir.join("main.dict"), "jjad\t甲\t100\njjad\t乙\t90\n").unwrap();
    let second = ok(request);
    assert_eq!(second["rows"][0]["source"], "普通候选");
    assert_eq!(second["rows"][1]["actual"], 2);
    assert_eq!(second["rows"][1]["candidate"]["text"], "2026年11月12日");
    assert_ne!(first["resources"], second["resources"]);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(ok(json!({"op":"State","session":session})), before);
    release(&[session, runtime]);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn shortcut_inspection_and_config_save_share_normalization_and_conflict_rules() {
    let (dir, runtime, session) = fixture();
    let path = dir.join("config.toml");
    let loaded = ok(json!({"op":"ReadConfig","path":path}));
    let mut config = loaded["config"].clone();
    let rules = json!([
        {"code":"JJAD","position":2,"kind":"date_time","format":"yyyy年MM月dd日","enabled":true},
        {"code":"jjad","position":3,"kind":"date_time","format":"yyyy-MM-dd","enabled":true}
    ]);
    config["shortcuts"] = json!({"rules":rules});
    let table = core_engine::dictionary::TableConfig {
        wildcard_key: Some('J'),
        ..Default::default()
    };
    let inspected = ok(json!({"op":"InspectShortcuts","rules":rules,"table":table}));
    assert_eq!(inspected["rules"][0]["code"], "jjad");
    assert_eq!(inspected["rules"][0]["enabled"], true);
    assert_eq!(inspected["states"][0]["effective"], false);
    let saved =
        ok(json!({"op":"SaveConfig","path":path,"config":config,"expected":loaded["version"]}));
    assert_eq!(saved["config"]["shortcuts"]["rules"][0]["code"], "jjad");
    let bytes = std::fs::read(&path).unwrap();
    config["shortcuts"]["rules"][1]["position"] = json!(2);
    config["shortcuts"]["rules"][1]["enabled"] = json!(false);
    assert_eq!(
        invoke(json!({"op":"SaveConfig","path":path,"config":config,"expected":saved["version"]}))
            ["ok"],
        false
    );
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    release(&[
        session,
        runtime,
        loaded["version"].as_u64().unwrap(),
        saved["version"].as_u64().unwrap(),
    ]);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn table_metadata_reads_display_name_without_parsing_entries() {
    let (dir, runtime, session) = fixture();
    let path = dir.join("display.dict");
    std::fs::write(
        &path,
        "\u{feff}---\r\ndisplay_name: '我的五笔 #1'\r\n---\r\ninvalid entry\n",
    )
    .unwrap();
    let header = ok(json!({"op":"ReadTableConfig", "path":path}));
    assert_eq!(header["display_name"], "我的五笔 #1");
    assert!(header.get("entries").is_none());
    assert!(header.get("version").is_none());
    release(&[session, runtime]);
    std::fs::remove_dir_all(dir).unwrap();
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
        for kind in 1..=3 {
            assert_eq!(mywubi_v1_buffer_free(buffer.allocation | kind), 3);
        }
        assert_eq!(
            invoke(json!({"op":"Destroy","handle":buffer.allocation}))["error"]["code"],
            3
        );
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
