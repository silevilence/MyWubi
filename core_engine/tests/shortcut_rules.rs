use core_engine::shortcuts::*;

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
