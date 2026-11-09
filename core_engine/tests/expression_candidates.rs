use core_engine::{
    candidates::{CandidateHint, MAX_WINDOW},
    expressions::*,
    runtime::RuntimeSnapshot,
    session::*,
    *,
};
use std::sync::Arc;

fn library(kind: ExpressionKind, rows: &[(&str, &[&str])]) -> Arc<ExpressionLibrary> {
    Arc::new(
        ExpressionLibrary::new(
            ExpressionDocument {
                kind,
                name: "fixture".into(),
                version: "1".into(),
                sources: vec![ExpressionSource {
                    id: "test".into(),
                    url: "local:test".into(),
                    revision: "1".into(),
                    license: "MIT".into(),
                    notice: "Test fixture".into(),
                }],
                entries: rows
                    .iter()
                    .enumerate()
                    .map(|(order, (text, triggers))| ExpressionEntry {
                        text: (*text).into(),
                        name: format!("名称{order}"),
                        triggers: triggers.iter().map(|s| (*s).into()).collect(),
                        tags: vec![],
                        aliases: vec![],
                        order: order as u32,
                        sources: vec!["test".into()],
                    })
                    .collect(),
            },
            kind,
        )
        .unwrap(),
    )
}

fn resources() -> Arc<ExpressionResources> {
    Arc::new(ExpressionResources {
        emoji: ExpressionResource {
            library: Some(library(
                ExpressionKind::Emoji,
                &[
                    ("😀", &["开心", "快乐"]),
                    ("👩🏽‍💻", &["开心"]),
                    ("🚳", &["禁止自行车"]),
                    ("🙃", &["😀"]),
                ],
            )),
            ..Default::default()
        },
        kaomoji: ExpressionResource {
            library: Some(library(
                ExpressionKind::Kaomoji,
                &[
                    ("(^_^)", &["开心"]),
                    ("😀", &["开心"]),
                    ("o\u{308}　\\_/", &["开心"]),
                ],
            )),
            ..Default::default()
        },
    })
}

fn runtime(text: &str, size: u8) -> RuntimeSnapshot {
    let mut config = Config::default();
    config.basic.candidate_count = size;
    config.basic.auto_commit_unique = false;
    config.expressions.emoji.append = true;
    config.expressions.kaomoji.append = true;
    RuntimeSnapshot {
        dict: Dictionary::from_text(text).unwrap(),
        temporary: Arc::new(TemporaryTables {
            pinyin: Some(
                Dictionary::from_text("---\ntype: pinyin\nmax_code_len: 64\n---\nkai'xin\t开心\n")
                    .unwrap(),
            ),
            english: Some(
                Dictionary::from_text("---\nmax_code_len: 64\n---\nhappy\t开心\n").unwrap(),
            ),
            ..Default::default()
        }),
        expressions: resources(),
        config,
        revision: 1,
        config_path: "config.toml".into(),
        system_table_path: "main.dict".into(),
        user_table_path: "user.dict".into(),
    }
}

fn engine(text: &str, size: u8) -> SemanticEngine {
    SemanticEngine::new(
        runtime(text, size).state_machine(),
        BehaviorProfile::TouchV1,
    )
}

fn input(engine: &mut SemanticEngine, text: &str) -> ActionResult {
    text.chars()
        .map(|c| engine.dispatch(Action::InputCode(c)))
        .last()
        .unwrap()
}

#[test]
fn full_text_pages_have_expression_continuations_with_one_shared_order() {
    let mut e = engine(
        "a\t开心\t100\nab\t快乐\t90\nac\t其他\t80\nad\t不开心\t70\n",
        2,
    );
    input(&mut e, "a");
    let window = e.candidate_window(0, MAX_WINDOW);
    assert_eq!(
        window
            .candidates
            .iter()
            .map(|c| c.text.as_str())
            .collect::<Vec<_>>(),
        [
            "开心",
            "快乐",
            "😀",
            "👩🏽‍💻",
            "(^_^)",
            "o\u{308}　\\_/",
            "其他",
            "不开心"
        ]
    );
    assert_eq!(
        window.candidates[2].hints,
        [CandidateHint::ExpressionName("名称0".into())]
    );
    let mut pages = Vec::new();
    loop {
        pages.extend_from_slice(e.state_machine().candidates());
        if e.state_machine().page() + 1 == e.state_machine().total_pages() {
            break;
        }
        e.dispatch(Action::MoveCandidatePage(true));
    }
    assert_eq!(
        pages,
        window
            .candidates
            .iter()
            .map(|c| c.text.clone())
            .collect::<Vec<_>>()
    );
    assert_eq!(e.state_machine().candidates(), ["其他", "不开心"]);
    assert_eq!(
        e.dispatch(Action::SelectCandidate(
            window.query_revision,
            window.candidates[2].id
        ))
        .disposition,
        Disposition::Rejected(Rejection::StaleCandidate)
    );
    let current = e.candidate_window(2, 1);
    assert_eq!(
        e.dispatch(Action::SelectCandidate(
            current.query_revision,
            current.candidates[0].id
        ))
        .transition,
        Transition::Commit("😀".into())
    );
}

