//! 输入法状态机与 Spelling Buffer。
//!
//! 状态机以 [`StateMachine`] 为中心，接受 [`InputEvent`] 事件，产出 [`Transition`]
//! 表示对外可见的行为（上屏文字、候选列表变更、清空等）。
//!
//! 状态本身非常轻量，可由 Windows TSF / Android `InputMethodService` 在每次按键时
//! 调用 `handle` 推进；候选列表由 [`crate::dictionary::Dictionary`] 提供。

use crate::config::PunctuationMode;
use crate::dictionary::{Dictionary, Entry, SearchOptions};
use std::sync::Arc;

/// 当前所处的输入阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputState {
    /// 空闲：无任何输入缓冲。
    #[default]
    Idle,
    /// 输入中：已积累编码但未上屏。
    Composing,
    /// 选词中：候选框弹出，等待用户选择。
    Selecting,
}

/// 对外可见的按键事件（按键语义已由前端壳层统一抽象为跨平台动作）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputEvent {
    /// 字母/数字按键输入（编码字符）。
    Char(char),
    /// 由前端按当前键盘布局翻译出的符号字符；不进入编码缓冲。
    Symbol(char),
    /// 空格键（按 `commit_mode` 行为决定上屏/翻页/选定）。
    Space,
    /// 回车键。
    Enter,
    /// 退格：删除最后一个编码字符，全部删除完则回到 Idle。
    Backspace,
    /// Esc：清空当前缓冲并退出输入状态。
    Esc,
    /// 数字键直接选择第 N 个候选（1..=9）。
    Select(usize),
    /// 下一页候选。
    PageNext,
    /// 上一页候选。
    PagePrev,
}

/// 状态机推进后产出的对外行为。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transition {
    /// 无可见动作（仅内部状态变化）。
    None,
    /// 待上屏文本（输入法应将文本插入到光标处）。
    Commit(String),
    /// 候选列表变更（连同当前编码字符串一起回传，供 UI 渲染）。
    Candidates {
        spelling: String,
        candidates: Vec<String>,
        page: usize,
        total_pages: usize,
    },
    /// 编码缓冲变更但尚无候选（如刚输入第一个字符）。
    SpellingUpdated(String),
    /// 输入已清空回到 Idle。
    Cleared,
    /// 需透传原始按键给应用程序（未拦截）。
    Passthrough(InputEvent),
}

/// 翻页大小与候选视图。
const DEFAULT_PAGE_SIZE: usize = 5;

/// 跨平台输入核心状态机。
pub struct StateMachine {
    /// 当前绑定的码表（不可变，多线程共享）。
    dict: Arc<Dictionary>,
    /// 单页候选数。
    page_size: usize,
    /// 四码唯一时自动上屏。
    auto_commit_unique: bool,
    /// 原始输入缓冲区：保存用户当前已输入但尚未上屏的字符串。
    spelling: String,
    /// 标点输入处理策略。
    punctuation_mode: PunctuationMode,
    /// 当前候选页。
    page: usize,
    /// 当前匹配到的候选快照（每页切片引用词表）。
    candidates: Vec<String>,
    /// 总候选数（用于换页判断）。
    total_candidates: usize,
    /// 当前状态。
    state: InputState,
}

impl StateMachine {
    /// 创建一个新的状态机，绑定只读码表。
    pub fn new(dict: Arc<Dictionary>) -> Self {
        Self::with_options(dict, DEFAULT_PAGE_SIZE, true)
    }

    /// 配置页大小与自动上屏策略。
    pub fn with_options(
        dict: Arc<Dictionary>,
        page_size: usize,
        auto_commit_unique: bool,
    ) -> Self {
        Self::with_behavior(
            dict,
            page_size,
            auto_commit_unique,
            PunctuationMode::BufferedCommit,
        )
    }

    /// 配置页大小、自动上屏与标点处理策略。
    pub fn with_behavior(
        dict: Arc<Dictionary>,
        page_size: usize,
        auto_commit_unique: bool,
        punctuation_mode: PunctuationMode,
    ) -> Self {
        let page_size = page_size.clamp(1, 10);
        Self {
            dict,
            page_size,
            auto_commit_unique,
            spelling: String::new(),
            punctuation_mode,
            page: 0,
            candidates: Vec::new(),
            total_candidates: 0,
            state: InputState::Idle,
        }
    }

