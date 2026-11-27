use core_engine::{group_input::*, runtime::RuntimeSnapshot, session::*, *};
use std::sync::Arc;
fn identity() -> MappingIdentity {
    MappingIdentity {
        id: "pairs".into(),
        version: "1".into(),
        revision: 1,
    }
}
fn runtime(revision: u64, auto: bool) -> Arc<RuntimeSnapshot> {
    let mut config = Config::default();
    config.basic.auto_commit_unique = auto;
    Arc::new(RuntimeSnapshot {
        revision,
        dict: Dictionary::from_text("---\nmax_code_len: 1\n---\nj\t甲\nk\t乙\n").unwrap(),
        config,
        temporary: Arc::default(),
        expressions: Arc::default(),
        config_path: "c".into(),
        system_table_path: "s".into(),
        user_table_path: "u".into(),
    })
}
fn session(auto: bool) -> Session {
    let mut s = Session::new(
        &runtime(1, auto),
        BehaviorProfile::TouchV1,
        HostContext {
            supports_preedit: false,
            ..Default::default()
        },
    );
    bind_pairs(&mut s);
    s
}

fn bind_pairs(s: &mut Session) {
    apply(
        s,
        Action::BindMapping(
            identity(),
            Definition {
                charset: "jk".into(),
                groups: [("pair".into(), "jk".into())].into(),
            },
        ),
    );
}
fn apply(s: &mut Session, action: Action) -> PreparedEdit {
    let plan = s.prepare(action).unwrap();
    let ack = s.acknowledge(plan.token, HostOutcome::Applied);
    if let Some(follow) = ack.follow_up {
        s.acknowledge(follow.token, HostOutcome::Applied);
    }
    plan
}
#[test]
fn reject_before_write_keeps_groups_and_stale_selection_cannot_commit() {
    let mut s = session(false);
    apply(&mut s, Action::InputGroup(identity(), "pair".into()));
    let window = s.candidate_window(0, 10);
    let before = s.snapshot().clone();
    let plan = s
        .prepare(Action::SelectCandidate(
            window.query_revision,
            window.candidates[0].id,
        ))
        .unwrap();
    assert!(matches!(plan.op,EditSessionOp::CommitAndReplace{ref text,..} if text=="甲"));
    s.acknowledge(plan.token, HostOutcome::RejectedBeforeWrite);
    assert_eq!(s.snapshot(), &before);
    apply(&mut s, Action::ResolveGroup(window.query_revision, 0, 'k'));
    assert!(s
        .prepare(Action::SelectCandidate(
            window.query_revision,
            window.candidates[0].id
        ))
        .is_err());
    let committed = apply(&mut s, Action::Space);
    assert!(matches!(committed.op,EditSessionOp::CommitAndReplace{ref text,..} if text=="乙"));
}
#[test]
fn ambiguous_full_code_waits_for_resolution_before_unique_autocommit() {
    let mut s = session(true);
    let plan = apply(&mut s, Action::InputGroup(identity(), "pair".into()));
    assert_eq!(plan.op, EditSessionOp::NoOp);
    let query = s.candidate_window(0, 0).query_revision;
    let plan = apply(&mut s, Action::ResolveGroup(query, 0, 'j'));
    assert!(matches!(plan.op,EditSessionOp::CommitAndReplace{ref text,..} if text=="甲"));
}

#[test]
fn fifth_group_commits_the_preferred_word_and_starts_a_fresh_group() {
    let mut r = runtime(1, false);
    Arc::make_mut(&mut r).dict =
        Dictionary::from_text("---\nmax_code_len: 4\n---\njjjj\t甲\t20\nkkkk\t乙\t10\n").unwrap();
    let mut s = Session::new(&r, BehaviorProfile::TouchV1, HostContext::default());
    let group = || Action::InputGroup(identity(), "pair".into());
    bind_pairs(&mut s);
    for _ in 0..4 {
        assert_eq!(apply(&mut s, group()).op, EditSessionOp::NoOp);
    }
    let before = s.snapshot().clone();
    let fifth = s.prepare(group()).unwrap();
    assert!(
        matches!(&fifth.op, EditSessionOp::CommitAndReplace { text, .. } if text == "甲"),
        "{:?}",
        fifth.op
    );
    s.acknowledge(fifth.token, HostOutcome::RejectedBeforeWrite);
    assert_eq!(s.snapshot(), &before);
    assert_eq!(apply(&mut s, group()).op, fifth.op);
    assert_eq!(s.snapshot().spelling, "[jk]");
    for _ in 0..3 {
        apply(&mut s, Action::InputCode('k'));
    }
    let next = apply(&mut s, Action::InputCode('j'));
    assert!(
        matches!(next.op, EditSessionOp::CommitThenUpdate { text, spelling, .. } if text == "乙" && spelling == "j")
    );
    for _ in 0..3 {
        apply(&mut s, Action::InputCode('j'));
    }
    let mixed = apply(&mut s, group());
    assert!(matches!(mixed.op, EditSessionOp::CommitAndReplace { text, .. } if text == "甲"));
    assert_eq!(mixed.target, EditTarget::OwnedComposition);
    assert_eq!(s.snapshot().spelling, "[jk]");
}

