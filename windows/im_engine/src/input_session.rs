//! Windows adapter: execute one synchronous host operation outside the core lock and
//! publish GDI data only after the shared session accepts its acknowledgement.
use crate::candidate_data::{CandidateData, CandidateItem, ScreenPoint, ThemeSnapshot};
use crate::RuntimeSnapshot;
use arc_swap::ArcSwap;
use core_engine::session::{
    AckStatus, HostContext, HostOutcome, PreparedEdit, Session, SessionStatus,
};
pub(crate) use core_engine::session::{DisplayAttrKind, EditSessionOp};
#[cfg(test)]
use core_engine::Dictionary;
use core_engine::{Action, BehaviorProfile, Config, InputEvent, InputMode};
use parking_lot::Mutex;
use std::sync::Arc;

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

pub(crate) struct InputSession {
    model: Mutex<Option<crate::model_client::Client>>,
    core: Mutex<Session>,
    candidates: Arc<ArcSwap<CandidateData>>,
}
impl InputSession {
    pub fn from_snapshot(
        snapshot: &RuntimeSnapshot,
        candidates: Arc<ArcSwap<CandidateData>>,
    ) -> Self {
        Self {
            model: Mutex::new(None),
            core: Mutex::new(Session::new(
                snapshot,
                BehaviorProfile::LegacyV1,
                HostContext::default(),
            )),
            candidates,
        }
    }
    #[cfg(test)]
    pub fn new(
        dict: Arc<Dictionary>,
        config: Config,
        revision: u64,
        candidates: Arc<ArcSwap<CandidateData>>,
    ) -> Self {
        Self::from_snapshot(
            &RuntimeSnapshot {
                dict,
                config,
                revision,
                temporary: Arc::default(),
                config_path: "c".into(),
                system_table_path: "s".into(),
                user_table_path: "u".into(),
            },
            candidates,
        )
    }
    pub fn config(&self) -> Config {
        self.core.lock().config().clone()
    }
    pub fn theme(&self) -> ThemeSnapshot {
        ThemeSnapshot::from_config(self.core.lock().config())
    }
    pub fn chinese(&self) -> bool {
        self.core.lock().input_gate()
    }
    pub fn selecting(&self) -> bool {
        let core = self.core.lock();
        core.has_prediction()
            || core.state_machine().state() == core_engine::state_machine::InputState::Selecting
    }
    pub fn mode(&self) -> InputMode {
        self.core.lock().state_machine().mode()
    }
    pub fn temporary_trigger(&self) -> Option<char> {
        self.core.lock().state_machine().temporary_trigger()
    }
    pub fn test_key(&self, event: &InputEvent, system_modifier: bool) -> bool {
        let handled = self.core.lock().would_handle_legacy(event, system_modifier);
        if !handled {
            self.clear_prediction_context();
        }
        handled
    }
    pub fn clear_prediction_context(&self) {
        if let Some(client) = self.model.lock().as_ref() {
            client.invalidate();
        }
        let mut core = self.core.lock();
        let visible = core.has_prediction();
        core.clear_prediction_context();
        if visible {
            self.candidates
                .store(Arc::new(CandidateData::hidden(ThemeSnapshot::from_config(
                    core.config(),
                ))));
        }
    }
    pub fn abandon_prediction(&self) {
        let mut core = self.core.lock();
        if let Some(request) = core.prediction_request() {
            core.complete_prediction(
                request.key,
                Err(core_engine::assistance::Unavailable::Cancelled),
            );
        }
        if let Some(client) = self.model.lock().as_ref() {
            client.invalidate();
        }
    }
    pub fn pump_prediction(&self) -> bool {
        let mut core = self.core.lock();
        let mut changed = core.expire_prediction();
        if let Some(request) = core.prediction_request() {
            let mut client = self.model.lock();
            if client.is_none() {
                *client = crate::model_client::Client::new().ok();
            }
            if let Some(client) = client.as_ref() {
                client.submit(
                    core.model_store_path(),
                    core.config().prediction.clone(),
                    request.clone(),
                );
                if let Some(result) = client.take(request.key) {
                    changed |= core.complete_prediction(request.key, result);
                }
            } else {
                changed |= core.complete_prediction(
                    request.key,
                    Err(core_engine::assistance::Unavailable::Load),
                );
            }
        } else if let Some(client) = self.model.lock().as_ref() {
            if !core.config().prediction.enabled || core.config().prediction.model.is_none() {
                client.release();
            } else {
                client.invalidate();
            }
        }
        if changed {
            let mut data = candidate_view(
                core.snapshot(),
                core.config(),
                self.candidates.load().anchor,
            );
            if core.has_prediction() {
                let window = core.candidate_window(0, 64);
                data.prediction_query = Some(window.query_revision.version);
                data.prediction_ids = window.candidates.iter().map(|c| c.id.0).collect();
            }
            self.candidates.store(Arc::new(data));
        }
        core.prediction_request().is_some()
    }
    pub fn sync_prediction_context(&self, editor: u64, prefix: crate::prediction_context::Prefix) {
        let mut core = self.core.lock();
        let visible = core.has_prediction();

        if core.context().editor != editor {
            let mut context = core.context();
            context.editor = editor;
            context.epoch = context.epoch.wrapping_add(1);
            core.invalidate_context(context, true);
        }
        core.set_assistance_allowed(prefix.allowed);
        let context = core.context();
        core.observe_prefix(context.editor, context.epoch, prefix.text.as_deref());
        if visible && !core.has_prediction() {
            self.candidates
                .store(Arc::new(CandidateData::hidden(ThemeSnapshot::from_config(
                    core.config(),
                ))));
        }
    }

