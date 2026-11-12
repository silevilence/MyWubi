use core_engine::{candidates::*, expressions::*, runtime::RuntimeSnapshot, session::*, *};
use std::sync::Arc;

fn library(kind: ExpressionKind, count: usize) -> Arc<ExpressionLibrary> {
    Arc::new(
        ExpressionLibrary::new(
            ExpressionDocument {
                kind,
                name: "测试".into(),
                version: "1".into(),
                sources: vec![ExpressionSource {
                    id: "test".into(),
                    url: "local:test".into(),
                    revision: "1".into(),
                    license: "MIT".into(),
                    notice: "Test data".into(),
                }],
                entries: (0..count)
                    .map(|i| ExpressionEntry {
                        text: if i == 0 {
                            "🚳".into()
                        } else {
                            format!("o\u{308}　\\_/{i}")
                        },
                        name: "禁止自行车".into(),
                        triggers: vec!["禁止自行车".into(), "开心".into()],
                        tags: vec![],
                        aliases: vec![],
                        order: i as u32,
                        sources: vec!["test".into()],
                    })
                    .collect(),
            },
            kind,
        )
        .unwrap(),
    )
}

fn runtime() -> RuntimeSnapshot {
    let mut config = Config::default();
    config.basic.auto_commit_unique = false;
    config.basic.candidate_count = 3;
    config.expressions.emoji = ExpressionSelection {
        append: true,
        search: true,
        resource: None,
    };
    config.expressions.kaomoji = config.expressions.emoji.clone();
    RuntimeSnapshot {
        dict: Dictionary::from_text(
            "a\t禁止\nb\t自行车\nc\t禁止自行车\nd\t开心\ne\to\nf\t\u{308}\ng\t👩🏽‍💻\n",
        )
        .unwrap(),
        temporary: Arc::new(TemporaryTables {
            pinyin: Some(
                Dictionary::from_text("---\ntype: pinyin\nmax_code_len: 64\n---\njin'zhi\t禁止\n")
                    .unwrap(),
            ),
            ..Default::default()
        }),
        expressions: Arc::new(ExpressionResources {
            emoji: ExpressionResource {
                library: Some(library(ExpressionKind::Emoji, 612)),
                ..Default::default()
            },
            kaomoji: ExpressionResource {
                library: Some(library(ExpressionKind::Kaomoji, 2)),
                ..Default::default()
            },
            ..Default::default()
        }),
        config,
        revision: 1,
        config_path: "config.toml".into(),
        system_table_path: "main.dict".into(),
        user_table_path: "user.dict".into(),
    }
}

fn apply(s: &mut Session, a: Action) -> PreparedEdit {
    let edit = s.prepare(a).unwrap();
    assert_eq!(
        s.acknowledge(edit.token, HostOutcome::Applied).status,
        AckStatus::Published
    );
    edit
}
fn local(s: &mut Session, a: Action) {
    assert_eq!(apply(s, a).op, EditSessionOp::NoOp);
    assert!(!s.context().owns_preedit);
    assert!(!s.has_prediction());
}
fn begin(s: &mut Session) {
    local(s, Action::BeginTemporary(TemporaryMode::Emoji));
}
fn word(s: &mut Session, c: char) {
    local(s, Action::InputCode(c));
    local(s, Action::Space);
}
fn profiles() -> [BehaviorProfile; 2] {
    [BehaviorProfile::LegacyV1, BehaviorProfile::TouchV1]
}

