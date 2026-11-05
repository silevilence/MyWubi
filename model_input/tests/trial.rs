use model_input::{compatibility, package, trial};
use std::{fs, path::Path};

fn fixture(root: &Path, arch: &str) {
    fs::create_dir_all(root).unwrap();
    let mut gguf = b"GGUF".to_vec();
    gguf.extend(3u32.to_le_bytes());
    gguf.extend(0u64.to_le_bytes());
    gguf.extend(1u64.to_le_bytes());
    let key = b"general.architecture";
    gguf.extend((key.len() as u64).to_le_bytes());
    gguf.extend(key);
    gguf.extend(8u32.to_le_bytes());
    gguf.extend((arch.len() as u64).to_le_bytes());
    gguf.extend(arch.as_bytes());
    fs::write(root.join("model.gguf"), gguf).unwrap();
    let hash = package::sha256(&root.join("model.gguf")).unwrap();
    let policy = serde_json::json!({"version":1,"adapter":"gguf-continuation-v1","model_sha256":hash,"vocabulary_sha256":null,"minimum_score":-1,"minimum_margin":0,"evidence":"Synthetic diagnostic fixture, no quality claim"});
    fs::write(root.join("policy.json"), policy.to_string()).unwrap();
    let mut manifest = model_input::download::available_models().remove(0).manifest;
    manifest.id = "fixture".into();
    manifest.version = "v1".into();
    manifest.adapter = "gguf-continuation-v1".into();
    manifest.capabilities.proposes_words = false;
    manifest.generation_policy = None;
    manifest.prediction_policy = Some("policy.json".into());
    manifest.files = ["model.gguf", "policy.json"]
        .iter()
        .map(|name| package::Artifact {
            path: (*name).into(),
            bytes: fs::metadata(root.join(name)).unwrap().len(),
            sha256: package::sha256(&root.join(name)).unwrap(),
        })
        .collect();
    fs::write(
        root.join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
}
fn edit(root: &Path, mutate: impl FnOnce(&mut serde_json::Value)) {
    let path = root.join("manifest.json");
    let mut m = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    mutate(&mut m);
    fs::write(path, m.to_string()).unwrap();
}

#[test]
fn import_and_trial_diagnose_four_failures_without_changing_saved_model() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    let original = b"[prediction]\nenabled = false\n";
    fs::write(&config, original).unwrap();
    for case in ["bad", "architecture", "vocabulary", "policy"] {
        let root = dir.path().join(case);
        fixture(
            &root,
            if case == "architecture" {
                "unadapted"
            } else {
                "qwen2"
            },
        );
        match case {
            "bad" => fs::write(root.join("model.gguf"), b"broken").unwrap(),
            "vocabulary" => edit(&root, |m| {
                m["backend"] = "OnnxRuntime".into();
                m["adapter"] = "character-step-v1".into();
                m["vocabulary"] = "missing.json".into();
            }),
            "policy" => {
                fs::remove_file(root.join("policy.json")).unwrap();
            }
            _ => {}
        }
        let error = compatibility::check(&root).err().unwrap().to_string();
        assert!(
            error.contains(match case {
                "bad" => "损坏",
                "architecture" => "不支持的 GGUF 架构",
                "vocabulary" => "缺少分词器",
                _ => "缺少预测策略",
            }),
            "{error}"
        );
        let request = trial::Request {
            package: root.clone(),
            prefix: "北京".into(),
            threads: 2,
            wait_ms: 1000,
        };
        let reply: serde_json::Value =
            serde_json::from_slice(&trial::exchange(&serde_json::to_vec(&request).unwrap()))
                .unwrap();
        assert_eq!(reply["error"], error);
        let import = serde_json::json!({"op":"Import","config":config,"source":root});
        let reply: serde_json::Value = serde_json::from_slice(&model_input::catalog::exchange(
            &serde_json::to_vec(&import).unwrap(),
        ))
        .unwrap();
        assert_eq!(reply["error"], error);
        assert_eq!(fs::read(&config).unwrap(), original);
        assert!(!dir.path().join("models/fixture--v1").exists());
    }
}

#[test]
fn malformed_requests_and_oversized_metadata_are_bounded() {
    for bytes in [vec![b'x'; 65537], b"{}".to_vec()] {
        let reply: serde_json::Value = serde_json::from_slice(&trial::exchange(&bytes)).unwrap();
        assert_eq!(reply["ok"], false);
    }
    let dir = tempfile::tempdir().unwrap();
    fixture(dir.path(), "qwen2");
    assert!(compatibility::check(dir.path()).is_ok());
}

#[test]
#[ignore = "requires explicitly prepared real local GGUF and llama backend"]
fn real_local_package_import_and_trial_preserve_config() {
    let source = std::env::var_os("MI31_LOCAL_PACKAGE").expect("MI31_LOCAL_PACKAGE");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    core_engine::Config::default().save(&path).unwrap();
    let before = fs::read(&path).unwrap();
    compatibility::check(Path::new(&source)).unwrap();
    let imported = model_input::catalog::Catalog::for_config(&path)
        .import(Path::new(&source))
        .unwrap();
    let package = core_engine::config_path::model_store_path(&path)
        .join(format!("{}--{}", imported.id, imported.version));
    let report = trial::run(trial::Request {
        package,
        prefix: "北京".into(),
        threads: 2,
        wait_ms: 1000,
    })
    .unwrap();
    assert_eq!(report.words, vec!["市"]);
    assert!(report.predict_ms <= 1000);
    assert_eq!(before, fs::read(&path).unwrap());
    println!("{}", serde_json::to_string(&report).unwrap());
}
