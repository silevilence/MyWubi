use core_engine::*;
use std::sync::Arc;

fn engine(profile: BehaviorProfile) -> SemanticEngine {
    let main = Dictionary::from_text("a\t主\n").unwrap();
    let mut sm = StateMachine::with_options(main, 5, false);
    sm.set_temporary_tables(Arc::new(TemporaryTables {
        pinyin: Some(Dictionary::from_text("---\ntype: pinyin\n---\nni\t你\n").unwrap()),
        english: Some(Dictionary::from_text("---\nmax_code_len: 64\n---\nhello\thello\n").unwrap()),
        ..Default::default()
    }));
    SemanticEngine::new(sm, profile)
}

fn text(engine: &mut SemanticEngine, text: &str) {
    for c in text.chars() {
        engine.dispatch(Action::InputText(c, TextIntent::Normal));
    }
}

#[test]
fn persistent_english_and_temporary_pinyin_restore_base_without_entry_key() {
    let mut e = engine(BehaviorProfile::TouchV1);
    e.dispatch(Action::SetBaseMode(BaseMode::English));
    text(&mut e, "he");
    assert_eq!(
        e.dispatch(Action::Space).transition,
        Transition::Commit("hello ".into())
    );
    assert_eq!(e.base_mode(), BaseMode::English);
    e.dispatch(Action::BeginTemporary(TemporaryMode::Pinyin));
    for a in [Action::Space, Action::Enter] {
        assert_eq!(e.dispatch(a).transition, Transition::None);
    }
    text(&mut e, "ni");
    assert_eq!(
        e.dispatch(Action::Space).transition,
        Transition::Commit("你".into())
    );
    assert_eq!(e.temporary_mode(), None);
    assert_eq!(e.base_mode(), BaseMode::English);
    assert_eq!(
        e.dispatch(Action::Enter).disposition,
        Disposition::NotHandled
    );
    text(&mut e, "he");
    assert_eq!(
        e.dispatch(Action::InputText('2', TextIntent::Normal))
            .transition,
        Transition::Commit("he2".into())
    );
}

#[test]
fn duplicate_requests_and_unavailable_targets_preserve_input() {
    let mut e = engine(BehaviorProfile::TouchV1);
    text(&mut e, "a");
    assert_eq!(
        e.dispatch(Action::SetBaseMode(BaseMode::Main)).transition,
        Transition::None
    );
    assert_eq!(e.state_machine().spelling(), "a");
    assert_eq!(
        e.dispatch(Action::BeginTemporary(TemporaryMode::Pinyin))
            .transition,
        Transition::Commit("a".into())
    );
    text(&mut e, "ni");
    assert_eq!(
        e.dispatch(Action::BeginTemporary(TemporaryMode::Pinyin))
            .transition,
        Transition::None
    );
    assert_eq!(e.state_machine().spelling(), "ni");
    let mut e = SemanticEngine::new(
        StateMachine::new(Dictionary::from_text("a\t主\n").unwrap()),
        BehaviorProfile::TouchV1,
    );
    text(&mut e, "a");
    assert!(matches!(
        e.dispatch(Action::BeginTemporary(TemporaryMode::Pinyin))
            .disposition,
        Disposition::Rejected(_)
    ));
    assert_eq!(e.state_machine().spelling(), "a");
}

#[test]
fn temporary_english_remains_one_shot_and_return_mode_survives_replacement() {
    for profile in [BehaviorProfile::LegacyV1, BehaviorProfile::TouchV1] {
        let mut e = engine(profile);
        e.dispatch(Action::BeginTemporary(TemporaryMode::English));
        text(&mut e, "he");
        assert_eq!(
            e.dispatch(Action::Space).transition,
            Transition::Commit("hello".into())
        );
        assert_eq!(e.state_machine().mode(), InputMode::Main);
    }
    let mut e = engine(BehaviorProfile::TouchV1);
    e.dispatch(Action::SetBaseMode(BaseMode::English));
    e.dispatch(Action::BeginTemporary(TemporaryMode::Pinyin));
    text(&mut e, "ni");
    assert_eq!(
        e.dispatch(Action::BeginTemporary(TemporaryMode::English))
            .transition,
        Transition::Cleared
    );
    e.dispatch(Action::Cancel);
    assert_eq!(e.base_mode(), BaseMode::English);
    assert_eq!(e.temporary_mode(), None);
}

#[test]
fn touch_english_retains_raw_input_when_completion_is_broken() {
    let mut sm = StateMachine::new(Dictionary::from_text("a\t主\n").unwrap());
    sm.set_temporary_tables(Arc::new(TemporaryTables {
        english_error: Some("bad table".into()),
        ..Default::default()
    }));
    let legacy = SemanticEngine::new(sm.clone(), BehaviorProfile::LegacyV1);
    let touch = SemanticEngine::new(sm, BehaviorProfile::TouchV1);
    assert!(!legacy.capability(TemporaryMode::English).available);
    assert!(touch.capability(TemporaryMode::English).available);
    assert!(!touch.capability(TemporaryMode::English).candidate_support);
}

#[test]
fn clear_round_preserves_temporary_mode_but_end_restores_base() {
    let mut e = engine(BehaviorProfile::TouchV1);
    e.dispatch(Action::SetBaseMode(BaseMode::English));
    e.dispatch(Action::BeginTemporary(TemporaryMode::Pinyin));
    text(&mut e, "ni");
    let old = e.candidate_window(0, 1);
    e.clear_composition();
    assert_eq!(e.temporary_mode(), Some(TemporaryMode::Pinyin));
    assert_eq!(e.state_machine().spelling(), "");
    assert_ne!(e.candidate_window(0, 1).query_revision, old.query_revision);
    assert_eq!(
        e.dispatch(Action::CommitPreferred).transition,
        Transition::None
    );
    assert_eq!(e.temporary_mode(), Some(TemporaryMode::Pinyin));
    e.end_temporary();
    assert_eq!(e.base_mode(), BaseMode::English);
    assert_eq!(e.temporary_mode(), None);
}