#[test]
fn partial_pages_keep_next_text_group_first_and_do_not_recurse_or_substring_match() {
    let mut e = engine("a\t开心\t4\nab\t其他\t3\nac\t另词\t2\nad\t不开心\t1\n", 3);
    input(&mut e, "a");
    assert_eq!(e.state_machine().total_pages(), 4);
    e.dispatch(Action::MoveCandidatePage(true));
    assert_eq!(e.state_machine().candidates(), ["😀", "👩🏽‍💻", "(^_^)"]);
    e.dispatch(Action::MoveCandidatePage(true));
    assert_eq!(e.state_machine().candidates(), ["o\u{308}　\\_/"]);
    e.dispatch(Action::MoveCandidatePage(true));
    assert_eq!(e.state_machine().candidates(), ["不开心"]);
    input(&mut e, "d");
    assert_eq!(e.candidate_window(0, 100).total, 1);
    assert_eq!(e.state_machine().candidates(), ["不开心"]);
}

#[test]
fn single_text_leaves_space_for_expressions_and_only_enabled_types_append() {
    for (emoji, kaomoji, expected) in [(true, false, 3), (false, true, 4), (false, false, 1)] {
        let mut r = runtime("a\t开心\n", 5);
        r.config.expressions.emoji.append = emoji;
        r.config.expressions.kaomoji.append = kaomoji;
        r.config.expressions.emoji.search = true;
        r.config.expressions.kaomoji.search = true;
        let mut e = SemanticEngine::new(r.state_machine(), BehaviorProfile::TouchV1);
        input(&mut e, "a");
        assert_eq!(e.state_machine().candidates().len(), expected);
        assert_eq!(e.state_machine().candidates()[0], "开心");
    }
}

#[test]
fn unique_full_code_auto_commit_is_deferred_only_when_an_expression_is_available() {
    for (word, enabled, commits) in [
        ("开心", true, false),
        ("开心", false, true),
        ("不开心", true, true),
    ] {
        let mut r = runtime(&format!("aaaa\t{word}\n"), 5);
        r.config.basic.auto_commit_unique = true;
        r.config.expressions.emoji.append = enabled;
        r.config.expressions.kaomoji.append = false;
        let mut e = SemanticEngine::new(r.state_machine(), BehaviorProfile::LegacyV1);
        assert_eq!(
            matches!(input(&mut e, "aaaa").transition, Transition::Commit(_)),
            commits
        );
        if !commits {
            assert_eq!(
                e.dispatch(Action::Space).transition,
                Transition::Commit(word.into())
            );
        }
    }
}

#[test]
fn temporary_pinyin_matches_whole_candidate_but_both_english_modes_do_not_append() {
    let mut e = engine("a\t开心\n", 5);
    e.dispatch(Action::BeginTemporary(TemporaryMode::Pinyin));
    input(&mut e, "kaixin");
    assert_eq!(e.candidate_window(0, 10).total, 5);
    assert_eq!(
        e.dispatch(Action::SelectCandidate(
            e.candidate_window(0, 10).query_revision,
            e.candidate_window(0, 10).candidates[2].id,
        ))
        .transition,
        Transition::Commit("👩🏽‍💻".into())
    );
    assert_eq!(e.temporary_mode(), None);
    for action in [
        Action::BeginTemporary(TemporaryMode::English),
        Action::SetBaseMode(BaseMode::English),
    ] {
        e.dispatch(action);
        input(&mut e, "happy");
        assert_eq!(e.candidate_window(0, 10).total, 1);
        e.dispatch(Action::Cancel);
    }
}

