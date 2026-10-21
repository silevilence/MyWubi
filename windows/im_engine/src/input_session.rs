//! Windows 输入会话协调。测试与 TSF 回调使用同一行为入口；编辑 adapter 在锁外运行。
use crate::candidate_data::{CandidateData, CandidateItem, ScreenPoint, ThemeSnapshot};
use crate::RuntimeSnapshot;
use arc_swap::ArcSwap;
use core_engine::{Config, Dictionary, InputEvent, StateMachine, Transition};
use parking_lot::Mutex;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DisplayAttrKind {
    Input,
    Converted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EditSessionOp {
    CompositionUpdate {
        spelling: String,
        attr: DisplayAttrKind,
    },
    CommitAndReplace {
        text: String,
        replace_len: usize,
    },
    CommitThenUpdate {
        text: String,
        replace_len: usize,
        spelling: String,
        attr: DisplayAttrKind,
    },
    EndComposition {
        delete_text: bool,
    },
    NoOp,
}

struct State {
    sm: StateMachine,
    config: Config,
    revision: u64,
    generation: u64,
    chinese: bool,
    needs_cleanup: bool,
}

struct Update {
    generation: u64,
    op: EditSessionOp,
    view: CandidateData,
    consumed: bool,
}

pub(crate) struct InputSession {
    state: Mutex<State>,
    candidates: Arc<ArcSwap<CandidateData>>,
}

impl InputSession {
    pub fn new(
        dict: Arc<Dictionary>,
        config: Config,
        revision: u64,
        candidates: Arc<ArcSwap<CandidateData>>,
    ) -> Self {
        let sm = state_machine(dict, &config);
        Self {
            state: Mutex::new(State {
                sm,
                config,
                revision,
                generation: 0,
                chinese: true,
                needs_cleanup: false,
            }),
            candidates,
        }
    }

    pub fn config(&self) -> Config {
        self.state.lock().config.clone()
    }
    pub fn theme(&self) -> ThemeSnapshot {
        ThemeSnapshot::from_config(&self.state.lock().config)
    }
    pub fn chinese(&self) -> bool {
        self.state.lock().chinese
    }
    pub fn selecting(&self) -> bool {
        self.state.lock().sm.state() == core_engine::state_machine::InputState::Selecting
    }

    pub fn test_key(&self, event: &InputEvent, system_modifier: bool) -> bool {
        accepts(&self.state.lock(), event, system_modifier)
    }

    pub fn handle<E: std::fmt::Display>(
        &self,
        event: InputEvent,
        system_modifier: bool,
        anchor: Option<ScreenPoint>,
        edit: impl FnMut(&EditSessionOp) -> Result<Option<ScreenPoint>, E>,
    ) -> bool {
        let update = {
            let mut state = self.state.lock();
            if !accepts(&state, &event, system_modifier) {
                return false;
            }
            let transition = state.sm.handle(event);
            state.generation += 1;
            plan(&state, transition, &self.candidates.load(), anchor)
        };
        self.execute(update, edit)
    }

    /// 先前编辑可能部分成功，不重复提交文本；先保留宿主文本并结束旧 composition。
    pub fn recover<E: std::fmt::Display>(
        &self,
        edit: impl FnMut(&EditSessionOp) -> Result<Option<ScreenPoint>, E>,
    ) -> bool {
        let update = {
            let mut state = self.state.lock();
            if !state.needs_cleanup {
                return true;
            }
            state.generation += 1;
            cleared_update(&state, EditSessionOp::EndComposition { delete_text: false })
        };
        self.execute(update, edit);
        !self.state.lock().needs_cleanup
    }

    pub fn set_chinese<E: std::fmt::Display>(
        &self,
        chinese: bool,
        edit: impl FnMut(&EditSessionOp) -> Result<Option<ScreenPoint>, E>,
    ) {
        let update = {
            let mut state = self.state.lock();
            if state.chinese == chinese {
                return;
            }
            state.chinese = chinese;
            if chinese {
                return;
            }
            let op = state
                .sm
                .commit_raw_current()
                .map(|(text, replace_len)| EditSessionOp::CommitAndReplace { text, replace_len })
                .unwrap_or(EditSessionOp::EndComposition { delete_text: false });
            state.generation += 1;
            cleared_update(&state, op)
        };
        self.execute(update, edit);
    }

    pub fn lose_focus<E: std::fmt::Display>(
        &self,
        edit: impl FnMut(&EditSessionOp) -> Result<Option<ScreenPoint>, E>,
    ) {
        let update = {
            let mut state = self.state.lock();
            state.sm.reset();
            state.generation += 1;
            cleared_update(&state, EditSessionOp::EndComposition { delete_text: false })
        };
        self.execute(update, edit);
    }

    /// 宿主已终止 composition；作废尚在返回途中的旧结果，避免候选框再次出现。
    pub fn terminated(&self) {
        let mut state = self.state.lock();
        state.sm.reset();
        state.needs_cleanup = false;
        state.generation += 1;
        self.candidates
            .store(Arc::new(CandidateData::hidden(ThemeSnapshot::from_config(
                &state.config,
            ))));
    }

    pub fn reload<E: std::fmt::Display>(
        &self,
        snapshot: &RuntimeSnapshot,
        edit: impl FnMut(&EditSessionOp) -> Result<Option<ScreenPoint>, E>,
    ) {
        let update = {
            let mut state = self.state.lock();
            if state.revision == snapshot.revision {
                return;
            }
            state.sm = state_machine(Arc::clone(&snapshot.dict), &snapshot.config);
            state.config = snapshot.config.clone();
            state.revision = snapshot.revision;
            state.generation += 1;
            cleared_update(&state, EditSessionOp::EndComposition { delete_text: false })
        };
        self.execute(update, edit);
    }

    fn execute<E: std::fmt::Display>(
        &self,
        mut update: Update,
        mut edit: impl FnMut(&EditSessionOp) -> Result<Option<ScreenPoint>, E>,
    ) -> bool {
        let result = if matches!(update.op, EditSessionOp::NoOp) {
            Ok(None)
        } else {
            edit(&update.op)
        };
        let mut state = self.state.lock();
        if state.generation != update.generation {
            return update.consumed;
        }
        match result {
            Ok(anchor) => {
                state.needs_cleanup = false;
                if update.view.visible {
                    update.view.anchor = anchor.or(update.view.anchor);
                }
                self.candidates.store(Arc::new(update.view));
            }
            Err(error) => {
                log::warn!("[TSF] 输入会话编辑失败，等待清理旧 composition: {error}");
                state.sm.reset();
                state.needs_cleanup = true;
                self.candidates
                    .store(Arc::new(CandidateData::hidden(ThemeSnapshot::from_config(
                        &state.config,
                    ))));
            }
        }
        update.consumed
    }
}

fn state_machine(dict: Arc<Dictionary>, config: &Config) -> StateMachine {
    let max_code_len = dict.table_config().max_code_len as usize;
    StateMachine::with_full_behavior(
        dict,
        config.basic.candidate_count as usize,
        config.basic.auto_commit_unique,
        config.basic.punctuation_mode,
        max_code_len,
        config.basic.commit_on_max_code_overflow,
    )
}

fn accepts(state: &State, event: &InputEvent, system_modifier: bool) -> bool {
    if !state.chinese || system_modifier || state.needs_cleanup {
        return false;
    }
    if !state.sm.would_consume(event) {
        return false;
    }
    match event {
        InputEvent::Char(c) | InputEvent::Symbol(c) if state.sm.accepts_code_char(*c) => true,
        InputEvent::Symbol(c) if !c.is_ascii() => true,
        _ => !state.sm.spelling().is_empty(),
    }
}

fn cleared_update(state: &State, op: EditSessionOp) -> Update {
    Update {
        generation: state.generation,
        op,
        view: CandidateData::hidden(ThemeSnapshot::from_config(&state.config)),
        consumed: true,
    }
}

/// 同一个 match 同时决定 composition 与候选显示，新增 Transition 无需同步两份映射。
fn plan(
    state: &State,
    transition: Transition,
    current: &CandidateData,
    anchor: Option<ScreenPoint>,
) -> Update {
    let theme = ThemeSnapshot::from_config(&state.config);
    let mut view = CandidateData::hidden(theme);
    view.anchor = anchor.or(current.anchor);
    let mut consumed = true;
    let op = match transition {
        Transition::None => {
            view = current.clone();
            EditSessionOp::NoOp
        }
        Transition::Commit(text) => EditSessionOp::CommitAndReplace {
            text,
            replace_len: 0,
        },
        Transition::Candidates {
            spelling,
            candidates,
            code_hints,
            page,
            total_pages,
        } => {
            if spelling.is_empty() {
                EditSessionOp::EndComposition { delete_text: true }
            } else {
                set_candidates(
                    &mut view,
                    &spelling,
                    candidates,
                    code_hints,
                    page,
                    total_pages,
                    state.config.basic.show_code_hints,
                );
                EditSessionOp::CompositionUpdate {
                    spelling,
                    attr: DisplayAttrKind::Converted,
                }
            }
        }
        Transition::CommitThenCandidates {
            text,
            replace_len,
            spelling,
            candidates,
            code_hints,
            page,
            total_pages,
        } => {
            set_candidates(
                &mut view,
                &spelling,
                candidates,
                code_hints,
                page,
                total_pages,
                state.config.basic.show_code_hints,
            );
            EditSessionOp::CommitThenUpdate {
                text,
                replace_len,
                spelling,
                attr: DisplayAttrKind::Converted,
            }
        }
        Transition::SpellingUpdated(spelling) => {
            view.visible = !spelling.is_empty();
            view.spelling = spelling.clone();
            if spelling.is_empty() {
                EditSessionOp::EndComposition { delete_text: true }
            } else {
                EditSessionOp::CompositionUpdate {
                    spelling,
                    attr: DisplayAttrKind::Input,
                }
            }
        }
        Transition::CommitThenSpelling {
            text,
            replace_len,
            spelling,
        } => {
            view.visible = !spelling.is_empty();
            view.spelling = spelling.clone();
            EditSessionOp::CommitThenUpdate {
                text,
                replace_len,
                spelling,
                attr: DisplayAttrKind::Input,
            }
        }
        Transition::Cleared => EditSessionOp::EndComposition { delete_text: true },
        Transition::Passthrough(_) => {
            view = current.clone();
            consumed = false;
            EditSessionOp::NoOp
        }
    };
    Update {
        generation: state.generation,
        op,
        view,
        consumed,
    }
}

fn set_candidates(
    view: &mut CandidateData,
    spelling: &str,
    candidates: Vec<String>,
    hints: Vec<String>,
    page: usize,
    total_pages: usize,
    show_hints: bool,
) {
    view.visible = true;
    view.spelling = spelling.to_string();
    view.page = page;
    view.total_pages = total_pages;
    view.items = candidates
        .into_iter()
        .enumerate()
        .map(|(i, text)| CandidateItem {
            label: format!("{}. ", i + 1),
            text,
            hint: if show_hints {
                hints.get(i).cloned().unwrap_or_default()
            } else {
                String::new()
            },
        })
        .collect();
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_engine::Entry;

    #[derive(Default)]
    struct Editor {
        spelling: String,
        committed: String,
        fail: bool,
        replace_len: usize,
    }
    impl Editor {
        fn apply(&mut self, op: &EditSessionOp) -> Result<Option<ScreenPoint>, &'static str> {
            if self.fail {
                return Err("editor rejected operation");
            }
            match op {
                EditSessionOp::CompositionUpdate { spelling, .. } => {
                    self.spelling = spelling.clone()
                }
                EditSessionOp::CommitAndReplace { text, replace_len } => {
                    self.replace_len = *replace_len;
                    self.committed.push_str(text);
                    self.spelling.clear();
                }
                EditSessionOp::CommitThenUpdate {
                    text,
                    spelling,
                    replace_len,
                    ..
                } => {
                    self.replace_len = *replace_len;
                    self.committed.push_str(text);
                    self.spelling = spelling.clone();
                }
                EditSessionOp::EndComposition { delete_text } => {
                    if !delete_text {
                        self.committed.push_str(&self.spelling);
                    }
                    self.spelling.clear();
                }
                EditSessionOp::NoOp => {}
            }
            Ok(Some(ScreenPoint { x: 80, y: 100 }))
        }
    }
    fn setup() -> (
        InputSession,
        Arc<ArcSwap<CandidateData>>,
        Editor,
        Arc<Dictionary>,
    ) {
        let dict = Dictionary::from_entries(
            vec![
                Entry {
                    code: "aaaa".into(),
                    word: "王".into(),
                    weight: 1,
                },
                Entry {
                    code: "b".into(),
                    word: "工".into(),
                    weight: 1,
                },
            ],
            None,
            Default::default(),
        )
        .unwrap();
        let mut config = Config::default();
        config.basic.auto_commit_unique = false;
        let candidates = Arc::new(ArcSwap::from_pointee(CandidateData::default()));
        let session = InputSession::new(dict.clone(), config, 0, candidates.clone());
        (session, candidates, Editor::default(), dict)
    }
    fn type_text(session: &InputSession, editor: &mut Editor, text: &str) {
        for c in text.chars() {
            assert!(session.test_key(&InputEvent::Char(c), false));
            assert!(session.handle(InputEvent::Char(c), false, None, |op| editor.apply(op)));
        }
    }

    #[test]
    fn overflow_then_switch_to_english_commits_raw_next_code() {
        let (session, candidates, mut editor, _) = setup();
        type_text(&session, &mut editor, "aaaab");
        assert_eq!(editor.committed, "王");
        assert_eq!(editor.replace_len, 4);
        assert_eq!(editor.spelling, "b");
        assert_eq!(candidates.load().items[0].text, "工");
        session.set_chinese(false, |op| editor.apply(op));
        assert_eq!(editor.committed, "王b");
        assert!(editor.spelling.is_empty());
        assert!(!candidates.load().visible);
        assert!(!session.test_key(&InputEvent::Char('b'), false));
        assert!(!session.handle(InputEvent::Char('b'), false, None, |op| editor.apply(op)));
    }

    #[test]
    fn external_termination_clears_candidates_and_next_input_starts_fresh() {
        let (session, candidates, mut editor, _) = setup();
        type_text(&session, &mut editor, "b");
        editor.spelling.clear(); // host ended its composition.
        session.terminated();
        assert!(!candidates.load().visible);
        type_text(&session, &mut editor, "a");
        assert_eq!(editor.spelling, "a");
        assert_eq!(candidates.load().spelling, "a");
    }

    #[test]
    fn failed_edit_hides_candidates_blocks_until_recovery_and_does_not_recommit() {
        let (session, candidates, mut editor, _) = setup();
        type_text(&session, &mut editor, "b");
        editor.fail = true;
        session.handle(InputEvent::Space, false, None, |op| editor.apply(op));
        assert!(!candidates.load().visible);
        assert!(!session.test_key(&InputEvent::Char('a'), false));
        assert!(!session.recover(|op| editor.apply(op)));
        editor.fail = false;
        assert!(session.recover(|op| editor.apply(op)));
        assert_eq!(editor.committed, "b"); // preserves host spelling; never retries uncertain commit.
        type_text(&session, &mut editor, "a");
        assert_eq!(editor.spelling, "a");
    }

    #[test]
    fn reload_and_focus_loss_end_old_composition_and_hide_candidates() {
        let (session, candidates, mut editor, dict) = setup();
        type_text(&session, &mut editor, "b");
        let mut config = session.config();
        config.appearance.font_size = 25;
        let snapshot = RuntimeSnapshot {
            revision: 1,
            dict,
            config,
            config_path: "config.toml".into(),
            system_table_path: "a.dict".into(),
            user_table_path: "u.dict".into(),
        };
        session.reload(&snapshot, |op| editor.apply(op));
        assert_eq!(editor.committed, "b");
        assert!(!candidates.load().visible);
        assert_eq!(candidates.load().theme.font_size, 25);
        type_text(&session, &mut editor, "a");
        session.lose_focus(|op| editor.apply(op));
        assert_eq!(editor.committed, "ba");
        assert!(!candidates.load().visible);
        assert!(!session.test_key(&InputEvent::Backspace, false));
    }

    #[test]
    fn external_termination_during_edit_invalidates_old_publication() {
        let (session, candidates, _, _) = setup();
        session.handle(InputEvent::Char('b'), false, None, |_| {
            session.terminated();
            Ok::<_, &'static str>(None)
        });
        assert!(!candidates.load().visible);
        assert!(!session.selecting());
    }

    #[test]
    fn candidate_and_spelling_views_share_edit_result_and_precise_anchor() {
        let (session, candidates, mut editor, _) = setup();
        type_text(&session, &mut editor, "b");
        session.handle(
            InputEvent::Char('z'),
            false,
            Some(ScreenPoint { x: 1, y: 2 }),
            |op| editor.apply(op),
        );
        assert_eq!(editor.spelling, "bz");
        assert_eq!(candidates.load().spelling, "bz");
        assert!(candidates.load().items.is_empty());
        assert_eq!(candidates.load().anchor.unwrap().x, 80);
        session.handle(InputEvent::Esc, false, None, |op| editor.apply(op));
        assert!(editor.spelling.is_empty());
        assert!(!candidates.load().visible);
    }

    #[test]
    fn preflight_and_execution_agree_on_idle_and_modified_keys() {
        let (session, candidates, mut editor, _) = setup();
        for event in [
            InputEvent::Backspace,
            InputEvent::Space,
            InputEvent::Esc,
            InputEvent::Symbol('.'),
        ] {
            assert!(!session.test_key(&event, false));
            assert!(!session.handle(event, false, None, |op| editor.apply(op)));
        }
        assert!(!session.test_key(&InputEvent::Char('b'), true));
        assert!(!session.handle(InputEvent::Char('b'), true, None, |op| editor.apply(op)));
        assert!(!candidates.load().visible);
        assert!(session.test_key(&InputEvent::Symbol('。'), false));
        session.handle(InputEvent::Symbol('。'), false, None, |op| {
            editor.apply(op)
        });
        assert_eq!(editor.committed, "。");
    }

    #[test]
    fn partial_edit_failure_never_replays_already_committed_text() {
        let (session, candidates, mut editor, _) = setup();
        type_text(&session, &mut editor, "aaaa");
        session.handle(InputEvent::Char('b'), false, None, |op| {
            if let EditSessionOp::CommitThenUpdate { text, .. } = op {
                editor.committed.push_str(text);
                editor.spelling.clear();
            }
            Err::<Option<ScreenPoint>, _>("next composition failed")
        });
        assert!(!candidates.load().visible);
        assert!(session.recover(|op| editor.apply(op)));
        type_text(&session, &mut editor, "b");
        assert_eq!(editor.committed, "王");
        assert_eq!(editor.spelling, "b");
    }

    #[test]
    fn unchanged_runtime_does_not_interrupt_composition() {
        let (session, candidates, mut editor, dict) = setup();
        type_text(&session, &mut editor, "b");
        let snapshot = RuntimeSnapshot {
            revision: 0,
            dict,
            config: session.config(),
            config_path: "c".into(),
            system_table_path: "s".into(),
            user_table_path: "u".into(),
        };
        session.reload(&snapshot, |_| -> Result<Option<ScreenPoint>, &str> {
            panic!("unexpected edit")
        });
        assert_eq!(editor.spelling, "b");
        assert!(candidates.load().visible);
    }

    #[test]
    fn missing_precise_anchor_preserves_fallback_and_previous_anchor() {
        let (session, candidates, _, _) = setup();
        session.handle(
            InputEvent::Char('a'),
            false,
            Some(ScreenPoint { x: 10, y: 20 }),
            |_| Ok::<_, &str>(None),
        );
        session.handle(InputEvent::Char('a'), false, None, |_| Ok::<_, &str>(None));
        assert_eq!(candidates.load().anchor.unwrap().x, 10);
    }
    #[test]
    fn preflight_matches_execution_for_idle_candidates_and_unmatched_spelling() {
        let events = [
            InputEvent::Select(0),
            InputEvent::Select(1),
            InputEvent::Select(2),
            InputEvent::Char('2'),
            InputEvent::Char('b'),
            InputEvent::Char('!'),
            InputEvent::Char('中'),
            InputEvent::Symbol('!'),
            InputEvent::Symbol('。'),
            InputEvent::Space,
            InputEvent::Enter,
            InputEvent::Esc,
            InputEvent::Backspace,
            InputEvent::PageNext,
            InputEvent::PagePrev,
        ];
        for spelling in ["", "b", "z", "z!"] {
            for event in &events {
                let (session, _, mut editor, _) = setup();
                type_text(&session, &mut editor, spelling);
                let predicted = session.test_key(event, false);
                let consumed = session.handle(event.clone(), false, None, |op| editor.apply(op));
                assert_eq!(
                    predicted, consumed,
                    "spelling={spelling:?}, event={event:?}"
                );
            }
        }
    }

    #[test]
    fn changing_editor_cleans_old_composition_before_starting_new_input() {
        let (session, candidates, mut first, _) = setup();
        let mut second = Editor::default();
        type_text(&session, &mut first, "b");
        first.fail = true;
        session.lose_focus(|op| first.apply(op));
        assert!(!candidates.load().visible);
        assert!(!session.handle(InputEvent::Char('a'), false, None, |op| second.apply(op)));
        assert!(second.spelling.is_empty());
        first.fail = false;
        assert!(session.recover(|op| first.apply(op)));
        type_text(&session, &mut second, "a");
        assert_eq!(first.committed, "b");
        assert!(first.spelling.is_empty());
        assert_eq!(second.spelling, "a");
        assert_eq!(candidates.load().spelling, "a");
    }
}