#[test]
fn literal_kana_alias_round_is_local_and_commits_only_selected_result() {
    for profile in profiles() {
        let mut r = runtime();
        r.config.expressions.kana.search = true;
        Arc::get_mut(&mut r.expressions).unwrap().kana = ExpressionResource {
            library: Some(Arc::new(
                ExpressionLibrary::load(
                    &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("../expressions/kana.mwe"),
                    ExpressionKind::Kana,
                )
                .unwrap(),
            )),
            ..Default::default()
        };
        let mut s = Session::new(&r, profile, HostContext::default());
        apply(&mut s, Action::OpenModeChooser);
        let menu = s.candidate_window(0, 20);
        assert_eq!(menu.total, 8);
        local(
            &mut s,
            Action::SelectCandidate(menu.query_revision, menu.candidates[4].id),
        );
        assert_eq!(
            s.snapshot().expression.as_ref().unwrap().kind,
            ExpressionKind::Kana
        );
        local(&mut s, Action::SetExpressionInput(ExpressionInput::Literal));
        for c in "shi".chars() {
            local(&mut s, Action::InputText(c, TextIntent::Normal));
        }
        assert_eq!(s.snapshot().expression.as_ref().unwrap().text, "shi");
        assert!(matches!(
            s.prepare(Action::SetExpressionInput(ExpressionInput::Words)),
            Err(SessionError::Rejected(_))
        ));
        local(&mut s, Action::FindExpressions);
        let results = s.candidate_window(0, 10);
        assert_eq!(
            &results.candidates[..2]
                .iter()
                .map(|c| c.text.as_str())
                .collect::<Vec<_>>(),
            &["し", "シ"]
        );
        let edit = apply(&mut s, Action::InputText('1', TextIntent::Normal));
        assert_eq!(
            edit.op,
            EditSessionOp::CommitAndReplace {
                text: "し".into(),
                replace_len: 0
            }
        );
        assert!(s.snapshot().expression.is_none());
        assert_eq!(s.state_machine().mode(), InputMode::Main);
    }
}

#[test]
fn literal_editing_keeps_digits_symbols_graphemes_and_requires_clear_before_switch() {
    for profile in profiles() {
        let mut s = Session::new(&runtime(), profile, HostContext::default());
        begin(&mut s);
        local(&mut s, Action::SetExpressionInput(ExpressionInput::Literal));
        for c in "1+".chars() {
            local(&mut s, Action::InputText(c, TextIntent::Normal));
        }
        local(&mut s, Action::InsertLiteral("👩🏽‍💻".into()));
        local(&mut s, Action::Backspace);
        assert_eq!(s.snapshot().expression.as_ref().unwrap().text, "1+");
        local(&mut s, Action::Enter);
        assert_eq!(s.snapshot().expression.as_ref().unwrap().text, "1+");
        local(&mut s, Action::FindExpressions);
        assert_eq!(
            s.snapshot().expression.as_ref().unwrap().phase,
            ExpressionPhase::Results
        );
        assert_eq!(s.candidate_window(0, 9).total, 0);
        local(&mut s, Action::EditExpressionQuery);
        assert_eq!(s.snapshot().expression.as_ref().unwrap().text, "1+");
        local(&mut s, Action::ClearExpressionQuery);
        local(&mut s, Action::SetExpressionInput(ExpressionInput::Words));
        word(&mut s, 'a');
        assert_eq!(s.snapshot().expression.as_ref().unwrap().text, "禁止");
        assert!(matches!(
            s.prepare(Action::SetExpressionInput(ExpressionInput::Literal)),
            Err(SessionError::Rejected(_))
        ));
        local(&mut s, Action::ClearExpressionQuery);
        local(&mut s, Action::SetExpressionInput(ExpressionInput::Literal));
        local(&mut s, Action::InsertLiteral("x".repeat(512)));
        assert!(matches!(
            s.prepare(Action::InsertLiteral("x".into())),
            Err(SessionError::Rejected(_))
        ));
        assert_eq!(s.snapshot().expression.as_ref().unwrap().text.len(), 512);
        local(&mut s, Action::EndTemporary);
        assert!(s.snapshot().expression.is_none());
    }
}

