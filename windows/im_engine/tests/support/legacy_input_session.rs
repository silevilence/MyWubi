// Frozen CP-01 reference (2012667), compiled only in differential tests.
//! Windows 输入会话协调。测试与 TSF 回调使用同一行为入口；编辑 adapter 在锁外运行。
use crate::candidate_data::{CandidateData, CandidateItem, ScreenPoint, ThemeSnapshot};
use crate::RuntimeSnapshot;
use arc_swap::ArcSwap;
use core_engine::{Config, Dictionary, InputEvent, InputMode, StateMachine, Transition};
use parking_lot::Mutex;
use std::sync::Arc;

/// 只有 adapter 确认尚未提交文字时，才允许恢复临时输入供用户重试。
pub(crate) trait EditFailure: std::fmt::Display {
    fn retry_safe(&self) -> bool {
        false
    }
}
impl EditFailure for &str {}
impl EditFailure for String {}

#[derive(Debug)]
pub(crate) struct SessionEditError {
    pub message: String,
    pub text_committed: bool,
}
impl std::fmt::Display for SessionEditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.message.fmt(f)
    }
}
impl EditFailure for SessionEditError {
    fn retry_safe(&self) -> bool {
        !self.text_committed
    }
}

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
    cleanup_delete_text: bool,
}

struct Update {
    rollback: Option<StateMachine>,
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
    pub fn from_snapshot(
        snapshot: &RuntimeSnapshot,
        candidates: Arc<ArcSwap<CandidateData>>,
    ) -> Self {
        let session = Self::new(
            Arc::clone(&snapshot.dict),
            snapshot.config.clone(),
            snapshot.revision,
            candidates,
        );
        session
            .state
            .lock()
            .sm
            .set_temporary_tables(Arc::clone(&snapshot.temporary));
        session
    }
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
                cleanup_delete_text: false,
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
    pub fn mode(&self) -> InputMode {
        self.state.lock().sm.mode()
    }
    pub fn temporary_trigger(&self) -> Option<char> {
        self.state.lock().sm.temporary_trigger()
    }

    pub fn test_key(&self, event: &InputEvent, system_modifier: bool) -> bool {
        accepts(&self.state.lock(), event, system_modifier)
    }

    pub fn handle<E: EditFailure>(
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
            let rollback = (state.sm.mode() != InputMode::Main).then(|| state.sm.clone());
            let transition = state.sm.handle(event);
            state.generation += 1;
            let mut update = plan(&state, transition, &self.candidates.load(), anchor);
            update.rollback = rollback;
            update
        };
        self.execute(update, edit)
    }

    /// 先前编辑可能部分成功，不重复提交文本；先保留宿主文本并结束旧 composition。
    pub fn recover<E: EditFailure>(
        &self,
        edit: impl FnMut(&EditSessionOp) -> Result<Option<ScreenPoint>, E>,
    ) -> bool {
        let update = {
            let mut state = self.state.lock();
            if !state.needs_cleanup {
                return true;
            }
            state.generation += 1;
            cleared_update(
                &state,
                EditSessionOp::EndComposition {
                    delete_text: state.cleanup_delete_text,
                },
            )
        };
        self.execute(update, edit);
        !self.state.lock().needs_cleanup
    }

    pub fn set_chinese<E: EditFailure>(
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
            let temporary = state.sm.mode() != InputMode::Main;
            let op = if temporary {
                state.sm.reset();
                EditSessionOp::EndComposition { delete_text: true }
            } else {
                state
                    .sm
                    .commit_raw_current()
                    .map(|(text, replace_len)| EditSessionOp::CommitAndReplace {
                        text,
                        replace_len,
                    })
                    .unwrap_or(EditSessionOp::EndComposition { delete_text: false })
            };
            state.generation += 1;
            cleared_update(&state, op)
        };
        self.execute(update, edit);
    }

    pub fn lose_focus<E: EditFailure>(
        &self,
        edit: impl FnMut(&EditSessionOp) -> Result<Option<ScreenPoint>, E>,
    ) {
        let update = {
            let mut state = self.state.lock();
            let temporary = state.sm.mode() != InputMode::Main;
            state.sm.reset();
            state.generation += 1;
            cleared_update(
                &state,
                EditSessionOp::EndComposition {
                    delete_text: temporary,
                },
            )
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

    pub fn reload<E: EditFailure>(
        &self,
        snapshot: &RuntimeSnapshot,
        edit: impl FnMut(&EditSessionOp) -> Result<Option<ScreenPoint>, E>,
    ) {
        let update = {
            let mut state = self.state.lock();
            if state.revision == snapshot.revision {
                return;
            }
            let temporary = state.sm.mode() != core_engine::InputMode::Main;
            state.sm = state_machine(Arc::clone(&snapshot.dict), &snapshot.config);
            state
                .sm
                .set_temporary_tables(Arc::clone(&snapshot.temporary));
            state.config = snapshot.config.clone();
            state.revision = snapshot.revision;
            state.generation += 1;
            cleared_update(
                &state,
                EditSessionOp::EndComposition {
                    delete_text: temporary,
                },
            )
        };
        self.execute(update, edit);
    }

    fn execute<E: EditFailure>(
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
                state.cleanup_delete_text = false;
                if update.view.visible {
                    update.view.anchor = anchor.or(update.view.anchor);
                }
                self.candidates.store(Arc::new(update.view));
            }
            Err(error) => {
                if error.retry_safe() {
                    if let Some(previous) = update.rollback {
                        state.sm = previous;
                        log::warn!("[TSF] 临时输入编辑失败，保留输入供重试: {error}");
                        return update.consumed;
                    }
                }
                log::warn!("[TSF] 输入会话编辑失败，等待清理旧 composition: {error}");
                state.sm.reset();
                state.needs_cleanup = true;
                state.cleanup_delete_text = matches!(
                    update.op,
                    EditSessionOp::EndComposition { delete_text: true }
                );
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
    if state.sm.mode() != InputMode::Main
        || matches!(event, InputEvent::Char(c) | InputEvent::Symbol(c) if state.sm.is_temporary_trigger(*c))
    {
        return true;
    }
    match event {
        InputEvent::Char(c) | InputEvent::Symbol(c) if state.sm.accepts_code_char(*c) => true,
        InputEvent::Symbol(c) if !c.is_ascii() => true,
        _ => !state.sm.spelling().is_empty(),
    }
}

fn cleared_update(state: &State, op: EditSessionOp) -> Update {
    Update {
        rollback: None,
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
            if state.sm.mode() != InputMode::Main {
                set_candidates(
                    &mut view,
                    &spelling,
                    candidates,
                    code_hints,
                    page,
                    total_pages,
                    true,
                );
                if spelling.is_empty() {
                    EditSessionOp::EndComposition { delete_text: true }
                } else {
                    EditSessionOp::CompositionUpdate {
                        spelling,
                        attr: DisplayAttrKind::Input,
                    }
                }
            } else {
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
    if state.sm.mode() != InputMode::Main {
        view.visible = true;
        view.spelling = format!("{}  {}", state.sm.mode().label(), state.sm.spelling())
            .trim_end()
            .to_string();
    }
    Update {
        rollback: None,
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
