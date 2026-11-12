use core_engine::{expression_search::*, expressions::*};
use std::{collections::BTreeSet, path::Path, sync::OnceLock, time::Instant};

fn full(kind: ExpressionKind) -> &'static ExpressionLibrary {
    static EMOJI: OnceLock<ExpressionLibrary> = OnceLock::new();
    static KAOMOJI: OnceLock<ExpressionLibrary> = OnceLock::new();
    let (slot, name) = match kind {
        ExpressionKind::Emoji => (&EMOJI, "emoji.mwe"),
        ExpressionKind::Kaomoji => (&KAOMOJI, "kaomoji.mwe"),
        _ => panic!("this regression fixture only covers the original full libraries"),
    };
    slot.get_or_init(|| {
        ExpressionLibrary::load(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../expressions")
                .join(name),
            kind,
        )
        .unwrap()
    })
}
fn outputs<'a>(library: &'a ExpressionLibrary, query: &str) -> Vec<(&'a str, MatchQuality)> {
    library
        .search(query, &[])
        .matches
        .into_iter()
        .map(|row| {
            (
                library.document().entries[row.index].text.as_str(),
                row.quality,
            )
        })
        .collect()
}
fn fixture(rows: &[(&str, &str, &[&str], &[&str])]) -> ExpressionLibrary {
    ExpressionLibrary::new(
        ExpressionDocument {
            kind: ExpressionKind::Emoji,
            name: "fixture".into(),
            version: "1".into(),
            sources: vec![ExpressionSource {
                id: "test".into(),
                url: "local:test".into(),
                revision: "1".into(),
                license: "MIT".into(),
                notice: "Test data".into(),
            }],
            entries: rows
                .iter()
                .enumerate()
                .map(|(i, (text, name, tags, aliases))| ExpressionEntry {
                    text: (*text).into(),
                    name: (*name).into(),
                    triggers: vec![(*name).into()],
                    tags: tags.iter().map(|s| (*s).into()).collect(),
                    aliases: aliases.iter().map(|s| (*s).into()).collect(),
                    order: i as u32,
                    sources: vec!["test".into()],
                })
                .collect(),
        },
        ExpressionKind::Emoji,
    )
    .unwrap()
}

#[test]
fn official_bicycle_restrictions_are_recalled_without_promoting_ordinary_bicycles() {
    let library = full(ExpressionKind::Emoji);
    for query in [
        "禁止",
        "禁止车",
        "禁止自行车",
        "不准骑车",
        "禁止骑行",
        "不许单车",
    ] {
        let rows = outputs(library, query);
        assert!(rows.iter().any(|r| r.0 == "🚳"), "missing 🚳 for {query}");
        assert!(
            !rows.iter().any(|r| r.0 == "🚲"),
            "lost prohibition for {query}"
        );
        assert!(rows.iter().all(|r| r.1 != MatchQuality::Partial));
    }
    let exact = outputs(library, "禁止自行车");
    assert_eq!(exact[0], ("🚳", MatchQuality::Exact));
    for words in [vec!["禁止车"], vec!["禁止", "车"], vec!["禁", "止", "车"]] {
        let split = library.search(
            "禁止车",
            &words.into_iter().map(str::to_owned).collect::<Vec<_>>(),
        );
        assert_eq!(split.matches, library.search("禁止车", &[]).matches);
    }
    let rows = outputs(library, "禁车");
    assert!(rows
        .iter()
        .any(|r| r.0 == "🚳" && r.1 != MatchQuality::Partial));
}

#[test]
fn combined_cat_emotion_terms_match_both_and_negative_happiness_stays_negative() {
    let library = full(ExpressionKind::Emoji);
    let happy = outputs(library, "猫开心");
    for cat in ["😺", "😸", "😹"] {
        assert!(happy.iter().any(|r| r.0 == cat));
    }
    assert!(!happy.iter().any(|r| ["😿", "🚳", "😀"].contains(&r.0)));
    assert!(happy.iter().all(|r| r.1 == MatchQuality::AllTerms));
    assert_eq!(
        library
            .search("猫开心", &["猫".into(), "开心".into()])
            .matches,
        library.search("猫开心", &["猫开心".into()]).matches
    );
    let sad = outputs(library, "猫不开心");
    assert!(sad.iter().any(|r| r.0 == "😿"));
    assert!(!sad.iter().any(|r| ["😺", "😸", "😹"].contains(&r.0)));
}

