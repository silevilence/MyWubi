use core_engine::{group_input::*, runtime::RuntimeSnapshot, session::*, *};
use std::sync::Arc;

fn identity() -> MappingIdentity {
    MappingIdentity {
        id: "test.pairs".into(),
        version: "1".into(),
        revision: 7,
    }
}
fn definition() -> Definition {
    Definition {
        charset: "abcdefghijklmnopqrstuvwxyz".into(),
        groups: [("jk".into(), "jk".into())].into(),
    }
}
fn session(purpose: InputPurpose) -> Session {
    let mut config = Config::default();
    config.basic.auto_commit_unique = false;
    let runtime = RuntimeSnapshot {
        revision: 1,
        dict: Dictionary::from_text("j\t甲\nkj\t乙\n").unwrap(),
        expressions: Arc::default(),
        temporary: Arc::default(),
        config,
        config_path: "c".into(),
        system_table_path: "s".into(),
        user_table_path: "u".into(),
    };
    Session::new(
        &runtime,
        BehaviorProfile::TouchV1,
        HostContext {
            purpose,
            ..Default::default()
        },
    )
}
fn apply(s: &mut Session, action: Action) -> PreparedEdit {
    let plan = s.prepare(action).unwrap();
    s.acknowledge(plan.token, HostOutcome::Applied);
    plan
}
#[test]
fn group_is_one_position_and_hint_never_enters_host() {
    let mut s = session(InputPurpose::Text);
    apply(&mut s, Action::BindMapping(identity(), definition()));
    let plan = apply(&mut s, Action::InputGroup(identity(), "jk".into()));
    assert_eq!(plan.op, EditSessionOp::NoOp);
    assert_eq!(
        s.state_machine().group_sequence().unwrap().positions.len(),
        1
    );
    assert_eq!(s.snapshot().spelling, "[jk]");
    assert!(s.prepare(Action::Enter).is_err());
    assert!(s.prepare(Action::CommitRaw).is_err());
    let focus = s.lose_focus().unwrap();
    assert_eq!(focus.op, EditSessionOp::NoOp);
}
#[test]
fn stale_and_invalid_events_preserve_state_and_resolution_can_be_undone() {
    let mut s = session(InputPurpose::Text);
    apply(&mut s, Action::BindMapping(identity(), definition()));
    apply(&mut s, Action::InputGroup(identity(), "jk".into()));
    let old = s.candidate_window(0, 0).query_revision;
    let before = s.snapshot().clone();
    let mut stale = identity();
    stale.revision += 1;
    assert!(s.prepare(Action::InputGroup(stale, "jk".into())).is_err());
    assert!(s
        .prepare(Action::InputGroup(identity(), "absent".into()))
        .is_err());
    assert_eq!(s.snapshot(), &before);
    apply(&mut s, Action::ResolveGroup(old, 0, 'j'));
    assert_eq!(
        s.state_machine()
            .group_sequence()
            .unwrap()
            .resolved()
            .as_deref(),
        Some("j")
    );
    assert!(s.prepare(Action::ResolveGroup(old, 0, 'k')).is_err());
    let query = s.candidate_window(0, 0).query_revision;
    apply(&mut s, Action::UndoGroupResolution(query));
    assert!(s
        .state_machine()
        .group_sequence()
        .unwrap()
        .resolved()
        .is_none());
    apply(&mut s, Action::Backspace);
    assert!(s.state_machine().group_sequence().is_none());
}
#[test]
fn definitions_reject_empty_repeated_invalid_and_oversized_groups() {
    for chars in [
        "",
        "jj",
        "?",
        "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ",
    ] {
        let mut invalid = definition();
        invalid.groups.insert("jk".into(), chars.into());
        let mut s = session(InputPurpose::Text);
        assert!(s.prepare(Action::BindMapping(identity(), invalid)).is_err());
        assert!(s.snapshot().spelling.is_empty());
    }
}
#[test]
fn direct_and_password_hosts_reject_group_protocol_before_editing() {
    for purpose in [InputPurpose::Direct, InputPurpose::Password] {
        let mut s = session(purpose);
        assert!(s
            .prepare(Action::InputGroup(identity(), "jk".into()))
            .is_err());
        assert!(s.snapshot().spelling.is_empty());
    }
}
#[test]
fn json_facade_exposes_protocol_limits_without_handles() {
    let response: serde_json::Value = serde_json::from_slice(&core_engine::ffi::call(
        br#"{"version":1,"request":{"op":"KeyboardCapabilities"}}"#,
    ))
    .unwrap();
    assert_eq!(response["value"]["max_positions"], 32);
    assert_eq!(response["value"]["input_groups"], 1);
    let action = Action::InputGroup(identity(), "jk".into());
    assert_eq!(
        serde_json::from_str::<Action>(&serde_json::to_string(&action).unwrap()).unwrap(),
        action
    );
}

#[test]
fn rust_and_jni_share_position_fixtures() {
    let fixtures:serde_json::Value=serde_json::from_str(include_str!("fixtures/protocol/groups.json")).unwrap();
    let mut s=session(InputPurpose::Text);
    for fixture in fixtures.as_array().unwrap() {
        let action=serde_json::from_value(fixture["action"].clone()).unwrap();
        let plan=apply(&mut s,action);
        assert_eq!(plan.op,EditSessionOp::NoOp);
        assert_eq!(s.snapshot().spelling,fixture["hint"].as_str().unwrap());
        assert_eq!(s.state_machine().group_sequence().map_or(0,|s|s.positions.len()),fixture["positions"].as_u64().unwrap() as usize);
    }
}
