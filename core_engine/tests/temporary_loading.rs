use core_engine::{config::Hotkey, Dictionary, TemporaryTables};
use std::path::PathBuf;

struct Bundle(PathBuf);
impl Bundle {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "mywubi-aux-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        Self(root)
    }
    fn write(&self, name: &str, contents: &str) {
        std::fs::write(self.0.join(name), contents).unwrap();
    }
}
impl Drop for Bundle {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn missing_corrupt_and_restored_auxiliary_tables_leave_main_usable() {
    let bundle = Bundle::new();
    bundle.write("main.dict", "---\ntemporary:\n  key: '`'\n  pinyin_table: py.dict\n  reverse_lookup: true\n---\na\t你好\n");
    let main_path = bundle.0.join("main.dict");
    let main = Dictionary::load(&main_path).unwrap();
    let load = || TemporaryTables::load(&main, &main_path, &Hotkey::default());
    let missing = load();
    assert_eq!(missing.paths, [bundle.0.join("py.dict")]);
    assert_eq!(missing.load_errors.len(), 1);
    assert!(missing.english_error.is_none());
    assert_eq!(main.exact("a")[0].word, "你好");
    bundle.write("py.dict", "bad file");
    assert!(!load().load_errors.is_empty());
    // The auxiliary points back to main; this must never recursively load.
    bundle.write("py.dict", "---\ntype: pinyin\nmax_code_len: 64\ntemporary:\n  key: '`'\n  pinyin_table: main.dict\n---\nni'hao\t你好\n");
    let valid = load();
    assert!(valid.load_errors.is_empty());
    assert!(valid.pinyin.is_some());
    assert_eq!(valid.reverse.unwrap().codes("你好"), ["a"]);
    std::fs::remove_file(bundle.0.join("py.dict")).unwrap();
    assert!(!load().load_errors.is_empty());
}

#[test]
fn absolute_english_reference_and_relocated_bundle_load_correctly() {
    let bundle = Bundle::new();
    let external = Bundle::new();
    external.write("en.dict", "brb\tbe right back\n");
    bundle.write("py.dict", "---\ntype: pinyin\n---\nni\t你\n");
    let main_text = format!(
        "---\ntemporary:\n  key: '`'\n  pinyin_table: py.dict\n  english_table: '{}'\n---\na\t主\n",
        external.0.join("en.dict").display()
    );
    bundle.write("main.dict", &main_text);
    let main = Dictionary::load(bundle.0.join("main.dict")).unwrap();
    let load = TemporaryTables::load(&main, &bundle.0.join("main.dict"), &Hotkey::default());
    assert!(load.load_errors.is_empty());
    assert_eq!(load.english.unwrap().exact("brb")[0].word, "be right back");
    let moved = Bundle::new();
    std::fs::copy(bundle.0.join("py.dict"), moved.0.join("py.dict")).unwrap();
    moved.write("main.dict", &main_text);
    let moved_main = Dictionary::load(moved.0.join("main.dict")).unwrap();
    assert!(
        TemporaryTables::load(&moved_main, &moved.0.join("main.dict"), &Hotkey::default())
            .load_errors
            .is_empty()
    );
}