#[test]
fn multiword_lookup_never_edits_host_until_result_confirmation() {
    for profile in profiles() {
        let mut r = runtime();
        r.config.prediction.enabled = true;
        let mut s = Session::new(&r, profile, HostContext::default());
        begin(&mut s);
        word(&mut s, 'a');
        local(&mut s, Action::InputCode('b'));
        let words = s.candidate_window(0, 10);
        assert_eq!(words.total, 1); // No append during query composition.
        local(
            &mut s,
            Action::SelectCandidate(words.query_revision, words.candidates[0].id),
        );
        let q = s.snapshot().expression.as_ref().unwrap();
        assert_eq!(q.words, ["禁止", "自行车"]);
        assert_eq!(q.text, "禁止自行车");
        assert_eq!(q.phase, ExpressionPhase::Editing);
        local(&mut s, Action::FindExpressions);
        let results = s.candidate_window(0, 10);
        assert_eq!(results.total, 612);
        let commit = apply(
            &mut s,
            Action::SelectCandidate(results.query_revision, results.candidates[0].id),
        );
        assert_eq!(commit.target, EditTarget::CurrentSelection);
        assert_eq!(
            commit.op,
            EditSessionOp::CommitAndReplace {
                text: "🚳".into(),
                replace_len: 0
            }
        );
        assert_eq!(s.state_machine().mode(), InputMode::Main);
        assert!(s.snapshot().expression.is_none());
        assert!(!s.has_prediction());
        assert!(matches!(
            s.prepare(Action::SelectCandidate(
                results.query_revision,
                results.candidates[0].id
            )),
            Err(SessionError::Rejected(Rejection::StaleCandidate))
        ));
    }
}

#[test]
fn find_confirms_pending_word_and_invalid_code_is_preserved() {
    for profile in profiles() {
        let mut s = Session::new(&runtime(), profile, HostContext::default());
        begin(&mut s);
        local(&mut s, Action::FindExpressions);
        assert_eq!(
            s.snapshot().expression.as_ref().unwrap().phase,
            ExpressionPhase::Editing
        );
        local(&mut s, Action::InputCode('z'));
        local(&mut s, Action::FindExpressions);
        assert_eq!(s.state_machine().spelling(), "z");
        assert!(s.snapshot().expression.as_ref().unwrap().message.is_some());
        local(&mut s, Action::Space);
        local(&mut s, Action::Enter);
        local(&mut s, Action::CommitRaw);
        assert_eq!(s.state_machine().spelling(), "z");
        local(&mut s, Action::Cancel);
        local(&mut s, Action::InputCode('c'));
        local(&mut s, Action::FindExpressions);
        assert_eq!(s.snapshot().expression.as_ref().unwrap().text, "禁止自行车");
        assert_eq!(
            s.snapshot().expression.as_ref().unwrap().phase,
            ExpressionPhase::Results
        );
        let before = s.candidate_window(0, 10);
        local(&mut s, Action::FindExpressions);
        local(&mut s, Action::Enter);
        assert_eq!(s.candidate_window(0, 10), before);
    }
}

#[test]
fn backspace_and_escape_are_stage_aware_and_grapheme_safe_across_word_boundaries() {
    let mut s = Session::new(&runtime(), BehaviorProfile::TouchV1, HostContext::default());
    begin(&mut s);
    word(&mut s, 'e');
    word(&mut s, 'f');
    word(&mut s, 'g');
    assert_eq!(s.snapshot().expression.as_ref().unwrap().text, "o\u{308}👩🏽‍💻");
    local(&mut s, Action::Backspace);
    assert_eq!(s.snapshot().expression.as_ref().unwrap().text, "o\u{308}");
    local(&mut s, Action::Backspace);
    assert_eq!(s.snapshot().expression.as_ref().unwrap().text, "");
    word(&mut s, 'e');
    local(&mut s, Action::InputCode('z'));
    local(&mut s, Action::Cancel);
    assert_eq!(s.snapshot().expression.as_ref().unwrap().text, "o");
    local(&mut s, Action::FindExpressions);
    assert_eq!(s.candidate_window(0, 10).total, 0);
    local(&mut s, Action::Space);
    local(&mut s, Action::Backspace);
    assert_eq!(s.snapshot().expression.as_ref().unwrap().text, "o");
    local(&mut s, Action::FindExpressions);
    local(&mut s, Action::Cancel);
    assert_eq!(
        s.snapshot().expression.as_ref().unwrap().phase,
        ExpressionPhase::Editing
    );
    local(&mut s, Action::Cancel);
    assert!(s.snapshot().expression.is_none());
}