    pub fn handle<E: EditFailure>(
        &self,
        event: InputEvent,
        system_modifier: bool,
        anchor: Option<ScreenPoint>,
        edit: impl FnMut(&EditSessionOp) -> Result<Option<ScreenPoint>, E>,
    ) -> bool {
        let plan = {
            let mut core = self.core.lock();
            if !core.would_handle_legacy(&event, system_modifier) {
                return false;
            }
            core.prepare(Action::Legacy(event))
        };
        match plan {
            Ok(plan) => self.execute(plan, anchor, edit),
            Err(_) => false,
        }
    }
    pub fn choose_prediction<E: EditFailure>(
        &self,
        version: u64,
        id: usize,
        edit: impl FnMut(&EditSessionOp) -> Result<Option<ScreenPoint>, E>,
    ) {
        let plan = {
            let mut core = self.core.lock();
            if !core.has_prediction() {
                return;
            }
            core.prepare(Action::SelectCandidate(
                core_engine::candidates::QueryRevision { engine: 0, version },
                core_engine::candidates::CandidateId(id),
            ))
        };
        if let Ok(plan) = plan {
            self.execute(plan, self.candidates.load().anchor, edit);
        }
    }
    pub fn recover<E: EditFailure>(
        &self,
        edit: impl FnMut(&EditSessionOp) -> Result<Option<ScreenPoint>, E>,
    ) -> bool {
        let plan = self.core.lock().recover();
        if let Ok(Some(plan)) = plan {
            self.execute(plan, None, edit);
        }
        self.core.lock().status() != SessionStatus::NeedsCleanup
    }
    pub fn set_chinese<E: EditFailure>(
        &self,
        chinese: bool,
        edit: impl FnMut(&EditSessionOp) -> Result<Option<ScreenPoint>, E>,
    ) {
        if !chinese {
            self.clear_prediction_context();
        }
        let plan = self.core.lock().set_input_gate(chinese);
        if let Ok(Some(plan)) = plan {
            self.execute(plan, None, edit);
        }
    }
    pub fn lose_focus<E: EditFailure>(
        &self,
        edit: impl FnMut(&EditSessionOp) -> Result<Option<ScreenPoint>, E>,
    ) {
        self.clear_prediction_context();
        let plan = self.core.lock().lose_focus();
        if let Ok(plan) = plan {
            self.execute(plan, None, edit);
        }
    }
    pub fn terminated(&self) {
        self.clear_prediction_context();
        let mut core = self.core.lock();
        core.host_terminated();
        self.candidates
            .store(Arc::new(CandidateData::hidden(ThemeSnapshot::from_config(
                core.config(),
            ))));
    }
    pub fn reload<E: EditFailure>(
        &self,
        snapshot: &RuntimeSnapshot,
        edit: impl FnMut(&EditSessionOp) -> Result<Option<ScreenPoint>, E>,
    ) {
        if self.core.lock().config().prediction != snapshot.config.prediction {
            if let Some(client) = self.model.lock().as_ref() {
                client.release();
            }
        }
        let plan = {
            let mut core = self.core.lock();
            let had_prediction = core.has_prediction();
            let plan = core.apply_runtime(Arc::new(snapshot.clone()));
            if had_prediction && !core.has_prediction() {
                self.candidates.store(Arc::new(candidate_view(
                    core.snapshot(),
                    core.config(),
                    self.candidates.load().anchor,
                )));
            }
            plan
        };
        if let Ok(Some(plan)) = plan {
            self.execute(plan, None, edit);
        }
    }

