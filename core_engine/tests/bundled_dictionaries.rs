use core_engine::{
    config::Hotkey, Dictionary, InputEvent, InputMode, SearchOptions, StateMachine, TableType,
    TemporaryTables, Transition,
};
use std::{path::Path, sync::Arc};

fn table_path(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tables")
        .join(name)
}

#[test]
fn bundled_wubi_has_real_codes_weights_and_valid_configuration() {
    let dictionary = Dictionary::load(table_path("wubi86.dict")).unwrap();
    assert_eq!(dictionary.len(), 136_239);
    let config = dictionary.table_config();
    assert_eq!(config.display_name, "五笔86（Rime）");
    assert_eq!(config.table_type, TableType::Shape);
    assert_eq!(config.wildcard_key, Some('z'));
    let report = core_engine::validate_table(config, dictionary.entries(), 10);
    assert!(report.is_valid(), "{:?}", report.issues);
    for (code, word) in [
        ("a", "工"),
        ("g", "一"),
        ("wq", "你"),
        ("gggg", "王"),
        ("wqiy", "你"),
        ("lgyi", "国"),
        ("ggtt", "五笔"),
    ] {
        let matches = dictionary.search(code, SearchOptions::default());
        assert_eq!(
            (matches[0].code.as_str(), matches[0].word.as_str()),
            (code, word)
        );
    }
    assert_eq!(
        dictionary.search("gggg", SearchOptions::default())[0].weight,
        827_000_000
    );
}

#[test]
fn bundled_pinyin_loads_and_preserves_readings_and_boundaries() {
    let dictionary = Dictionary::load(table_path("pinyin.dict")).unwrap();
    assert_eq!(dictionary.table_config().table_type, TableType::Pinyin);
    assert_eq!(dictionary.len(), 65_125);
    for (input, expected) in [
        ("nihao", "你好"),
        ("ni'hao", "你好"),
        ("xi'an", "西安"),
        ("chongqing", "重庆"),
        ("lv", "绿"),
    ] {
        assert!(
            dictionary
                .search(input, SearchOptions::default())
                .iter()
                .any(|entry| entry.word == expected),
            "{input} should find {expected}"
        );
    }
    assert!(dictionary
        .search("xi'an", SearchOptions::default())
        .iter()
        .all(|entry| entry.code.starts_with("xi'an")));
}

#[test]
fn bundled_english_loads_and_supports_both_spellings_and_prefix_completion() {
    let dictionary = Dictionary::load(table_path("english.dict")).unwrap();
    assert_eq!(dictionary.len(), 79_650);
    for word in ["hello", "computer", "color", "colour"] {
        assert_eq!(
            dictionary.search(word, SearchOptions::default())[0].word,
            word
        );
    }
    assert_eq!(
        dictionary.search("i", SearchOptions::default())[0].word,
        "I"
    );
    // Temporary English collects all matches before paginating candidates.
    assert!(dictionary
        .search_literal(
            "compu",
            SearchOptions {
                prefer_exact: true,
                limit: dictionary.len(),
            },
        )
        .iter()
        .take(5)
        .any(|entry| entry.word == "computer"));
}

#[test]
fn bundled_tables_work_through_relative_references_and_temporary_input() {
    let main = Dictionary::load(table_path("wubi86.dict")).unwrap();
    let tables = Arc::new(TemporaryTables::load(
        &main,
        &table_path("wubi86.dict"),
        &Hotkey::default(),
    ));
    assert!(tables.load_errors.is_empty(), "{:?}", tables.load_errors);
    assert!(tables
        .reverse
        .as_ref()
        .unwrap()
        .codes("你")
        .iter()
        .any(|code| code == "wqiy"));
    let mut state = StateMachine::new(main);
    state.set_temporary_tables(tables);
    for (input, expected) in [
        ("`1nihao", "你好"),
        ("`1xi'an", "西安"),
        ("`2hello", "hello"),
    ] {
        for character in input.chars() {
            assert!(!matches!(
                state.handle(InputEvent::Char(character)),
                Transition::Commit(_)
            ));
        }
        assert_eq!(
            state.handle(InputEvent::Space),
            Transition::Commit(expected.into())
        );
        assert_eq!(state.mode(), InputMode::Main);
    }
    state.handle(InputEvent::Char('a'));
    assert_eq!(
        state.handle(InputEvent::Space),
        Transition::Commit("工".into())
    );
}
