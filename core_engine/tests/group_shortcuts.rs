use core_engine::{candidates::CandidateHint, group_input::*, shortcut_time::*, shortcuts::*, *};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
struct ClockCount(AtomicUsize);
impl Clock for ClockCount {
    fn now(&self) -> Result<LocalTime, String> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(LocalTime {
            year: 2026,
            month: 11,
            day: 14,
            hour: 8,
            minute: 9,
            second: 10,
            utc_offset_seconds: 28800,
            timezone: "Asia/Shanghai".into(),
        })
    }
}
fn identity() -> MappingIdentity {
    MappingIdentity {
        id: "pairs".into(),
        version: "1".into(),
        revision: 1,
    }
}
fn engine() -> (SemanticEngine, Arc<ClockCount>) {
    let dict =
        Dictionary::from_text("---\nmax_code_len: 4\n---\njjad\t2026\t100\njjad\t普通词\t90\n")
            .unwrap();
    let mut sm = StateMachine::with_options(dict, 1, true);
    sm.set_shortcut_rules(&[
        ShortcutRule {
            code: "jjad".into(),
            position: 2,
            kind: ShortcutKind::DateTime,
            format: "yyyy".into(),
            enabled: true,
        },
        ShortcutRule {
            code: "jjad".into(),
            position: 4,
            kind: ShortcutKind::DateTime,
            format: "yyyy".into(),
            enabled: true,
        },
    ])
    .unwrap();
    let clock = Arc::new(ClockCount(AtomicUsize::new(0)));
    sm.set_clock(clock.clone());
    let mut engine = SemanticEngine::new(sm, BehaviorProfile::TouchV1);
    engine.dispatch(Action::BindMapping(
        identity(),
        Definition {
            charset: "jka d".replace(' ', ""),
            groups: [("pair".into(), "jk".into())].into(),
        },
    ));
    (engine, clock)
}
fn ambiguous(engine: &mut SemanticEngine) {
    engine.dispatch(Action::InputGroup(identity(), "pair".into()));
    for c in "jad".chars() {
        engine.dispatch(Action::InputCode(c));
    }
}
#[test]
fn unresolved_rule_never_reads_clock_resolved_rule_keeps_duplicate_identities_and_frozen_pages() {
    let (mut engine, clock) = engine();
    ambiguous(&mut engine);
    assert_eq!(clock.0.load(Ordering::SeqCst), 0);
    assert!(engine.state_machine().group_shortcut_protected());
    assert!(engine.candidate_window(0, 10).candidates.iter().all(|c| !c
        .hints
        .iter()
        .any(|h| matches!(h, CandidateHint::DateTime(_)))));
    let revision = engine.candidate_window(0, 0).query_revision;
    engine.dispatch(Action::ResolveGroup(revision, 0, 'j'));
    let all = engine.candidate_window(0, 10);
    assert_eq!(all.total, 4);
    assert_eq!(
        all.candidates
            .iter()
            .map(|c| c.text.as_str())
            .collect::<Vec<_>>(),
        vec!["2026", "2026", "普通词", "2026"]
    );
    assert!(!all.candidates[0]
        .hints
        .iter()
        .any(|h| matches!(h, CandidateHint::DateTime(_))));
    assert!(all.candidates[1]
        .hints
        .iter()
        .any(|h| matches!(h, CandidateHint::DateTime(_))));
    for _ in 0..3 {
        engine.dispatch(Action::MoveCandidatePage(true));
        engine.candidate_window(0, 10);
    }
    assert_eq!(clock.0.load(Ordering::SeqCst), 1);
    engine.dispatch(Action::Space);
    assert!(engine.state_machine().committed_shortcut());
}
#[test]
fn explicit_ordinary_choice_enter_and_undo_keep_distinct_rules() {
    let (mut engine, clock) = engine();
    ambiguous(&mut engine);
    let old = engine.candidate_window(0, 0).query_revision;
    engine.dispatch(Action::ResolveGroup(old, 0, 'j'));
    let current = engine.candidate_window(0, 0).query_revision;
    engine.dispatch(Action::UndoGroupResolution(current));
    assert_eq!(clock.0.load(Ordering::SeqCst), 1);
    assert!(engine.state_machine().shortcut_snapshot().is_none());
    let revision = engine.candidate_window(0, 0).query_revision;
    engine.dispatch(Action::ResolveGroup(revision, 0, 'j'));
    assert_eq!(clock.0.load(Ordering::SeqCst), 2);
    let window = engine.candidate_window(0, 1);
    engine.dispatch(Action::SelectCandidate(
        window.query_revision,
        window.candidates[0].id,
    ));
    assert!(!engine.state_machine().committed_shortcut());
    ambiguous(&mut engine);
    let revision = engine.candidate_window(0, 0).query_revision;
    engine.dispatch(Action::ResolveGroup(revision, 0, 'j'));
    assert_eq!(
        engine.dispatch(Action::Enter).transition,
        Transition::Commit("jjad".into())
    );
}