    fn execute<E: EditFailure>(
        &self,
        plan: PreparedEdit,
        anchor: Option<ScreenPoint>,
        mut edit: impl FnMut(&EditSessionOp) -> Result<Option<ScreenPoint>, E>,
    ) -> bool {
        let consumed = plan.consumed();
        let mut next_view = {
            let core = self.core.lock();
            if !core.is_current(plan.token) {
                return consumed;
            }
            match &plan.preview {
                core_engine::candidates::ViewUpdate::Reset(view) => {
                    candidate_view(view, core.config(), None)
                }
                core_engine::candidates::ViewUpdate::Unchanged => {
                    (*self.candidates.load_full()).clone()
                }
                core_engine::candidates::ViewUpdate::Replace(view) => {
                    let current = self.candidates.load();
                    candidate_view(view, core.config(), anchor.or(current.anchor))
                }
            }
        };
        // TSF is synchronous and thread-affine. No core lock is held across callbacks.
        let result = if matches!(plan.op, EditSessionOp::NoOp) {
            Ok(None)
        } else {
            edit(&plan.op)
        };
        let outcome = match &result {
            Ok(_) => HostOutcome::Applied,
            Err(error) if error.retry_safe() => HostOutcome::RejectedBeforeWrite,
            Err(error) => {
                log::warn!("[TSF] 输入会话编辑失败，等待清理旧 composition: {error}");
                HostOutcome::OutcomeUnknown
            }
        };
        let mut core = self.core.lock();
        let ack = core.acknowledge(plan.token, outcome);
        match ack.status {
            AckStatus::Published => {
                if core.has_prediction() || core.waiting_for_prediction() {
                    next_view = candidate_view(
                        core.snapshot(),
                        core.config(),
                        anchor.or(self.candidates.load().anchor),
                    );
                }
                if core.has_prediction() {
                    let window = core.candidate_window(0, 64);
                    next_view.prediction_query = Some(window.query_revision.version);
                    next_view.prediction_ids = window.candidates.iter().map(|c| c.id.0).collect();
                }
                if next_view.visible || core.waiting_for_prediction() {
                    if let Ok(precise) = result {
                        next_view.anchor = precise.or(next_view.anchor);
                    }
                }
                self.candidates.store(Arc::new(next_view));
            }
            AckStatus::NeedsCleanup => {
                self.candidates
                    .store(Arc::new(CandidateData::hidden(ThemeSnapshot::from_config(
                        core.config(),
                    ))))
            }
            AckStatus::Retained | AckStatus::Stale => {}
        }
        consumed
    }
}