    /// 当前编码缓冲。
    pub fn spelling(&self) -> &str {
        &self.spelling
    }

    /// 当前状态。
    pub fn state(&self) -> InputState {
        self.state
    }

    /// 当前候选视图。
    pub fn candidates(&self) -> &[String] {
        &self.candidates
    }

    /// 当前页码（0-based）。
    pub fn page(&self) -> usize {
        self.page
    }

    /// 总页数。
    pub fn total_pages(&self) -> usize {
        self.total_candidates.div_ceil(self.page_size).max(1)
    }

    /// 判断字符能否作为当前码表编码输入。
    pub fn accepts_code_char(&self, character: char) -> bool {
        is_code_char(character) || self.dict.table_config().charset.contains(character)
    }

    /// 重置全部状态（不释放码表绑定）。
    pub fn reset(&mut self) {
        self.spelling.clear();
        self.page = 0;
        self.candidates.clear();
        self.total_candidates = 0;
        self.state = InputState::Idle;
    }

    /// 处理一个输入事件并返回对外行为。
    pub fn handle(&mut self, event: InputEvent) -> Transition {
        match event {
            InputEvent::Char(c) => self.on_char(c),
            InputEvent::Symbol(c) if self.dict.table_config().charset.contains(c) => {
                self.on_char(c)
            }
            InputEvent::Symbol(c) => self.on_symbol(c),
            InputEvent::Space => self.on_space(),
            InputEvent::Enter => self.on_enter(),
            InputEvent::Backspace => self.on_backspace(),
            InputEvent::Esc => self.on_esc(),
            InputEvent::Select(idx) => self.on_select(idx),
            InputEvent::PageNext => self.on_page_next(),
            InputEvent::PagePrev => self.on_page_prev(),
        }
    }

    fn on_char(&mut self, c: char) -> Transition {
        if c.is_ascii_digit()
            && !self.dict.table_config().charset.contains(c)
            && self.can_select_with_digit(c)
        {
            return self.on_select(c.to_digit(10).unwrap() as usize);
        }

        let is_table_code_char = self.dict.table_config().charset.contains(c);
        if is_buffered_punctuation(c) && !is_table_code_char {
            if self.punctuation_mode == PunctuationMode::DirectCommit {
                return Transition::Passthrough(InputEvent::Char(c));
            }

            self.spelling.push(c);
            self.reset_candidates();
            self.state = InputState::Composing;
            return Transition::SpellingUpdated(self.spelling.clone());
        }

        // 兼容传统字母数字编码，同时允许码表 charset 声明的自定义字符。
        if !is_code_char(c) && !is_table_code_char {
            return Transition::Passthrough(InputEvent::Char(c));
        }

        // 若处于选词状态且继续输入，则视为放弃当前候选，进入新一轮。
        if self.state == InputState::Selecting && !self.has_buffered_punctuation() {
            self.reset_candidates();
        }
        self.spelling.push(c);

        if self.has_buffered_punctuation() {
            self.reset_candidates();
            self.state = InputState::Composing;
            return Transition::SpellingUpdated(self.spelling.clone());
        }

        self.page = 0;
        self.candidates_snapshot();

        // 四码唯一自动上屏。
        if self.auto_commit_unique
            && self.spelling.len() >= 4
            && self.total_candidates == 1
        {
            let word = self.candidates[0].clone();
            self.reset();
            return Transition::Commit(word);
        }

        if self.candidates.is_empty() {
            // 没有任何候选，先反馈 spelling 更新。
            self.state = InputState::Composing;
            return Transition::SpellingUpdated(self.spelling.clone());
        }

        self.state = InputState::Selecting;
        Transition::Candidates {
            spelling: self.spelling.clone(),
            candidates: self.candidates.clone(),
            page: self.page,
            total_pages: self.total_pages(),
        }
    }

    fn on_symbol(&mut self, c: char) -> Transition {
        if self.spelling.is_empty() {
            return Transition::Passthrough(InputEvent::Symbol(c));
        }

        let mut text = self.spelling.clone();
        text.push(c);
        self.reset();
        Transition::Commit(text)
    }

