use core_engine::{expression_search::MatchQuality, expressions::*};
use serde_json::Value;
use std::{collections::BTreeSet, path::Path};

fn load(kind: ExpressionKind) -> ExpressionLibrary {
    ExpressionLibrary::load(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../expressions")
            .join(format!("{}.mwe", kind.slug())),
        kind,
    )
    .unwrap()
}

#[test]
fn kana_inventory_and_all_exact_reading_aliases_match_the_published_manifest() {
    let library = load(ExpressionKind::Kana);
    let manifest: Value =
        serde_json::from_str(include_str!("../../expressions/kana-coverage.json")).unwrap();
    let entries = &library.document().entries;
    assert_eq!(entries.len(), 224);
    assert_eq!(manifest["count"], entries.len());
    assert_eq!(
        entries
            .iter()
            .map(|e| &e.text)
            .collect::<BTreeSet<_>>()
            .len(),
        entries.len()
    );
    assert_eq!(
        serde_json::to_value(entries.iter().map(|e| &e.text).collect::<Vec<_>>()).unwrap(),
        manifest["outputs"]
    );
    for (query, expected) in manifest["queries"].as_object().unwrap() {
        for spelling in [query.clone(), query.to_ascii_uppercase()] {
            let result = library.search(&spelling, &[]);
            let exact: Vec<_> = result
                .matches
                .iter()
                .filter(|row| row.quality == MatchQuality::Exact)
                .map(|row| entries[row.index].text.as_str())
                .collect();
            assert_eq!(
                serde_json::to_value(exact).unwrap(),
                *expected,
                "{spelling}"
            );
        }
    }
    let encoded = library.encode().unwrap();
    let restored = ExpressionLibrary::parse(&encoded, ExpressionKind::Kana).unwrap();
    for output in ["あ", "ア", "ぁ", "が", "か\u{3099}", "パ", "ハ\u{309a}"] {
        assert!(restored
            .document()
            .entries
            .iter()
            .any(|entry| entry.text == output));
    }
}

#[test]
fn shi_has_deterministic_hiragana_then_katakana_results_without_transliteration() {
    let library = load(ExpressionKind::Kana);
    let result = library.search("shi", &[]);
    let outputs: Vec<_> = result
        .matches
        .iter()
        .map(|row| library.document().entries[row.index].text.as_str())
        .collect();
    assert_eq!(outputs, ["し", "シ"]);
    // The shared search may offer explicitly partial character matches; it must
    // not synthesize a Japanese word or label the whole romaji string exact.
    for row in library.search("konnichiha", &[]).matches {
        assert_ne!(row.quality, MatchQuality::Exact);
        assert_ne!(library.document().entries[row.index].text, "こんにちは");
    }
}
