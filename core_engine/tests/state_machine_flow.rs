//! 状态机集成测试：模拟经典形码输入流，验证上屏词条是否符合预期。
//!
//! 这些测试以端到端视角驱动 [`core_engine::StateMachine`]，覆盖典型写作者
//! 操作序列（连续打字、空格首选、数字选词、翻页、退格修订、Esc 清空、
//! 回车上屏原始编码、自动上屏等）。

use core_engine::{
    dictionary::{Entry, LoadOptions},
    state_machine::{InputEvent, StateMachine, Transition},
    Dictionary,
};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// 构建一份五笔风格的小型码表。
fn dict() -> Arc<Dictionary> {
    let entries = vec![
        Entry { code: "a".into(),     word: "工".into(),  weight: 999 },
        Entry { code: "ggll".into(),  word: "王".into(),  weight: 100 },
        Entry { code: "ggll".into(),  word: "壬".into(),  weight: 50 },
        Entry { code: "ggll".into(),  word: "丰".into(),  weight: 20 },
        Entry { code: "ggh".into(),   word: "理".into(),  weight: 80 },
        Entry { code: "gghg".into(),  word: "五".into(),  weight: 200 },
        Entry { code: "ggtt".into(),  word: "五笔".into(), weight: 60 },
        Entry { code: "ggli".into(),  word: "班".into(),  weight: 30 },
        Entry { code: "gt".into(),    word: "五".into(),  weight: 10 },
    ];
    Dictionary::from_entries(entries, None, LoadOptions::default()).unwrap()
}

/// 工具：把字符序列依次送入状态机，返回提交词条列表。
fn drive_chars(sm: &mut StateMachine, chars: &str) -> Vec<String> {
    let mut commits = Vec::new();
    for c in chars.chars() {
        let t = sm.handle(InputEvent::Char(c));
        if let Transition::Commit(s) = t {
            commits.push(s);
        }
    }
    commits
}