#[test]
fn keyboard_numbers_and_touch_ids_commit_identical_complete_sequences() {
    for index in 1..=5 {
        let mut e = engine("a\t开心\n", 5);
        input(&mut e, "a");
        let window = e.candidate_window(0, 10);
        let mut touch = e.clone();
        let expected = Transition::Commit(window.candidates[index - 1].text.clone());
        assert_eq!(
            e.dispatch(Action::Legacy(InputEvent::Char(
                char::from_digit(index as u32, 10).unwrap()
            )))
            .transition,
            expected
        );
        assert_eq!(
            touch
                .dispatch(Action::SelectCandidate(
                    window.query_revision,
                    window.candidates[index - 1].id
                ))
                .transition,
            expected
        );
    }
}

#[test]
fn all_long_tail_results_are_reachable_with_nine_numeric_slots_and_bounded_touch_windows() {
    let outputs: Vec<_> = (0..611).map(|i| format!("(表情{i})")).collect();
    let rows: Vec<_> = outputs
        .iter()
        .map(|s| (s.as_str(), &["开心"][..]))
        .collect();
    let mut r = runtime("a\t开心\n", 10);
    r.expressions = Arc::new(ExpressionResources {
        emoji: ExpressionResource {
            library: Some(library(ExpressionKind::Emoji, &rows)),
            ..Default::default()
        },
        ..Default::default()
    });
    let mut e = SemanticEngine::new(r.state_machine(), BehaviorProfile::LegacyV1);
    input(&mut e, "a");
    let mut keyboard = Vec::new();
    assert!(!e.state_machine().would_consume(&InputEvent::Select(10)));
    assert_eq!(
        e.dispatch(Action::Legacy(InputEvent::Select(10)))
            .disposition,
        Disposition::NotHandled
    );
    loop {
        assert!(e.state_machine().candidates().len() <= 9);
        keyboard.extend_from_slice(e.state_machine().candidates());
        if e.state_machine().page() + 1 == e.state_machine().total_pages() {
            break;
        }
        e.dispatch(Action::MoveCandidatePage(true));
    }
    let mut touch = Vec::new();
    loop {
        let view = e.candidate_window(touch.len(), usize::MAX);
        assert!(!view.candidates.is_empty());
        assert!(view.candidates.len() <= MAX_WINDOW);
        touch.extend(view.candidates.into_iter().map(|c| c.text));
        if touch.len() == view.total {
            break;
        }
    }
    assert_eq!(keyboard, touch);
    assert_eq!(touch.len(), 612);
    assert_eq!(touch.last(), outputs.last());
    assert_eq!(
        e.dispatch(Action::Legacy(InputEvent::Select(9))).transition,
        Transition::Commit("(表情610)".into())
    );
}

fn apply(s: &mut Session, action: Action) -> PreparedEdit {
    let edit = s.prepare(action).unwrap();
    assert_eq!(
        s.acknowledge(edit.token, HostOutcome::Applied).status,
        AckStatus::Published
    );
    edit
}

#[test]
fn rejected_host_commit_preserves_expression_identity_and_never_commits_its_hint() {
    for profile in [BehaviorProfile::TouchV1, BehaviorProfile::LegacyV1] {
        let mut s = Session::new(&runtime("a\t开心\n", 5), profile, HostContext::default());
        if profile == BehaviorProfile::LegacyV1 {
            apply(&mut s, Action::BeginTemporary(TemporaryMode::Pinyin));
            apply(&mut s, Action::InputCode('k'));
        } else {
            apply(&mut s, Action::InputCode('a'));
        }
        let view = s.candidate_window(4, 1);
        let choose = Action::SelectCandidate(view.query_revision, view.candidates[0].id);
        let edit = s.prepare(choose.clone()).unwrap();
        assert_eq!(
            edit.op,
            EditSessionOp::CommitAndReplace {
                text: "o\u{308}　\\_/".into(),
                replace_len: 0
            }
        );
        s.acknowledge(edit.token, HostOutcome::RejectedBeforeWrite);
        assert_eq!(s.candidate_window(4, 1), view);
        assert_eq!(apply(&mut s, choose.clone()).op, edit.op);
        assert!(matches!(
            s.prepare(choose),
            Err(SessionError::Rejected(Rejection::StaleCandidate))
        ));
    }
}