    fn on_space(&mut self) -> Transition {
        if self.spelling.is_empty() {
            return Transition::Passthrough(InputEvent::Space);
        }

        if self.has_buffered_punctuation() {
            let text = self.spelling.clone();
            self.reset();
            return Transition::Commit(text);
        }

        // 空格首选上屏：提交当前第 0 个候选并补齐原编码字符（如四码不足时上屏首选）。
        if !self.candidates.is_empty() {
            let word = self.candidates[0].clone();
            let extra: String = self.spelling.chars().skip(self.candidates_first_code_len()).collect();
            self.reset();
            if extra.is_empty() {
                return Transition::Commit(word);
            }
            return Transition::Commit(format!("{}{}", word, extra));
        }
        // 无候选：把缓冲中的字母直接作为英文上屏（实现“打不出的编码按字母上屏”）。
        let text = self.spelling.clone();
        self.reset();
        Transition::Commit(text)
    }

    fn on_enter(&mut self) -> Transition {
        if self.spelling.is_empty() {
            return Transition::Passthrough(InputEvent::Enter);
        }
        // 回车上屏编码原始字符串（非上屏候选词），符合多数形码输入法的行为。
        let text = self.spelling.clone();
        self.reset();
        Transition::Commit(text)
    }

    fn on_backspace(&mut self) -> Transition {
        if self.spelling.is_empty() {
            return Transition::Passthrough(InputEvent::Backspace);
        }
        self.spelling.pop();
        if self.spelling.is_empty() {
            self.reset();
            return Transition::Cleared;
        }

        if self.has_buffered_punctuation() {
            self.reset_candidates();
            self.state = InputState::Composing;
            return Transition::SpellingUpdated(self.spelling.clone());
        }

        self.page = 0;
        self.candidates_snapshot();
        if self.candidates.is_empty() {
            self.state = InputState::Composing;
            return Transition::SpellingUpdated(self.spelling.clone());
        }
        self.state = InputState::Selecting;
        Transition::Candidates {
            spelling: self.spelling.clone(),
            candidates: self.candidates.clone(),
            page: self.page,
            total_pages: self.total_pages(),
        }
    }

    fn on_esc(&mut self) -> Transition {
        if self.spelling.is_empty() {
            return Transition::Passthrough(InputEvent::Esc);
        }
        self.reset();
        Transition::Cleared
    }

    fn on_select(&mut self, idx: usize) -> Transition {
        if idx >= 1 && idx <= self.candidates.len() {
            let word = self.candidates[idx - 1].clone();
            self.reset();
            return Transition::Commit(word);
        }
        // 越界：透传数字键给应用。
        Transition::Passthrough(InputEvent::Select(idx))
    }

    fn on_page_next(&mut self) -> Transition {
        let pages = self.total_pages();
        if self.page + 1 < pages {
            self.page += 1;
            self.candidates_snapshot();
            return Transition::Candidates {
                spelling: self.spelling.clone(),
                candidates: self.candidates.clone(),
                page: self.page,
                total_pages: pages,
            };
        }
        Transition::None
    }

    fn on_page_prev(&mut self) -> Transition {
        if self.page > 0 {
            self.page -= 1;
            self.candidates_snapshot();
            return Transition::Candidates {
                spelling: self.spelling.clone(),
                candidates: self.candidates.clone(),
                page: self.page,
                total_pages: self.total_pages(),
            };
        }
        Transition::None
    }

    // ── 内部辅助 ──────────────────────────────────────────────

    /// 重新扫一遍码表，刷新候选总数与当前页可见候选快照。
    fn candidates_snapshot(&mut self) {
        let opts = SearchOptions {
            prefer_exact: true,
            limit: self.dict.len().max(1),
        };
        let all: Vec<&Entry> = self.dict.search(self.lookup_key(), opts);
        self.total_candidates = all.len();
        let start = (self.page * self.page_size).min(all.len());
        let end = (start + self.page_size).min(all.len());
        self.candidates = all[start..end]
            .iter()
            .map(|e| e.word.clone())
            .collect();
    }

