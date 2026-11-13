use core_engine::{
    candidates::*, expressions::*, runtime::RuntimeSnapshot, session::*, shortcut_time::*,
    shortcuts::*, *,
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

struct TestClock {
    calls: AtomicUsize,
    value: Mutex<Result<LocalTime, String>>,
}
impl TestClock {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            calls: AtomicUsize::new(0),
            value: Mutex::new(Ok(time(12, 7, 8, 9))),
        })
    }
    fn count(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}
impl Clock for TestClock {
    fn now(&self) -> Result<LocalTime, String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.value.lock().unwrap().clone()
    }
}
fn time(day: u8, hour: u8, minute: u8, second: u8) -> LocalTime {
    LocalTime {
        year: 2026,
        month: 11,
        day,
        hour,
        minute,
        second,
        utc_offset_seconds: 28800,
        timezone: "Asia/Shanghai".into(),
    }
}
fn rule(code: &str, position: u32, format: &str) -> ShortcutRule {
    ShortcutRule {
        code: code.into(),
        position,
        format: format.into(),
        kind: ShortcutKind::DateTime,
        enabled: true,
    }
}
fn runtime(
    words: &[&str],
    capacity: u8,
    rules: Vec<ShortcutRule>,
    special: Option<&str>,
) -> RuntimeSnapshot {
    let mut config = Config::default();
    config.basic.candidate_count = capacity;
    config.basic.auto_commit_unique = false;
    config.shortcuts.rules = rules;
    let text = words
        .iter()
        .enumerate()
        .map(|(i, word)| format!("jjad\t{word}\t{}\n", words.len() - i))
        .collect::<String>();
    let mut resources = ExpressionResources::default();
    if let Some(output) = special {
        config.expressions.symbols.append = true;
        resources.symbols.library = Some(Arc::new(
            ExpressionLibrary::new(
                ExpressionDocument {
                    kind: ExpressionKind::Symbols,
                    name: "fixture".into(),
                    version: "1".into(),
                    sources: vec![ExpressionSource {
                        id: "test".into(),
                        url: "local:test".into(),
                        revision: "1".into(),
                        license: "MIT".into(),
                        notice: "test".into(),
                    }],
                    entries: vec![ExpressionEntry {
                        text: output.into(),
                        name: "符号".into(),
                        triggers: vec!["甲".into()],
                        aliases: vec![],
                        tags: vec![],
                        order: 0,
                        sources: vec!["test".into()],
                    }],
                },
                ExpressionKind::Symbols,
            )
            .unwrap(),
        ));
    }
    RuntimeSnapshot {
        revision: 1,
        dict: Dictionary::from_text(&text).unwrap(),
        temporary: Arc::default(),
        expressions: Arc::new(resources),
        config,
        config_path: "config.toml".into(),
        system_table_path: "main.dict".into(),
        user_table_path: "user.dict".into(),
    }
}
fn apply(s: &mut Session, action: Action) -> PreparedEdit {
    let plan = s.prepare(action).unwrap();
    assert_eq!(
        s.acknowledge(plan.token, HostOutcome::Applied).status,
        AckStatus::Published
    );
    plan
}
fn input(s: &mut Session, text: &str) {
    for c in text.chars() {
        let plan = apply(s, Action::InputCode(c));
        assert!(
            !matches!(
                plan.op,
                EditSessionOp::CommitAndReplace { .. } | EditSessionOp::CommitThenUpdate { .. }
            ),
            "unexpected commit {text}: {:?}",
            plan.op
        );
    }
}
fn words(s: &Session) -> Vec<String> {
    s.candidate_window(0, 256)
        .candidates
        .into_iter()
        .map(|c| c.text)
        .collect()
}
fn pages(s: &mut Session) -> Vec<Vec<String>> {
    let mut pages = vec![s.snapshot().items.iter().map(|i| i.text.clone()).collect()];
    while s.state_machine().page() + 1 < s.state_machine().total_pages() {
        apply(s, Action::MoveCandidatePage(true));
        pages.push(s.snapshot().items.iter().map(|i| i.text.clone()).collect());
    }
    pages
}

