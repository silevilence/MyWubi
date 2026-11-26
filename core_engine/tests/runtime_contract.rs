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

#[test]
fn empty_user_dictionary_reuses_immutable_system_snapshot() {
    let files = Files::new();
    let system = Dictionary::from_text("a\t甲\t10\n").unwrap();
    for contents in [None, Some("# empty user dictionary\n")] {
        if let Some(contents) = contents {
            files.write("user.dict", contents);
        }
        let merged =
            merge_dictionary(system.clone(), &files.0.join("user.dict"), &files.config()).unwrap();
        assert!(std::sync::Arc::ptr_eq(&system, &merged));
        assert_eq!(merged.exact("a")[0].word, "甲");
    }
    files.write("user.dict", "a\t甲\t999\n");
    let merged =
        merge_dictionary(system.clone(), &files.0.join("user.dict"), &files.config()).unwrap();
    assert!(!std::sync::Arc::ptr_eq(&system, &merged));
    assert_eq!(merged.exact("a")[0].weight, 999);
    assert_eq!(system.exact("a")[0].weight, 10);
}

#[test]
fn additive_dictionary_keeps_signed_scores_and_system_bytes() {
    let f = Files::new();
    let main = "a\t甲\t100\na\t乙\t80\naa\t甲\t20\n";
    f.write("main.dict", main);
    f.write(
        "user.dict",
        "# mywubi-user-dict version=2\na\t甲\t-30\na\t丙\t-8\naa\t甲\t9223372036854775807\n",
    );
    let snapshot = f.build(LoadPolicy::Reload).unwrap();
    let scores: Vec<_> = snapshot
        .dict
        .exact("a")
        .iter()
        .map(|e| (e.word.as_str(), e.weight))
        .collect();
    assert_eq!(scores, [("乙", 80), ("甲", 70), ("丙", -8)]);
    assert_eq!(snapshot.dict.exact("aa")[0].weight, i64::MAX);
    assert_eq!(
        std::fs::read_to_string(f.0.join("main.dict")).unwrap(),
        main
    );
}

#[test]
fn migration_and_import_preserve_legacy_weights_and_new_signed_values() {
    let f = Files::new();
    f.write("main.dict", "a\t甲\t100\na\t乙\t80\n");
    f.write("user.dict", "a\t甲\t70\n");
    let path = f.0.join("user.dict");
    let base = f.0.join("main.dict");
    let mut user = UserDictionary::for_edit(&path, Some(&base)).unwrap();
    assert_eq!(user.entries()[0].weight, -30);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "a\t甲\t70\n");
    f.write("import.dict", "a\t乙\t60\nb\t丙\t9\n");
    user.import(f.0.join("import.dict")).unwrap();
    user.export(f.0.join("export.dict")).unwrap();
    let mut roundtrip = UserDictionary::for_edit(&f.0.join("other.dict"), Some(&base)).unwrap();
    roundtrip.import(f.0.join("export.dict")).unwrap();
    assert_eq!(roundtrip.entries(), user.entries());
    let s = f.build(LoadPolicy::Reload).unwrap();
    assert_eq!(
        s.dict
            .exact("a")
            .iter()
            .map(|e| e.weight)
            .collect::<Vec<_>>(),
        [70, 60]
    );
}

#[test]
fn moving_a_word_can_split_ties_and_reuses_its_adjustment() {
    let f = Files::new();
    f.write(
        "main.dict",
        "a\t甲\t9\na\t乙\t9\na\t丙\t8\na\t丁\t5\na\t戊\t1\n",
    );
    let path = f.0.join("user.dict");
    for (rank, expected) in [(2, "甲戊乙丙丁"), (1, "戊甲乙丙丁"), (3, "甲乙戊丙丁")]
    {
        let mut user = UserDictionary::for_edit(&path, Some(&f.0.join("main.dict"))).unwrap();
        user.move_to_rank(&f.build(LoadPolicy::Reload).unwrap().dict, "a", "戊", rank)
            .unwrap();
        assert_eq!(user.entries().len(), 1);
        let s = f.build(LoadPolicy::Reload).unwrap();
        assert_eq!(
            s.dict
                .exact("a")
                .iter()
                .map(|e| e.word.as_str())
                .collect::<String>(),
            expected
        );
    }
}
