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
        expressions: Arc::default(),
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

#[test]
fn personal_prediction_is_independent_of_collection_and_preserves_candidate_identity() {
    use core_engine::assistance::*;
    for profile in [BehaviorProfile::LegacyV1, BehaviorProfile::TouchV1] {
        let mut runtime = runtime(false, 1);
        runtime.config.personal.predict = true;
        let mut s = Session::new(&runtime, profile, HostContext::default());
        s.observe_personal_right(Some(""));
        apply(&mut s, Action::InsertLiteral("明天去".into()));
        let work = s
            .prediction_request()
            .expect("personal prediction without general model");
        assert!(!s.config().personal.collect);
        assert!(!s.config().personal.learn);
        let response = PredictionResponse::Proposed(ProposalResponse {
            protocol: 1,
            key: work.key,
            words: vec!["图书馆".into()],
        });
        assert!(s.complete_model_prediction(work.key, Ok(response)));
        let old = choose(&s, "图书馆");
        assert!(
            matches!(apply(&mut s,old.clone()),EditSessionOp::CommitAndReplace{text,..} if text=="图书馆")
        );
        assert!(s.prepare(old).is_err());
        s.clear_prediction_context();
        apply(&mut s, Action::InputCode('b'));
        let mut next = runtime.clone();
        next.revision = 2;
        next.config.personal.predict = false;
        next.config.personal.collect = true;
        assert!(s.apply_runtime(Arc::new(next)).unwrap().is_none());
        assert_eq!(s.snapshot().spelling, "b");
    }
}

#[test]
fn a_changed_right_context_invalidates_personal_results_before_publication() {
    use core_engine::assistance::*;
    let mut r = runtime(false, 1);
    r.config.personal.predict = true;
    let mut s = Session::new(&r, BehaviorProfile::TouchV1, HostContext::default());
    s.observe_personal_right(Some("出差"));
    apply(&mut s, Action::InsertLiteral("明天去".into()));
    let request = s.prediction_request().unwrap();
    s.observe_personal_right(Some("旅游"));
    assert!(!s.complete_model_prediction(
        request.key,
        Ok(PredictionResponse::Proposed(ProposalResponse {
            protocol: 1,
            key: request.key,
            words: vec!["北京".into()]
        }))
    ));
    assert!(!s.has_prediction());
}

#[test]
fn model_and_loading_hot_switches_preserve_pending_code_in_both_profiles() {
    for profile in [BehaviorProfile::LegacyV1, BehaviorProfile::TouchV1] {
        let mut s = session(profile);
        apply(&mut s, Action::InputCode('b'));
        for revision in 2..6 {
            let mut r = runtime(revision != 5, revision);
            if revision < 4 {
                r.config.prediction.model = Some(config::ModelSelection {
                    id: "not-installed".into(),
                    version: "v1".into(),
                });
            }
            r.config.prediction.loading = if revision == 3 {
                config::ModelLoading::PreloadKeep
            } else {
                config::ModelLoading::OnDemand
            };
            r.config.prediction.wait_ms = 700;
            assert!(s.apply_runtime(Arc::new(r)).unwrap().is_none());
            assert_eq!(s.snapshot().spelling, "b");
            assert!(s.prediction_request().is_none());
        }
        assert!(
            matches!(apply(&mut s, Action::Space), EditSessionOp::CommitAndReplace { text, .. } if text == "北")
        );
    }
}