#[test]
fn fixed_format_tokens_leap_days_and_timezone_snapshots() {
    let render_format = |format: &str, t: &LocalTime| {
        shortcut_time::render(&shortcut_format::compile(format).unwrap(), t)
    };
    assert_eq!(render_format("MM/mm", &time(12, 7, 8, 9)).unwrap(), "11/08");
    let new_year = LocalTime {
        year: 2027,
        month: 1,
        day: 1,
        hour: 0,
        minute: 0,
        second: 0,
        utc_offset_seconds: -18000,
        timezone: "America/New_York".into(),
    };
    assert_eq!(
        render_format("yyyy-MM-dd HH:mm:ss", &new_year).unwrap(),
        "2027-01-01 00:00:00"
    );
    assert_eq!(
        render_format("yyyy-MM-dd'T'HH:mm:ss", &time(12, 7, 8, 9)).unwrap(),
        "2026-11-12T07:08:09"
    );
    let mut t = time(29, 0, 0, 0);
    t.month = 2;
    t.year = 2024;
    assert_eq!(render_format("yyyy-MM-dd", &t).unwrap(), "2024-02-29");
    t.year = 2100;
    assert!(render_format("yyyy", &t).is_err());
    t.year = 2000;
    assert!(render_format("yyyy", &t).is_ok());
    t.utc_offset_seconds = i32::MIN;
    assert!(t.validate().is_err());
    assert!(shortcut_time::render(
        &[shortcut_format::FormatPart::Literal("年".repeat(513))],
        &time(12, 0, 0, 0)
    )
    .is_err());
}

#[test]
fn preview_uses_the_same_draft_merge_sources_and_page_locations_for_both_profiles() {
    let r = runtime(
        &["甲", "乙"],
        10,
        vec![
            rule("jjad", 2, "yyyy年MM月dd日"),
            rule("jjad", 8, "yyyy-MM-dd"),
        ],
        Some("表"),
    );
    for profile in [BehaviorProfile::LegacyV1, BehaviorProfile::TouchV1] {
        let p = shortcut_preview::from_runtime(&r, "JJAD", time(12, 7, 8, 9), profile).unwrap();
        assert_eq!(p.effective_capacity, 9);
        assert_eq!(
            p.rows
                .iter()
                .map(|r| r.candidate.text.as_str())
                .collect::<Vec<_>>(),
            ["2026年11月12日", "2026-11-12"]
        );
        assert_eq!(
            p.rows
                .iter()
                .map(|r| (r.target, r.actual, r.page, r.slot))
                .collect::<Vec<_>>(),
            [(Some(2), 1, 1, 1), (Some(8), 2, 1, 2)]
        );
        let p = shortcut_preview::from_runtime(&r, "jjad", time(12, 7, 8, 9), profile).unwrap();
        assert_eq!(
            p.rows
                .iter()
                .map(|r| r.candidate.text.as_str())
                .collect::<Vec<_>>(),
            ["甲", "2026年11月12日", "乙", "表", "2026-11-12"]
        );
        assert_eq!(p.rows[4].actual, 5);
        assert_eq!(p.rows[4].target, Some(8));
        assert_eq!(p.rows[0].source, "普通候选");
        assert_eq!(p.rows[3].source, ExpressionKind::Symbols.label());
        assert_eq!(p.rows[1].source, "日期时间");
        assert!(p.resources.iter().any(|s| s.contains("合并码表快照")));
    }
}

#[test]
fn temporary_modes_never_create_date_candidates_or_read_time() {
    let clock = TestClock::new();
    let mut r = runtime(&[], 3, vec![rule("jjad", 1, "日期")], Some("符号"));
    r.config.expressions.symbols.search = true;
    r.temporary = Arc::new(TemporaryTables {
        pinyin: Some(Dictionary::from_text("---\ntype: pinyin\n---\nni\t你\n").unwrap()),
        english: Some(Dictionary::from_text("jjad\tword\n").unwrap()),
        ..Default::default()
    });
    for mode in [
        TemporaryMode::Pinyin,
        TemporaryMode::English,
        TemporaryMode::Symbols,
    ] {
        let mut s = Session::new_with_clock(
            &r,
            BehaviorProfile::TouchV1,
            HostContext::default(),
            clock.clone(),
        );
        apply(&mut s, Action::BeginTemporary(mode));
        if mode == TemporaryMode::Symbols {
            apply(&mut s, Action::SetExpressionInput(ExpressionInput::Literal));
        }
        for c in "jjad".chars() {
            let p = s.prepare(Action::InputCode(c)).unwrap();
            s.acknowledge(p.token, HostOutcome::Applied);
        }
        assert!(!words(&s).contains(&"日期".into()));
        assert_eq!(clock.count(), 0);
    }
}