#[test]
fn automatic_full_code_and_overflow_only_accumulate_local_words() {
    for auto in [false, true] {
        let mut r = runtime();
        r.config.basic.auto_commit_unique = auto;
        r.dict = Dictionary::from_text("---\nmax_code_len: 1\n---\na\t禁止\nb\t自行车\n").unwrap();
        for profile in profiles() {
            let mut s = Session::new(&r, profile, HostContext::default());
            begin(&mut s);
            local(&mut s, Action::InputCode('a'));
            local(&mut s, Action::InputCode('b'));
            local(&mut s, Action::FindExpressions);
            assert_eq!(s.snapshot().expression.as_ref().unwrap().text, "禁止自行车");
            assert_eq!(s.candidate_window(0, 1).candidates[0].text, "🚳");
        }
    }
}

#[test]
fn paging_and_large_windows_keep_all_results_reachable_and_old_clicks_stale() {
    let mut s = Session::new(&runtime(), BehaviorProfile::TouchV1, HostContext::default());
    begin(&mut s);
    word(&mut s, 'c');
    local(&mut s, Action::FindExpressions);
    let before = s.candidate_window(0, MAX_WINDOW);
    assert_eq!(before.candidates.len(), MAX_WINDOW);
    let mut ids = Vec::new();
    let mut offset = 0;
    while offset < before.total {
        let window = s.candidate_window(offset, MAX_WINDOW);
        offset += window.candidates.len();
        ids.extend(window.candidates.into_iter().map(|c| c.id.0));
    }
    assert_eq!(ids, (0..612).collect::<Vec<_>>());
    local(&mut s, Action::MoveCandidatePage(true));
    assert!(matches!(
        s.prepare(Action::SelectCandidate(
            before.query_revision,
            before.candidates[0].id
        )),
        Err(SessionError::Rejected(Rejection::StaleCandidate))
    ));
    let edit = apply(&mut s, Action::Legacy(InputEvent::Select(2)));
    assert_eq!(
        edit.op,
        EditSessionOp::CommitAndReplace {
            text: "o\u{308}　\\_/4".into(),
            replace_len: 0
        }
    );
}

#[test]
fn entry_and_mode_changes_preserve_nonempty_input_and_restore_previous_mode() {
    for profile in profiles() {
        let mut s = Session::new(&runtime(), profile, HostContext::default());
        apply(&mut s, Action::InputCode('a'));
        assert!(matches!(
            s.prepare(Action::BeginTemporary(TemporaryMode::Emoji)),
            Err(SessionError::Rejected(Rejection::Unavailable(_)))
        ));
        assert_eq!(s.state_machine().spelling(), "a");
        apply(&mut s, Action::Cancel);
        apply(&mut s, Action::BeginTemporary(TemporaryMode::Pinyin));
        begin(&mut s);
        local(&mut s, Action::BeginTemporary(TemporaryMode::Kaomoji));
        word(&mut s, 'c');
        assert!(matches!(
            s.prepare(Action::BeginTemporary(TemporaryMode::Emoji)),
            Err(SessionError::Rejected(Rejection::Unavailable(_)))
        ));
        assert!(matches!(
            s.prepare(Action::OpenModeChooser),
            Err(SessionError::Rejected(Rejection::Unavailable(_)))
        ));
        local(&mut s, Action::FindExpressions);
        apply(&mut s, Action::Space);
        assert_eq!(s.state_machine().mode(), InputMode::Pinyin);
        assert_eq!(s.temporary_mode(), Some(TemporaryMode::Pinyin));
    }
}

