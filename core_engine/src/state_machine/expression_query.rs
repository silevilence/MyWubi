//! A local word-building round followed by an explicit expression selection.
//! Only selecting a result may produce a host commit.
use super::*;
use crate::expression_search::{ExpressionMatch, MatchQuality};
use crate::{candidates::*, expressions::ExpressionKind, Capability, TemporaryMode};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone)]
pub(super) struct ExpressionQuery {
    kind: ExpressionKind,
    input: ExpressionInput,
    literal: String,
    words: Vec<String>,
    builder: Box<StateMachine>,
    word_code: String,
    word_candidates: Arc<Vec<Entry>>,
    results: Option<Arc<Vec<ExpressionMatch>>>,
    message: Option<String>,
    return_mode: InputMode,
    pub(super) return_temporary: Option<TemporaryMode>,
}

impl ExpressionQuery {
    fn text(&self) -> String {
        if self.input == ExpressionInput::Literal {
            self.literal.clone()
        } else {
            self.words.concat()
        }
    }

    fn nonempty(&self) -> bool {
        !self.literal.is_empty() || !self.words.is_empty() || !self.builder.spelling.is_empty()
    }

    fn accept_word(&mut self, text: String) -> bool {
        if self.words.iter().map(|s| s.chars().count()).sum::<usize>() + text.chars().count() > 512
        {
            self.message = Some("查询最多 512 个字符，请先删减关键词。".into());
            return false;
        }
        self.words.push(text);
        self.message = None;
        true
    }

    fn select_word(&mut self, index: usize) {
        let Some(word) = index
            .checked_sub(1)
            .and_then(|i| self.builder.candidates.get(i))
            .cloned()
        else {
            self.message = Some("当前编码没有可确认的文字，请修改编码。".into());
            return;
        };
        if self.accept_word(word) {
            self.builder.reset();
        }
    }

    fn input_code(&mut self, c: char) {
        if self.input == ExpressionInput::Literal {
            if c.is_control() {
                self.message = Some("查询不接受控制字符。".into());
            } else if self.literal.chars().count() >= 512 {
                self.message = Some("查询最多 512 个字符，请先删减关键词。".into());
            } else {
                self.literal.push(c);
                self.message = None;
            }
            return;
        }
        if !self.builder.accepts_code_char(c) {
            self.message = Some("请使用当前主码表输入查询文字。".into());
            return;
        }
        if self.builder.spelling.chars().count() >= 512 {
            self.message = Some("编码过长，请先修改或确认。".into());
            return;
        }
        let before = self.builder.clone();
        // Explicit code routing cannot open the ordinary auxiliary menu or append expressions.
        let transition = self.builder.on_code_char(c);
        let word = match transition {
            Transition::Commit(text)
            | Transition::CommitThenCandidates { text, .. }
            | Transition::CommitThenSpelling { text, .. } => Some(text),
            _ => None,
        };
        if let Some(word) = word {
            if !self.accept_word(word) {
                self.builder = before;
            }
        } else {
            self.message = None;
        }
    }

    fn backspace(&mut self) {
        if self.input == ExpressionInput::Literal {
            if let Some((start, _)) = self.literal.grapheme_indices(true).next_back() {
                self.literal.truncate(start);
            }
        } else if !self.builder.spelling.is_empty() {
            self.builder.on_backspace();
        } else {
            // Segment the whole query: a combining mark may cross a selected-word boundary.
            let text = self.text();
            if let Some((start, _)) = text.grapheme_indices(true).next_back() {
                let mut remaining = start;
                self.words.retain_mut(|word| {
                    let keep = remaining.min(word.len());
                    remaining -= keep;
                    word.truncate(keep);
                    !word.is_empty()
                });
            }
        }
        self.message = None;
    }
}

impl StateMachine {
    pub fn expression_view(&self) -> Option<ExpressionQueryView> {
        let query = self.expression_query.as_ref()?;
        Some(ExpressionQueryView {
            kind: query.kind,
            input: query.input,
            phase: if query.results.is_some() {
                ExpressionPhase::Results
            } else {
                ExpressionPhase::Editing
            },
            text: query.text(),
            words: query.words.clone(),
            message: query.message.clone(),
        })
    }