#[test]
fn c01_c02_dynamic_pages_and_no_hit_keep_their_respective_boundaries() {
    for profile in [BehaviorProfile::LegacyV1, BehaviorProfile::TouchV1] {
        let clock = TestClock::new();
        let base = ["甲", "乙", "丙", "丁", "戊", "己"];
        let r = runtime(&base, 3, vec![], Some("表"));
        let mut s = Session::new_with_clock(&r, profile, HostContext::default(), clock.clone());
        input(&mut s, "jjad");
        assert_eq!(
            pages(&mut s),
            [vec!["甲", "乙", "丙"], vec!["表"], vec!["丁", "戊", "己"]]
        );
        assert_eq!(clock.count(), 0);
        for position in [1, 2, 3, 4, 5] {
            let r = runtime(&base, 3, vec![rule("jjad", position, "日期")], Some("表"));
            let mut s = Session::new_with_clock(&r, profile, HostContext::default(), clock.clone());
            input(&mut s, "jjad");
            let mut expected = vec!["甲", "乙", "丙", "表", "丁", "戊", "己"];
            expected.insert(position as usize - 1, "日期");
            assert_eq!(words(&s), expected);
            assert_eq!(
                pages(&mut s),
                expected.chunks(3).map(|p| p.to_vec()).collect::<Vec<_>>()
            );
        }
    }
}

#[test]
fn c03_c04_c05_sparse_positions_and_same_text_keep_distinct_sources() {
    let clock = TestClock::new();
    for (base, rules, special, expected) in [
        (
            vec![],
            vec![rule("jjad", 2, "日期甲"), rule("jjad", 3, "日期乙")],
            None,
            vec!["日期甲", "日期乙"],
        ),
        (
            vec!["甲", "乙"],
            vec![rule("jjad", 2, "日期甲"), rule("jjad", 8, "日期乙")],
            None,
            vec!["甲", "日期甲", "乙", "日期乙"],
        ),
        (
            vec!["2026-11-12", "甲", "乙"],
            vec![rule("jjad", 3, "yyyy-MM-dd")],
            None,
            vec!["2026-11-12", "甲", "2026-11-12", "乙"],
        ),
        (
            vec!["甲"],
            vec![rule("jjad", 3, "'X'"), rule("jjad", 4, "'X'")],
            Some("X"),
            vec!["甲", "X", "X"],
        ),
    ] {
        let r = runtime(&base, 3, rules, special);
        let mut s = Session::new_with_clock(
            &r,
            BehaviorProfile::TouchV1,
            HostContext::default(),
            clock.clone(),
        );
        input(&mut s, "jjad");
        assert_eq!(words(&s), expected);
        let rows = s.candidate_window(0, 256).candidates;
        let dynamic: Vec<_> = rows
            .iter()
            .filter(|c| {
                c.hints
                    .iter()
                    .any(|h| matches!(h, CandidateHint::DateTime(_)))
            })
            .collect();
        assert_eq!(dynamic.len(), r.config.shortcuts.rules.len());
        assert_eq!(
            rows.iter()
                .map(|c| c.id.0)
                .collect::<std::collections::HashSet<_>>()
                .len(),
            rows.len()
        );
    }
}

