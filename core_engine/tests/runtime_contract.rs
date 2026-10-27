use core_engine::{runtime::*, *};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static ID: AtomicU64 = AtomicU64::new(0);
struct Files(PathBuf);
impl Files {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "mywubi-runtime-{}-{}",
            std::process::id(),
            ID.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&p).unwrap();
        Self(p)
    }
    fn write(&self, p: &str, s: &str) {
        std::fs::write(self.0.join(p), s).unwrap();
    }
    fn config(&self) -> Config {
        let mut c = Config::default();
        c.dictionary.system_table = "main.dict".into();
        c.dictionary.user_table = "user.dict".into();
        c.dictionary.enable_user_dict = true;
        c
    }
    fn build(&self, policy: LoadPolicy) -> Result<RuntimeSnapshot, BuildError> {
        RuntimeBuilder::build(
            self.0.join("config.toml"),
            self.config(),
            BehaviorProfile::TouchV1,
            policy,
        )
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn merge_overrides_weights_but_reverse_index_only_uses_original_main() {
    let f = Files::new();
    f.write(
        "main.dict",
        "---\ntemporary:\n  reverse_lookup: true\n---\na\t甲\t10\nb\t乙\n",
    );
    f.write("user.dict", "a\t甲\t99\nc\t丙\t5\n");
    let s = f.build(LoadPolicy::Reload).unwrap();
    assert_eq!(s.dict.exact("a")[0].weight, 99);
    assert_eq!(s.dict.exact("c")[0].word, "丙");
    assert!(s.temporary.reverse.as_ref().unwrap().codes("丙").is_empty());
    assert_eq!(s.temporary.reverse.as_ref().unwrap().codes("甲"), ["a"]);
}

#[test]
fn bad_auxiliary_reports_new_dependency_and_failed_or_late_build_keeps_current() {
    let f = Files::new();
    f.write("main.dict", "a\t甲\n");
    let store = RuntimeStore::new(f.build(LoadPolicy::Reload).unwrap());
    let original = store.snapshot();
    let old = store.request();
    let built = f.build(LoadPolicy::Reload).unwrap();
    let latest = store.request();
    f.write(
        "main.dict",
        "---\ntemporary:\n  pinyin_table: missing.dict\n---\na\t乙\n",
    );
    let error = f.build(LoadPolicy::Reload).unwrap_err();
    assert!(error.dependencies.contains(&f.0.join("missing.dict")));
    assert!(!store.publish(old, built));
    assert_eq!(store.snapshot().revision, original.revision);
    assert!(f.build(LoadPolicy::Startup).unwrap().dict.exact("a").len() == 1);
    f.write("missing.dict", "---\ntype: pinyin\n---\nni\t你\n");
    assert!(store.publish(latest, f.build(LoadPolicy::Reload).unwrap()));
    assert_eq!(store.snapshot().dict.exact("a")[0].word, "乙");
}

#[test]
fn invalid_user_data_rejects_reload_and_legacy_startup_falls_back_to_empty() {
    let f = Files::new();
    f.write("main.dict", "a\t甲\n");
    f.write("user.dict", "invalid");
    assert!(f.build(LoadPolicy::Reload).is_err());
    f.config().save(f.0.join("config.toml")).unwrap();
    assert!(RuntimeBuilder::legacy_startup(f.0.join("config.toml"))
        .dict
        .is_empty());
}

#[test]
fn legacy_fallback_keeps_main_usable_without_retrying_auxiliary_loading() {
    let f = Files::new();
    f.write(
        "main.dict",
        "---\ntemporary:\n  key: '`'\n  pinyin_table: py.dict\n---\na\t甲\n",
    );
    f.write("py.dict", "---\ntype: pinyin\n---\nni\t你\n");
    let s = RuntimeBuilder::legacy_fallback(f.0.join("config.toml"), f.config());
    assert_eq!(s.dict.exact("a")[0].word, "甲");
    assert!(s.temporary.pinyin.is_none());
    assert!(s.temporary.paths.is_empty());
}