#[test]
fn generated_words_use_confirmed_append_identity_and_retry_in_both_profiles() {
    use core_engine::assistance::*;
    for profile in [BehaviorProfile::LegacyV1, BehaviorProfile::TouchV1] {
        let mut r = runtime(true, 1);
        r.config.prediction.model = Some(config::ModelSelection {
            id: "generator".into(),
            version: "v1".into(),
        });
        let mut s = Session::new(&r, profile, HostContext::default());
        apply(&mut s, Action::InsertLiteral("研究量子".into()));
        let work = s.prediction_request().unwrap();
        assert!(!s.would_handle(&Action::Space));
        let response = PredictionResponse::Proposed(ProposalResponse {
            protocol: 1,
            key: work.key,
            words: vec!["纠缠态".into(), "计算".into()],
        });
        assert!(s.complete_model_prediction(work.key, Ok(response.clone())));
        assert!(!s.complete_model_prediction(work.key, Ok(response)));
        let action = choose(&s, "纠缠态");
        let old = action.clone();
        let plan = s.prepare(action).unwrap();
        assert_eq!(
            plan.op,
            EditSessionOp::CommitAndReplace {
                text: "纠缠态".into(),
                replace_len: 0
            }
        );
        s.acknowledge(plan.token, HostOutcome::RejectedBeforeWrite);
        assert!(s.prediction_request().is_none());
        apply(&mut s, old.clone());
        assert_eq!(s.prediction_request().unwrap().prefix, "研究量子纠缠态");
        assert!(matches!(
            s.prepare(old),
            Err(SessionError::Rejected(Rejection::StaleCandidate))
        ));
    }
}

#[test]
fn generated_responses_after_external_edit_or_new_code_never_publish() {
    use core_engine::assistance::*;
    for profile in [BehaviorProfile::LegacyV1, BehaviorProfile::TouchV1] {
        let mut r = runtime(true, 1);
        r.config.prediction.model = Some(config::ModelSelection {
            id: "generator".into(),
            version: "v1".into(),
        });
        let mut s = Session::new(&r, profile, HostContext::default());
        apply(&mut s, Action::InsertLiteral("明天去".into()));
        let work = s.prediction_request().unwrap();
        apply(&mut s, Action::InputCode('b'));
        assert!(!s.complete_model_prediction(
            work.key,
            Ok(PredictionResponse::Proposed(ProposalResponse {
                protocol: 1,
                key: work.key,
                words: vec!["探亲".into()]
            }))
        ));
        assert!(!s.has_prediction());
        assert_eq!(s.candidate_window(0, 10).candidates[0].text, "北");
    }
}

#[test]
fn selected_model_can_receive_confirmed_context_without_dictionary_recall() {
    let mut r = runtime(true, 1);
    r.config.prediction.model = Some(config::ModelSelection {
        id: "generator".into(),
        version: "v1".into(),
    });
    let mut s = Session::new(&r, BehaviorProfile::TouchV1, HostContext::default());
    apply(&mut s, Action::InputText('龘', TextIntent::Literal));
    let request = s.prediction_request().unwrap();
    assert_eq!(request.prefix, "龘");
    assert!(request.candidates.is_empty());
    assert!(!s.has_prediction());
    assert!(!s.would_handle(&Action::Space));
}

#[test]
fn visible_predictions_accept_space_and_number_keys_without_literal_characters() {
    for profile in [BehaviorProfile::LegacyV1, BehaviorProfile::TouchV1] {
        for action in [
            Action::Space,
            Action::Legacy(InputEvent::Space),
            Action::Legacy(InputEvent::Char('1')),
            Action::Legacy(InputEvent::Select(1)),
            Action::InputCode('1'),
            Action::InputText('1', TextIntent::Normal),
            Action::CommitPreferred,
        ] {
            let mut s = session(profile);
            north(&mut s);
            let expected = s.candidate_window(0, 64).candidates[0].text.clone();
            assert!(s.would_handle(&action), "{profile:?}: {action:?}");
            assert_eq!(
                apply(&mut s, action),
                EditSessionOp::CommitAndReplace {
                    text: expected,
                    replace_len: 0
                }
            );
        }
    }
}