#[test]
fn c06_global_nine_ten_eleven_have_identical_numeric_and_click_commits() {
    let clock = TestClock::new();
    let text: Vec<_> = (0..12).map(|i| format!("普通{i}")).collect();
    let base: Vec<_> = text.iter().map(String::as_str).collect();
    for capacity in [1, 9, 10] {
        for position in [9, 10, 11] {
            for profile in [BehaviorProfile::LegacyV1, BehaviorProfile::TouchV1] {
                let r = runtime(
                    &base,
                    capacity,
                    vec![rule("jjad", position, "yyyy-MM-dd")],
                    None,
                );
                let preview =
                    shortcut_preview::from_runtime(&r, "jjad", time(12, 7, 8, 9), profile).unwrap();
                let row = preview
                    .rows
                    .iter()
                    .find(|row| row.target.is_some())
                    .unwrap();
                let effective = capacity.min(9) as usize;
                assert_eq!(
                    (row.target, row.actual, row.page, row.slot),
                    (
                        Some(position),
                        position as usize,
                        (position as usize - 1) / effective + 1,
                        (position as usize - 1) % effective + 1
                    )
                );
                for click in [false, true] {
                    let mut s =
                        Session::new_with_clock(&r, profile, HostContext::default(), clock.clone());
                    input(&mut s, "jjad");
                    let effective = capacity.min(9) as usize;
                    let page = (position as usize - 1) / effective;
                    let slot = (position as usize - 1) % effective + 1;
                    for _ in 0..page {
                        apply(&mut s, Action::MoveCandidatePage(true));
                    }
                    let window = s.candidate_window(position as usize - 1, 1);
                    let action = if click {
                        Action::SelectCandidate(window.query_revision, window.candidates[0].id)
                    } else if profile == BehaviorProfile::LegacyV1 {
                        Action::Legacy(InputEvent::Char(char::from(b'0' + slot as u8)))
                    } else {
                        Action::InputText(char::from(b'0' + slot as u8), TextIntent::Normal)
                    };
                    let plan = apply(&mut s, action);
                    assert!(
                        matches!(plan.op,EditSessionOp::CommitAndReplace {ref text,..} if text=="2026-11-12")
                    );
                    assert!(!plan.learnable);
                }
            }
        }
    }
}

#[test]
fn c08_c09_first_full_match_freezes_time_and_safe_retry_does_not_retake_it() {
    let clock = TestClock::new();
    *clock.value.lock().unwrap() = Ok(time(12, 23, 59, 59));
    let r = runtime(
        &["甲", "乙", "丙", "丁"],
        2,
        vec![rule("jjad", 2, "yyyy-MM-dd HH:mm:ss")],
        None,
    );
    let mut s = Session::new_with_clock(
        &r,
        BehaviorProfile::TouchV1,
        HostContext::default(),
        clock.clone(),
    );
    input(&mut s, "jja");
    for _ in 0..5 {
        assert!(s.would_handle(&Action::InputCode('d')));
    }
    assert_eq!(clock.count(), 0);
    *clock.value.lock().unwrap() = Ok(time(13, 0, 0, 1));
    input(&mut s, "d");
    assert_eq!(clock.count(), 1);
    let original = s.candidate_window(1, 1);
    assert_eq!(original.candidates[0].text, "2026-11-13 00:00:01");
    *clock.value.lock().unwrap() = Ok(time(13, 0, 1, 2));
    for _ in 0..10 {
        assert!(s.would_handle(&Action::Space));
        s.candidate_window(0, 256);
    }
    apply(&mut s, Action::MoveCandidatePage(true));
    apply(&mut s, Action::MoveCandidatePage(false));
    assert_eq!(clock.count(), 1);
    let current = s.candidate_window(1, 1);
    let select = Action::SelectCandidate(current.query_revision, current.candidates[0].id);
    let first = s.prepare(select.clone()).unwrap();
    assert_eq!(
        s.acknowledge(first.token, HostOutcome::RejectedBeforeWrite)
            .status,
        AckStatus::Retained
    );
    let retry = s.prepare(select).unwrap();
    assert_eq!(retry.op, first.op);
    assert_eq!(clock.count(), 1);
    s.acknowledge(retry.token, HostOutcome::RejectedBeforeWrite);
    apply(&mut s, Action::Backspace);
    input(&mut s, "d");
    assert_eq!(clock.count(), 2);
    assert_eq!(
        s.candidate_window(1, 1).candidates[0].text,
        "2026-11-13 00:01:02"
    );
    assert!(s
        .prepare(Action::SelectCandidate(
            original.query_revision,
            original.candidates[0].id
        ))
        .is_err());
    let mut next = r.clone();
    next.revision = 2;
    let cleanup = s.apply_runtime(Arc::new(next)).unwrap().unwrap();
    s.acknowledge(cleanup.token, HostOutcome::Applied);
    input(&mut s, "jjad");
    assert_eq!(clock.count(), 3);
}