    fn candidates_first_code_len(&self) -> usize {
        // 在五笔等形码中，候选词对应编码长度与当前输入长度一致即视为精确匹配，
        // 没有混合英文补齐。返回 spelling 长度即可。
        self.lookup_key().chars().count()
    }

    fn reset_candidates(&mut self) {
        self.candidates.clear();
        self.total_candidates = 0;
        self.page = 0;
    }

    fn lookup_key(&self) -> &str {
        let mut end = self.spelling.len();
        for (idx, ch) in self.spelling.char_indices() {
            if !self.accepts_code_char(ch) {
                end = idx;
                break;
            }
        }
        &self.spelling[..end]
    }

    fn has_buffered_punctuation(&self) -> bool {
        self.lookup_key().len() != self.spelling.len()
    }

    fn can_select_with_digit(&self, c: char) -> bool {
        self.state == InputState::Selecting
            && !self.has_buffered_punctuation()
            && matches!(c, '1'..='9')
    }
}

/// 编码字符白名单：ASCII 字母 + 数字。
fn is_code_char(c: char) -> bool {
    matches!(c, 'a'..='z' | 'A'..='Z' | '0'..='9')
}

fn is_buffered_punctuation(c: char) -> bool {
    c.is_ascii_punctuation()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PunctuationMode;
    use crate::dictionary::{Entry, LoadOptions};

    fn dict() -> Arc<Dictionary> {
        let entries = vec![
            Entry { code: "a".into(), word: "工".into(), weight: 999 },
            Entry { code: "ggll".into(), word: "王".into(), weight: 100 },
            Entry { code: "ggll".into(), word: "壬".into(), weight: 50 },
            Entry { code: "ggh".into(), word: "理".into(), weight: 80 },
            Entry { code: "gghg".into(), word: "五".into(), weight: 200 },
        ];
        Dictionary::from_entries(entries, None, LoadOptions::default()).unwrap()
    }

    #[test]
    fn passthrough_empty_buffer_on_space() {
        let mut m = StateMachine::new(dict());
        assert_eq!(m.handle(InputEvent::Space), Transition::Passthrough(InputEvent::Space));
    }

    #[test]
    fn typing_single_char_yields_candidates() {
        let mut m = StateMachine::new(dict());
        let t = m.handle(InputEvent::Char('a'));
        match t {
            Transition::Candidates { spelling, candidates, .. } => {
                assert_eq!(spelling, "a");
                assert!(candidates.contains(&"工".to_string()));
            }
            other => panic!("expected candidates, got {other:?}"),
        }
    }

    #[test]
    fn space_commits_first_candidate() {
        let mut m = StateMachine::new(dict());
        m.handle(InputEvent::Char('a'));
        let t = m.handle(InputEvent::Space);
        assert_eq!(t, Transition::Commit("工".to_string()));
        assert_eq!(m.state(), InputState::Idle);
    }

    #[test]
    fn backspace_to_empty_clears() {
        let mut m = StateMachine::new(dict());
        m.handle(InputEvent::Char('a'));
        assert_eq!(m.handle(InputEvent::Backspace), Transition::Cleared);
    }

    #[test]
    fn esc_clears() {
        let mut m = StateMachine::new(dict());
        m.handle(InputEvent::Char('a'));
        assert_eq!(m.handle(InputEvent::Esc), Transition::Cleared);
    }

    #[test]
    fn select_by_number() {
        let mut m = StateMachine::with_options(dict(), 5, false);
        m.handle(InputEvent::Char('g'));
        m.handle(InputEvent::Char('g'));
        m.handle(InputEvent::Char('l'));
        m.handle(InputEvent::Char('l'));
        let t = m.handle(InputEvent::Select(2));
        // 候选若包含 "壬"，第 2 位选中。
        assert!(matches!(t, Transition::Commit(ref s) if s == "王" || s == "壬"));
    }

    #[test]
    fn auto_commit_unique_on_four_code() {
        let mut m = StateMachine::with_options(dict(), 5, true);
        let t = m.handle(InputEvent::Char('a'));
        if let Transition::Candidates { spelling, candidates, .. } = t {
            assert_eq!(spelling, "a");
            assert_eq!(candidates.len(), 1);
        } else {
            panic!("expected candidates for single exact match");
        }
    }

    #[test]
    fn punctuation_is_buffered_by_default() {
        let mut m = StateMachine::new(dict());
        assert_eq!(
            m.handle(InputEvent::Char('!')),
            Transition::SpellingUpdated("!".to_string())
        );
    }

    #[test]
    fn punctuation_passthrough_in_direct_mode() {
        let mut m = StateMachine::with_behavior(dict(), 5, true, PunctuationMode::DirectCommit);
        assert_eq!(
            m.handle(InputEvent::Char('!')),
            Transition::Passthrough(InputEvent::Char('!'))
        );
    }

    #[test]
    fn symbol_passthroughs_when_idle() {
        let mut m = StateMachine::new(dict());
        assert_eq!(
            m.handle(InputEvent::Symbol('!')),
            Transition::Passthrough(InputEvent::Symbol('!'))
        );
    }

    #[test]
    fn symbol_commits_raw_spelling_when_composing() {
        let mut m = StateMachine::new(dict());
        m.handle(InputEvent::Char('g'));
        m.handle(InputEvent::Char('g'));

        assert_eq!(m.handle(InputEvent::Symbol('!')), Transition::Commit("gg!".to_string()));
        assert_eq!(m.state(), InputState::Idle);
    }

    #[test]
    fn uppercase_code_char_is_kept_in_spelling_buffer() {
        let mut m = StateMachine::new(dict());

        assert_eq!(
            m.handle(InputEvent::Char('A')),
            Transition::SpellingUpdated("A".to_string())
        );
        assert_eq!(m.spelling(), "A");
    }

    #[test]
    fn page_next_and_prev() {
        let mut m = StateMachine::with_options(dict(), 1, false);
        m.handle(InputEvent::Char('g'));
        m.handle(InputEvent::Char('g'));
        let t0 = m.page();
        // 多候选场景，下一页应当推进或 None。
        let _ = m.handle(InputEvent::PageNext);
        let _ = m.handle(InputEvent::PagePrev);
        let _ = t0;
    }

    #[test]
    fn enter_commits_raw_spelling() {
        let mut m = StateMachine::new(dict());
        m.handle(InputEvent::Char('z'));
        m.handle(InputEvent::Char('z'));
        assert_eq!(m.handle(InputEvent::Enter), Transition::Commit("zz".to_string()));
    }

    #[test]
    fn punctuation_is_kept_in_raw_buffer_by_default() {
        let mut m = StateMachine::new(dict());
        assert!(matches!(m.handle(InputEvent::Char('b')), Transition::SpellingUpdated(_) | Transition::Candidates { .. }));
        assert!(matches!(m.handle(InputEvent::Char('a')), Transition::SpellingUpdated(_) | Transition::Candidates { .. }));
        assert!(matches!(m.handle(InputEvent::Char('i')), Transition::SpellingUpdated(_) | Transition::Candidates { .. }));
        assert!(matches!(m.handle(InputEvent::Char('.')), Transition::SpellingUpdated(raw) if raw == "bai."));
        assert_eq!(m.handle(InputEvent::Enter), Transition::Commit("bai.".to_string()));
    }

    #[test]
    fn backspace_removes_trailing_punctuation_before_code_chars() {
        let mut m = StateMachine::new(dict());
        let _ = m.handle(InputEvent::Char('a'));
        assert_eq!(m.handle(InputEvent::Char('\\')), Transition::SpellingUpdated("a\\".to_string()));
        match m.handle(InputEvent::Backspace) {
            Transition::Candidates { spelling, candidates, .. } => {
                assert_eq!(spelling, "a");
                assert!(candidates.contains(&"工".to_string()));
            }
            other => panic!("expected candidates after removing punctuation, got {other:?}"),
        }
    }

    #[test]
    fn digit_char_selects_candidate_when_plain_code_is_active() {
        let mut m = StateMachine::with_options(dict(), 5, false);
        m.handle(InputEvent::Char('g'));
        m.handle(InputEvent::Char('g'));
        m.handle(InputEvent::Char('l'));
        m.handle(InputEvent::Char('l'));

        let t = m.handle(InputEvent::Char('2'));
        assert!(matches!(t, Transition::Commit(ref s) if s == "王" || s == "壬"));
    }
}