#[test]
fn prediction_numbers_follow_the_displayed_order_and_explicit_literals_stay_literal() {
    for profile in [BehaviorProfile::LegacyV1, BehaviorProfile::TouchV1] {
        for index in 1..=5 {
            let mut s = session(profile);
            north(&mut s);
            let expected = s.candidate_window(0, 64).candidates[index - 1].text.clone();
            let event = InputEvent::Char(char::from_digit(index as u32, 10).unwrap());
            assert!(s.would_handle_legacy(&event, false));
            assert!(!s.would_handle_legacy(&event, true));
            assert_eq!(
                apply(&mut s, Action::Legacy(event)),
                EditSessionOp::CommitAndReplace {
                    text: expected,
                    replace_len: 0
                }
            );
        }
    }
    let mut s = session(BehaviorProfile::TouchV1);
    north(&mut s);
    assert_eq!(
        apply(&mut s, Action::InputText('1', TextIntent::Literal)),
        EditSessionOp::CommitAndReplace {
            text: "1".into(),
            replace_len: 0
        }
    );
}

#[test]
fn prediction_candidates_contain_words_without_status_labels() {
    let mut s = session(BehaviorProfile::LegacyV1);
    north(&mut s);
    assert!(s.snapshot().items.iter().all(|c| c.hint.is_empty()));
    assert!(s
        .candidate_window(0, 64)
        .candidates
        .iter()
        .all(|c| c.hints.is_empty()));
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
fn enter_ignores_suggestions_and_runtime_disable_preserves_pending_code() {
    let mut s = session(BehaviorProfile::TouchV1);
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

#[test]
fn model_prediction_waits_once_preserves_ids_and_discards_late_or_stale_scores() {
    use core_engine::assistance::*;
    let mut r = runtime(true, 9);
    r.config.prediction.model = Some(core_engine::config::ModelSelection {
        id: "test".into(),
        version: "v1".into(),
    });
    let mut s = Session::new(&r, BehaviorProfile::TouchV1, HostContext::default());
    north(&mut s);
    assert!(!s.snapshot().visible);
    assert!(s.candidate_window(0, 64).candidates.is_empty());
    let work = s.prediction_request().unwrap();
    let response = ScoreResponse {
        prediction_accepted: Some(work.candidates.iter().map(|c| c.id).collect()),
        protocol: 1,
        key: work.key,
        scores: work
            .candidates
            .iter()
            .enumerate()
            .map(|(i, c)| CandidateScore {
                id: c.id,
                value: i as f32,
            })
            .collect(),
    };
    let mut old = work.key;
    old.request += 1;
    assert!(!s.complete_prediction(old, Ok(response.clone())));
    assert!(s.prediction_request().is_some());
    assert!(s.complete_prediction(work.key, Ok(response.clone())));
    let window = s.candidate_window(0, 64);
    assert_eq!(window.candidates[0].id, work.candidates.last().unwrap().id);
    let chosen = window.candidates[0].clone();
    assert!(!s.complete_prediction(work.key, Ok(response)));
    assert!(
        matches!(apply(&mut s,Action::SelectCandidate(window.query_revision,chosen.id)),EditSessionOp::CommitAndReplace{text,..} if text==chosen.text)
    );
    let old = s.prediction_request();
    apply(&mut s, Action::Legacy(InputEvent::Char('b')));
    if let Some(work) = old {
        assert!(!s.complete_prediction(work.key, Err(Unavailable::Timeout)));
    }
    assert!(!s.snapshot().prediction);
}
#[test]
fn model_wait_timeout_hides_predictions_without_intercepting_input() {
    use core_engine::assistance::Unavailable;
    let mut r = runtime(true, 10);
    r.config.prediction.wait_ms = 50;
    r.config.prediction.model = Some(core_engine::config::ModelSelection {
        id: "test".into(),
        version: "v1".into(),
    });
    let mut s = Session::new(&r, BehaviorProfile::LegacyV1, HostContext::default());
    north(&mut s);
    let key = s.prediction_request().unwrap().key;
    assert!(!s.would_handle(&Action::Legacy(InputEvent::Select(1))));
    std::thread::sleep(std::time::Duration::from_millis(55));
    assert!(s.expire_prediction());
    assert!(!s.snapshot().prediction);
    assert!(!s.complete_prediction(key, Err(Unavailable::Inference)));
    let mut t = Session::new(&r, BehaviorProfile::TouchV1, HostContext::default());
    north(&mut t);
    let plan = t.prepare(Action::Space).unwrap();
    assert!(!plan.consumed());
    assert!(t.prediction_request().is_none());
}

#[test]
fn selecting_a_whole_word_does_not_start_an_overlapping_tail_chain() {
    for profile in [BehaviorProfile::LegacyV1, BehaviorProfile::TouchV1] {
        let mut s = session(profile);
        apply(&mut s, Action::InsertLiteral("五一".into()));
        let next = choose(&s, "期间");
        apply(&mut s, next);
        assert!(!s.has_prediction());
        assert!(
            !s.snapshot().prediction,
            "stopped predictions must clear the old prediction view"
        );
        assert!(!s.would_handle(&Action::Space));
        let baseline = Session::new(&runtime(false, 1), profile, HostContext::default());
        assert_eq!(
            s.would_handle(&Action::Legacy(InputEvent::Char('1'))),
            baseline.would_handle(&Action::Legacy(InputEvent::Char('1')))
        );
        // A new coded word starts a fresh prediction opportunity.
        north(&mut s);
        assert!(s.has_prediction());
    }
}

#[test]
fn model_predictions_require_valid_admission_and_only_show_admitted_ids() {
    use core_engine::{assistance::*, candidates::CandidateId};
    for profile in [BehaviorProfile::LegacyV1, BehaviorProfile::TouchV1] {
        for case in 0..5 {
            let mut r = runtime(true, 10);
            r.config.prediction.model = Some(core_engine::config::ModelSelection {
                id: "fixture".into(),
                version: "v1".into(),
            });
            let mut s = Session::new(&r, profile, HostContext::default());
            north(&mut s);
            let work = s.prediction_request().unwrap();
            let chosen = work.candidates.last().unwrap().clone();
            let accepted = match case {
                0 => None,
                1 => Some(vec![]),
                2 => Some(vec![CandidateId(usize::MAX)]),
                3 => Some(vec![chosen.id, chosen.id]),
                _ => Some(vec![chosen.id]),
            };
            let result = ScoreResponse {
                protocol: 1,
                key: work.key,
                prediction_accepted: accepted,
                scores: work
                    .candidates
                    .iter()
                    .enumerate()
                    .map(|(i, c)| CandidateScore {
                        id: c.id,
                        value: 100.0 - i as f32,
                    })
                    .collect(),
            };
            assert!(s.complete_prediction(work.key, Ok(result)));
            if case < 4 {
                assert!(!s.has_prediction());
                assert!(!s.would_handle(&Action::Space));
            } else {
                let window = s.candidate_window(0, 64);
                assert_eq!(window.candidates.len(), 1);
                assert_eq!(window.candidates[0].id, chosen.id);
                assert!(
                    matches!(apply(&mut s,Action::Space),EditSessionOp::CommitAndReplace{text,..} if text==chosen.text)
                );
            }
        }
    }
}

#[test]
fn prediction_commit_keeps_the_entire_confirmed_prefix() {
    use core_engine::assistance::*;
    for profile in [BehaviorProfile::LegacyV1, BehaviorProfile::TouchV1] {
        let mut r = runtime(true, 10);
        r.config.prediction.model = Some(core_engine::config::ModelSelection {
            id: "fixture".into(),
            version: "v1".into(),
        });
        let mut s = Session::new(&r, profile, HostContext::default());
        apply(&mut s, Action::InsertLiteral("北京".into()));
        let work = s.prediction_request().unwrap();
        assert_eq!(work.prefix, "北京");
        let chosen = work.candidates.iter().find(|c| c.text == "市").unwrap().id;
        let response = ScoreResponse {
            protocol: 1,
            key: work.key,
            prediction_accepted: Some(vec![chosen]),
            scores: work
                .candidates
                .iter()
                .map(|c| CandidateScore {
                    id: c.id,
                    value: 0.,
                })
                .collect(),
        };
        s.complete_prediction(work.key, Ok(response));
        apply(&mut s, Action::Space);
        if let Some(next) = s.prediction_request() {
            assert_eq!(next.prefix, "北京市");
        }
    }
}
