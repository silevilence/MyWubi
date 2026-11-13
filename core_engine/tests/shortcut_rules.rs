use core_engine::shortcuts::*;

#[test]
fn frozen_c07_validation_and_pause_contract() {
    let document: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/validation/special-input/cases.json"
    ))
    .unwrap();
    let case = document["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "SI-C07")
        .unwrap();
    for (raw, expected) in case["normalize"].as_object().unwrap() {
        assert_eq!(
            validate_structure(&[rule(raw, 2)]).unwrap()[0].code,
            expected.as_str().unwrap()
        );
    }
    for code in case["invalid_codes"].as_array().unwrap() {
        assert!(validate_structure(&[rule(code.as_str().unwrap(), 2)]).is_err());
    }
    for length in case["valid_lengths"].as_array().unwrap() {
        for position in case["position_bounds"].as_array().unwrap() {
            validate_structure(&[rule(
                &"a".repeat(length.as_u64().unwrap() as usize),
                position.as_u64().unwrap() as u32,
            )])
            .unwrap();
        }
    }
    for enabled in [true, false] {
        let mut duplicate = rule("jjad", 2);
        duplicate.enabled = enabled;
        assert!(validate_structure(&[rule("JJAD", 2), duplicate]).is_err());
    }
    let rules = vec![rule("jjad", 2), rule("abcd", 2)];
    for key in case["conflict_keys"].as_array().unwrap() {
        let environment = ShortcutEnvironment {
            wildcard: key.as_str().unwrap().chars().next(),
            auxiliary_entry: None,
        };
        assert!(!availability(&rules[0], environment).effective);
        assert!(availability(&rules[1], environment).effective);
        assert!(rules[0].enabled);
        assert!(availability(&rules[0], ShortcutEnvironment::default()).effective);
    }
}

