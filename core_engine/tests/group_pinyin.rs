use core_engine::{
    group_input::Position,
    group_search::{query, SearchStatus, WORK_BUDGET},
    Dictionary,
};
fn positions(text: &str) -> Vec<Position> {
    text.chars().map(Position::exact).collect()
}
#[test]
fn normalized_groups_respect_explicit_syllable_boundaries_and_no_initials() {
    let dict=Dictionary::from_text("---\ntype: pinyin\nmax_code_len: 64\n---\nxian\t先\t100\nxi'an\t西安\t90\nni'hao\t你好\t80\n").unwrap();
    let mut groups = positions("xian");
    groups[0] = Position {
        characters: "wxyz".into(),
        selected: None,
    };
    let result = query(&dict, &groups, WORK_BUDGET);
    assert_eq!(result.status, SearchStatus::Complete);
    assert_eq!(
        result
            .candidates
            .iter()
            .map(|m| m.text.as_str())
            .collect::<Vec<_>>(),
        vec!["先", "西安"]
    );
    assert_eq!(
        query(&dict, &positions("xi'an"), WORK_BUDGET).candidates[0].text,
        "西安"
    );
    assert!(query(&dict, &positions("x'ian"), WORK_BUDGET)
        .candidates
        .is_empty());
    assert!(query(&dict, &positions("nh"), WORK_BUDGET)
        .candidates
        .is_empty());
    assert_eq!(
        query(&dict, &positions("nihao"), WORK_BUDGET).candidates[0].codes,
        vec!["ni'hao"]
    );
    groups[0].selected = Some('w');
    assert!(query(&dict, &groups, WORK_BUDGET).candidates.is_empty());
}
#[test]
fn digit_groups_match_independent_enumeration_for_multisyllable_codes() {
    let dict=Dictionary::from_text("---\ntype: pinyin\nmax_code_len: 64\n---\nni\t你\t20\nmi\t米\t10\nni'hao\t你好\t30\nmi'hao\t米好\t5\n").unwrap();
    let groups = vec![
        Position {
            characters: "mno".into(),
            selected: None,
        },
        Position {
            characters: "ghi".into(),
            selected: None,
        },
    ];
    let result = query(&dict, &groups, WORK_BUDGET);
    assert_eq!(
        result
            .candidates
            .iter()
            .map(|m| m.text.as_str())
            .collect::<Vec<_>>(),
        vec!["你", "米", "你好", "米好"]
    );
    assert_eq!(
        query(&dict, &groups, 1).status,
        SearchStatus::BudgetExceeded
    );
}

#[test]
fn pinyin_group_session_commits_without_exposing_hint_and_returns_to_main() {
    use core_engine::{
        group_input::{Definition, MappingIdentity},
        runtime::RuntimeSnapshot,
        session::*,
        *,
    };
    use std::sync::Arc;
    let temporary = temporary::TemporaryTables {
        pinyin: Some(
            Dictionary::from_text("---\ntype: pinyin\nmax_code_len: 64\n---\nni'hao\t你好\t30\n")
                .unwrap(),
        ),
        ..Default::default()
    };
    let runtime = RuntimeSnapshot {
        revision: 1,
        dict: Dictionary::from_text("a\t工\n").unwrap(),
        config: Config::default(),
        temporary: Arc::new(temporary),
        expressions: Arc::default(),
        config_path: "c".into(),
        system_table_path: "s".into(),
        user_table_path: "u".into(),
    };
    let mut s = Session::new(&runtime, BehaviorProfile::TouchV1, HostContext::default());
    let identity = MappingIdentity {
        id: "pinyin".into(),
        version: "1".into(),
        revision: 1,
    };
    let apply = |s: &mut Session, a| {
        let plan = s.prepare(a).unwrap();
        s.acknowledge(plan.token, HostOutcome::Applied);
        plan
    };
    apply(&mut s, Action::BeginTemporary(TemporaryMode::Pinyin));
    apply(
        &mut s,
        Action::BindMapping(
            identity.clone(),
            Definition {
                charset: "mno".into(),
                groups: [("6".into(), "mno".into())].into(),
            },
        ),
    );
    assert_eq!(
        apply(&mut s, Action::InputGroup(identity, "6".into())).op,
        EditSessionOp::NoOp
    );
    for c in "ihao".chars() {
        apply(&mut s, Action::InputCode(c));
    }
    let window = s.candidate_window(0, 10);
    assert_eq!(window.candidates[0].text, "你好");
    let plan = apply(
        &mut s,
        Action::SelectCandidate(window.query_revision, window.candidates[0].id),
    );
    assert!(matches!(plan.op,EditSessionOp::CommitAndReplace{ref text,..} if text=="你好"));
    assert_eq!(s.state_machine().mode(), InputMode::Main);
}