#[test]
fn clock_failure_is_local_to_one_query_and_preserves_base_candidates() {
    let clock = TestClock::new();
    *clock.value.lock().unwrap() = Err("测试时钟失败".into());
    let r = runtime(
        &["甲", "乙", "丙", "丁"],
        3,
        vec![rule("jjad", 2, "yyyy")],
        Some("表"),
    );
    let mut s = Session::new_with_clock(
        &r,
        BehaviorProfile::TouchV1,
        HostContext::default(),
        clock.clone(),
    );
    input(&mut s, "jjad");
    assert_eq!(s.snapshot().shortcut_error.as_deref(), Some("测试时钟失败"));
    assert_eq!(
        pages(&mut s),
        [vec!["甲", "乙", "丙"], vec!["表"], vec!["丁"]]
    );
    assert_eq!(clock.count(), 1);
    *clock.value.lock().unwrap() = Ok(time(12, 7, 8, 9));
    apply(&mut s, Action::Cancel);
    input(&mut s, "jjad");
    assert_eq!(clock.count(), 2);
    assert!(s.snapshot().shortcut_error.is_none());
}

#[test]
fn retry_of_first_full_match_preedit_reuses_even_an_unpublished_time_snapshot() {
    let clock = TestClock::new();
    let r = runtime(&[], 3, vec![rule("jjad", 1, "HH:mm:ss")], None);
    let mut s = Session::new_with_clock(
        &r,
        BehaviorProfile::TouchV1,
        HostContext::default(),
        clock.clone(),
    );
    input(&mut s, "jja");
    let first = s.prepare(Action::InputCode('d')).unwrap();
    assert_eq!(clock.count(), 1);
    s.acknowledge(first.token, HostOutcome::RejectedBeforeWrite);
    *clock.value.lock().unwrap() = Ok(time(13, 0, 0, 1));
    let retry = apply(&mut s, Action::InputCode('d'));
    assert_eq!(clock.count(), 1);
    assert_eq!(retry.op, first.op);
    assert_eq!(words(&s), ["07:08:09"]);
    apply(&mut s, Action::Backspace);
    input(&mut s, "d");
    assert_eq!(clock.count(), 2);
    assert_eq!(words(&s), ["00:00:01"]);
}

#[test]
fn timezone_and_year_changes_only_affect_a_new_query() {
    let clock = TestClock::new();
    let r = runtime(&[], 3, vec![rule("jjad", 1, "yyyy-MM-dd HH:mm:ss")], None);
    let mut s = Session::new_with_clock(
        &r,
        BehaviorProfile::TouchV1,
        HostContext::default(),
        clock.clone(),
    );
    input(&mut s, "jjad");
    let next = LocalTime {
        year: 2027,
        month: 1,
        day: 1,
        hour: 0,
        minute: 0,
        second: 1,
        utc_offset_seconds: -18000,
        timezone: "America/New_York".into(),
    };
    *clock.value.lock().unwrap() = Ok(next.clone());
    apply(&mut s, Action::MoveCandidatePage(true));
    assert_eq!(words(&s), ["2026-11-12 07:08:09"]);
    assert_eq!(
        s.state_machine().shortcut_snapshot().unwrap().timezone,
        "Asia/Shanghai"
    );
    assert_eq!(clock.count(), 1);
    apply(&mut s, Action::Cancel);
    input(&mut s, "jjad");
    assert_eq!(words(&s), ["2027-01-01 00:00:01"]);
    assert_eq!(s.state_machine().shortcut_snapshot(), Some(&next));
    assert_eq!(clock.count(), 2);
}