#[test]
fn resource_change_cancels_old_query_even_when_prediction_settings_change_too() {
    let r = runtime("a\t开心\n", 5);
    let mut s = Session::new(&r, BehaviorProfile::TouchV1, HostContext::default());
    apply(&mut s, Action::InputCode('a'));
    let old = s.candidate_window(1, 1);
    let mut next = r.clone();
    next.revision += 1;
    next.config.prediction.enabled = !next.config.prediction.enabled;
    next.expressions = Arc::new(ExpressionResources {
        emoji: ExpressionResource {
            library: Some(library(ExpressionKind::Emoji, &[("😇", &["开心"])])),
            ..Default::default()
        },
        ..Default::default()
    });
    let cleanup = s
        .apply_runtime(Arc::new(next))
        .unwrap()
        .expect("resource change needs cleanup");
    s.acknowledge(cleanup.token, HostOutcome::Applied);
    assert!(matches!(
        s.prepare(Action::SelectCandidate(
            old.query_revision,
            old.candidates[0].id
        )),
        Err(SessionError::Rejected(Rejection::StaleCandidate))
    ));
    apply(&mut s, Action::InputCode('a'));
    assert_eq!(s.candidate_window(1, 1).candidates[0].text, "😇");
}

#[test]
fn password_context_never_appends_and_returning_to_text_restores_resources() {
    for profile in [BehaviorProfile::TouchV1, BehaviorProfile::LegacyV1] {
        let mut s = Session::new(
            &runtime("a\t开心\n", 5),
            profile,
            HostContext {
                purpose: InputPurpose::Password,
                ..Default::default()
            },
        );
        apply(&mut s, Action::InputCode('a'));
        assert!(s
            .candidate_window(0, 10)
            .candidates
            .iter()
            .all(|c| c.text != "😀"));
        s.invalidate_context(HostContext::default(), true);
        apply(&mut s, Action::InputCode('a'));
        assert_eq!(s.candidate_window(0, 10).total, 5);
    }
}

#[test]
fn disabled_resources_preserve_legacy_input_and_paging_traces() {
    let mut r = runtime("a\t开心\t3\nab\t不开心\t2\nac\t其他\t1\n", 2);
    r.config.expressions = ExpressionsConfig::default();
    let mut configured = r.state_machine();
    r.expressions = Arc::default();
    let mut baseline = r.state_machine();
    for event in [
        InputEvent::Char('a'),
        InputEvent::PageNext,
        InputEvent::PagePrev,
        InputEvent::Char('b'),
        InputEvent::Backspace,
        InputEvent::Select(2),
        InputEvent::Char('z'),
        InputEvent::Space,
        InputEvent::Char('a'),
        InputEvent::Esc,
    ] {
        assert_eq!(configured.handle(event.clone()), baseline.handle(event));
        assert_eq!(configured.candidates(), baseline.candidates());
    }
}

#[test]
fn prediction_does_not_append_expressions_and_late_model_result_cannot_change_live_ids() {
    use core_engine::assistance::*;
    let mut r = runtime("a\t开心\n", 5);
    r.config.prediction.enabled = true;
    r.config.prediction.model = Some(config::ModelSelection {
        id: "fixture".into(),
        version: "1".into(),
    });
    let mut s = Session::new(&r, BehaviorProfile::TouchV1, HostContext::default());
    apply(&mut s, Action::InsertLiteral("研究量子".into()));
    let work = s.prediction_request().unwrap();
    let response = PredictionResponse::Proposed(ProposalResponse {
        protocol: 1,
        key: work.key,
        words: vec!["开心".into()],
    });
    assert!(s.complete_model_prediction(work.key, Ok(response.clone())));
    let prediction = s.candidate_window(0, 10);
    assert_eq!(
        prediction
            .candidates
            .iter()
            .map(|c| c.text.as_str())
            .collect::<Vec<_>>(),
        ["开心"]
    );
    apply(&mut s, Action::InputCode('a'));
    let before = s.candidate_window(0, 10);
    assert!(!s.complete_model_prediction(work.key, Ok(response)));
    assert_eq!(s.candidate_window(0, 10), before);
    assert_eq!(before.total, 5);
    assert!(matches!(
        s.prepare(Action::SelectCandidate(
            prediction.query_revision,
            prediction.candidates[0].id
        )),
        Err(SessionError::Rejected(Rejection::StaleCandidate))
    ));
}

