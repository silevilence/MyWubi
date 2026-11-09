//! A local word-building round followed by an explicit expression selection.
//! Only selecting a result may produce a host commit.
use super::*;
use crate::{candidates::*, expressions::ExpressionKind, Capability, TemporaryMode};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone)]
pub(super) struct ExpressionQuery {
    kind: ExpressionKind,
    words: Vec<String>,
    builder: Box<StateMachine>,
    word_code: String,
    word_candidates: Arc<Vec<Entry>>,
    results: Option<Arc<Vec<usize>>>,
    message: Option<String>,
    return_mode: InputMode,
    pub(super) return_temporary: Option<TemporaryMode>,
}

impl ExpressionQuery {
    fn text(&self) -> String {
        self.words.concat()
    }

    fn nonempty(&self) -> bool {
        !self.words.is_empty() || !self.builder.spelling.is_empty()
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
        if !self.builder.spelling.is_empty() {
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
        let (selection, resource) = match kind {
            ExpressionKind::Emoji => (&self.expression_config.emoji, &self.expressions.emoji),
            ExpressionKind::Kaomoji => (&self.expression_config.kaomoji, &self.expressions.kaomoji),
        };
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
                    InputMode::Main
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
        };
        self.expression_query = Some(Box::new(ExpressionQuery {
            kind,
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
                if query.words.is_empty() {
                    query.message = Some("请先输入并确认查询文字。".into());
                } else if let Some(library) = self.expression_library(query.kind) {
                    // EM-10 extends this exact result stage with local fuzzy ranking.
                    let text = query.text();
                    let results = library
                        .document()
                        .entries
                        .iter()
                        .enumerate()
                        .filter(|(_, e)| {
                            e.name == text
                                || e.triggers.contains(&text)
                                || e.aliases.contains(&text)
                        })
                        .map(|(i, _)| i)
                        .collect::<Vec<_>>();
                    query.message = results
                        .is_empty()
                        .then(|| "没有匹配结果，可返回修改关键词。".into());
                    query.results = Some(Arc::new(results));
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

    pub(super) fn expression_candidate(&self, index: usize) -> Option<Candidate> {
        let query = self.expression_query.as_ref()?;
        if let Some(results) = &query.results {
            let entry = self
                .expression_library(query.kind)?
                .document()
                .entries
                .get(*results.get(index)?)?;
            Some(Candidate {
                id: CandidateId(index),
                text: entry.text.clone(),
                exact: true,
                hints: vec![CandidateHint::ExpressionName(entry.name.clone())],
            })
        } else {
            let entry = query.word_candidates.get(index)?;
            Some(Candidate {
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
                .map(|c| match c.hints.into_iter().next() {
                    Some(CandidateHint::ExpressionName(name)) => name,
                    _ => String::new(),
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
