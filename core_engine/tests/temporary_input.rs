use core_engine::{
    Dictionary, InputEvent as E, InputMode, StateMachine, TemporaryTables, Transition as T,
};
use std::sync::Arc;

fn setup() -> StateMachine {
    let main = Dictionary::from_text("---\ntemporary:\n  key: '`'\n---\na\t主\t1\n").unwrap();
    let pinyin = Dictionary::from_text("---\ntype: pinyin\nmax_code_len: 64\n---\nni'hao\t你好\t10\nni'hao'ma\t你好吗\t1\nzhong'guo'ren\t中国人\t1\n").unwrap();
    let english =
        Dictionary::from_text("---\nwildcard_key: z\n---\nbrb\tbe right back\t1\n").unwrap();
    let mut sm = StateMachine::new(main);
    sm.set_temporary_tables(Arc::new(TemporaryTables {
        pinyin: Some(pinyin),
        english: Some(english),
        ..Default::default()
    }));
    sm
}

fn type_text(sm: &mut StateMachine, text: &str) {
    for c in text.chars() {
        assert!(sm.would_consume(&E::Char(c)), "{c}");
        assert!(
            !matches!(sm.handle(E::Char(c)), T::Commit(_)),
            "unexpected auto commit: {text}"
        );
    }
}

fn enter(sm: &mut StateMachine, index: usize) {
    assert!(sm.would_consume(&E::Char('`')));
    assert!(
        matches!(sm.handle(E::Char('`')), T::Candidates { spelling, .. } if spelling.is_empty())
    );
    assert!(sm.would_consume(&E::Select(index)));
    sm.handle(E::Select(index));
}

#[test]
fn commits_once_and_the_next_key_uses_the_main_table() {
    for (mode, text, event, expected) in [
        (1, "nihao", E::Space, "你好"),
        (1, "nihao", E::Char('2'), "你好吗"),
        (1, "ni'hao", E::Enter, "ni'hao"),
        (1, "zz", E::Space, "zz"),
        (2, "brb", E::Space, "be right back"),
        (2, "brb", E::Enter, "brb"),
        (2, "Hello!", E::Space, "Hello!"),
    ] {
        let mut sm = setup();
        enter(&mut sm, mode);
        type_text(&mut sm, text);
        assert_eq!(sm.handle(event), T::Commit(expected.into()));
        assert_eq!(sm.mode(), InputMode::Main);
        sm.handle(E::Char('a'));
        assert_eq!(sm.candidates(), ["主"]);
    }
}

#[test]
fn empty_modes_cancel_without_committing_menu_labels() {
    for event in [E::Backspace, E::Esc] {
        let mut sm = setup();
        sm.handle(E::Char('`'));
        assert!(sm.would_consume(&event));
        assert_eq!(sm.handle(event.clone()), T::Cleared);
        enter(&mut sm, 1);
        assert_eq!(sm.handle(E::Enter), T::None);
        assert_eq!(sm.handle(E::Space), T::None);
        assert_eq!(sm.handle(event), T::Cleared);
    }
}

#[test]
fn backspace_to_empty_preserves_mode_until_next_backspace() {
    let mut sm = setup();
    enter(&mut sm, 1);
    type_text(&mut sm, "n");
    sm.handle(E::Backspace);
    assert_eq!(sm.mode(), InputMode::Pinyin);
    assert!(sm.spelling().is_empty());
    assert_eq!(sm.handle(E::Backspace), T::Cleared);
    assert_eq!(sm.mode(), InputMode::Main);
}

#[test]
fn unavailable_modes_remain_in_menu_and_english_without_dictionary_works() {
    let mut sm = setup();
    sm.set_temporary_tables(Arc::default());
    enter(&mut sm, 1);
    assert_eq!(sm.mode(), InputMode::Menu);
    sm.handle(E::Char('2'));
    type_text(&mut sm, "hello");
    assert_eq!(sm.handle(E::Space), T::Commit("hello".into()));
}

#[test]
fn functional_configuration_is_not_active_inside_temporary_modes() {
    let mut sm = setup();
    enter(&mut sm, 2);
    type_text(&mut sm, "brz");
    assert!(sm.candidates().is_empty()); // z must not act as wildcard for brb.
    type_text(&mut sm, "`");
    assert_eq!(sm.mode(), InputMode::English);
    assert_eq!(sm.handle(E::Enter), T::Commit("brz`".into()));
    sm.handle(E::Char('a'));
    sm.handle(E::Char('`'));
    assert_eq!(sm.mode(), InputMode::Main); // existing main spelling cannot open menu.
}

#[test]
fn invalid_candidate_number_does_not_commit_or_modify_spelling() {
    let mut sm = setup();
    enter(&mut sm, 2);
    type_text(&mut sm, "brb");
    assert_eq!(sm.handle(E::Char('9')), T::None);
    assert_eq!(sm.spelling(), "brb");
    assert_eq!(sm.mode(), InputMode::English);
}

#[test]
fn apostrophes_are_editable_constraints_and_raw_commits_keep_them() {
    let mut sm = setup();
    enter(&mut sm, 1);
    assert_eq!(sm.handle(E::Char('\'')), T::None);
    type_text(&mut sm, "zhong'");
    assert_eq!(sm.handle(E::Char('\'')), T::None);
    sm.handle(E::Backspace);
    assert_eq!(sm.spelling(), "zhong");
    assert_eq!(sm.candidates(), ["中国人"]);
    sm.handle(E::Esc);
    enter(&mut sm, 1);
    type_text(&mut sm, "ni'h'ma");
    assert!(sm.candidates().is_empty());
    for _ in 0..3 {
        sm.handle(E::Backspace);
    }
    assert_eq!(sm.spelling(), "ni'h");
    assert_eq!(sm.candidates(), ["你好", "你好吗"]);
    assert_eq!(sm.handle(E::Enter), T::Commit("ni'h".into()));
    enter(&mut sm, 1);
    type_text(&mut sm, "ni'h'ma");
    assert_eq!(sm.handle(E::Space), T::Commit("ni'h'ma".into()));
}

#[test]
fn english_punctuation_and_long_words_never_commit_automatically() {
    for tables in [true, false] {
        let mut sm = setup();
        if !tables {
            sm.set_temporary_tables(Arc::default());
        }
        enter(&mut sm, 2);
        type_text(&mut sm, "brb;LongWord.Email@example.com!");
        assert_eq!(sm.handle(E::Char('1')), T::None);
        assert_eq!(
            sm.handle(E::Enter),
            T::Commit("brb;LongWord.Email@example.com!".into())
        );
        assert_eq!(sm.mode(), InputMode::Main);
    }
}

#[test]
fn temporary_candidates_page_without_changing_commit_rules() {
    let main = Dictionary::from_text("---\ntemporary:\n  key: '`'\n---\na\t主\n").unwrap();
    let english = Dictionary::from_text("aa\tfirst\t10\nab\tsecond\t1\n").unwrap();
    let mut sm = StateMachine::with_options(main, 1, true);
    sm.set_temporary_tables(Arc::new(TemporaryTables {
        english: Some(english),
        ..Default::default()
    }));
    enter(&mut sm, 2);
    type_text(&mut sm, "a");
    assert_eq!(sm.candidates(), ["first"]);
    sm.handle(E::PageNext);
    assert_eq!(sm.candidates(), ["second"]);
    assert_eq!(sm.handle(E::Char('1')), T::Commit("second".into()));
    assert_eq!(sm.mode(), InputMode::Main);
}