#[test]
fn configuration_save_normalizes_both_frontends_and_invalid_rules_preserve_disk() {
    use core_engine::{config::ValidationContext, Config};
    let root = std::env::temp_dir().join(format!(
        "si06-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("config.toml");
    assert!(Config::from_str("").unwrap().shortcuts.rules.is_empty());
    for context in [ValidationContext::DesktopSave, ValidationContext::TouchSave] {
        let mut config = Config::default();
        config.shortcuts.rules = vec![rule("JJAD", 2), rule("jjad", 3)];
        config.shortcuts.rules[1].format = "yyyy-MM-dd".into();
        config.save_for(&path, context).unwrap();
        let saved = std::fs::read(&path).unwrap();
        assert_eq!(Config::load(&path).unwrap().shortcuts.rules[0].code, "jjad");
        assert_eq!(config.shortcuts.rules[0].code, "JJAD");
        config.shortcuts.rules[1].enabled = false;
        config.shortcuts.rules[1].position = 2;
        let error = config.save_for(&path, context).unwrap_err().to_string();
        assert!(error.contains("shortcuts.rules[1].position"), "{error}");
        assert_eq!(std::fs::read(&path).unwrap(), saved);
        assert_eq!(config.shortcuts.rules[1].position, 2);
    }
    std::fs::remove_dir_all(root).unwrap();
}

fn rule(code: &str, position: u32) -> ShortcutRule {
    ShortcutRule {
        code: code.into(),
        position,
        kind: ShortcutKind::DateTime,
        format: "yyyy年MM月dd日".into(),
        enabled: true,
    }
}

#[test]
fn effective_index_keeps_rule_identity_and_excludes_disabled_or_incompatible_prefixes() {
    let rows = vec![
        rule("JJAD", 3),
        rule("jjad", 2),
        ShortcutRule {
            enabled: false,
            ..rule("zzad", 1)
        },
        rule("jjaad", 4),
    ];
    let index = RuleSet::new(&rows, ShortcutEnvironment::default()).unwrap();
    assert!(index.is_prefix("JJA"));
    assert!(index.is_prefix("JJAA"));
    assert!(!index.is_prefix("zz"));
    assert!(!index.is_prefix("jjad"));
    assert!(index.reaches("jjad"));
    assert_eq!(
        index
            .exact("JJAD")
            .iter()
            .map(|r| (r.rule_index, r.position))
            .collect::<Vec<_>>(),
        [(1, 2), (0, 3)]
    );
    let paused = RuleSet::new(
        &rows,
        ShortcutEnvironment {
            wildcard: Some('J'),
            auxiliary_entry: None,
        },
    )
    .unwrap();
    assert!(paused.is_empty());
    assert!(!paused.reaches("j"));
    assert!(rows[0].enabled);
}

#[test]
fn canonicalization_preserves_drafts_and_reports_disabled_duplicate_row() {
    let mut rows = vec![rule("JJAD", 2), rule("jjad", 3)];
    rows[1].format = "yyyy-MM-dd".into();
    let canonical = validate_structure(&rows).unwrap();
    assert_eq!(rows[0].code, "JJAD");
    assert_eq!(canonical[0].code, "jjad");
    assert_eq!(canonical[0].position, 2);
    assert_eq!(canonical[1].position, 3);
    assert_eq!(canonical[1].format, "yyyy-MM-dd");
    rows[1].position = 2;
    rows[1].enabled = false;
    let before = rows.clone();
    let error = validate_structure(&rows).unwrap_err();
    assert_eq!(error.rule, Some(1));
    assert_eq!(error.field, "position");
    assert_eq!(rows, before);
}

#[test]
fn code_and_global_position_boundaries_are_explicit() {
    for code in ["j".to_string(), "j".repeat(32)] {
        for position in [1, 10_000] {
            assert!(validate_structure(&[rule(&code, position)]).is_ok());
        }
    }
    for code in ["", "jj1", "jj;", "jj ad", "é", "ｊｊａｄ", "j\n"] {
        assert_eq!(
            validate_structure(&[rule(code, 1)]).unwrap_err().field,
            "code"
        );
    }
    assert_eq!(
        validate_structure(&[rule(&"j".repeat(33), 1)])
            .unwrap_err()
            .field,
        "code"
    );
    for position in [0, 10_001, u32::MAX] {
        assert_eq!(
            validate_structure(&[rule("jjad", position)])
                .unwrap_err()
                .field,
            "position"
        );
    }
}

#[test]
fn total_and_per_code_limits_include_disabled_rules() {
    let mut rows: Vec<_> = (1..=32).map(|position| rule("jjad", position)).collect();
    for row in &mut rows {
        row.enabled = false;
    }
    assert!(validate_structure(&rows).is_ok());
    rows.push(rule("JJAD", 33));
    assert_eq!(validate_structure(&rows).unwrap_err().rule, Some(32));
    let mut many = Vec::new();
    for index in 0..1025 {
        // Three alphabetic suffixes give each rule a distinct code.
        let code = format!(
            "j{}{}{}",
            char::from(b'a' + ((index / 676) % 26) as u8),
            char::from(b'a' + ((index / 26) % 26) as u8),
            char::from(b'a' + (index % 26) as u8)
        );
        let mut row = rule(&code, 2);
        row.enabled = false;
        many.push(row);
    }
    assert!(validate_structure(&many[..1024]).is_ok());
    assert_eq!(validate_structure(&many).unwrap_err().rule, None);
}

#[test]
fn format_length_counts_unicode_scalars_and_roundtrips_without_normalization() {
    let mut row = rule("jjad", 2);
    row.format = "年".repeat(256);
    assert!(validate_structure(&[row.clone()]).is_ok());
    row.format.push('月');
    assert_eq!(
        validate_structure(&[row.clone()]).unwrap_err().field,
        "format"
    );
    row.format = "年\u{301}".repeat(128);
    assert!(validate_structure(&[row.clone()]).is_ok());
    let encoded = serde_json::to_string(&row).unwrap();
    assert_eq!(serde_json::from_str::<ShortcutRule>(&encoded).unwrap(), row);
    let encoded = toml::to_string(&row).unwrap();
    assert_eq!(toml::from_str::<ShortcutRule>(&encoded).unwrap(), row);
}

#[test]
fn format_tokens_and_quoted_literals_have_one_cross_platform_grammar() {
    use core_engine::shortcut_format::{compile, DateField::*, FormatPart::*};
    assert_eq!(
        compile("yyyy年MM月dd日 HH:mm:ss").unwrap(),
        [
            Field(Year),
            Literal("年".into()),
            Field(Month),
            Literal("月".into()),
            Field(Day),
            Literal("日 ".into()),
            Field(Hour),
            Literal(":".into()),
            Field(Minute),
            Literal(":".into()),
            Field(Second),
        ]
    );
    assert_eq!(
        compile("'ISO' yyyy-MM-dd'T'HH:mm:ss 'o''clock' ''")
            .unwrap()
            .last(),
        Some(&Literal(" o'clock '".into()))
    );
    assert_eq!(compile("'yyyy'").unwrap(), [Literal("yyyy".into())]);
    for format in ["yy", "YYYY", "M", "hh", "a", "%Y", "yyyy-MM-ddT", "'未闭合"] {
        let mut row = rule("jjad", 2);
        row.format = format.into();
        let error = validate_structure(&[row]).unwrap_err();
        assert_eq!(error.field, "format", "{format}");
        assert_eq!(error.rule, Some(0));
    }
    assert_eq!(compile("年T"), Err(1));
}

#[test]
fn table_conflicts_pause_and_restore_without_mutating_enable_intent() {
    let row = rule("jJAD", 2);
    for environment in [
        ShortcutEnvironment {
            wildcard: Some('J'),
            auxiliary_entry: None,
        },
        ShortcutEnvironment {
            wildcard: None,
            auxiliary_entry: Some('A'),
        },
    ] {
        let status = availability(&row, environment);
        assert!(!status.compatible);
        assert!(!status.effective);
        assert!(status.reason.is_some());
        assert!(validate_structure(&[row.clone()]).is_ok());
        assert!(row.enabled);
    }
    let status = availability(
        &row,
        ShortcutEnvironment {
            wildcard: Some('*'),
            auxiliary_entry: Some('`'),
        },
    );
    assert!(status.compatible && status.effective);
    let disabled = ShortcutRule {
        enabled: false,
        ..row
    };
    let status = availability(&disabled, ShortcutEnvironment::default());
    assert!(status.compatible);
    assert!(!status.effective);
    assert!(status.reason.is_none());
}

#[test]
fn active_table_projection_does_not_require_a_matching_word_or_obey_main_code_length() {
    let wildcard = core_engine::Dictionary::from_text(
        "---\nmax_code_len: 4\nwildcard_key: j\ncharset: abcdefghijklmnopqrstuvwxyz\n---\na\t普通\n"
    ).unwrap();
    let ordinary = core_engine::Dictionary::from_text("a\t普通\n").unwrap();
    let row = rule("jjaad", 2);
    assert!(validate_structure(&[row.clone()]).is_ok());
    assert!(!availability(&row, wildcard.table_config().into()).effective);
    assert!(availability(&row, ordinary.table_config().into()).effective);
    assert_eq!(row.code, "jjaad");
    assert!(row.enabled);
}
