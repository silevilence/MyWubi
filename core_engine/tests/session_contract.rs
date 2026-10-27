use core_engine::{runtime::RuntimeSnapshot, session::*, *};
use std::sync::Arc;

fn runtime(revision: u64) -> Arc<RuntimeSnapshot> {
    Arc::new(RuntimeSnapshot {
        revision,
        dict: Dictionary::from_text("---\ntemporary:\n  key: '`'\n---\na\t甲\naaaa\t王\nb\t工\n")
            .unwrap(),
        temporary: Arc::new(TemporaryTables {
            pinyin: Some(Dictionary::from_text("---\ntype: pinyin\n---\nni\t你\n").unwrap()),
            ..Default::default()
        }),
        config: {
            let mut c = Config::default();
            c.basic.auto_commit_unique = false;
            c
        },
        config_path: "c".into(),
        system_table_path: "s".into(),
        user_table_path: "u".into(),
    })
}
fn session(profile: BehaviorProfile) -> Session {
    Session::new(
        &runtime(1),
        profile,
        HostContext {
            editor: 7,
            epoch: 1,
            ..Default::default()
        },
    )
}
fn apply(s: &mut Session, a: Action) -> PreparedEdit {
    let p = s.prepare(a).unwrap();
    assert!(s.is_current(p.token));
    assert_eq!(
        s.acknowledge(p.token, HostOutcome::Applied).status,
        AckStatus::Published
    );
    p
}
fn text(s: &mut Session, text: &str) {
    for c in text.chars() {
        apply(s, Action::InputText(c, TextIntent::Normal));
    }
}
fn legacy_text(s: &mut Session, text: &str) {
    for c in text.chars() {
        apply(s, Action::Legacy(InputEvent::Char(c)));
    }
}

#[test]
fn touch_preview_is_not_published_busy_does_not_consume_next_key_and_ack_is_unique() {
    let mut s = session(BehaviorProfile::TouchV1);
    let p = s.prepare(Action::InputCode('a')).unwrap();
    assert_eq!(s.status(), SessionStatus::AwaitingAck);
    assert_eq!(s.state_machine().spelling(), "");
    assert!(!s.snapshot().visible);
    assert_eq!(s.candidate_window(0, 10).total, 0);
    assert_eq!(s.prepare(Action::Space), Err(SessionError::Busy));
    assert_eq!(
        s.acknowledge(p.token, HostOutcome::Applied).status,
        AckStatus::Published
    );
    assert_eq!(s.candidate_window(0, 10).total, 2);
    assert_eq!(
        s.acknowledge(p.token, HostOutcome::Applied).status,
        AckStatus::Stale
    );
    assert_eq!(
        apply(&mut s, Action::Space).op,
        EditSessionOp::CommitAndReplace {
            text: "甲".into(),
            replace_len: 0
        }
    );
}

#[test]
fn active_mode_switch_waits_for_success_and_rejection_preserves_old_input() {
    let mut s = session(BehaviorProfile::TouchV1);
    text(&mut s, "a");
    let switch = s.prepare(Action::SetBaseMode(BaseMode::English)).unwrap();
    assert_eq!(s.base_mode(), BaseMode::Main);
    assert_eq!(s.state_machine().spelling(), "a");
    assert_eq!(
        s.acknowledge(switch.token, HostOutcome::RejectedBeforeWrite)
            .status,
        AckStatus::Retained
    );
    assert_eq!(s.state_machine().spelling(), "a");
    apply(&mut s, Action::SetBaseMode(BaseMode::English));
    assert_eq!(s.base_mode(), BaseMode::English);
    apply(&mut s, Action::BeginTemporary(TemporaryMode::Pinyin));
    text(&mut s, "ni");
    apply(&mut s, Action::Space);
    assert_eq!(s.base_mode(), BaseMode::English);
}

#[test]
fn legacy_nested_input_advances_processing_before_ack_and_outer_cannot_publish() {
    let mut s = session(BehaviorProfile::LegacyV1);
    let outer = s.prepare(Action::Legacy(InputEvent::Char('a'))).unwrap();
    assert_eq!(s.state_machine().spelling(), "a");
    assert!(!s.snapshot().visible);
    let inner = s.prepare(Action::Legacy(InputEvent::Char('a'))).unwrap();
    assert!(!s.is_current(outer.token));
    s.acknowledge(inner.token, HostOutcome::Applied);
    assert_eq!(
        s.acknowledge(outer.token, HostOutcome::OutcomeUnknown)
            .status,
        AckStatus::Stale
    );
    assert_eq!(s.snapshot().spelling, "aa");
    assert_eq!(s.status(), SessionStatus::Ready);
}