#[test]
fn wrong_prefix_follows_original_overflow_and_auxiliary_entry_keeps_priority() {
    let clock = TestClock::new();
    for profile in [BehaviorProfile::LegacyV1, BehaviorProfile::TouchV1] {
        for overflow in [false, true] {
            for auto in [false, true] {
                let mut r = runtime(&[], 3, vec![rule("jjaad", 1, "日期")], None);
                r.dict = Dictionary::from_text("jjaa\t普通\n").unwrap();
                r.config.basic.auto_commit_unique = auto;
                r.config.basic.commit_on_max_code_overflow = overflow;
                let mut s =
                    Session::new_with_clock(&r, profile, HostContext::default(), clock.clone());
                input(&mut s, "jjaa");
                assert_eq!(s.state_machine().spelling(), "jjaa");
                let p = apply(&mut s, Action::InputCode('b'));
                if overflow {
                    assert!(
                        matches!(p.op,EditSessionOp::CommitThenUpdate {ref text,ref spelling,..} if text=="普通" && spelling=="b")
                    );
                    assert!(p.learnable);
                } else {
                    assert_eq!(s.state_machine().spelling(), "jjaab");
                }
            }
        }
    }
    assert_eq!(clock.count(), 0);
    let mut r = runtime(&[], 3, vec![rule("jjad", 1, "日期")], None);
    r.dict =
        Dictionary::from_text("---\ncharset: a\ntemporary:\n  key: j\n---\na\t普通\n").unwrap();
    let mut baseline = r.clone();
    baseline.config.shortcuts.rules.clear();
    let mut s = Session::new_with_clock(
        &r,
        BehaviorProfile::LegacyV1,
        HostContext::default(),
        clock.clone(),
    );
    let mut old = Session::new_with_clock(
        &baseline,
        BehaviorProfile::LegacyV1,
        HostContext::default(),
        clock.clone(),
    );
    for event in [
        InputEvent::Char('j'),
        InputEvent::Esc,
        InputEvent::Char('a'),
        InputEvent::Space,
    ] {
        let a = apply(&mut s, Action::Legacy(event.clone()));
        let b = apply(&mut old, Action::Legacy(event));
        assert_eq!(a.op, b.op);
        assert_eq!(s.snapshot(), old.snapshot());
    }
    assert_eq!(clock.count(), 0);
}

#[test]
fn c11_long_and_overlapping_codes_only_suppress_automatic_commits_while_reachable() {
    for auto in [false, true] {
        for overflow in [false, true] {
            for code in ["jjaad", "jjadx", "abcdefghijklmnopqrstuvwxyzabcdef"] {
                let mut r = runtime(&[], 3, vec![rule(code, 1, "日期")], None);
                r.dict = Dictionary::from_text("jjaa\t普通\njjad\t普通\nabcd\t普通\n").unwrap();
                if code == "jjadx" {
                    r.config.shortcuts.rules.push(rule("jjad", 1, "短码日期"));
                }
                r.config.basic.auto_commit_unique = auto;
                r.config.basic.commit_on_max_code_overflow = overflow;
                let clock = TestClock::new();
                let mut s = Session::new_with_clock(
                    &r,
                    BehaviorProfile::TouchV1,
                    HostContext::default(),
                    clock,
                );
                input(&mut s, code);
                assert_eq!(s.state_machine().spelling(), code);
                assert_eq!(words(&s)[0], "日期");
                let plan = apply(&mut s, Action::InputCode('z'));
                if overflow {
                    assert!(
                        matches!(plan.op, EditSessionOp::CommitThenUpdate { ref text, ref spelling, .. } if text == "日期" && spelling == "z")
                    );
                    assert!(!plan.learnable);
                } else {
                    assert_eq!(s.state_machine().spelling(), format!("{code}z"));
                    assert!(words(&s).is_empty());
                }
            }
        }
    }
}

#[test]
fn c07_disabled_and_conflicting_rules_keep_legacy_input_trace_and_restore_after_reload() {
    for conflict in [false, true] {
        let clock = TestClock::new();
        let mut r = runtime(&["普通"], 3, vec![rule("jjadx", 1, "日期")], None);
        r.config.basic.auto_commit_unique = true;
        if conflict {
            r.dict = Dictionary::from_text("---\nwildcard_key: j\n---\naaad\t普通\n").unwrap();
        } else {
            r.config.shortcuts.rules[0].enabled = false;
        }
        let mut baseline = r.clone();
        baseline.config.shortcuts.rules.clear();
        let mut old = Session::new_with_clock(
            &baseline,
            BehaviorProfile::LegacyV1,
            HostContext::default(),
            clock.clone(),
        );
        let mut s = Session::new_with_clock(
            &r,
            BehaviorProfile::LegacyV1,
            HostContext::default(),
            clock.clone(),
        );
        for c in "jjadx".chars() {
            let action = Action::Legacy(InputEvent::Char(c));
            let a = apply(&mut old, action.clone());
            let b = apply(&mut s, action);
            assert_eq!(a.op, b.op);
            assert_eq!(a.disposition, b.disposition);
            assert_eq!(old.snapshot(), s.snapshot());
        }
        assert_eq!(clock.count(), 0);
        r.revision += 1;
        r.dict = Dictionary::from_text("").unwrap();
        r.config.shortcuts.rules[0].enabled = true;
        let cleanup = s.apply_runtime(Arc::new(r)).unwrap().unwrap();
        s.acknowledge(cleanup.token, HostOutcome::Applied);
        input(&mut s, "jjadx");
        assert_eq!(words(&s), ["日期"]);
        assert_eq!(clock.count(), 1);
    }
}

