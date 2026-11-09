use core_engine::expressions::{ExpressionKind, ExpressionLibrary};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

#[test]
fn kaomoji_is_an_attributed_lossless_subset_of_pinned_sources_and_all_triggers_work() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../expressions");
    let library =
        ExpressionLibrary::load(&root.join("kaomoji.mwe"), ExpressionKind::Kaomoji).unwrap();
    let first: BTreeMap<String, Vec<String>> = serde_json::from_str(
        &std::fs::read_to_string(root.join("sources/kaomojiya-kaomoji.json")).unwrap(),
    )
    .unwrap();
    let second: Vec<serde_json::Value> = serde_json::from_str(
        &std::fs::read_to_string(root.join("sources/kaomojikan-kaomoji.json")).unwrap(),
    )
    .unwrap();
    let outputs: BTreeMap<&str, &core_engine::expressions::ExpressionEntry> = library
        .document()
        .entries
        .iter()
        .map(|entry| (entry.text.as_str(), entry))
        .collect();
    assert_eq!(outputs.len(), 34354);
    let source_one: BTreeSet<_> = first.values().flatten().map(String::as_str).collect();
    let source_two: BTreeSet<_> = second
        .iter()
        .map(|item| item["text"].as_str().unwrap())
        .collect();
    let mut trigger_outputs: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for (&text, entry) in &outputs {
        assert!(
            source_one.contains(text) || source_two.contains(text),
            "invented or changed output {text}"
        );
        assert!(entry.sources.iter().all(|id| match id.as_str() {
            "kaomojiya" => source_one.contains(text),
            "kaomojikan" => source_two.contains(text),
            "mywubi-zh" => true,
            _ => false,
        }));
        assert!(entry
            .triggers
            .iter()
            .all(|word| word.chars().any(|c| ('\u{3400}'..='\u{9fff}').contains(&c))));
        for trigger in &entry.triggers {
            trigger_outputs.entry(trigger).or_default().insert(text);
        }
    }
    for (word, expected) in trigger_outputs {
        assert_eq!(
            library
                .exact(word)
                .map(|entry| entry.text.as_str())
                .collect::<BTreeSet<_>>(),
            expected
        );
    }
    for required in [
        "开心", "悲伤", "生气", "惊讶", "害羞", "感谢", "道歉", "问候", "鼓励", "动物", "拥抱",
    ] {
        assert!(
            library.exact(required).next().is_some(),
            "missing category {required}"
        );
    }
    assert_eq!(
        ExpressionLibrary::parse(&library.encode().unwrap(), ExpressionKind::Kaomoji)
            .unwrap()
            .document(),
        library.document()
    );
}

#[test]
fn bundled_emoji_matches_every_official_sequence_and_preserves_compatibility_aliases() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../expressions");
    let library = ExpressionLibrary::load(&root.join("emoji.mwe"), ExpressionKind::Emoji).unwrap();
    let official = std::fs::read_to_string(root.join("sources/emoji-test-18.0.txt")).unwrap();
    let mut expected = BTreeSet::new();
    let mut alternate = BTreeSet::new();
    for line in official
        .lines()
        .filter(|line| !line.starts_with('#') && line.contains(';'))
    {
        let (points, rest) = line.split_once(';').unwrap();
        let status = rest.split_once('#').unwrap().0.trim();
        let text: String = points
            .split_whitespace()
            .map(|point| char::from_u32(u32::from_str_radix(point, 16).unwrap()).unwrap())
            .collect();
        if matches!(status, "fully-qualified" | "component") {
            expected.insert(text);
        } else {
            alternate.insert(text);
        }
    }
    let entries = &library.document().entries;
    let actual: BTreeSet<_> = entries.iter().map(|entry| entry.text.clone()).collect();
    assert_eq!(expected, actual);
    assert_eq!(entries.len(), 3972);
    let mut aliases = BTreeSet::new();
    let mut triggers: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for entry in entries {
        assert!(
            entry
                .triggers
                .iter()
                .any(|word| word.chars().any(|c| ('\u{3400}'..='\u{9fff}').contains(&c))),
            "{}",
            entry.name
        );
        for trigger in &entry.triggers {
            triggers.entry(trigger).or_default().insert(&entry.text);
        }
        for alias in &entry.aliases {
            assert_eq!(
                alias.replace('\u{fe0f}', ""),
                entry.text.replace('\u{fe0f}', "")
            );
            assert!(aliases.insert(alias.clone()), "ambiguous alias {alias}");
        }
    }
    assert_eq!(alternate, aliases);
    // Validate that every trigger reaches every variant, not just one example per category.
    for (word, outputs) in triggers {
        assert_eq!(
            library
                .exact(word)
                .map(|entry| entry.text.as_str())
                .collect::<BTreeSet<_>>(),
            outputs
        );
    }
    assert_eq!(
        ExpressionLibrary::parse(&library.encode().unwrap(), ExpressionKind::Emoji)
            .unwrap()
            .document(),
        library.document()
    );
}