    pub(super) fn expression_capability(&self, kind: ExpressionKind) -> Capability {
        let selection = self.expression_config.selection(kind);
        let resource = self.expressions.resource(kind);
        let reason = if !self.expressions_allowed {
            Some("当前输入范围不允许表情辅助输入".into())
        } else if !selection.search {
            Some("请先在软件设置中开启该词库的专用模式".into())
        } else if resource.library.is_none() {
            Some(
                resource
                    .error
                    .clone()
                    .unwrap_or_else(|| "词库未加载".into()),
            )
        } else {
            None
        };
        Capability {
            available: reason.is_none(),
            candidate_support: reason.is_none(),
            reason,
        }
    }

    pub(super) fn begin_expression(
        &mut self,
        kind: ExpressionKind,
        previous: Option<TemporaryMode>,
    ) -> Result<Transition, String> {
        if let Some(reason) = self.expression_capability(kind).reason {
            return Err(reason);
        }
        if !self.spelling.is_empty() || self.expression_query.as_ref().is_some_and(|q| q.nonempty())
        {
            return Err("输入缓冲非空，请先完成或取消当前查询".into());
        }
        let (return_mode, return_temporary) = self
            .expression_query
            .as_ref()
            .map(|q| (q.return_mode, q.return_temporary))
            .unwrap_or((
                if self.mode == InputMode::Menu {
                    self.menu_return
                } else {
                    self.mode
                },
                previous,
            ));
        let builder = StateMachine::with_full_behavior(
            Arc::clone(&self.dict),
            self.page_size.min(9),
            self.auto_commit_unique,
            PunctuationMode::BufferedCommit,
            self.max_code_len,
            self.commit_on_max_code_overflow,
        );
        self.reset();
        self.mode = match kind {
            ExpressionKind::Emoji => InputMode::Emoji,
            ExpressionKind::Kaomoji => InputMode::Kaomoji,
            ExpressionKind::Kana => InputMode::Kana,
            ExpressionKind::Math => InputMode::Math,
            ExpressionKind::Punctuation => InputMode::Punctuation,
            ExpressionKind::Symbols => InputMode::Symbols,
        };
        self.expression_query = Some(Box::new(ExpressionQuery {
            kind,
            input: ExpressionInput::Words,
            literal: String::new(),
            words: Vec::new(),
            builder: Box::new(builder),
            word_code: String::new(),
            word_candidates: Arc::default(),
            results: None,
            message: None,
            return_mode,
            return_temporary,
        }));
        Ok(self.refresh_expression())
    }

    pub(super) fn exit_expression(&mut self) -> Transition {
        let mode = self
            .expression_query
            .as_ref()
            .map_or(InputMode::Main, |q| q.return_mode);
        self.reset();
        self.mode = mode;
        if mode != InputMode::Main {
            self.refresh_temporary()
        } else {
            Transition::Cleared
        }
    }

    pub(super) fn find_expressions(&mut self) -> Transition {
        let Some(mut query) = self.expression_query.take() else {
            return Transition::None;
        };
        if query.results.is_none() {
            if !query.builder.spelling.is_empty() {
                query.select_word(1);
            }
            if query.builder.spelling.is_empty() {
                if query.text().trim().is_empty() {
                    query.message = Some("请先输入并确认查询文字。".into());
                } else if let Some(library) = self.expression_library(query.kind) {
                    let text = query.text();
                    let result = library.search(&text, &query.words);
                    query.message = if result.matches.is_empty() {
                        Some("没有匹配结果，可返回修改关键词。".into())
                    } else if result
                        .matches
                        .iter()
                        .all(|row| row.quality == MatchQuality::Partial)
                    {
                        Some("没有满足全部关键词的结果；以下仅为部分匹配。".into())
                    } else {
                        None
                    };
                    query.results = Some(Arc::new(result.matches));
                    self.page = 0;
                }
            }
        }
        self.expression_query = Some(query);
        self.refresh_expression()
    }