#[test]
fn rejected_commit_retries_once_and_unconfirmed_results_are_not_published() {
    for profile in profiles() {
        let mut s = Session::new(&runtime(), profile, HostContext::default());
        begin(&mut s);
        word(&mut s, 'c');
        let old = s.snapshot().clone();
        let find = s.prepare(Action::FindExpressions).unwrap();
        assert_eq!(s.snapshot(), &old);
        s.acknowledge(find.token, HostOutcome::Applied);
        let result = s.candidate_window(0, 1);
        let choose = Action::SelectCandidate(result.query_revision, result.candidates[0].id);
        let edit = s.prepare(choose.clone()).unwrap();
        assert_eq!(
            s.acknowledge(edit.token, HostOutcome::RejectedBeforeWrite)
                .status,
            AckStatus::Retained
        );
        assert_eq!(s.candidate_window(0, 1), result);
        assert_eq!(apply(&mut s, choose.clone()).op, edit.op);
        assert!(s.prepare(choose).is_err());
    }
}

#[test]
fn exit_focus_gate_reload_and_privacy_never_flush_query_to_host() {
    for profile in profiles() {
        for route in 0..5 {
            let r = runtime();
            let mut s = Session::new(&r, profile, HostContext::default());
            begin(&mut s);
            word(&mut s, 'a');
            local(&mut s, Action::InputCode('b'));
            let old = s.candidate_window(0, 1);
            let plan = match route {
                0 => Some(s.lose_focus().unwrap()),
                1 => s.set_input_gate(false).unwrap(),
                2 => {
                    let mut r = r.clone();
                    r.revision += 1;
                    s.apply_runtime(Arc::new(r)).unwrap()
                }
                3 => {
                    s.set_assistance_allowed(false);
                    None
                }
                _ => {
                    s.host_terminated();
                    None
                }
            };
            if let Some(plan) = plan {
                assert_eq!(plan.op, EditSessionOp::NoOp);
                s.acknowledge(plan.token, HostOutcome::Applied);
            }
            assert!(s.snapshot().expression.is_none());
            if let Ok(stale) = s.prepare(Action::SelectCandidate(
                old.query_revision,
                old.candidates[0].id,
            )) {
                assert_eq!(
                    route, 1,
                    "only the disabled input gate may ignore an action"
                );
                assert!(!stale.consumed());
                assert_eq!(stale.op, EditSessionOp::NoOp);
            }
        }
    }
}

#[test]
fn disabled_or_private_modes_reject_entry_and_literal_actions_cannot_leak_text() {
    for profile in profiles() {
        let mut r = runtime();
        r.config.expressions.emoji.search = false;
        let mut s = Session::new(&r, profile, HostContext::default());
        assert!(!s.capability(TemporaryMode::Emoji).available);
        assert!(s
            .prepare(Action::BeginTemporary(TemporaryMode::Emoji))
            .is_err());
        let mut s = Session::new(
            &runtime(),
            profile,
            HostContext {
                allows_assistance: false,
                ..Default::default()
            },
        );
        assert!(s
            .prepare(Action::BeginTemporary(TemporaryMode::Emoji))
            .is_err());
        s.set_assistance_allowed(true);
        begin(&mut s);
        word(&mut s, 'a');
        for action in [
            Action::InsertLiteral("正文".into()),
            Action::InputSymbol('。'),
            Action::InputText('x', TextIntent::Literal),
        ] {
            assert!(s.prepare(action).is_err());
        }
        assert_eq!(s.snapshot().expression.as_ref().unwrap().text, "禁止");
    }
}

#[test]
fn uncertain_result_commit_never_replays_or_finishes_an_unowned_host_composition() {
    for profile in profiles() {
        for outcome in [
            HostOutcome::OutcomeUnknown,
            HostOutcome::PartiallyApplied { completed_steps: 1 },
        ] {
            let mut s = Session::new(&runtime(), profile, HostContext::default());
            begin(&mut s);
            word(&mut s, 'c');
            local(&mut s, Action::FindExpressions);
            let old = s.candidate_window(0, 1);
            let select = Action::SelectCandidate(old.query_revision, old.candidates[0].id);
            let edit = s.prepare(select.clone()).unwrap();
            s.acknowledge(edit.token, outcome);
            assert!(s.recover().unwrap().is_none());
            assert!(s.snapshot().expression.is_none());
            assert!(s.prepare(select).is_err());
        }
    }
}