#[test]
fn legacy_only_temporary_handle_rolls_back_not_switch_or_runtime_adoption() {
    let mut s = session(BehaviorProfile::LegacyV1);
    legacy_text(&mut s, "`1ni");
    let p = s.prepare(Action::Legacy(InputEvent::Space)).unwrap();
    assert_eq!(s.state_machine().mode(), InputMode::Main);
    assert_eq!(
        s.acknowledge(p.token, HostOutcome::RejectedBeforeWrite)
            .status,
        AckStatus::Retained
    );
    assert_eq!(s.state_machine().spelling(), "ni");
    let p = s.set_input_gate(false).unwrap().unwrap();
    assert!(!s.input_gate());
    assert_eq!(
        s.acknowledge(p.token, HostOutcome::RejectedBeforeWrite)
            .status,
        AckStatus::NeedsCleanup
    );
    assert_eq!(s.state_machine().mode(), InputMode::Main);
    let p = s.apply_runtime(runtime(2)).unwrap().unwrap();
    assert_eq!(s.revision(), 2);
    s.acknowledge(p.token, HostOutcome::RejectedBeforeWrite);
    assert_eq!(s.revision(), 2);
}

#[test]
fn partial_and_unknown_results_never_replay_commits_and_recovery_preserves_intent() {
    for outcome in [
        HostOutcome::PartiallyApplied { completed_steps: 1 },
        HostOutcome::OutcomeUnknown,
    ] {
        let mut s = session(BehaviorProfile::TouchV1);
        text(&mut s, "a");
        let p = s.prepare(Action::Space).unwrap();
        s.acknowledge_with_ownership(p.token, outcome, true);
        assert_eq!(s.status(), SessionStatus::NeedsCleanup);
        let cleanup = s.recover().unwrap().unwrap();
        assert_eq!(
            cleanup.op,
            EditSessionOp::EndComposition { delete_text: false }
        );
        s.acknowledge_with_ownership(cleanup.token, HostOutcome::OutcomeUnknown, true);
        let retry = s.recover().unwrap().unwrap();
        assert_eq!(retry.op, cleanup.op);
        s.acknowledge(retry.token, HostOutcome::Applied);
        assert_eq!(s.status(), SessionStatus::Ready);
        assert_eq!(s.last_outcome(), Some(HostOutcome::Applied));
    }
    let mut s = session(BehaviorProfile::TouchV1);
    apply(&mut s, Action::BeginTemporary(TemporaryMode::Pinyin));
    text(&mut s, "ni");
    let cancel = s.lose_focus().unwrap();
    s.acknowledge_with_ownership(cancel.token, HostOutcome::OutcomeUnknown, true);
    assert_eq!(
        s.recover().unwrap().unwrap().op,
        EditSessionOp::EndComposition { delete_text: true }
    );
}

#[test]
fn context_change_and_close_interrupt_pending_and_do_not_delete_new_selection() {
    let mut s = session(BehaviorProfile::TouchV1);
    text(&mut s, "a");
    let old = s.prepare(Action::Space).unwrap();
    let new = HostContext {
        editor: 8,
        epoch: 2,
        ..Default::default()
    };
    s.invalidate_context(new, true);
    assert!(!s.is_current(old.token));
    assert!(s.recover().unwrap().is_none());
    assert_eq!(
        s.acknowledge(old.token, HostOutcome::Applied).status,
        AckStatus::Stale
    );
    let p = s.prepare(Action::InputCode('b')).unwrap();
    s.close();
    assert!(!s.is_current(p.token));
    assert_eq!(s.prepare(Action::Space), Err(SessionError::Closed));
    assert_eq!(
        s.acknowledge(p.token, HostOutcome::Applied).status,
        AckStatus::Stale
    );
}

#[test]
fn local_preedit_host_uses_current_selection_and_unknown_result_is_not_retried() {
    let mut s = Session::new(
        &runtime(1),
        BehaviorProfile::TouchV1,
        HostContext {
            supports_preedit: false,
            ..Default::default()
        },
    );
    let p = apply(&mut s, Action::InputCode('a'));
    assert_eq!(p.op, EditSessionOp::NoOp);
    assert!(s.snapshot().visible);
    let commit = s.prepare(Action::Space).unwrap();
    assert_eq!(commit.target, EditTarget::CurrentSelection);
    s.acknowledge(commit.token, HostOutcome::OutcomeUnknown);
    assert!(s.recover().unwrap().is_none());
    assert_eq!(s.candidate_window(0, 10).total, 0);
}