    pub(super) fn edit_expression(&mut self) -> Transition {
        let Some(query) = &mut self.expression_query else {
            return Transition::None;
        };
        query.results = None;
        query.message = None;
        self.page = 0;
        self.refresh_expression()
    }

    pub(super) fn set_expression_input(
        &mut self,
        input: ExpressionInput,
    ) -> Result<Transition, String> {
        let Some(query) = &mut self.expression_query else {
            return Ok(Transition::None);
        };
        if query.input == input {
            return Ok(self.refresh_expression());
        }
        if query.nonempty() || query.results.is_some() {
            return Err("请先清空查询，再切换输入方式。".into());
        }
        query.input = input;
        query.message = None;
        Ok(self.refresh_expression())
    }

    pub(super) fn clear_expression_query(&mut self) -> Transition {
        if let Some(query) = &mut self.expression_query {
            query.words.clear();
            query.literal.clear();
            query.builder.reset();
            query.results = None;
            query.message = None;
            self.page = 0;
        }
        self.refresh_expression()
    }

    pub(super) fn expression_candidate(&self, index: usize) -> Option<Candidate> {
        let query = self.expression_query.as_ref()?;
        if let Some(results) = &query.results {
            let result = results.get(index)?;
            let entry = self
                .expression_library(query.kind)?
                .document()
                .entries
                .get(result.index)?;
            Some(Candidate {
                expression_kind: Some(query.kind),
                id: CandidateId(index),
                text: entry.text.clone(),
                exact: result.quality == MatchQuality::Exact,
                hints: vec![
                    CandidateHint::ExpressionName(entry.name.clone()),
                    CandidateHint::ExpressionMatch(
                        match result.quality {
                            MatchQuality::Exact => "名称/别名匹配",
                            MatchQuality::AllTerms => "关键词匹配",
                            MatchQuality::Loose => "宽松匹配",
                            MatchQuality::Partial => "部分匹配",
                        }
                        .into(),
                    ),
                ],
            })
        } else {
            let entry = query.word_candidates.get(index)?;
            Some(Candidate {
                expression_kind: None,
                id: CandidateId(index),
                text: entry.word.clone(),
                exact: entry.code == query.builder.spelling,
                hints: entry
                    .code
                    .strip_prefix(query.builder.lookup_key())
                    .filter(|s| !s.is_empty())
                    .map(|s| vec![CandidateHint::CodeSuffix(s.into())])
                    .unwrap_or_default(),
            })
        }
    }

    pub(super) fn select_expression_candidate(&mut self, index: usize) -> Transition {
        let Some(candidate) = self.expression_candidate(index) else {
            return Transition::None;
        };
        let Some(query) = self.expression_query.as_mut() else {
            return Transition::None;
        };
        if query.results.is_some() {
            self.exit_expression();
            Transition::Commit(candidate.text)
        } else {
            if query.accept_word(candidate.text) {
                query.builder.reset();
            }
            self.refresh_expression()
        }
    }

    pub(super) fn handle_expression_code(&mut self, c: char) -> Transition {
        if let Some(query) = self
            .expression_query
            .as_mut()
            .filter(|q| q.results.is_none())
        {
            query.input_code(c);
        }
        self.refresh_expression()
    }

