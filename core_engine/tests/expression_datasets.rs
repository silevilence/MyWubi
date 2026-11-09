use core_engine::expressions::{ExpressionKind, ExpressionLibrary};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

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
