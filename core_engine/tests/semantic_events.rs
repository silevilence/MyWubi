use core_engine::*;

fn engine() -> SemanticEngine {
    SemanticEngine::new(
        StateMachine::with_options(
            Dictionary::from_text("---\ncharset: a1\n---\na\t工\na1\t甲\n").unwrap(),
            5,
            false,
        ),
        BehaviorProfile::TouchV1,
    )
}

#[test]
fn explicit_digits_do_not_select_and_literal_intent_does_not_lookup() {
    let mut engine = engine();
    engine.dispatch(Action::InputCode('a'));
    engine.dispatch(Action::InputText('1', TextIntent::Normal));
    assert_eq!(engine.state_machine().spelling(), "a1");
    assert_eq!(
        engine.dispatch(Action::Space).transition,
        Transition::Commit("甲".into())
    );
    assert_eq!(
        engine
            .dispatch(Action::InputText('1', TextIntent::Literal))
            .transition,
        Transition::Commit("1".into())
    );
}

#[test]
fn legacy_mapping_is_exact_for_events_and_preflight_is_pure() {
    for tail in [
        InputEvent::Space,
        InputEvent::Enter,
        InputEvent::Char('1'),
        InputEvent::Select(9),
        InputEvent::PageNext,
        InputEvent::Symbol('，'),
        InputEvent::Esc,
    ] {
        let mut engine = engine();
        let mut old = engine.state_machine().clone();
        for event in [InputEvent::Char('a'), tail] {
            let action = Action::from(event.clone());
            let before = engine.state_machine().spelling().to_owned();
            assert_eq!(engine.would_handle(&action), old.would_consume(&event));
            assert_eq!(engine.state_machine().spelling(), before);
            assert_eq!(engine.dispatch(action).transition, old.handle(event));
            assert_eq!(engine.state_machine().spelling(), old.spelling());
            assert_eq!(engine.state_machine().candidates(), old.candidates());
        }
    }
}

#[test]
fn declared_symbols_and_wildcards_are_code_but_literal_text_commits_once() {
    let dict =
        Dictionary::from_text("---\ncharset: 'a1;*'\nwildcard_key: '*'\n---\na;\t甲\na1\t乙\n")
            .unwrap();
    let mut engine = SemanticEngine::new(
        StateMachine::with_options(dict, 5, false),
        BehaviorProfile::TouchV1,
    );
    for c in "a*".chars() {
        engine.dispatch(Action::InputText(c, TextIntent::Normal));
    }
    assert_eq!(engine.state_machine().candidates().len(), 2);
    let result = engine.dispatch(Action::InsertLiteral("🙂12".into()));
    assert!(matches!(result.transition, Transition::Commit(s) if s.ends_with("🙂12")));
    assert!(engine.state_machine().spelling().is_empty());
}

#[test]
fn undeclared_touch_digit_is_literal_while_explicit_code_never_selects() {
    let sm = StateMachine::with_options(Dictionary::from_text("a\t工\n").unwrap(), 5, false);
    let mut normal = SemanticEngine::new(sm.clone(), BehaviorProfile::TouchV1);
    normal.dispatch(Action::InputCode('a'));
    assert_eq!(
        normal
            .dispatch(Action::InputText('1', TextIntent::Normal))
            .transition,
        Transition::Commit("工1".into())
    );
    let mut explicit = SemanticEngine::new(sm, BehaviorProfile::TouchV1);
    explicit.dispatch(Action::InputCode('a'));
    explicit.dispatch(Action::InputCode('1'));
    assert_eq!(explicit.state_machine().spelling(), "a1");
}