#[test]
fn group_overflow_disabled_keeps_both_positions() {
    let mut r = runtime(1, false);
    Arc::make_mut(&mut r)
        .config
        .basic
        .commit_on_max_code_overflow = false;
    let mut s = Session::new(&r, BehaviorProfile::TouchV1, HostContext::default());
    bind_pairs(&mut s);
    for _ in 0..2 {
        assert_eq!(
            apply(&mut s, Action::InputGroup(identity(), "pair".into())).op,
            EditSessionOp::NoOp
        );
    }
    assert_eq!(s.snapshot().spelling, "[jk][jk]");
}
#[test]
fn clearing_a_position_restores_ambiguity_and_enter_requires_resolution() {
    let mut s = session(false);
    apply(&mut s, Action::InputGroup(identity(), "pair".into()));
    let query = s.candidate_window(0, 0).query_revision;
    apply(&mut s, Action::ResolveGroup(query, 0, 'j'));
    let revision = s.candidate_window(0, 0).query_revision;
    apply(&mut s, Action::ClearGroupResolution(revision, 0));
    assert_eq!(s.snapshot().spelling, "[jk]");
    assert!(s.prepare(Action::Enter).is_err());
    let query = s.candidate_window(0, 0).query_revision;
    apply(&mut s, Action::ResolveGroup(query, 0, 'k'));
    let plan = apply(&mut s, Action::Enter);
    assert!(matches!(plan.op,EditSessionOp::CommitAndReplace{ref text,..} if text=="k"));
}
#[test]
fn backspace_deletes_one_position_and_keeps_other_selections() {
    let mut r = runtime(1, false);
    Arc::make_mut(&mut r)
        .config
        .basic
        .commit_on_max_code_overflow = false;
    let mut s = Session::new(&r, BehaviorProfile::TouchV1, HostContext::default());
    bind_pairs(&mut s);
    for _ in 0..2 {
        apply(&mut s, Action::InputGroup(identity(), "pair".into()));
    }
    for (index, letter) in [(1, 'k'), (0, 'j')] {
        let revision = s.candidate_window(0, 0).query_revision;
        apply(&mut s, Action::ResolveGroup(revision, index, letter));
    }
    let revision = s.candidate_window(0, 0).query_revision;
    apply(&mut s, Action::ClearGroupResolution(revision, 1));
    assert_eq!(s.snapshot().spelling, "j[jk]");
    assert!(s
        .prepare(Action::ClearGroupResolution(revision, 0))
        .is_err());
    apply(&mut s, Action::Backspace);
    assert_eq!(s.snapshot().spelling, "j");
    apply(&mut s, Action::Backspace);
    assert!(s.snapshot().spelling.is_empty());
}
#[test]
fn runtime_and_mapping_switch_wait_for_group_input_to_finish() {
    let mut s = session(false);
    apply(&mut s, Action::InputGroup(identity(), "pair".into()));
    assert!(s.apply_runtime(runtime(2, false)).unwrap().is_none());
    assert_eq!(s.revision(), 1);
    assert!(s
        .prepare(Action::BindMapping(
            identity(),
            Definition {
                charset: "jk".into(),
                groups: [("pair".into(), "jk".into())].into()
            }
        ))
        .is_err());
    apply(&mut s, Action::Cancel);
    assert_eq!(s.revision(), 2);
}
#[test]
fn unknown_host_result_never_replays_the_candidate_or_group_hint() {
    let mut s = session(false);
    apply(&mut s, Action::InputGroup(identity(), "pair".into()));
    let plan = s.prepare(Action::Space).unwrap();
    s.acknowledge(plan.token, HostOutcome::OutcomeUnknown);
    if let Some(recovery) = s.recover().unwrap() {
        assert!(!matches!(
            recovery.op,
            EditSessionOp::CommitAndReplace { .. }
                | EditSessionOp::CommitThenUpdate { .. }
                | EditSessionOp::CompositionUpdate { .. }
        ));
    }
}

#[test]
fn group_shortcut_prefix_protection_is_pure_and_ends_on_wrong_prefix() {
    use core_engine::shortcuts::{ShortcutKind, ShortcutRule};
    struct NoClock;
    impl core_engine::shortcut_time::Clock for NoClock {
        fn now(&self) -> Result<core_engine::shortcut_time::LocalTime, String> {
            panic!("ambiguous prefix must not read time")
        }
    }
    let dict = Dictionary::from_text("---\nmax_code_len: 1\n---\nj\t甲\n").unwrap();
    let mut sm = StateMachine::with_options(dict, 5, true);
    sm.set_shortcut_rules(&[ShortcutRule {
        code: "jjad".into(),
        position: 1,
        kind: ShortcutKind::DateTime,
        format: "yyyy-MM-dd".into(),
        enabled: true,
    }])
    .unwrap();
    sm.set_clock(Arc::new(NoClock));
    let mut engine = SemanticEngine::new(sm, BehaviorProfile::TouchV1);
    engine.dispatch(Action::BindMapping(
        identity(),
        Definition {
            charset: "jk".into(),
            groups: [("pair".into(), "jk".into())].into(),
        },
    ));
    engine.dispatch(Action::InputGroup(identity(), "pair".into()));
    for c in "jad".chars() {
        engine.dispatch(Action::InputCode(c));
        assert!(engine.state_machine().group_shortcut_protected());
    }
    assert_eq!(
        engine
            .state_machine()
            .group_sequence()
            .unwrap()
            .positions
            .len(),
        4
    );
    assert!(engine.state_machine().shortcut_snapshot().is_none());
    engine.dispatch(Action::InputCode('x'));
    assert!(!engine.state_machine().group_shortcut_protected());
}
