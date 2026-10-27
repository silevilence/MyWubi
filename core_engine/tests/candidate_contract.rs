use core_engine::*;

fn engine() -> SemanticEngine {
    SemanticEngine::new(
        StateMachine::with_options(
            Dictionary::from_text("a\t甲\t3\nab\t乙\t2\nac\t丙\t1\n").unwrap(),
            1,
            false,
        ),
        BehaviorProfile::TouchV1,
    )
}

#[test]
fn window_is_bounded_and_does_not_change_page_or_query_identity() {
    let mut e = engine();
    e.dispatch(Action::InputCode('a'));
    let first = e.candidate_window(0, usize::MAX);
    assert_eq!(first.candidates.len(), 3);
    assert_eq!(e.state_machine().candidates(), ["甲"]);
    e.dispatch(Action::MoveCandidatePage(true));
    let second = e.candidate_window(0, 3);
    assert_eq!(first.query_revision, second.query_revision);
    assert_eq!(first.candidates, second.candidates);
    assert!(second.view_revision > first.view_revision);
    assert_eq!(e.state_machine().page(), 1);
    assert!(e.candidate_window(usize::MAX, 10).candidates.is_empty());
    let selected = e.dispatch(Action::SelectCandidate(
        first.query_revision,
        first.candidates[2].id,
    ));
    assert_eq!(selected.transition, Transition::Commit("丙".into()));
}

#[test]
fn stale_cross_session_and_invalidated_queries_cannot_select_same_position() {
    let mut a = engine();
    let mut b = engine();
    a.dispatch(Action::InputCode('a'));
    b.dispatch(Action::InputCode('a'));
    let view = a.candidate_window(0, 1);
    let action = Action::SelectCandidate(view.query_revision, view.candidates[0].id);
    assert_eq!(
        b.dispatch(action.clone()).disposition,
        Disposition::Rejected(Rejection::StaleCandidate)
    );
    a.invalidate_query();
    assert_eq!(
        a.dispatch(action).disposition,
        Disposition::Rejected(Rejection::StaleCandidate)
    );
    assert_eq!(a.state_machine().spelling(), "a");
}

#[test]
fn structured_hint_is_never_committed_and_none_keeps_view_revision() {
    let mut e = engine();
    e.dispatch(Action::InputCode('a'));
    let view = e.candidate_window(1, 1);
    assert_eq!(
        view.candidates[0].hints,
        [core_engine::candidates::CandidateHint::CodeSuffix(
            "b".into()
        )]
    );
    assert_eq!(
        e.dispatch(Action::MoveCandidatePage(false)).transition,
        Transition::None
    );
    assert_eq!(e.candidate_window(1, 1).view_revision, view.view_revision);
    assert_eq!(
        e.dispatch(Action::SelectCandidate(
            view.query_revision,
            view.candidates[0].id
        ))
        .transition,
        Transition::Commit("乙".into())
    );
}