    pub(super) fn handle_expression(&mut self, event: InputEvent) -> Transition {
        if event == InputEvent::FindExpressions {
            return self.find_expressions();
        }
        let Some(query) = self.expression_query.as_mut() else {
            return Transition::None;
        };
        if query.results.is_some() {
            match event {
                InputEvent::Esc | InputEvent::Backspace => return self.edit_expression(),
                InputEvent::Space => {
                    return self.select_expression_candidate(self.page * self.page_size.min(9))
                }
                InputEvent::Select(index) if (1..=9).contains(&index) => {
                    if index <= self.candidates.len() {
                        return self.select_expression_candidate(
                            self.page * self.page_size.min(9) + index - 1,
                        );
                    }
                }
                InputEvent::Char(c) if ('1'..='9').contains(&c) => {
                    return self.handle_expression(InputEvent::Select(c as usize - '0' as usize))
                }
                InputEvent::PageNext if self.page + 1 < self.total_pages() => self.page += 1,
                InputEvent::PagePrev if self.page > 0 => self.page -= 1,
                _ => {}
            }
        } else {
            if query.input == ExpressionInput::Literal {
                match event {
                    InputEvent::Char(c) | InputEvent::Symbol(c) => query.input_code(c),
                    InputEvent::Space => query.input_code(' '),
                    InputEvent::Backspace => query.backspace(),
                    InputEvent::Esc => return self.exit_expression(),
                    InputEvent::Enter => {
                        query.message = Some("请使用查找操作；查询文字不会上屏。".into());
                    }
                    _ => {}
                }
                return self.refresh_expression();
            }
            match event {
                InputEvent::Esc if query.builder.spelling.is_empty() => {
                    return self.exit_expression()
                }
                InputEvent::Esc => {
                    query.builder.reset();
                    query.message = None;
                }
                InputEvent::Backspace => query.backspace(),
                InputEvent::Space => query.select_word(1),
                InputEvent::Select(index) => query.select_word(index),
                InputEvent::Char(c)
                    if ('1'..='9').contains(&c)
                        && !query.builder.dict.table_config().charset.contains(c) =>
                {
                    query.select_word(c as usize - '0' as usize)
                }
                InputEvent::Char(c) | InputEvent::Symbol(c) => query.input_code(c),
                InputEvent::PageNext => {
                    query.builder.on_page_next();
                }
                InputEvent::PagePrev => {
                    query.builder.on_page_prev();
                }
                InputEvent::Enter => {
                    query.message = Some("请使用查找操作查看表情，查询文字不会上屏。".into())
                }
                InputEvent::FindExpressions => return Transition::None,
            }
        }
        self.refresh_expression()
    }

    pub(super) fn refresh_expression(&mut self) -> Transition {
        if let Some(query) = self
            .expression_query
            .as_mut()
            .filter(|q| q.results.is_none())
        {
            if query.word_code != query.builder.spelling {
                query.word_candidates = Arc::new(if query.builder.spelling.is_empty() {
                    Vec::new()
                } else {
                    query
                        .builder
                        .dict
                        .search(
                            query.builder.lookup_key(),
                            SearchOptions {
                                prefer_exact: true,
                                limit: query.builder.dict.len(),
                            },
                        )
                        .into_iter()
                        .cloned()
                        .collect()
                });
                query.word_code.clone_from(&query.builder.spelling);
            }
        }
        let Some(query) = self.expression_query.as_ref() else {
            return Transition::None;
        };
        if let Some(results) = &query.results {
            self.total_candidates = results.len();
            self.spelling.clear();
            self.page = self.page.min(self.total_pages().saturating_sub(1));
            let start = self.page * self.page_size.min(9);
            let candidates: Vec<_> = (start..(start + self.page_size.min(9)).min(results.len()))
                .filter_map(|i| self.expression_candidate(i))
                .collect();
            self.candidates = candidates.iter().map(|c| c.text.clone()).collect();
            self.code_hints = candidates
                .into_iter()
                .map(|c| {
                    c.hints
                        .into_iter()
                        .filter_map(|hint| match hint {
                            CandidateHint::ExpressionName(text)
                            | CandidateHint::ExpressionMatch(text) => Some(text),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join(" · ")
                })
                .collect();
        } else {
            self.spelling.clone_from(&query.builder.spelling);
            self.candidates.clone_from(&query.builder.candidates);
            self.code_hints.clone_from(&query.builder.code_hints);
            self.total_candidates = query.builder.total_candidates;
            self.page = query.builder.page;
        }
        self.state = if self.candidates.is_empty() {
            InputState::Composing
        } else {
            InputState::Selecting
        };
        Transition::Candidates {
            spelling: self.spelling.clone(),
            candidates: self.candidates.clone(),
            code_hints: self.code_hints.clone(),
            page: self.page,
            total_pages: self.total_pages(),
        }
    }
}