fn candidate_view(
    view: &core_engine::candidates::SessionView,
    config: &Config,
    anchor: Option<ScreenPoint>,
) -> CandidateData {
    let mut data = CandidateData::hidden(ThemeSnapshot::from_config(config));
    data.visible = view.visible;
    data.spelling = view.spelling.clone();
    data.page = view.page;
    data.total_pages = view.total_pages;
    // Cleared operations historically discard anchors; active views retain the supplied fallback.
    data.anchor = anchor;
    data.items = view
        .items
        .iter()
        .enumerate()
        .map(|(i, item)| CandidateItem {
            disabled: view.mode == InputMode::Menu && !item.hint.is_empty(),
            label: format!("{}. ", i + 1),
            text: item.text.clone(),
            hint: if view.mode == InputMode::Menu {
                String::new()
            } else {
                item.hint.clone()
            },
        })
        .collect();
    data
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_engine::Entry;

    #[test]
    fn prediction_keys_commit_visible_candidates_without_status_words() {
        for event in [
            InputEvent::Space,
            InputEvent::Char('1'),
            InputEvent::Char('2'),
            InputEvent::Select(3),
        ] {
            let mut config = Config::default();
            config.prediction.enabled = true;
            config.basic.auto_commit_unique = false;
            let view = Arc::new(ArcSwap::from_pointee(CandidateData::default()));
            let session = InputSession::new(
                Dictionary::from_text("b\t北\n").unwrap(),
                config,
                0,
                view.clone(),
            );
            let mut editor = Editor::default();
            type_text(&session, &mut editor, "b");
            session.handle(InputEvent::Space, false, None, |op| editor.apply(op));
            assert!(view.load().spelling.is_empty());
            assert!(view.load().items.iter().all(|item| item.hint.is_empty()));
            let index = match event {
                InputEvent::Char('2') => 1,
                InputEvent::Select(3) => 2,
                _ => 0,
            };
            let expected = format!("北{}", view.load().items[index].text);
            assert!(session.test_key(&event, false));
            assert!(session.handle(event, false, None, |op| editor.apply(op)));
            assert_eq!(editor.committed, expected);
            assert!(editor.spelling.is_empty());
        }
    }

    #[test]
    fn unavailable_mode_choices_use_disabled_style_without_error_text() {
        let view = core_engine::candidates::SessionView {
            visible: true,
            mode: InputMode::Menu,
            items: vec![core_engine::candidates::PresentationItem {
                text: "拼音".into(),
                hint: "未配置拼音码表".into(),
            }],
            ..Default::default()
        };
        let data = candidate_view(&view, &Config::default(), None);
        assert!(data.spelling.is_empty());
        assert!(data.items[0].disabled);
        assert!(data.items[0].hint.is_empty());
    }

    #[test]
    fn model_wait_retains_the_confirmed_commit_anchor() {
        let mut config = Config::default();
        config.basic.auto_commit_unique = false;
        config.prediction.enabled = true;
        config.prediction.model = Some(core_engine::config::ModelSelection {
            id: "fixture".into(),
            version: "v1".into(),
        });
        let view = Arc::new(ArcSwap::from_pointee(CandidateData::default()));
        let session = InputSession::new(
            Dictionary::from_text("b\t北\t100\n").unwrap(),
            config,
            0,
            view.clone(),
        );
        for event in [InputEvent::Char('b'), InputEvent::Space] {
            assert!(
                session.handle(event, false, None, |_| Ok::<_, SessionEditError>(Some(
                    ScreenPoint { x: 19, y: 37 }
                )))
            );
        }
        assert!(session.core.lock().waiting_for_prediction());
        assert!(!view.load().visible);
        let anchor = view.load().anchor.unwrap();
        assert_eq!((anchor.x, anchor.y), (19, 37));
    }

    #[test]
    fn frozen_legacy_driver_matches_shared_adapter_across_keys_controls_and_failures() {
        use crate::legacy_baseline as old;
        // Only the intentionally removed UI labels differ from the frozen input behavior.
        fn without_status_labels(data: &CandidateData) -> String {
            let mut data = data.clone();
            let menu = data.spelling.starts_with(InputMode::Menu.label());
            for mode in [InputMode::Menu, InputMode::Pinyin, InputMode::English] {
                if let Some(spelling) = data.spelling.strip_prefix(mode.label()) {
                    data.spelling = spelling.trim_start().into();
                    break;
                }
            }
            if menu {
                for item in &mut data.items {
                    item.disabled = !item.hint.is_empty();
                    item.hint.clear();
                }
            }
            format!("{data:?}")
        }
        for seed in ["", "a", "aaaa", "z", "`", "`1", "`1ni", "`2Hello"] {
            for failure in 0..3 {
                let (new, new_view, _) = temporary_setup();
                let old_view = Arc::new(ArcSwap::from_pointee(CandidateData::default()));
                let core = new.core.lock();
                let snapshot = RuntimeSnapshot {
                    dict: Dictionary::from_text("---\ntemporary:\n  key: '`'\n  reverse_lookup: true\n---\nwq\t你好\na\t主\n").unwrap(),
                    temporary: Arc::new(core_engine::TemporaryTables { pinyin: Some(Dictionary::from_text("---\ntype: pinyin\nmax_code_len: 64\n---\nni'hao\t你好\n").unwrap()), reverse: Some(core_engine::ReverseLookup::new(&Dictionary::from_text("wq\t你好\na\t主\n").unwrap())), ..Default::default() }),
                    config: core.config().clone(), revision: core.revision(), config_path: "c".into(), system_table_path: "s".into(), user_table_path: "u".into(),
                };
                drop(core);
                let old = old::InputSession::from_snapshot(&snapshot, old_view.clone());
                let mut new_ops = Vec::new();
                let mut old_ops = Vec::new();
                for c in seed.chars() {
                    new.handle(InputEvent::Char(c), false, None, |op| {
                        new_ops.push(format!("{op:?}"));
                        Ok::<_, SessionEditError>(Some(ScreenPoint { x: 4, y: 8 }))
                    });
                    old.handle(InputEvent::Char(c), false, None, |op| {
                        old_ops.push(format!("{op:?}"));
                        Ok::<_, old::SessionEditError>(Some(ScreenPoint { x: 4, y: 8 }))
                    });
                }
                for event in [
                    InputEvent::PagePrev,
                    InputEvent::PageNext,
                    InputEvent::Select(9),
                    InputEvent::Char('1'),
                    InputEvent::Backspace,
                    InputEvent::Symbol('，'),
                    InputEvent::Space,
                    InputEvent::Enter,
                    InputEvent::Esc,
                ] {
                    assert_eq!(old.test_key(&event, false), new.test_key(&event, false));
                    let new_consumed = new.handle(event.clone(), false, None, |op| {
                        new_ops.push(format!("{op:?}"));
                        if failure == 0 {
                            Ok(None)
                        } else {
                            Err(SessionEditError {
                                message: "injected".into(),
                                text_committed: failure == 2,
                            })
                        }
                    });
                    let old_consumed = old.handle(event.clone(), false, None, |op| {
                        old_ops.push(format!("{op:?}"));
                        if failure == 0 {
                            Ok(None)
                        } else {
                            Err(old::SessionEditError {
                                message: "injected".into(),
                                text_committed: failure == 2,
                            })
                        }
                    });
                    assert_eq!(old_consumed, new_consumed, "{seed} {failure} {event:?}");
                    assert_eq!(old.mode(), new.mode());
                    assert_eq!(old_ops, new_ops);
                    assert_eq!(
                        without_status_labels(&old_view.load_full()),
                        format!("{:?}", new_view.load_full()),
                        "{seed} {failure} {event:?}"
                    );
                    let a = new.recover(|op| {
                        new_ops.push(format!("{op:?}"));
                        Ok::<_, &str>(None)
                    });
                    let b = old.recover(|op| {
                        old_ops.push(format!("{op:?}"));
                        Ok::<_, &str>(None)
                    });
                    assert_eq!(a, b);
                    assert_eq!(old_ops, new_ops);
                }
                for control in 0..4 {
                    match control {
                        0 => {
                            new.set_chinese(false, |_| Err::<Option<ScreenPoint>, _>("failed"));
                            old.set_chinese(false, |_| Err::<Option<ScreenPoint>, _>("failed"));
                        }
                        1 => {
                            new.lose_focus(|_| Ok::<_, &str>(None));
                            old.lose_focus(|_| Ok::<_, &str>(None));
                        }
                        2 => {
                            let mut updated = snapshot.clone();
                            updated.revision += 1;
                            new.reload(&updated, |_| Err::<Option<ScreenPoint>, _>("failed"));
                            old.reload(&updated, |_| Err::<Option<ScreenPoint>, _>("failed"));
                        }
                        _ => {
                            new.terminated();
                            old.terminated();
                        }
                    }
                    assert_eq!(old.chinese(), new.chinese());
                    assert_eq!(old.mode(), new.mode());
                    assert_eq!(old.selecting(), new.selecting());
                    assert_eq!(old.temporary_trigger(), new.temporary_trigger());
                    assert_eq!(old.theme().font_size, new.theme().font_size);
                    assert_eq!(
                        old.config().basic.candidate_count,
                        new.config().basic.candidate_count
                    );
                    assert_eq!(
                        without_status_labels(&old_view.load_full()),
                        format!("{:?}", new_view.load_full())
                    );
                }
            }
        }
    }

    #[test]
    fn nested_input_sees_advanced_state_and_old_result_cannot_publish() {
        let (session, candidates, _, _) = setup();
        session.handle(InputEvent::Char('a'), false, None, |op| {
            assert!(matches!(op, EditSessionOp::CompositionUpdate { spelling, attr: DisplayAttrKind::Converted } if spelling == "a"));
            assert_eq!(session.core.lock().state_machine().spelling(), "a");
            assert!(!candidates.load().visible);
            assert!(session.handle(InputEvent::Char('a'), false, None, |_| {
                Ok::<_, &str>(Some(ScreenPoint { x: 12, y: 34 }))
            }));
            Err::<Option<ScreenPoint>, _>("outer failed after nested input")
        });
        assert_eq!(session.core.lock().state_machine().spelling(), "aa");
        assert_eq!(candidates.load().spelling, "aa");
        assert_eq!(candidates.load().anchor.map(|p| (p.x, p.y)), Some((12, 34)));
        assert!(
            !(session.core.lock().status() == core_engine::session::SessionStatus::NeedsCleanup)
        );
    }

    #[test]
    fn switch_and_reload_advance_before_edit_and_failure_does_not_rollback() {
        let (session, _, mut editor) = temporary_setup();
        type_text(&session, &mut editor, "`1ni");
        session.set_chinese(false, |op| {
            assert!(!session.chinese());
            assert_eq!(session.mode(), InputMode::Main);
            assert_eq!(*op, EditSessionOp::EndComposition { delete_text: true });
            Err::<Option<ScreenPoint>, _>(SessionEditError {
                message: "denied".into(),
                text_committed: false,
            })
        });
        assert!(!session.chinese());
        assert_eq!(session.mode(), InputMode::Main);
        let snapshot = RuntimeSnapshot {
            temporary: Arc::default(),
            dict: Dictionary::from_text("x\t新\n").unwrap(),
            config: Config::default(),
            revision: 42,
            config_path: "c".into(),
            system_table_path: "s".into(),
            user_table_path: "u".into(),
        };
        session.reload(&snapshot, |_| {
            assert_eq!(session.core.lock().revision(), 42);
            session.terminated();
            Err::<Option<ScreenPoint>, _>("late failure")
        });
        assert_eq!(session.core.lock().revision(), 42);
        assert!(
            !(session.core.lock().status() == core_engine::session::SessionStatus::NeedsCleanup)
        );
    }

    #[test]
    fn none_preserves_view_and_preflight_does_not_advance_generation() {
        let (session, candidates, mut editor, _) = setup();
        type_text(&session, &mut editor, "b");
        let before = candidates.load_full();
        let generation = session.core.lock().generation();
        for _ in 0..3 {
            assert!(session.test_key(&InputEvent::PageNext, false));
        }
        assert_eq!(session.core.lock().generation(), generation);
        session.handle(InputEvent::PageNext, false, None, |_| -> Result<_, &str> {
            panic!("None must not execute an edit")
        });
        assert_eq!(
            format!("{:?}", candidates.load_full()),
            format!("{before:?}")
        );
    }

    fn temporary_setup() -> (InputSession, Arc<ArcSwap<CandidateData>>, Editor) {
        let main = Dictionary::from_text(
            "---\ntemporary:\n  key: '`'\n  reverse_lookup: true\n---\nwq\t你好\na\t主\n",
        )
        .unwrap();
        let pinyin =
            Dictionary::from_text("---\ntype: pinyin\nmax_code_len: 64\n---\nni'hao\t你好\n")
                .unwrap();
        let tables = Arc::new(core_engine::TemporaryTables {
            pinyin: Some(pinyin),
            reverse: Some(core_engine::ReverseLookup::new(&main)),
            ..Default::default()
        });
        let candidates = Arc::new(ArcSwap::from_pointee(CandidateData::default()));
        let mut config = Config::default();
        config.basic.show_code_hints = false;
        let snapshot = RuntimeSnapshot {
            temporary: tables,
            dict: main,
            config,
            revision: 0,
            config_path: "config.toml".into(),
            system_table_path: "main.dict".into(),
            user_table_path: "user.dict".into(),
        };
        (
            InputSession::from_snapshot(&snapshot, candidates.clone()),
            candidates,
            Editor::default(),
        )
    }

    #[test]
    fn temporary_menu_never_enters_document_and_commit_returns_to_main() {
        let (session, candidates, mut editor) = temporary_setup();
        type_text(&session, &mut editor, "`");
        assert!(candidates.load().visible);
        assert_eq!(candidates.load().items.len(), 2);
        assert!(editor.spelling.is_empty());
        type_text(&session, &mut editor, "1");
        assert!(candidates.load().spelling.is_empty());
        assert!(editor.spelling.is_empty());
        type_text(&session, &mut editor, "ni'hao");
        assert_eq!(editor.spelling, "ni'hao");
        assert_eq!(candidates.load().items[0].hint, "wq");
        session.handle(InputEvent::Space, false, None, |op| editor.apply(op));
        assert_eq!(editor.committed, "你好");
        assert_eq!(session.mode(), InputMode::Main);
        type_text(&session, &mut editor, "a");
        assert_eq!(candidates.load().items[0].text, "主");
    }

    #[test]
    fn failed_temporary_commit_before_write_preserves_input_for_retry() {
        let (session, candidates, mut editor) = temporary_setup();
        type_text(&session, &mut editor, "`1nihao");
        session.handle(InputEvent::Space, false, None, |_| {
            Err::<Option<ScreenPoint>, _>(SessionEditError {
                message: "lock denied".into(),
                text_committed: false,
            })
        });
        assert_eq!(session.mode(), InputMode::Pinyin);
        assert!(candidates.load().visible);
        assert!(editor.committed.is_empty());
        session.handle(InputEvent::Space, false, None, |op| editor.apply(op));
        assert_eq!(editor.committed, "你好");
        assert_eq!(session.mode(), InputMode::Main);
    }

    #[test]
    fn failed_cleanup_after_temporary_text_write_never_recommits() {
        let (session, candidates, mut editor) = temporary_setup();
        type_text(&session, &mut editor, "`1nihao");
        session.handle(InputEvent::Space, false, None, |op| {
            editor.apply(op).unwrap();
            Err::<Option<ScreenPoint>, _>(SessionEditError {
                message: "cleanup failed".into(),
                text_committed: true,
            })
        });
        assert!(!candidates.load().visible);
        assert!(session.recover(|op| editor.apply(op)));
        assert_eq!(editor.committed, "你好");
        assert_eq!(session.mode(), InputMode::Main);
    }

    #[test]
    fn failed_temporary_cancellation_recovery_still_deletes_raw_input() {
        let (session, _, mut editor) = temporary_setup();
        type_text(&session, &mut editor, "`1ni");
        session.lose_focus(|_| Err::<Option<ScreenPoint>, _>("lock denied"));
        assert!(session.recover(|op| editor.apply(op)));
        assert!(editor.spelling.is_empty());
        assert!(editor.committed.is_empty());
    }

    #[test]
    fn temporary_focus_switch_reload_and_empty_backspace_clear_session() {
        for action in 0..4 {
            let (session, candidates, mut editor) = temporary_setup();
            type_text(&session, &mut editor, "`1ni");
            match action {
                0 => session.lose_focus(|op| editor.apply(op)),
                1 => session.set_chinese(false, |op| editor.apply(op)),
                2 => {
                    let snapshot = RuntimeSnapshot {
                        temporary: Arc::default(),
                        dict: Dictionary::from_text("a\t新\n").unwrap(),
                        config: session.config(),
                        revision: 1,
                        config_path: "c".into(),
                        system_table_path: "s".into(),
                        user_table_path: "u".into(),
                    };
                    session.reload(&snapshot, |op| editor.apply(op));
                }
                _ => {
                    for _ in 0..3 {
                        assert!(session
                            .handle(InputEvent::Backspace, false, None, |op| editor.apply(op)));
                    }
                }
            }
            assert_eq!(session.mode(), InputMode::Main);
            assert!(editor.committed.is_empty());
            assert!(editor.spelling.is_empty());
            assert!(!candidates.load().visible);
        }
    }

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
            temporary: Arc::default(),
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
            temporary: Arc::default(),
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
