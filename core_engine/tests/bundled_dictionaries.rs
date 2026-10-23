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
    let main = Dictionary::from_text(
        "---\ntemporary:\n  key: '`'\n  pinyin_table: pinyin.dict\n  english_table: english.dict\n---\na\t工\t1\n",
    )
    .unwrap();
    let tables = Arc::new(TemporaryTables::load(
        &main,
        &table_path("wubi86.dict"),
        &Hotkey::default(),
    ));
    assert!(tables.load_errors.is_empty(), "{:?}", tables.load_errors);
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
}