#[test]
fn a_superseded_legacy_prepare_cannot_publish_an_old_query_or_submit_it() {
    let mut s = Session::new(
        &runtime(),
        BehaviorProfile::LegacyV1,
        HostContext::default(),
    );
    begin(&mut s);
    let first = s.prepare(Action::InputCode('a')).unwrap();
    let second = s.prepare(Action::Space).unwrap();
    assert_eq!(first.op, EditSessionOp::NoOp);
    assert_eq!(second.op, EditSessionOp::NoOp);
    assert_eq!(
        s.acknowledge(first.token, HostOutcome::Applied).status,
        AckStatus::Stale
    );
    assert_eq!(
        s.acknowledge(second.token, HostOutcome::Applied).status,
        AckStatus::Published
    );
    assert_eq!(s.snapshot().expression.as_ref().unwrap().text, "禁止");
}

#[test]
fn partial_search_is_explicit_in_header_and_candidate_hints_but_never_in_output() {
    let mut r = runtime();
    r.dict = Dictionary::from_text("a\t禁止猫\n").unwrap();
    let mut s = Session::new(&r, BehaviorProfile::TouchV1, HostContext::default());
    begin(&mut s);
    word(&mut s, 'a');
    local(&mut s, Action::FindExpressions);
    assert!(s
        .snapshot()
        .expression
        .as_ref()
        .unwrap()
        .message
        .as_ref()
        .unwrap()
        .contains("部分匹配"));
    let window = s.candidate_window(0, 1);
    assert!(!window.candidates[0].exact);
    assert!(window.candidates[0]
        .hints
        .contains(&CandidateHint::ExpressionMatch("部分匹配".into())));
    assert!(s.snapshot().items[0].hint.contains("部分匹配"));
    assert_eq!(
        apply(&mut s, Action::Space).op,
        EditSessionOp::CommitAndReplace {
            text: "🚳".into(),
            replace_len: 0
        }
    );
}

#[test]
fn mode_chooser_uses_versioned_clicks_without_a_main_table_trigger() {
    for profile in profiles() {
        let mut s = Session::new(&runtime(), profile, HostContext::default());
        assert!(s.state_machine().temporary_trigger().is_none());
        apply(&mut s, Action::OpenModeChooser);
        let menu = s.candidate_window(0, 10);
        assert_eq!(
            menu.candidates
                .iter()
                .map(|c| c.text.as_str())
                .collect::<Vec<_>>(),
            [
                "拼音",
                "英文",
                "Emoji",
                "颜文字",
                "假名",
                "数学符号",
                "标点",
                "其他符号"
            ]
        );
        local(
            &mut s,
            Action::SelectCandidate(menu.query_revision, menu.candidates[2].id),
        );
        word(&mut s, 'c');
        local(&mut s, Action::Legacy(InputEvent::FindExpressions));
        assert_eq!(s.candidate_window(0, 1).candidates[0].text, "🚳");
        assert!(s
            .prepare(Action::SelectCandidate(
                menu.query_revision,
                menu.candidates[0].id
            ))
            .is_err());
        local(&mut s, Action::Legacy(InputEvent::FindExpressions));
        local(&mut s, Action::EndTemporary);
        apply(&mut s, Action::BeginTemporary(TemporaryMode::Pinyin));
        apply(&mut s, Action::OpenModeChooser);
        let menu = s.candidate_window(0, 4);
        local(
            &mut s,
            Action::SelectCandidate(menu.query_revision, menu.candidates[3].id),
        );
        local(&mut s, Action::EndTemporary);
        assert_eq!(s.state_machine().mode(), InputMode::Pinyin);
    }
}