#[test]
fn group_clock_retry_unknown_write_and_rule_reload_follow_session_lifecycle() {
    use core_engine::{runtime::RuntimeSnapshot, session::*};
    let mut config = Config::default();
    config.basic.auto_commit_unique = false;
    config.shortcuts.rules = vec![ShortcutRule {
        code: "jjad".into(),
        position: 1,
        kind: ShortcutKind::DateTime,
        format: "yyyy".into(),
        enabled: true,
    }];
    let make = |revision, config| {
        Arc::new(RuntimeSnapshot {
            revision,
            config,
            dict: Dictionary::from_text("jjad\t普通词\t1\n").unwrap(),
            temporary: Arc::default(),
            expressions: Arc::default(),
            config_path: "c".into(),
            system_table_path: "s".into(),
            user_table_path: "u".into(),
        })
    };
    let runtime = make(1, config.clone());
    let clock = Arc::new(ClockCount(AtomicUsize::new(0)));
    let mut session = Session::new_with_clock(
        &runtime,
        BehaviorProfile::TouchV1,
        HostContext::default(),
        clock.clone(),
    );
    let apply = |s: &mut Session, a| {
        let p = s.prepare(a).unwrap();
        s.acknowledge(p.token, HostOutcome::Applied);
    };
    apply(
        &mut session,
        Action::BindMapping(
            identity(),
            Definition {
                charset: "jk".into(),
                groups: [("pair".into(), "jk".into())].into(),
            },
        ),
    );
    apply(&mut session, Action::InputGroup(identity(), "pair".into()));
    for c in "jad".chars() {
        apply(&mut session, Action::InputCode(c));
    }
    let revision = session.candidate_window(0, 0).query_revision;
    apply(&mut session, Action::ResolveGroup(revision, 0, 'j'));
    let view = session.candidate_window(0, 1);
    let select = Action::SelectCandidate(view.query_revision, view.candidates[0].id);
    let first = session.prepare(select.clone()).unwrap();
    session.acknowledge(first.token, HostOutcome::RejectedBeforeWrite);
    let retry = session.prepare(select).unwrap();
    assert_eq!(retry.op, first.op);
    assert_eq!(clock.0.load(Ordering::SeqCst), 1);
    session.acknowledge(retry.token, HostOutcome::OutcomeUnknown);
    if let Some(cleanup) = session.recover().unwrap() {
        assert!(!matches!(
            cleanup.op,
            EditSessionOp::CommitAndReplace { .. }
        ));
        session.acknowledge(cleanup.token, HostOutcome::Applied);
    }
    // A changed rule set invalidates the local group query without writing raw markers.
    apply(
        &mut session,
        Action::BindMapping(
            identity(),
            Definition {
                charset: "jk".into(),
                groups: [("pair".into(), "jk".into())].into(),
            },
        ),
    );
    apply(&mut session, Action::InputGroup(identity(), "pair".into()));
    let stale = session.candidate_window(0, 0).query_revision;
    config.shortcuts.rules.clear();
    if let Some(plan) = session.apply_runtime(make(2, config)).unwrap() {
        assert_eq!(plan.op, EditSessionOp::NoOp);
        session.acknowledge(plan.token, HostOutcome::Applied);
    }
    assert_ne!(session.candidate_window(0, 0).query_revision, stale);
    assert!(session.state_machine().group_sequence().is_none());
}

#[test]
fn uppercase_group_resolves_only_shortcut_case_and_long_prefix_stays_bounded() {
    let (mut engine, clock) = engine();
    engine.dispatch(Action::BindMapping(
        identity(),
        Definition {
            charset: "JK".into(),
            groups: [("pair".into(), "JK".into())].into(),
        },
    ));
    engine.dispatch(Action::InputGroup(identity(), "pair".into()));
    for c in "JAD".chars() {
        engine.dispatch(Action::InputCode(c));
    }
    assert_eq!(clock.0.load(Ordering::SeqCst), 0);
    let revision = engine.candidate_window(0, 0).query_revision;
    engine.dispatch(Action::ResolveGroup(revision, 0, 'J'));
    assert_eq!(clock.0.load(Ordering::SeqCst), 1);
    assert_eq!(
        engine.dispatch(Action::Enter).transition,
        Transition::Commit("JJAD".into())
    );
    let dict = Dictionary::from_text("---\nmax_code_len: 1\n---\nj\t甲\n").unwrap();
    let mut sm = StateMachine::with_options(dict, 5, true);
    sm.set_shortcut_rules(&[ShortcutRule {
        code: "j".repeat(32),
        position: 1,
        kind: ShortcutKind::DateTime,
        format: "yyyy".into(),
        enabled: true,
    }])
    .unwrap();
    let clock = Arc::new(ClockCount(AtomicUsize::new(0)));
    sm.set_clock(clock.clone());
    let mut engine = SemanticEngine::new(sm, BehaviorProfile::TouchV1);
    engine.dispatch(Action::BindMapping(
        identity(),
        Definition {
            charset: "jk".into(),
            groups: [("pair".into(), "jk".into())].into(),
        },
    ));
    for _ in 0..32 {
        engine.dispatch(Action::InputGroup(identity(), "pair".into()));
    }
    assert_eq!(
        engine
            .state_machine()
            .group_sequence()
            .unwrap()
            .positions
            .len(),
        32
    );
    engine.dispatch(Action::InputGroup(identity(), "pair".into()));
    assert_eq!(
        engine
            .state_machine()
            .group_sequence()
            .unwrap()
            .positions
            .len(),
        32
    );
    assert_eq!(clock.0.load(Ordering::SeqCst), 0);
}