#[test]
fn same_text_preserves_learning_identity_and_raw_uppercase_commits() {
    let clock = TestClock::new();
    let mut r = runtime(
        &["2026-11-12", "甲"],
        3,
        vec![rule("jjad", 2, "yyyy-MM-dd")],
        None,
    );
    r.config.basic.show_code_hints = false;
    for index in [0, 1] {
        let mut s = Session::new_with_clock(
            &r,
            BehaviorProfile::TouchV1,
            HostContext::default(),
            clock.clone(),
        );
        input(&mut s, "jjad");
        assert_eq!(s.snapshot().items[1].hint, "日期时间");
        let w = s.candidate_window(index, 1);
        let plan = apply(
            &mut s,
            Action::SelectCandidate(w.query_revision, w.candidates[0].id),
        );
        assert_eq!(plan.learnable, index == 0);
        assert!(
            matches!(plan.op,EditSessionOp::CommitAndReplace {ref text,..} if text == "2026-11-12")
        );
    }
    let mut s =
        Session::new_with_clock(&r, BehaviorProfile::TouchV1, HostContext::default(), clock);
    input(&mut s, "JJAD");
    assert!(words(&s).contains(&"2026-11-12".into()));
    let plan = apply(&mut s, Action::Enter);
    assert!(matches!(plan.op,EditSessionOp::CommitAndReplace {ref text,..} if text == "JJAD"));
    assert!(plan.learnable);
}

#[test]
fn context_modes_and_uncertain_writes_invalidate_the_old_query_without_replay() {
    let clock = TestClock::new();
    let r = runtime(&[], 3, vec![rule("jjad", 1, "日期")], None);
    for context in [
        HostContext {
            purpose: InputPurpose::Password,
            ..HostContext::default()
        },
        HostContext {
            purpose: InputPurpose::Direct,
            ..HostContext::default()
        },
        HostContext {
            allows_assistance: false,
            ..HostContext::default()
        },
    ] {
        let mut s = Session::new_with_clock(&r, BehaviorProfile::TouchV1, context, clock.clone());
        for c in "jjad".chars() {
            let _ = s
                .prepare(Action::InputCode(c))
                .map(|p| s.acknowledge(p.token, HostOutcome::Applied));
        }
        assert!(words(&s).is_empty());
    }
    assert_eq!(clock.count(), 0);
    let mut s = Session::new_with_clock(
        &r,
        BehaviorProfile::TouchV1,
        HostContext::default(),
        clock.clone(),
    );
    apply(&mut s, Action::SetBaseMode(BaseMode::English));
    for c in "jjad".chars() {
        apply(&mut s, Action::InputText(c, TextIntent::Normal));
    }
    assert_eq!(clock.count(), 0);
    apply(&mut s, Action::SetBaseMode(BaseMode::Main));
    for outcome in [
        HostOutcome::OutcomeUnknown,
        HostOutcome::PartiallyApplied { completed_steps: 1 },
    ] {
        input(&mut s, "jjad");
        let w = s.candidate_window(0, 1);
        let p = s
            .prepare(Action::SelectCandidate(
                w.query_revision,
                w.candidates[0].id,
            ))
            .unwrap();
        let ack = s.acknowledge(p.token, outcome);
        assert!(ack.follow_up.is_none());
        assert!(s
            .prepare(Action::SelectCandidate(
                w.query_revision,
                w.candidates[0].id
            ))
            .is_err());
        assert!(words(&s).is_empty());
    }
    input(&mut s, "jjad");
    let w = s.candidate_window(0, 1);
    s.host_terminated();
    assert!(s
        .prepare(Action::SelectCandidate(
            w.query_revision,
            w.candidates[0].id
        ))
        .is_err());
    assert!(s.state_machine().shortcut_snapshot().is_none());
}
