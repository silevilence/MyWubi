use core_engine::{expressions::*, runtime::*, *};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

fn library(kind: ExpressionKind, text: &str) -> ExpressionLibrary {
    ExpressionLibrary::new(
        ExpressionDocument {
            kind,
            name: "测试".into(),
            version: "1".into(),
            sources: vec![ExpressionSource {
                id: "fixture".into(),
                url: "local:test".into(),
                revision: "1".into(),
                license: "MIT".into(),
                notice: "Test fixture".into(),
            }],
            entries: vec![ExpressionEntry {
                text: text.into(),
                name: "开心".into(),
                triggers: vec!["开心".into(), "快乐".into(), "开心".into()],
                tags: vec![],
                aliases: vec![],
                order: 0,
                sources: vec!["fixture".into()],
            }],
        },
        kind,
    )
    .unwrap()
}

struct Files(PathBuf);
impl Files {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "mywubi-expressions-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("main.dict"), "a\t开心\n").unwrap();
        Self(path)
    }
    fn config(&self) -> Config {
        let mut config = Config::default();
        config.dictionary.system_table = "main.dict".into();
        config.dictionary.enable_user_dict = false;
        config.expressions.emoji.resource = Some("emoji.dict".into());
        config.expressions.kaomoji.resource = Some("kaomoji.mwe".into());
        config
    }
    fn build(
        &self,
        config: Config,
        profile: BehaviorProfile,
        policy: LoadPolicy,
    ) -> Result<RuntimeSnapshot, BuildError> {
        RuntimeBuilder::build(self.0.join("config.toml"), config, profile, policy)
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn old_config_defaults_off_and_independent_selections_roundtrip() {
    assert_eq!(
        Config::from_str("").unwrap().expressions,
        ExpressionsConfig::default()
    );
    let mut config = Config::from_str("[expressions.emoji]\nsearch=true\nresource='custom/表情.mwe'\n[expressions.kaomoji]\nappend=true").unwrap();
    let selected = config.expressions.clone();
    config.dictionary.system_table = "other.dict".into();
    assert_eq!(
        Config::from_str(&config.to_string_toml().unwrap())
            .unwrap()
            .expressions,
        selected
    );
    assert!(!config.expressions.emoji.append);
    assert!(!config.expressions.kaomoji.search);
}

#[test]
fn exact_triggers_preserve_outputs_and_do_not_match_substrings() {
    for text in ["👩🏽‍💻", "🏳️‍🌈", "  \\(≧▽≦)/\u{3000}e\u{301}  "] {
        let source = library(ExpressionKind::Kaomoji, text);
        let reloaded =
            ExpressionLibrary::parse(&source.encode().unwrap(), ExpressionKind::Kaomoji).unwrap();
        assert_eq!(reloaded.exact("开心").count(), 1);
        assert_eq!(
            reloaded.exact("快乐").next().unwrap().text.as_bytes(),
            text.as_bytes()
        );
        assert_eq!(reloaded.exact("不开心").count(), 0);
    }
}

#[test]
fn expression_cannot_be_main_even_when_renamed_or_manually_configured() {
    let files = Files::new();
    let text = library(ExpressionKind::Emoji, "😄").encode().unwrap();
    let path = files.0.join("emoji.dict");
    std::fs::write(&path, &text).unwrap();
    assert!(read_table_config(&path).is_err());
    assert!(read_table(&path).is_err());
    assert!(Dictionary::load(&path).is_err());
    assert!(Dictionary::from_text(&text).is_err());
    let mut config = files.config();
    config.dictionary.system_table = "emoji.dict".into();
    for profile in [BehaviorProfile::LegacyV1, BehaviorProfile::TouchV1] {
        assert!(files
            .build(config.clone(), profile, LoadPolicy::Startup)
            .is_err());
    }
    for context in [
        config::ValidationContext::DesktopSave,
        config::ValidationContext::TouchSave,
    ] {
        assert!(config
            .save_for(files.0.join("config.toml"), context)
            .is_err());
    }
}

#[test]
fn resources_reject_main_tables_wrong_kind_unattributed_and_multiline_entries() {
    assert!(ExpressionLibrary::parse("a\t你好\n", ExpressionKind::Emoji).is_err());
    let encoded = library(ExpressionKind::Emoji, "😄").encode().unwrap();
    assert!(ExpressionLibrary::parse(&encoded, ExpressionKind::Kaomoji).is_err());
    let original = library(ExpressionKind::Kaomoji, "(^_^)");
    for text in ["\n(^_^)", "a\tb", "a\0b", "a\u{2028}b", ""] {
        let mut doc = original.document().clone();
        doc.entries[0].text = text.into();
        assert!(ExpressionLibrary::new(doc, ExpressionKind::Kaomoji).is_err());
    }
    let mut doc = original.document().clone();
    doc.entries[0].sources = vec!["missing".into()];
    assert!(ExpressionLibrary::new(doc, ExpressionKind::Kaomoji).is_err());
}

#[test]
fn disabled_missing_resources_are_not_loaded_on_either_platform() {
    let files = Files::new();
    for profile in [BehaviorProfile::LegacyV1, BehaviorProfile::TouchV1] {
        let snapshot = files
            .build(files.config(), profile, LoadPolicy::Reload)
            .unwrap();
        assert_eq!(snapshot.expressions.errors().count(), 0);
        assert_eq!(snapshot.expressions.paths().count(), 0);
        assert_eq!(snapshot.dict.exact("a")[0].word, "开心");
    }
}

#[test]
fn saving_invalid_enabled_selection_preserves_existing_config() {
    let files = Files::new();
    let path = files.0.join("config.toml");
    let mut config = files.config();
    config.save(&path).unwrap();
    let before = std::fs::read(&path).unwrap();
    config.expressions.emoji.search = true;
    for content in ["a\t普通码表\n", "broken"] {
        std::fs::write(files.0.join("emoji.dict"), content).unwrap();
        for context in [
            config::ValidationContext::DesktopSave,
            config::ValidationContext::TouchSave,
        ] {
            assert!(config.save_for(&path, context).is_err());
            assert_eq!(std::fs::read(&path).unwrap(), before);
        }
    }
    config.expressions.emoji.search = false;
    config.save(&path).unwrap();
}

#[test]
fn startup_isolates_bad_resources_and_reload_preserves_old_snapshot_until_repair() {
    let files = Files::new();
    for profile in [BehaviorProfile::LegacyV1, BehaviorProfile::TouchV1] {
        let mut config = files.config();
        config.expressions.emoji.search = true;
        config.expressions.kaomoji.append = true;
        std::fs::write(
            files.0.join("kaomoji.mwe"),
            library(ExpressionKind::Kaomoji, "(^_^)").encode().unwrap(),
        )
        .unwrap();
        std::fs::write(files.0.join("emoji.dict"), "broken").unwrap();
        let start = files
            .build(config.clone(), profile, LoadPolicy::Startup)
            .unwrap();
        assert_eq!(start.dict.exact("a")[0].word, "开心");
        assert!(start.expressions.kaomoji.library.is_some());
        assert!(start
            .expressions
            .emoji
            .error
            .as_ref()
            .unwrap()
            .contains("emoji.dict"));
        let store = RuntimeStore::new(start);
        let old = store.snapshot().revision;
        let request = store.request();
        let failure = files
            .build(config.clone(), profile, LoadPolicy::Reload)
            .unwrap_err();
        assert!(failure.dependencies.contains(&files.0.join("emoji.dict")));
        assert_eq!(store.snapshot().revision, old);
        std::fs::write(
            files.0.join("emoji.dict"),
            library(ExpressionKind::Emoji, "😄").encode().unwrap(),
        )
        .unwrap();
        assert!(store.publish(
            request,
            files.build(config, profile, LoadPolicy::Reload).unwrap()
        ));
        assert!(store.snapshot().expressions.emoji.library.is_some());
    }
}
