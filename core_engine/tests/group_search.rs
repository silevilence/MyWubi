use core_engine::{group_input::Position, group_search::*, Dictionary, Entry};
use std::{cmp::Reverse, collections::HashSet};
fn position(chars: &str) -> Position {
    Position {
        characters: chars.into(),
        selected: None,
    }
}

#[test]
fn every_small_group_sequence_matches_exhaustive_reference_and_all_sources() {
    let mut entries = Vec::new();
    for a in ['a', 'b', 'c'] {
        for b in ['a', 'b', 'c'] {
            for c in ['a', 'b', 'c'] {
                let i = entries.len();
                entries.push(Entry {
                    code: format!("{a}{b}{c}"),
                    word: format!("word{}", i % 7),
                    weight: (i % 5) as u32,
                });
            }
        }
    }
    entries.push(Entry {
        code: "a".into(),
        word: "short".into(),
        weight: 0,
    });
    let dict = Dictionary::from_entries(entries, None, Default::default()).unwrap();
    let groups = ["a", "b", "c", "ab", "ac", "bc", "abc"];
    for a in groups {
        for b in groups {
            for c in groups {
                let positions = [position(a), position(b), position(c)];
                for length in 1..=3 {
                    let positions = &positions[..length];
                    let result = query(&dict, positions, WORK_BUDGET);
                    assert_eq!(result.status, SearchStatus::Complete);
                    let mut reference: Vec<_> = dict
                        .entries()
                        .iter()
                        .enumerate()
                        .filter(|(_, e)| {
                            e.code.chars().count() >= length
                                && e.code
                                    .chars()
                                    .zip(positions)
                                    .all(|(c, p)| p.characters.contains(c))
                        })
                        .collect();
                    reference.sort_by_key(|(i, e)| {
                        (e.code.chars().count() != length, Reverse(e.weight), *i)
                    });
                    let mut seen = HashSet::new();
                    let words: Vec<_> = reference
                        .iter()
                        .filter(|(_, e)| seen.insert(&e.word))
                        .map(|(_, e)| e.word.as_str())
                        .collect();
                    assert_eq!(
                        result
                            .candidates
                            .iter()
                            .map(|m| m.text.as_str())
                            .collect::<Vec<_>>(),
                        words
                    );
                    for item in result.candidates {
                        let codes: HashSet<_> = reference
                            .iter()
                            .filter(|(_, e)| e.word == item.text)
                            .map(|(_, e)| e.code.clone())
                            .collect();
                        assert_eq!(item.codes.into_iter().collect::<HashSet<_>>(), codes);
                    }
                }
            }
        }
    }
}
#[test]
fn insufficient_budget_never_claims_complete_or_unique() {
    let dict = Dictionary::from_text("a\t甲\nab\t乙\n").unwrap();
    let result = query(&dict, &[position("ab")], 0);
    assert_eq!(result.status, SearchStatus::BudgetExceeded);
    assert!(result.candidates.is_empty());
}
#[test]
fn explicit_letter_filters_without_destroying_original_group() {
    let dict = Dictionary::from_text("ab\t甲\nac\t乙\nbb\t丙\n").unwrap();
    let mut positions = [position("ab"), position("bc")];
    positions[0].selected = Some('a');
    let result = query(&dict, &positions, WORK_BUDGET);
    assert_eq!(result.candidates.len(), 2);
    assert_eq!(positions[0].characters, "ab");
    assert!(result
        .candidates
        .iter()
        .all(|m| m.codes.iter().all(|c| c.starts_with('a'))));
}

#[test]
fn windows_across_long_tail_keep_order_and_identity() {
    use core_engine::{
        group_input::{Definition, MappingIdentity},
        Action, BehaviorProfile, SemanticEngine, StateMachine,
    };
    let text = (0..600)
        .map(|i| format!("a\t词{i}\t{i}\n"))
        .collect::<String>();
    let dict = Dictionary::from_text(&text).unwrap();
    let mut engine = SemanticEngine::new(
        StateMachine::with_options(dict, 5, false),
        BehaviorProfile::TouchV1,
    );
    let identity = MappingIdentity {
        id: "test".into(),
        version: "1".into(),
        revision: 1,
    };
    engine.dispatch(Action::BindMapping(
        identity.clone(),
        Definition {
            charset: "ab".into(),
            groups: [("pair".into(), "ab".into())].into(),
        },
    ));
    engine.dispatch(Action::InputGroup(identity, "pair".into()));
    let first = engine.candidate_window(0, 256);
    let tail = engine.candidate_window(512, 256);
    assert_eq!(first.total, 600);
    assert_eq!(tail.candidates.len(), 88);
    assert_eq!(first.query_revision, tail.query_revision);
    assert_eq!(first.candidates[0].text, "词599");
    assert_eq!(tail.candidates.last().unwrap().text, "词0");
    assert_eq!(tail.candidates[0].id.0, 512);
}
