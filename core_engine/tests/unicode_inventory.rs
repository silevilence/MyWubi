use core_engine::expressions::{ExpressionKind, ExpressionLibrary};
use std::{collections::HashSet, path::Path};

#[test]
fn assigned_visible_unicode_symbols_and_punctuation_are_directly_selectable() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut outputs = HashSet::new();
    for kind in ExpressionKind::ALL {
        let library = ExpressionLibrary::load(&root.join(kind.default_path()), kind).unwrap();
        outputs.extend(library.document().entries.iter().map(|e| e.text.clone()));
    }
    let mut invisible = HashSet::new();
    for line in include_str!("../../expressions/sources/DerivedCoreProperties-18.0.txt").lines() {
        let parts: Vec<_> = line
            .split('#')
            .next()
            .unwrap()
            .split(';')
            .map(str::trim)
            .collect();
        if parts.get(1) != Some(&"Default_Ignorable_Code_Point") {
            continue;
        }
        let range: Vec<_> = parts[0]
            .split("..")
            .map(|p| u32::from_str_radix(p, 16).unwrap())
            .collect();
        invisible.extend(range[0]..=*range.last().unwrap());
    }
    for line in include_str!("../../expressions/sources/UnicodeData-18.0.txt").lines() {
        let parts: Vec<_> = line.split(';').collect();
        let cp = u32::from_str_radix(parts[0], 16).unwrap();
        if (parts[2].starts_with('P') || parts[2].starts_with('S') || parts[2].starts_with('N'))
            && !invisible.contains(&cp)
        {
            assert!(
                outputs.contains(&char::from_u32(cp).unwrap().to_string()),
                "Missing U+{:04X} {}",
                cp,
                parts[1]
            );
        }
    }
    for (kind, text, query) in [
        (ExpressionKind::Greek, "α", "greek small letter alpha"),
        (ExpressionKind::Enclosed, "①", "circled digit one"),
        (ExpressionKind::Latin, "ā", "a1"),
        (ExpressionKind::Phonetic, "ʃ", "latin small letter esh"),
        (ExpressionKind::Braille, "⣿", "U+28FF"),
        (
            ExpressionKind::Diacritics,
            "\u{0301}",
            "combining acute accent",
        ),
    ] {
        let library = ExpressionLibrary::load(&root.join(kind.default_path()), kind).unwrap();
        assert!(
            library
                .search(query, &[])
                .matches
                .iter()
                .any(|m| library.document().entries[m.index].text == text),
            "{query}"
        );
    }
}