#[test]
fn local_base_preedit_is_committed_raw_before_focus_loss_or_runtime_update() {
    for update_runtime in [false, true] {
        let mut s = Session::new(
            &runtime(1),
            BehaviorProfile::TouchV1,
            HostContext {
                supports_preedit: false,
                ..Default::default()
            },
        );
        apply(&mut s, Action::InputCode('a'));
        let finish = if update_runtime {
            s.apply_runtime(runtime(2)).unwrap().unwrap()
        } else {
            s.lose_focus().unwrap()
        };
        assert_eq!(
            finish.op,
            EditSessionOp::CommitAndReplace {
                text: "a".into(),
                replace_len: 0
            }
        );
        assert_eq!(finish.target, EditTarget::CurrentSelection);
        s.acknowledge(finish.token, HostOutcome::Applied);
        assert_eq!(s.state_machine().spelling(), "");
        assert_eq!(s.candidate_window(0, 10).total, 0);
    }
}

#[test]
fn deferred_runtime_is_applied_after_ack_and_only_latest_is_adopted() {
    let mut s = session(BehaviorProfile::TouchV1);
    let p = s.prepare(Action::InputCode('a')).unwrap();
    assert!(s.apply_runtime(runtime(2)).unwrap().is_none());
    assert!(s.apply_runtime(runtime(3)).unwrap().is_none());
    assert_eq!(s.revision(), 1);
    let ack = s.acknowledge(p.token, HostOutcome::Applied);
    let cleanup = ack.follow_up.unwrap();
    assert_eq!(s.revision(), 3);
    assert!(s.is_current(cleanup.token));
    s.acknowledge(cleanup.token, HostOutcome::Applied);
    assert_eq!(s.candidate_window(0, 10).total, 0);
}

#[test]
fn password_input_is_literal_without_candidate_previews() {
    let mut s = Session::new(
        &runtime(1),
        BehaviorProfile::TouchV1,
        HostContext {
            purpose: InputPurpose::Password,
            ..Default::default()
        },
    );
    let p = apply(&mut s, Action::InputCode('a'));
    assert_eq!(
        p.op,
        EditSessionOp::CommitAndReplace {
            text: "a".into(),
            replace_len: 0
        }
    );
    assert!(!s.snapshot().visible);
    assert_eq!(s.candidate_window(0, 10).total, 0);
    assert!(matches!(
        s.prepare(Action::BeginTemporary(TemporaryMode::Pinyin)),
        Err(SessionError::Rejected(_))
    ));
    assert!(!s.would_handle(&Action::Enter));
    assert_eq!(
        apply(&mut s, Action::Enter).disposition,
        Disposition::NotHandled
    );
    let p = apply(&mut s, Action::Legacy(InputEvent::Char('a')));
    assert_eq!(
        p.op,
        EditSessionOp::CommitAndReplace {
            text: "a".into(),
            replace_len: 0
        }
    );
    assert_eq!(s.candidate_window(0, 10).total, 0);
}

#[test]
fn unknown_ownership_is_not_inferred_from_a_previous_successful_preedit() {
    let mut s = session(BehaviorProfile::TouchV1);
    text(&mut s, "a");
    assert!(s.context().owns_preedit);
    let p = s.prepare(Action::Space).unwrap();
    s.acknowledge(p.token, HostOutcome::OutcomeUnknown);
    assert!(!s.context().owns_preedit);
    assert!(s.recover().unwrap().is_none());
    assert_eq!(s.candidate_window(0, 10).total, 0);
}

#[test]
fn changed_context_adopts_latest_waiting_runtime_without_waiting_for_stale_ack() {
    let mut s = session(BehaviorProfile::TouchV1);
    let p = s.prepare(Action::InputCode('a')).unwrap();
    s.apply_runtime(runtime(3)).unwrap();
    s.apply_runtime(runtime(2)).unwrap();
    s.invalidate_context(
        HostContext {
            editor: 9,
            epoch: 2,
            ..Default::default()
        },
        true,
    );
    assert_eq!(s.revision(), 3);
    assert_eq!(s.status(), SessionStatus::Ready);
    assert_eq!(
        s.acknowledge(p.token, HostOutcome::Applied).status,
        AckStatus::Stale
    );
    assert!(s.apply_runtime(runtime(1)).unwrap().is_none());
}

#[test]
fn none_keeps_view_and_overflow_has_one_edit_operation_with_converted_attribute() {
    let mut s = session(BehaviorProfile::LegacyV1);
    legacy_text(&mut s, "aaaa");
    let before = s.snapshot().clone();
    let p = apply(&mut s, Action::Legacy(InputEvent::PagePrev));
    assert_eq!(p.preview, core_engine::candidates::ViewUpdate::Unchanged);
    assert_eq!(*s.snapshot(), before);
    let p = apply(&mut s, Action::Legacy(InputEvent::Char('b')));
    assert_eq!(
        p.op,
        EditSessionOp::CommitThenUpdate {
            text: "王".into(),
            replace_len: 4,
            spelling: "b".into(),
            attr: DisplayAttrKind::Converted
        }
    );
}
