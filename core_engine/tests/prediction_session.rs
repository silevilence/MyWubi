use core_engine::{runtime::RuntimeSnapshot, session::*, *};
use std::sync::Arc;

fn runtime(enabled: bool, revision: u64) -> RuntimeSnapshot {
    let mut config = Config::default();
    config.prediction.enabled = enabled;
    config.basic.auto_commit_unique = false;
    let dict = Dictionary::from_text("b\t北\nbj\t北京\nj\t京\nt\t天气\np\t预报\n").unwrap();
    RuntimeSnapshot {
        dict,
        config,
        revision,
        temporary: Arc::default(),
        config_path: "c".into(),
        system_table_path: "s".into(),
        user_table_path: "u".into(),
    }
}
fn session(profile: BehaviorProfile) -> Session {
    Session::new(
        &runtime(true, 1),
        profile,
        HostContext {
            editor: 7,
            epoch: 1,
            ..Default::default()
        },
    )
}
fn apply(s: &mut Session, a: Action) -> EditSessionOp {
    let p = s.prepare(a).unwrap();
    assert_eq!(
        s.acknowledge(p.token, HostOutcome::Applied).status,
        AckStatus::Published
    );
    p.op
}
fn north(s: &mut Session) {
    apply(s, Action::Legacy(InputEvent::Char('b')));
    apply(s, Action::Legacy(InputEvent::Space));
}
fn choose(s: &Session, text: &str) -> Action {
    let w = s.candidate_window(0, 64);
    Action::SelectCandidate(
        w.query_revision,
        w.candidates.iter().find(|c| c.text == text).unwrap().id,
    )
}
#[test]
fn both_profiles_predict_only_after_ack_and_chain_word_remainders_without_duplication() {
    for profile in [BehaviorProfile::TouchV1, BehaviorProfile::LegacyV1] {
        let mut s = session(profile);
        apply(&mut s, Action::Legacy(InputEvent::Char('b')));
        let p = s.prepare(Action::Legacy(InputEvent::Space)).unwrap();
        assert!(!s.snapshot().prediction);
        s.acknowledge(p.token, HostOutcome::Applied);
        assert!(s.snapshot().prediction);
        let action = choose(&s, "京");
        assert_eq!(
            apply(&mut s, action),
            EditSessionOp::CommitAndReplace {
                text: "京".into(),
                replace_len: 0
            }
        );
        let action = choose(&s, "天气");
        assert_eq!(
            apply(&mut s, action),
            EditSessionOp::CommitAndReplace {
                text: "天气".into(),
                replace_len: 0
            }
        );
        assert!(s
            .candidate_window(0, 64)
            .candidates
            .iter()
            .any(|c| c.text == "预报"));
    }
}
#[test]
fn new_code_ignores_predictions_and_old_clicks_cannot_commit() {
    let mut s = session(BehaviorProfile::TouchV1);
    north(&mut s);
    let old = choose(&s, "京");
    assert!(matches!(
        apply(&mut s, Action::InputCode('t')),
        EditSessionOp::CompositionUpdate { .. }
    ));
    assert!(!s.snapshot().prediction);
    assert_eq!(
        s.prepare(old),
        Err(SessionError::Rejected(Rejection::StaleCandidate))
    );
    assert_eq!(s.state_machine().spelling(), "t");
}
#[test]
fn rejection_can_retry_but_uncertain_writes_and_editor_changes_invalidate_prediction() {
    for profile in [BehaviorProfile::TouchV1, BehaviorProfile::LegacyV1] {
        let mut s = session(profile);
        north(&mut s);
        let action = choose(&s, "京");
        let p = s.prepare(action.clone()).unwrap();
        assert_eq!(
            s.acknowledge(p.token, HostOutcome::RejectedBeforeWrite)
                .status,
            AckStatus::Retained
        );
        assert!(s.snapshot().prediction);
        let p = s.prepare(action.clone()).unwrap();
        s.acknowledge(p.token, HostOutcome::OutcomeUnknown);
        assert!(!s.has_prediction());
        s.invalidate_context(
            HostContext {
                editor: 8,
                epoch: 2,
                ..Default::default()
            },
            true,
        );
        assert_eq!(
            s.prepare(action),
            Err(SessionError::Rejected(Rejection::StaleCandidate))
        );
    }
}
#[test]
fn sensitive_editors_disabled_config_and_external_edits_never_predict() {
    let mut s = session(BehaviorProfile::TouchV1);
    north(&mut s);
    let old = choose(&s, "京");
    s.clear_prediction_context();
    assert!(!s.snapshot().visible);
    assert_eq!(
        s.prepare(old),
        Err(SessionError::Rejected(Rejection::StaleCandidate))
    );
    s.invalidate_context(
        HostContext {
            purpose: InputPurpose::Password,
            ..Default::default()
        },
        false,
    );
    apply(&mut s, Action::InsertLiteral("北京".into()));
    assert!(!s.has_prediction());
    let mut s = Session::new(
        &runtime(false, 1),
        BehaviorProfile::LegacyV1,
        HostContext::default(),
    );
    north(&mut s);
    assert!(!s.has_prediction());
}
#[test]
fn space_and_enter_ignore_suggestions_and_runtime_disable_preserves_pending_code() {
    let mut s = session(BehaviorProfile::TouchV1);
    north(&mut s);
    let p = s.prepare(Action::Space).unwrap();
    assert_eq!(p.disposition, Disposition::NotHandled);
    assert_eq!(p.op, EditSessionOp::NoOp);
    s.acknowledge(p.token, HostOutcome::Applied);
    assert!(!s.has_prediction());
    north(&mut s);
    let p = s.prepare(Action::Enter).unwrap();
    assert_eq!(p.disposition, Disposition::NotHandled);
    s.acknowledge(p.token, HostOutcome::Applied);
    north(&mut s);
    apply(&mut s, Action::InputCode('b'));
    assert!(s
        .apply_runtime(Arc::new(runtime(false, 2)))
        .unwrap()
        .is_none());
    assert_eq!(s.state_machine().spelling(), "b");
    assert!(s.snapshot().visible);
    assert!(!s.has_prediction());
}

#[test]
fn replacing_owned_preedit_keeps_confirmed_prefix_across_separate_code_commits() {
    for profile in [BehaviorProfile::LegacyV1, BehaviorProfile::TouchV1] {
        let mut s = session(profile);
        north(&mut s);
        apply(&mut s, Action::Legacy(InputEvent::Char('j')));
        apply(&mut s, Action::Legacy(InputEvent::Space));
        assert!(s
            .candidate_window(0, 64)
            .candidates
            .iter()
            .any(|c| c.text == "天气"));
    }
}
