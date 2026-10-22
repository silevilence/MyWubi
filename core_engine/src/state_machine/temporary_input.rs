//! 临时模式的独立事件路由，避免继承形码顶字或功能键行为。
use super::*;
use crate::temporary::InputMode;

impl StateMachine {
    pub(super) fn handle_temporary(&mut self, event: InputEvent) -> Transition {
        if event == InputEvent::Esc || (event == InputEvent::Backspace && self.spelling.is_empty())
        {
            self.reset();
            return Transition::Cleared;
        }
        if self.mode == InputMode::Menu {
            let index = match event {
                InputEvent::Select(index) => index,
                InputEvent::Char('1') => 1,
                InputEvent::Char('2') => 2,
                _ => return Transition::None,
            };
            match index {
                1 if self.temporary.pinyin.is_some() => self.mode = InputMode::Pinyin,
                2 if self.temporary.english_error.is_none() => self.mode = InputMode::English,
                _ => return Transition::None,
            }
            self.reset_candidates();
            return self.refresh_temporary();
        }
        match event {
            InputEvent::Backspace => {
                self.spelling.pop();
                self.page = 0;
            }
            InputEvent::Enter | InputEvent::Space => {
                if self.spelling.is_empty() {
                    return Transition::None;
                }
                let text = if event == InputEvent::Space {
                    self.candidates.first().unwrap_or(&self.spelling).clone()
                } else {
                    self.spelling.clone()
                };
                self.reset();
                return Transition::Commit(text);
            }
            InputEvent::Select(index) => return self.select_temporary(index),
            InputEvent::Char(c) if c.is_ascii_digit() => {
                return self.select_temporary(c.to_digit(10).unwrap_or(0) as usize);
            }
            InputEvent::Char(c) | InputEvent::Symbol(c) => {
                if self.mode == InputMode::Pinyin {
                    if c == '\'' {
                        if self.spelling.is_empty() || self.spelling.ends_with('\'') {
                            return Transition::None;
                        }
                    } else if !c.is_ascii_lowercase() {
                        return Transition::None;
                    }
                    let max = self
                        .temporary
                        .pinyin
                        .as_ref()
                        .map_or(64, |d| d.table_config().max_code_len as usize);
                    if c != '\'' && self.spelling.bytes().filter(|b| *b != b'\'').count() >= max {
                        return Transition::None;
                    }
                } else if c.is_control() {
                    return Transition::None;
                }
                self.spelling.push(c);
                self.page = 0;
            }
            InputEvent::PageNext if self.page + 1 < self.total_pages() => self.page += 1,
            InputEvent::PagePrev if self.page > 0 => self.page -= 1,
            _ => return Transition::None,
        }
        self.refresh_temporary()
    }

    fn select_temporary(&mut self, index: usize) -> Transition {
        if let Some(word) = index
            .checked_sub(1)
            .and_then(|index| self.candidates.get(index))
            .cloned()
        {
            self.reset();
            Transition::Commit(word)
        } else {
            Transition::None
        }
    }

    pub(super) fn refresh_temporary(&mut self) -> Transition {
        if self.mode == InputMode::Menu {
            self.candidates = vec!["拼音".into(), "英文".into()];
            self.code_hints = vec![
                self.temporary.pinyin_error.clone().unwrap_or_else(|| {
                    if self.temporary.pinyin.is_none() {
                        "未配置拼音码表".into()
                    } else {
                        String::new()
                    }
                }),
                self.temporary.english_error.clone().unwrap_or_default(),
            ];
            self.total_candidates = 2;
            self.state = InputState::Selecting;
        } else {
            let dict = match self.mode {
                InputMode::Pinyin => self.temporary.pinyin.as_ref(),
                InputMode::English => self.temporary.english.as_ref(),
                _ => None,
            };
            let all = dict
                .map(|dict| {
                    dict.search_literal(
                        &self.spelling,
                        SearchOptions {
                            prefer_exact: true,
                            limit: dict.len(),
                        },
                    )
                })
                .unwrap_or_default();
            self.total_candidates = all.len();
            let start = (self.page * self.page_size).min(all.len());
            let end = (start + self.page_size).min(all.len());
            self.candidates = all[start..end]
                .iter()
                .map(|entry| entry.word.clone())
                .collect();
            self.code_hints = vec![String::new(); self.candidates.len()];
            self.state = if self.candidates.is_empty() {
                InputState::Composing
            } else {
                InputState::Selecting
            };
        }
        Transition::Candidates {
            spelling: self.spelling.clone(),
            candidates: self.candidates.clone(),
            code_hints: self.code_hints.clone(),
            page: self.page,
            total_pages: if self.mode == InputMode::Menu {
                1
            } else {
                self.total_pages()
            },
        }
    }
}