#[test]
fn kaomoji_synonym_sets_are_related_without_modifying_the_strict_append_index() {
    let library = full(ExpressionKind::Kaomoji);
    let index_set = |q| {
        library
            .search(q, &[])
            .matches
            .iter()
            .map(|m| m.index)
            .collect::<BTreeSet<_>>()
    };
    let happy = index_set("开心");
    assert!(happy.len() > 500);
    assert_eq!(happy, index_set("高兴"));
    assert_eq!(happy, index_set("快乐"));
    assert!(library.exact("开心").count() > 0);
    assert_eq!(library.exact("猫开心").count(), 0);
    assert!(outputs(library, "qzxvkjunknown").is_empty());
}

#[test]
fn exact_aliases_precede_contains_then_loose_with_stable_ties_and_variants() {
    let library = fixture(&[
        ("1", "红色动物", &["红", "猫"], &[]),
        ("2", "红猫", &[], &[]),
        ("3", "别名猫", &[], &["红猫"]),
        ("4", "红色的猫咪", &[], &[]),
        ("👩🏻‍💻", "女程序员", &["编程"], &[]),
        ("👩🏽‍💻", "女程序员", &["编程"], &[]),
    ]);
    let rows = outputs(&library, "红猫");
    assert_eq!(rows[0], ("2", MatchQuality::Exact));
    assert_eq!(rows[1], ("3", MatchQuality::Exact));
    assert_eq!(rows[2].0, "1");
    assert_eq!(rows[3].0, "4");
    assert!(outputs(&library, "红物").contains(&("1", MatchQuality::Loose)));
    assert_eq!(
        outputs(&library, "女程序员")
            .iter()
            .map(|r| r.0)
            .collect::<Vec<_>>(),
        ["👩🏻‍💻", "👩🏽‍💻"]
    );
    assert_eq!(
        outputs(&library, "序员"),
        [
            ("👩🏻‍💻", MatchQuality::AllTerms),
            ("👩🏽‍💻", MatchQuality::AllTerms)
        ]
    );
}

#[test]
fn partial_matches_are_separate_and_missing_restrictions_are_never_silently_ignored() {
    let library = fixture(&[
        ("🚲", "自行车", &["车"], &[]),
        ("🐱", "猫", &[], &[]),
        ("🙂", "开心", &[], &[]),
        ("🙁", "不开心", &[], &[]),
        ("X", "不禁止自行车", &[], &[]),
    ]);
    for query in ["禁止猫", "不是猫", "猫量子纠缠"] {
        let rows = outputs(&library, query);
        assert!(!rows.is_empty(), "{query}");
        assert!(
            rows.iter().all(|r| r.1 == MatchQuality::Partial),
            "{query}: {rows:?}"
        );
    }
    assert_eq!(outputs(&library, "开心"), [("🙂", MatchQuality::Exact)]);
    let forbidden = outputs(&library, "禁止车");
    assert!(forbidden.iter().all(|r| r.1 == MatchQuality::Partial));
    assert!(outputs(&library, "qzxvkjunknown").is_empty());
    assert!(outputs(&library, "").is_empty());
    assert!(library.search(&"猫".repeat(513), &[]).matches.is_empty());
}

#[test]
fn full_libraries_report_explicit_search_cost_and_do_not_truncate_results() {
    for (kind, query) in [
        (ExpressionKind::Emoji, "禁止车"),
        (ExpressionKind::Kaomoji, "开心"),
    ] {
        let start = Instant::now();
        let library = full(kind);
        let cold = start.elapsed();
        let mut times = Vec::new();
        let mut last = None;
        for _ in 0..50 {
            let start = Instant::now();
            let results = library.search(query, &[query.into()]);
            times.push(start.elapsed().as_micros());
            if let Some(ref previous) = last {
                assert_eq!(previous, &results);
            }
            last = Some(results);
        }
        times.sort_unstable();
        let rows = last.unwrap().matches;
        assert_eq!(
            rows.iter().map(|m| m.index).collect::<BTreeSet<_>>().len(),
            rows.len()
        );
        if kind == ExpressionKind::Kaomoji {
            assert!(rows.len() > 500);
        }
        eprintln!(
            "{kind:?} search '{query}': {} rows, initialization observed={}ms, p50={}us p95={}us",
            rows.len(),
            cold.as_millis(),
            times[25],
            times[47]
        );
    }
}