#[test]
fn wildcard_occupies_exactly_one_code_position() {
    let path = std::env::temp_dir().join(format!(
        "mywubi-wildcard-{}.dict",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::write(
        &path,
        "---\nwildcard_key: z\ncharset: abcdefghijklmnopqrstuvwxyz\n---\ngdqq\t目标\t100\ngdqaa\t不应出现\t1000\n",
    )
    .unwrap();
    let dictionary = Dictionary::load(&path).unwrap();
    let _ = std::fs::remove_file(path);
    let mut machine = StateMachine::with_options(dictionary, 5, false);
    drive_chars(&mut machine, "gdq");

    let transition = machine.handle(InputEvent::Char('z'));

    assert!(
        matches!(transition, Transition::Candidates { candidates, .. } if candidates == ["目标"])
    );
}

#[test]
fn candidate_words_are_unique_across_matching_codes() {
    let dictionary = Dictionary::from_entries(
        vec![
            Entry { code: "eh".into(), word: "用".into(), weight: 100 },
            Entry { code: "eht".into(), word: "用".into(), weight: 50 },
            Entry { code: "ehv".into(), word: "月".into(), weight: 80 },
        ],
        None,
        LoadOptions::default(),
    )
    .unwrap();
    let mut machine = StateMachine::with_options(dictionary, 5, false);
    machine.handle(InputEvent::Char('e'));

    let transition = machine.handle(InputEvent::Char('h'));

    assert!(
        matches!(transition, Transition::Candidates { candidates, .. } if candidates == ["用", "月"])
    );
}

#[test]
fn real_wubi06_eh_candidates_contain_one_yong() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tables/wubi06.dict");
    let dictionary = Dictionary::load(path).unwrap();
    let mut machine = StateMachine::with_options(dictionary, 100, false);
    machine.handle(InputEvent::Char('e'));

    let transition = machine.handle(InputEvent::Char('h'));

    assert!(
        matches!(transition, Transition::Candidates { candidates, .. } if candidates.iter().filter(|word| word.as_str() == "用").count() == 1)
    );
}

// ── 1. 经典四码 + 空格首选 ────────────────────────────────────

#[test]
fn classic_four_code_space_first_commit() {
    let mut m = StateMachine::new(dict());
    drive_chars(&mut m, "ggll");
    // 键入完毕后应处于选词状态。
    assert_eq!(m.state(), core_engine::state_machine::InputState::Selecting);
    // 空格首选第一个候选（"王" weight 100 最高）。
    let t = m.handle(InputEvent::Space);
    assert_eq!(t, Transition::Commit("王".to_string()));
    assert_eq!(m.state(), core_engine::state_machine::InputState::Idle);
    assert!(m.spelling().is_empty());
}

// ── 2. 四码唯一时自动上屏 ───────────────────────────────────

#[test]
fn auto_commit_unique_immediate() {
    let mut m = StateMachine::new(dict());
    // "gghg" 在码表中只有一个候选 "五"，应在键入第四个字符时自动上屏。
    let commits = drive_chars(&mut m, "gghg");
    assert_eq!(commits, vec!["五".to_string()]);
    assert_eq!(m.state(), core_engine::state_machine::InputState::Idle);
}

// ── 3. 数字键选词 ───────────────────────────────────────────

#[test]
fn select_second_candidate_by_number() {
    let mut m = StateMachine::with_options(dict(), 5, false);
    drive_chars(&mut m, "ggll");
    // "壬" 为第二高频候选（"王 100" > "壬 50" > "丰 20"）。
    let t = m.handle(InputEvent::Select(2));
    assert_eq!(t, Transition::Commit("壬".to_string()));
    assert_eq!(m.state(), core_engine::state_machine::InputState::Idle);
}

#[test]
fn select_out_of_range_passthrough() {
    let mut m = StateMachine::with_options(dict(), 5, false);
    drive_chars(&mut m, "ggll");
    // 候选只有 3 个，选第 5 个应透传而非上屏。
    let t = m.handle(InputEvent::Select(5));
    assert_eq!(t, Transition::Passthrough(InputEvent::Select(5)));
    // 状态机状态保持。
    assert_eq!(m.state(), core_engine::state_machine::InputState::Selecting);
}

// ── 4. 退格修订 ─────────────────────────────────────────────

#[test]
fn backspace_revises_spelling() {
    let mut m = StateMachine::new(dict());
    drive_chars(&mut m, "ggll");
    assert_eq!(m.spelling(), "ggll");
    // 退格一次后回到 "ggl"，候选项重算。
    let t = m.handle(InputEvent::Backspace);
    assert_eq!(m.spelling(), "ggl");
    // 退格应刷新候选视图。
    assert!(matches!(t, Transition::Candidates { .. }));
    // "ggl" 前缀候选中最高频者为 "王"(ggll, weight 100)，空格首选其。
    let t2 = m.handle(InputEvent::Space);
    assert!(matches!(t2, Transition::Commit(ref s) if s == "王"));
}

#[test]
fn backspace_to_empty_clears_state() {
    let mut m = StateMachine::new(dict());
    drive_chars(&mut m, "a");
    let t = m.handle(InputEvent::Backspace);
    assert_eq!(t, Transition::Cleared);
    assert_eq!(m.state(), core_engine::state_machine::InputState::Idle);
}

#[test]
fn backspace_passthrough_when_idle() {
    let mut m = StateMachine::new(dict());
    let t = m.handle(InputEvent::Backspace);
    assert_eq!(t, Transition::Passthrough(InputEvent::Backspace));
}

// ── 5. Esc 清空 ───────────────────────────────────────────

#[test]
fn esc_resets_to_idle() {
    let mut m = StateMachine::new(dict());
    drive_chars(&mut m, "ggll");
    let t = m.handle(InputEvent::Esc);
    assert_eq!(t, Transition::Cleared);
    assert_eq!(m.state(), core_engine::state_machine::InputState::Idle);
    assert!(m.spelling().is_empty());
    assert!(m.candidates().is_empty());
}

#[test]
fn esc_passthrough_when_idle() {
    let mut m = StateMachine::new(dict());
    let t = m.handle(InputEvent::Esc);
    assert_eq!(t, Transition::Passthrough(InputEvent::Esc));
}

// ── 6. 回车上屏原始编码 ─────────────────────────────────────

#[test]
fn enter_commits_raw_spelling() {
    let mut m = StateMachine::new(dict());
    drive_chars(&mut m, "zz");
    let t = m.handle(InputEvent::Enter);
    assert_eq!(t, Transition::Commit("zz".to_string()));
    assert_eq!(m.state(), core_engine::state_machine::InputState::Idle);
}

#[test]
fn enter_passthrough_when_idle() {
    let mut m = StateMachine::new(dict());
    let t = m.handle(InputEvent::Enter);
    assert_eq!(t, Transition::Passthrough(InputEvent::Enter));
}

// ── 7. 翻页 ───────────────────────────────────────────────

#[test]
fn page_next_advances_and_prev_returns() {
    let mut m = StateMachine::with_options(dict(), 1, false);
    drive_chars(&mut m, "ggll");
    let page0 = m.page();
    assert_eq!(page0, 0);
    let t_next = m.handle(InputEvent::PageNext);
    assert!(matches!(t_next, Transition::Candidates { page, .. } if page == 1));
    assert_eq!(m.page(), 1);
    let t_prev = m.handle(InputEvent::PagePrev);
    assert!(matches!(t_prev, Transition::Candidates { page, .. } if page == 0));
    assert_eq!(m.page(), 0);
}

#[test]
fn page_next_at_last_page_is_none() {
    let mut m = StateMachine::with_options(dict(), 10, false);
    drive_chars(&mut m, "ggll");
    // 一页容量 10 大于候选总数，应停在最后一页，再次翻页无动作。
    let t = m.handle(InputEvent::PageNext);
    assert_eq!(t, Transition::None);
}

#[test]
fn page_prev_at_first_page_is_none() {
    let mut m = StateMachine::new(dict());
    drive_chars(&mut m, "ggll");
    let t = m.handle(InputEvent::PagePrev);
    assert_eq!(t, Transition::None);
}

// ── 8. 连续输入：上屏后继续输入 ─────────────────────────────

#[test]
fn keep_typing_after_commit_starts_new_session() {
    let mut m = StateMachine::new(dict());
    // 输入 "a" + 空格上屏 "工"。
    drive_chars(&mut m, "a");
    let t1 = m.handle(InputEvent::Space);
    assert_eq!(t1, Transition::Commit("工".to_string()));
    // 紧接着输入新一批编码。
    drive_chars(&mut m, "gghg");
    assert_eq!(m.state(), core_engine::state_machine::InputState::Idle);
}

// ── 9. 空格上屏无候选时回退英文 ─────────────────────────────

#[test]
fn space_with_no_candidate_commits_raw_letters() {
    let mut m = StateMachine::new(dict());
    drive_chars(&mut m, "zz");
    let t = m.handle(InputEvent::Space);
    assert_eq!(t, Transition::Commit("zz".to_string()));
    assert_eq!(m.state(), core_engine::state_machine::InputState::Idle);
}

#[test]
fn space_passthrough_when_idle() {
    let mut m = StateMachine::new(dict());
    let t = m.handle(InputEvent::Space);
    assert_eq!(t, Transition::Passthrough(InputEvent::Space));
}

// ── 10. 选词中再键入字母重置候选 ─────────────────────────────

#[test]
fn typing_during_selecting_resets_candidates() {
    let mut m = StateMachine::with_options(dict(), 5, false);
    drive_chars(&mut m, "gg");
    assert_eq!(m.state(), core_engine::state_machine::InputState::Selecting);
    // 再输入一个字符应刷新候选，而不是保持旧的选词状态。
    let t = m.handle(InputEvent::Char('l'));
    assert!(matches!(t, Transition::Candidates { spelling, .. } if spelling == "ggl"));
}

// ── 11. 非编码字符透传 ───────────────────────────────────

#[test]
fn symbol_passthrough_during_idle() {
    let mut m = StateMachine::new(dict());
    let t = m.handle(InputEvent::Symbol('!'));
    assert_eq!(t, Transition::Passthrough(InputEvent::Symbol('!')));
    assert_eq!(m.spelling(), "");
}

#[test]
fn uppercase_letter_is_kept_in_spelling_buffer() {
    let mut m = StateMachine::new(dict());
    let t = m.handle(InputEvent::Char('A'));
    assert_eq!(t, Transition::SpellingUpdated("A".to_string()));
    assert_eq!(m.spelling(), "A");
}

#[test]
fn symbol_after_spelling_commits_raw_spelling_then_symbol() {
    let mut m = StateMachine::new(dict());
    drive_chars(&mut m, "gg");

    let t = m.handle(InputEvent::Symbol('!'));

    assert_eq!(t, Transition::Commit("gg!".to_string()));
    assert_eq!(m.state(), core_engine::state_machine::InputState::Idle);
    assert_eq!(m.spelling(), "");
}

// ── 12. 完整写作流：多句上屏 ─────────────────────────────────

#[test]
fn full_writing_session_multiple_commits() {
    let mut m = StateMachine::new(dict());
    let mut commits = Vec::new();

    // 第一句："五"
    for c in "gghg".chars() {
        if let Transition::Commit(s) = m.handle(InputEvent::Char(c)) {
            commits.push(s);
        }
    }
    // 第二句："王"
    for c in "ggll".chars() {
        if let Transition::Commit(s) = m.handle(InputEvent::Char(c)) {
            commits.push(s);
        }
    }
    // 主动空格上屏（"ggll" 非唯一键，需要空格首选）。
    if let Transition::Commit(s) = m.handle(InputEvent::Space) {
        commits.push(s);
    }
    // 第三句："工"
    m.handle(InputEvent::Char('a'));
    if let Transition::Commit(s) = m.handle(InputEvent::Space) {
        commits.push(s);
    }

    assert_eq!(commits, vec!["五", "王", "工"]);
}

// ── 13. reset() 行为 ───────────────────────────────────────

#[test]
fn reset_clears_all_runtime_state() {
    let mut m = StateMachine::new(dict());
    drive_chars(&mut m, "ggll");
    assert_eq!(m.state(), core_engine::state_machine::InputState::Selecting);
    m.reset();
    assert_eq!(m.state(), core_engine::state_machine::InputState::Idle);
    assert!(m.spelling().is_empty());
    assert!(m.candidates().is_empty());
    assert_eq!(m.page(), 0);
}
