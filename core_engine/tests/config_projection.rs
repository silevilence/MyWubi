use core_engine::{
    config::{CommitMode, PunctuationMode, ValidationContext},
    config_path::{resolve_storage, StorageContext},
    Config,
};

#[test]
fn prediction_upgrade_defaults_off_and_restart_preserves_explicit_missing_selection() {
    use core_engine::config::{ModelLoading, ModelSelection};
    for old in [
        "",
        "[basic]\ncandidate_count=5",
        "[prediction]\nwait_ms=500",
    ] {
        let cfg = Config::from_str(old).unwrap();
        assert!(!cfg.prediction.enabled);
        assert!(cfg.prediction.model.is_none());
    }
    let directory = std::env::temp_dir().join(format!("mywubi-mi27-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("config.toml");
    let mut cfg = Config::default();
    cfg.prediction.enabled = true;
    cfg.prediction.model = Some(ModelSelection {
        id: "missing-model".into(),
        version: "v1".into(),
    });
    cfg.prediction.wait_ms = 1000;
    cfg.prediction.threads = 4;
    cfg.prediction.max_candidates = 3;
    cfg.prediction.idle_seconds = 120;
    cfg.prediction.loading = ModelLoading::PreloadKeep;
    cfg.save(&path).unwrap();
    assert_eq!(Config::load(&path).unwrap().prediction, cfg.prediction);
    cfg.prediction.enabled = false;
    cfg.save(&path).unwrap();
    let restarted = Config::load(&path).unwrap();
    assert_eq!(restarted.prediction, cfg.prediction);
    assert!(
        restarted.prediction.model.is_some(),
        "disabling must not silently switch to dictionary"
    );
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn inactive_values_roundtrip_without_changing_effective_projection() {
    let mut config = Config::default();
    let original = config.engine_options();
    config.basic.commit_mode = CommitMode::EnterCommit;
    config.basic.punctuation_mode = PunctuationMode::DirectCommit;
    config.dictionary.enable_exact_match = false;
    config.dictionary.enable_fuzzy = true;
    let text = config.to_string_toml().unwrap();
    let loaded = Config::from_str(&text).unwrap();
    assert_eq!(original, loaded.engine_options());
    assert_eq!(loaded.to_string_toml().unwrap(), text);
}

#[test]
fn desktop_conflicts_are_diagnostics_for_touch_and_read_but_block_desktop_save() {
    let mut config = Config::default();
    config.hotkey.page_next = config.hotkey.page_prev.clone();
    for context in [
        ValidationContext::Read,
        ValidationContext::TouchSave,
        ValidationContext::ActiveResources,
    ] {
        assert!(config.validate_for(context).is_ok());
    }
    assert!(config.validate_for(ValidationContext::DesktopSave).is_err());
}

#[test]
fn explicit_private_storage_ignores_executable_directory_and_preserves_existing_data() {
    let dir = std::env::temp_dir().join(format!("mywubi-private-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("config.toml"), "# existing").unwrap();
    let result = resolve_storage(StorageContext::Private { files_dir: &dir }).unwrap();
    assert_eq!(result.path, dir.join("config.toml"));
    assert!(!result.portable);
    assert_eq!(std::fs::read_to_string(&result.path).unwrap(), "# existing");
    std::fs::remove_dir_all(&dir).unwrap();
}