#[test]
fn full_shipped_libraries_preserve_all_matching_outputs_and_report_query_cost() {
    use std::{collections::BTreeSet, path::Path, time::Instant};
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../expressions");
    let emoji =
        Arc::new(ExpressionLibrary::load(&root.join("emoji.mwe"), ExpressionKind::Emoji).unwrap());
    let kaomoji = Arc::new(
        ExpressionLibrary::load(&root.join("kaomoji.mwe"), ExpressionKind::Kaomoji).unwrap(),
    );
    let expected: BTreeSet<_> = emoji
        .exact("开心")
        .chain(kaomoji.exact("开心"))
        .map(|entry| entry.text.clone())
        .chain(["开心".to_owned()])
        .collect();
    let mut r = runtime("a\t开心\n", 5);
    r.expressions = Arc::new(ExpressionResources {
        emoji: ExpressionResource {
            library: Some(emoji),
            ..Default::default()
        },
        kaomoji: ExpressionResource {
            library: Some(kaomoji),
            ..Default::default()
        },
    });
    let mut e = SemanticEngine::new(r.state_machine(), BehaviorProfile::TouchV1);
    let mut timings = Vec::new();
    for _ in 0..100 {
        e.dispatch(Action::Cancel);
        let start = Instant::now();
        input(&mut e, "a");
        timings.push(start.elapsed().as_micros());
    }
    timings.sort_unstable();
    eprintln!(
        "full-library append: {} candidates; p50={}us p95={}us",
        expected.len(),
        timings[50],
        timings[95]
    );
    let mut actual = BTreeSet::new();
    let mut offset = 0;
    loop {
        let window = e.candidate_window(offset, MAX_WINDOW);
        assert!(!window.candidates.is_empty());
        offset += window.candidates.len();
        actual.extend(window.candidates.into_iter().map(|c| c.text));
        if offset == window.total {
            break;
        }
    }
    assert_eq!(actual, expected);
}

#[test]
fn host_scope_disables_append_without_losing_code_and_invalidates_old_clicks() {
    let mut s = Session::new(
        &runtime("a\t开心\n", 5),
        BehaviorProfile::LegacyV1,
        HostContext::default(),
    );
    apply(&mut s, Action::InputCode('a'));
    let previous = s.candidate_window(1, 1);
    s.set_assistance_allowed(false);
    assert_eq!(s.state_machine().spelling(), "a");
    assert_eq!(s.candidate_window(0, 10).total, 1);
    assert_eq!(s.snapshot().items.len(), 1);
    assert!(matches!(
        s.prepare(Action::SelectCandidate(
            previous.query_revision,
            previous.candidates[0].id
        )),
        Err(SessionError::Rejected(Rejection::StaleCandidate))
    ));
    s.set_assistance_allowed(true);
    assert_eq!(s.candidate_window(0, 10).total, 5);
}

#[test]
fn main_legacy_expression_failure_uses_existing_cleanup_protocol() {
    let mut s = Session::new(
        &runtime("a\t开心\n", 5),
        BehaviorProfile::LegacyV1,
        HostContext::default(),
    );
    apply(&mut s, Action::InputCode('a'));
    let edit = s.prepare(Action::Legacy(InputEvent::Select(2))).unwrap();
    assert_eq!(
        edit.op,
        EditSessionOp::CommitAndReplace {
            text: "😀".into(),
            replace_len: 0
        }
    );
    assert_eq!(
        s.acknowledge(edit.token, HostOutcome::RejectedBeforeWrite)
            .status,
        AckStatus::NeedsCleanup
    );
    assert_eq!(s.candidate_window(0, 10).total, 0);
}

#[test]
fn editing_and_mode_changes_reject_old_expression_ids() {
    for action in [
        Action::Backspace,
        Action::SetBaseMode(BaseMode::English),
        Action::Cancel,
    ] {
        let mut e = engine("ab\t开心\n", 5);
        input(&mut e, "ab");
        let previous = e.candidate_window(1, 1);
        e.dispatch(action);
        assert_eq!(
            e.dispatch(Action::SelectCandidate(
                previous.query_revision,
                previous.candidates[0].id
            ))
            .disposition,
            Disposition::Rejected(Rejection::StaleCandidate)
        );
    }
}

#[test]
fn privacy_change_does_not_publish_pending_input_or_revive_expressions_on_ack() {
    for profile in [BehaviorProfile::LegacyV1, BehaviorProfile::TouchV1] {
        let mut s = Session::new(&runtime("a\t开心\n", 5), profile, HostContext::default());
        let pending = s.prepare(Action::InputCode('a')).unwrap();
        s.set_assistance_allowed(false);
        assert_eq!(s.candidate_window(0, 10).total, 0);
        assert!(!s.snapshot().visible);
        assert_eq!(
            s.acknowledge(pending.token, HostOutcome::Applied).status,
            AckStatus::Published
        );
        assert_eq!(s.candidate_window(0, 10).total, 1);
        assert_eq!(s.snapshot().items.len(), 1);
    }
}
