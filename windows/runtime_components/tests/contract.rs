use runtime_components::*;
fn component(id: &str, capability: &str) -> Component {
    let file = File {
        path: "model-worker.exe".into(),
        bytes: 1,
        sha256: "00".repeat(32),
    };
    Component {
        id: id.into(),
        version: "1".into(),
        application: "=0.7.0".into(),
        os: "windows".into(),
        architecture: "x86_64".into(),
        capabilities: vec![capability.into()],
        dependencies: vec![],
        license: "MIT; Apache-2.0; MSVC redistributable".into(),
        source: format!("https://github.com/silevilence/MyWubi/releases/download/v0.7.0/{id}.zip"),
        archive: File {
            path: format!("{id}.zip"),
            ..file.clone()
        },
        installed_bytes: 1,
        files: vec![file],
    }
}
#[test]
fn both_cpu_backends_share_one_payload_and_dependencies_are_deduplicated() {
    let mut cpu = component("cpu", "llama");
    cpu.capabilities.push("onnx".into());
    cpu.dependencies = vec!["crt--1".into(), "crt--1".into()];
    let mut crt = component("crt", "");
    crt.capabilities.clear();
    let all = vec![cpu, crt];
    for backend in ["llama", "onnx"] {
        let result = plan(&all, backend, "0.7.0", "windows", "x86_64").unwrap();
        assert_eq!(
            result.iter().map(|c| c.key()).collect::<Vec<_>>(),
            ["crt--1", "cpu--1"]
        );
    }
    assert!(matches!(
        plan(&[], "llama", "0.7.0", "windows", "x86_64"),
        Err(Error::Missing)
    ));
}
#[test]
fn incompatible_or_untrusted_components_cannot_activate() {
    let mut c = component("cpu", "llama");
    assert!(matches!(
        c.validate("0.7.0", "windows", "aarch64"),
        Err(Error::Incompatible)
    ));
    c.application = "<0.7.0".into();
    assert!(matches!(
        c.validate("0.7.0", "windows", "x86_64"),
        Err(Error::Incompatible)
    ));
    c.application = "=0.7.0".into();
    c.source = "https://example.org/native.zip".into();
    assert!(matches!(
        c.validate("0.7.0", "windows", "x86_64"),
        Err(Error::Manifest)
    ));
}
#[test]
fn cycles_missing_dependencies_and_windows_path_aliases_are_rejected() {
    let mut c = component("cpu", "llama");
    c.dependencies.push(c.key());
    assert!(matches!(
        plan(&[c.clone()], "llama", "0.7.0", "windows", "x86_64"),
        Err(Error::Manifest)
    ));
    c.dependencies = vec!["missing--1".into()];
    assert!(matches!(
        plan(&[c], "llama", "0.7.0", "windows", "x86_64"),
        Err(Error::Missing)
    ));
    for path in [
        "../evil.dll",
        "C:/evil.dll",
        "a\\b.dll",
        "NUL.txt",
        "com1.dll",
        "a./x",
        "/x",
    ] {
        assert!(!safe_path(path), "{path}");
    }
}
#[test]
fn missing_and_corrupt_files_are_not_installed() {
    let root = tempfile::tempdir().unwrap();
    let c = component("cpu", "llama");
    assert!(verify(root.path(), &c).is_err());
    std::fs::create_dir(root.path().join(c.key())).unwrap();
    std::fs::write(root.path().join(c.key()).join("model-worker.exe"), b"x").unwrap();
    assert!(matches!(verify(root.path(), &c), Err(Error::Integrity)));
}
