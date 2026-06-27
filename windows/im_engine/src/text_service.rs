//! Windows TSF 文本服务实现。
//!
//! 通过 `#[windows::core::implement]` 宏将 [`TextService`] 同时实现
//! `ITfTextInputProcessor` / `ITfThreadMgrEventSink` / `ITfKeyEventSink`
//! 三个核心 COM 接口，完成 ROADMAP“TSF 接口对接”阶段：
//!
//! * **激活/去激活**：`ITfTextInputProcessor::Activate` 中保存线程管理器，
//!   并把本对象作为 `ITfKeyEventSink` / `ITfThreadMgrEventSink` 注册到 TSF；
//!   `Deactivate` 中反转该过程，释放 cookie 与线程管理器引用。
//! * **按键过滤**：`ITfKeyEventSink::OnKeyDown` 把虚拟键翻译为
//!   [`InputEvent`] 后驱动 [`StateMachine`]，依据 [`Transition`]
//!   决定是否“吃掉”按键。上屏文本通过日志记录（实际文本插入将在
//!   “基于 Slint 的候选框”阶段使用 ITfEditSession 完成）。
//! * **焦点切换**：`ITfThreadMgrEventSink::OnSetFocus` 用于跟踪文档管理器焦点，
//!   为后续候选框定位提供基础（暂记当前 ITfDocumentMgr 指针）。

use core_engine::{Config, Dictionary, StateMachine, Transition};
use parking_lot::Mutex;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use windows::core::{implement, Interface, Ref, Result, BOOL, GUID, HRESULT};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::TextServices::{
    ITfComposition, ITfCompositionSink, ITfCompositionSink_Impl, ITfContext,
    ITfContextComposition, ITfDocumentMgr, ITfEditSession, ITfEditSession_Impl,
    ITfKeyEventSink, ITfKeyEventSink_Impl, ITfKeystrokeMgr, ITfRange, ITfSource,
    ITfTextInputProcessor, ITfTextInputProcessor_Impl, ITfThreadMgr,
    ITfThreadMgrEventSink, ITfThreadMgrEventSink_Impl,
    TF_ES_ASYNC, TF_DEFAULT_SELECTION, TF_SELECTION,
};

use arc_swap::ArcSwap;

use crate::candidate_data::{CandidateData, CandidateItem, ThemeSnapshot};
use crate::candidate_window::CandidateWindow;
use crate::key_filter;
use crate::screen_geometry::get_caret_position;

// ── 类型定义 ────────────────────────────────────────────────────────

/// 异步 ITfEditSession 的操作描述。由 apply_transition 根据 Transition
/// 构建，由 EditSession::DoEditSession 消费。
#[derive(Debug, Clone)]
enum EditSessionOp {
    /// 创建或更新 composition range 上的编码文本。
    CompositionUpdate { spelling: String },
    /// 将 composition range 文本替换为最终候选词，终止 composition。
    CommitAndReplace { text: String },
    /// 终止 composition。delete_text=true 时删除 range 文本（Esc），
    /// false 时保留文本（Passthrough）。
    EndComposition { delete_text: bool },
    /// 无操作。
    NoOp,
}

/// ITfEditSession 的 COM 实现。由 schedule_edit_session 创建并传递给
/// ITfContext::RequestEditSession 异步调度。
#[implement(ITfEditSession)]
struct EditSession {
    op: EditSessionOp,
    /// 指向 TextService 的原始指针（不增加引用计数）。
    /// 生命周期：EditSession 在 DoEditSession 调用完成后即被释放，
    /// 此时 TextService 一定仍然存活（Activate/Deactivate 生命周期保证）。
    service_ptr: *const TextService,
}

impl ITfEditSession_Impl for EditSession_Impl {
    fn DoEditSession(&self, ec: u32) -> Result<()> {
        let service = unsafe { &*self.service_ptr };
        service.execute_edit_session(ec, &self.op)
    }
}

/// TSF 会话状态：跟踪已注册的 sink 及其注册方式。
#[derive(Default)]
struct SinkState {
    /// ITfSource::AdviseSink cookie（仅 ITfSource 回退路径使用）。
    key_event: u32,
    /// ITfSource::AdviseSink cookie（ITfThreadMgrEventSink）。
    thread_event: u32,
    /// ITfTextInputProcessor::Activate 传入的 TIP 客户端 ID。
    /// 用于 ITfKeystrokeMgr::UnadviseKeyEventSink 的反注册。
    tid: u32,
    /// true=使用 ITfKeystrokeMgr，false=使用 ITfSource::AdviseSink 回退。
    using_keystroke_mgr: bool,
}

/// TSF 文本服务 COM 对象。
///
/// 一次实例对应一次 TIP 激活；`Activate` 时由系统通过 ClassFactory 创建并
/// 注入线程管理器，`Deactivate` 时自动反转连接并使本对象自洽回收。
///
/// 内部的 [`StateMachine`] 通过 `Mutex` 保护，多线程可见且无需发送数据
/// 跨线程锁竞争（TSF 单线程模型 + Mutex 互斥访问足够）。
#[implement(ITfTextInputProcessor, ITfThreadMgrEventSink, ITfKeyEventSink,
             ITfCompositionSink)]
pub struct TextService {
    /// 跨平台核心状态机。
    sm: Mutex<StateMachine>,
    /// 当前激活的线程管理器（用于后续插入文本）。
    thread_mgr: Mutex<Option<ITfThreadMgr>>,
    /// AdviseSink 返回的 cookie，Deactivate 时用于 Unadvise。
    cookies: Mutex<SinkState>,
    /// 当前焦点所在文档管理器（候选框定位参考）。
    focus_doc_mgr: Mutex<Option<ITfDocumentMgr>>,
    /// 外置 self 引用：由 [crate::factory::TextServiceFactory] 在创建时
    /// 注入本对象对应的 `IUnknown`，用于在 `Activate` 中调用
    /// `ITfSource::AdviseSink(IID_ITfKeyEventSink, self, ...)`。
    /// 始终是自身 COM 对象的强引用，但 [`TextService`] 是其内部状态，
    /// 进入 Drop 之前必须先 `take()` 出来否则会循环引用—正确做法见
    /// [`Self::release_self_unknown`]。
    self_unknown: Mutex<Option<windows::core::IUnknown>>,
    /// 候选框数据发布通道——TSF 按键处理通过 ArcSwap 推送 CandidateData。
    candidate_tx: Arc<ArcSwap<CandidateData>>,
    /// 候选框窗口实例（Activate 中启动，Deactivate 中关闭）。
    candidate_window: Mutex<Option<CandidateWindow>>,
    /// ── Phase 1 新增字段 ──
    /// 当前激活的 composition 对象。
    composition: Mutex<Option<ITfComposition>>,
    /// 当前是否处于 composing 状态。
    is_composing: AtomicBool,
}

impl TextService {
    /// 创建一个绑定码表与状态机的文本服务实例（不带 back-pointer）。
    pub fn new(dict: Arc<Dictionary>, page_size: usize, auto_commit_unique: bool, candidate_tx: Arc<ArcSwap<CandidateData>>) -> Self {
        let sm = StateMachine::with_options(dict, page_size, auto_commit_unique);
        Self {
            sm: Mutex::new(sm),
            thread_mgr: Mutex::new(None),
            cookies: Mutex::new(SinkState::default()),
            focus_doc_mgr: Mutex::new(None),
            self_unknown: Mutex::new(None),
            candidate_tx,
            candidate_window: Mutex::new(None),
            composition: Mutex::new(None),
            is_composing: AtomicBool::new(false),
        }
    }

    /// 从 [`Config`] 选择默认参数。
    pub fn from_config(dict: Arc<Dictionary>, cfg: &Config, candidate_tx: Arc<ArcSwap<CandidateData>>) -> Self {
        Self::new(
            dict,
            cfg.basic.candidate_count as usize,
            cfg.basic.auto_commit_unique,
            candidate_tx,
        )
    }

    /// 仅供 IClassFactory 内部注入 self 弱强引用（见
    /// [`crate::factory::TextServiceFactory`])。
    pub(crate) fn set_self_unknown(&self, unk: windows::core::IUnknown) {
        *self.self_unknown.lock() = Some(unk);
    }

        /// 内部：取一份 IUnknown 副本（已 AddRef），用于 AdviseSink 注册。
    fn clone_self_unknown(&self) -> Option<windows::core::IUnknown> {
        self.self_unknown.lock().clone()
    }

    /// Deactivate / Drop 之前清理自身保护：
    ///
    /// 在 [`ITfTextInputProcessor::Deactivate`] 完成后调用一次，移除自我持有
    /// 的 IUnknown 引用，避免循环引用。
    pub(crate) fn release_self_unknown(&self) {
        *self.self_unknown.lock() = None;
    }

    /// 内部：把 [`Transition`] 落实成副作用并返回 BOOL 表示“按键已消费”。
    ///
    /// `context` 为可选的 TSF `ITfContext`，用于候选框坐标获取和 EditSession 调度。
    fn apply_transition(&self, t: Transition, context: Option<&ITfContext>) -> BOOL {
        let theme = ThemeSnapshot::default();
        let op = match &t {
            Transition::None => EditSessionOp::NoOp,
            Transition::Commit(text) => {
                log::info!("[TSF] commit text: {text}");
                self.candidate_tx.store(Arc::new(CandidateData::hidden(theme)));
                EditSessionOp::CommitAndReplace { text: text.clone() }
            }
            Transition::Candidates { spelling, candidates, page, total_pages } => {
                if spelling.is_empty() {
                    self.candidate_tx.store(Arc::new(CandidateData::hidden(theme)));
                    return BOOL(1);
                }
                let items: Vec<CandidateItem> = candidates.iter().enumerate().map(|(i, text)| {
                    CandidateItem { label: format!("{}. ", i + 1), text: text.clone() }
                }).collect();
                let anchor = context.and_then(|ctx| get_caret_position(ctx));
                self.candidate_tx.store(Arc::new(CandidateData::visible(
                    spelling.clone(), items, 0, *page, *total_pages, anchor, theme,
                )));
                EditSessionOp::CompositionUpdate { spelling: spelling.clone() }
            }
            Transition::SpellingUpdated(s) => {
                log::debug!("[TSF] spelling={s}");
                let mut data = (**self.candidate_tx.load()).clone();
                data.spelling = s.clone();
                if data.items.is_empty() {
                    data.visible = false;
                }
                self.candidate_tx.store(Arc::new(data));
                EditSessionOp::CompositionUpdate { spelling: s.clone() }
            }
            Transition::Cleared => {
                self.candidate_tx.store(Arc::new(CandidateData::hidden(theme)));
                EditSessionOp::EndComposition { delete_text: true }
            }
            Transition::Passthrough(_) => {
                self.candidate_tx.store(Arc::new(CandidateData::hidden(theme)));
                EditSessionOp::EndComposition { delete_text: false }
            }
        };

        // 异步调度 EditSession 更新 composition（候选框已同步更新）
        if let Some(ctx) = context {
            self.schedule_edit_session(op, ctx);
        }

        match t {
            Transition::Passthrough(_) => BOOL(0),
            _ => BOOL(1),
        }
    }

    /// 异步调度一条 composition 操作到 TSF 编辑会话。
    fn schedule_edit_session(&self, op: EditSessionOp, context: &ITfContext) {
        let tid = self.cookies.lock().tid;
        if tid == 0 {
            log::warn!("[TSF] schedule_edit_session: tid==0，跳过");
            return;
        }
        let edit_session = EditSession {
            op,
            service_ptr: self as *const TextService,
        };
        let com_obj = windows::core::ComObject::new(edit_session);
        let edit_session_com: ITfEditSession = com_obj.to_interface();
        if let Err(e) = unsafe { context.RequestEditSession(tid, &edit_session_com, TF_ES_ASYNC) } {
            log::error!("[TSF] RequestEditSession 失败: {e}");
        }
    }

    /// 在 TSF EditSession 回调中执行实际的 composition 操作。
    fn execute_edit_session(&self, ec: u32, op: &EditSessionOp) -> Result<()> {
        match op {
            EditSessionOp::NoOp => Ok(()),
            EditSessionOp::CompositionUpdate { spelling } => {
                self.edit_session_composition_update(ec, spelling)
            }
            EditSessionOp::CommitAndReplace { text } => {
                self.edit_session_commit_and_replace(ec, text)
            }
            EditSessionOp::EndComposition { delete_text } => {
                self.edit_session_end_composition(ec, *delete_text)
            }
        }
    }

    fn edit_session_composition_update(&self, ec: u32, spelling: &str) -> Result<()> {
        // 获取焦点文档管理器的顶 context
        let (ctx, _doc_mgr) = self.get_focus_context()?;

        let mut comp_guard = self.composition.lock();

        if comp_guard.is_none() {
            // 首次：在光标处开始 composition
            let ctx_comp: ITfContextComposition = ctx.cast()?;
            let selection = self.get_selection_range(&ctx, ec)?;
            let new_comp = unsafe {
                ctx_comp.StartComposition(ec, &selection, None)
            }?;
            *comp_guard = Some(new_comp);
            self.is_composing.store(true, Ordering::Release);
        }

        if let Some(ref comp) = *comp_guard {
            let range: ITfRange = unsafe { comp.GetRange()? };
            let wide: Vec<u16> = spelling.encode_utf16().collect();
            unsafe { range.SetText(ec, 0, &wide) }?;
        }

        Ok(())
    }

    fn edit_session_commit_and_replace(&self, ec: u32, text: &str) -> Result<()> {
        let mut comp_guard = self.composition.lock();

        if let Some(comp) = comp_guard.take() {
            let range: ITfRange = unsafe { comp.GetRange()? };
            let wide: Vec<u16> = text.encode_utf16().collect();
            unsafe { range.SetText(ec, 0, &wide) }?;
            // 终止 composition
            unsafe { comp.EndComposition(ec) }?;
        } else {
            // 没有 active composition，直接插入文本到光标处
            let (ctx, _doc_mgr) = self.get_focus_context()?;
            let range = self.get_selection_range(&ctx, ec)?;
            let wide: Vec<u16> = text.encode_utf16().collect();
            unsafe { range.SetText(ec, 0, &wide) }?;
        }

        self.is_composing.store(false, Ordering::Release);
        Ok(())
    }

    fn edit_session_end_composition(&self, ec: u32, delete_text: bool) -> Result<()> {
        let mut comp_guard = self.composition.lock();

        if let Some(comp) = comp_guard.take() {
            if delete_text {
                let range: ITfRange = unsafe { comp.GetRange()? };
                unsafe { range.SetText(ec, 0, &[]) }?;  // 清空文本
            }
            unsafe { comp.EndComposition(ec) }?;
        }

        self.is_composing.store(false, Ordering::Release);
        Ok(())
    }

    /// 获取当前焦点文档管理器的顶 context。
    fn get_focus_context(&self) -> Result<(ITfContext, ITfDocumentMgr)> {
        let tm_guard = self.thread_mgr.lock();
        let tm = tm_guard.as_ref().ok_or_else(|| {
            windows::core::Error::from(HRESULT(-1))
        })?;
        let doc_mgr: ITfDocumentMgr = unsafe { tm.GetFocus() }?;
        let ctx: ITfContext = unsafe { doc_mgr.GetBase() }?;
        drop(tm_guard);
        Ok((ctx, doc_mgr))
    }

    /// 获取当前光标处的文本 range。
    fn get_selection_range(&self, ctx: &ITfContext, ec: u32) -> Result<ITfRange> {
        let mut sel = [TF_SELECTION::default()];
        let mut fetched: u32 = 0;
        unsafe { ctx.GetSelection(ec, TF_DEFAULT_SELECTION, &mut sel, &mut fetched) }?;
        if fetched == 0 || sel[0].range.is_none() {
            return Err(windows::core::Error::from(HRESULT(-1)));
        }
        Ok(Option::clone(&sel[0].range).unwrap())
    }
}

impl ITfTextInputProcessor_Impl for TextService_Impl {
    fn Activate(&self, ptim: Ref<'_, ITfThreadMgr>, tid: u32) -> Result<()> {
        // 拿到 ITfThreadMgr 拷贝并保存。
        let tm: ITfThreadMgr = match ptim.as_ref() {
            Some(tm) => tm.clone(),
            None => {
                log::error!("[TSF] Activate: ptim 为空");
                return Err(HRESULT(-1).into());
            }
        };
        *self.thread_mgr.lock() = Some(tm.clone());

        let mut state = self.cookies.lock();
        if let Some(punk_self) = self.clone_self_unknown() {
            // ── 主路径：通过 ITfKeystrokeMgr 注册按键事件 sink ──
            // 这是 Microsoft 官方 TSF 示例推荐的方式，比 ITfSource::AdviseSink
            // 更可靠，确保 OnKeyDown/OnTestKeyDown 被正确调用。
            let kmgr_ok = match tm.cast::<ITfKeystrokeMgr>() {
                Ok(kmgr) => {
                    match punk_self.cast::<ITfKeyEventSink>() {
                        Ok(key_sink) => {
                            match unsafe { kmgr.AdviseKeyEventSink(tid, &key_sink, true) } {
                                Ok(()) => {
                                    state.tid = tid;
                                    state.using_keystroke_mgr = true;
                                    log::info!("[TSF] ITfKeystrokeMgr::AdviseKeyEventSink 成功");
                                    true
                                }
                                Err(e) => {
                                    log::error!("[TSF] AdviseKeyEventSink 失败: {e}");
                                    false
                                }
                            }
                        }
                        Err(e) => {
                            log::error!("[TSF] QI for ITfKeyEventSink 失败: {e}");
                            false
                        }
                    }
                }
                Err(e) => {
                    log::warn!("[TSF] ITfKeystrokeMgr 不可用 ({e})，回退到 ITfSource::AdviseSink");
                    false
                }
            };

            if !kmgr_ok {
                // ── 回退路径：通过 ITfSource::AdviseSink 注册 ──
                if let Ok(source) = tm.cast::<ITfSource>() {
                    let iid_key = <ITfKeyEventSink as windows::core::Interface>::IID;
                    match unsafe { source.AdviseSink(&iid_key, &punk_self) } {
                        Ok(c) => {
                            state.key_event = c;
                            state.using_keystroke_mgr = false;
                            log::info!("[TSF] 回退: ITfSource::AdviseSink ITfKeyEventSink 成功");
                        }
                        Err(e) => log::error!("[TSF] 回退: AdviseSink ITfKeyEventSink 仍然失败: {e}"),
                    }
                }
            }

            // ── ITfThreadMgrEventSink：始终通过 ITfSource ──
            if let Ok(source) = tm.cast::<ITfSource>() {
                let iid_thread = <ITfThreadMgrEventSink as windows::core::Interface>::IID;
                match unsafe { source.AdviseSink(&iid_thread, &punk_self) } {
                    Ok(c) => state.thread_event = c,
                    Err(e) => log::error!("[TSF] AdviseSink ITfThreadMgrEventSink 失败: {e}"),
                }
            }
        } else {
            log::warn!("[TSF] Activate: self_unknown 未注入，跳过所有 AdviseSink");
        }
        drop(state);

        // 启动候选框窗口线程。
        {
            let mut cw = self.candidate_window.lock();
            if cw.is_none() {
                *cw = Some(CandidateWindow::spawn(Arc::clone(&self.candidate_tx)));
            }
        }

        log::info!("[TSF] TIP activated (tid={tid})");
        Ok(())
    }

    fn Deactivate(&self) -> Result<()> {
        // 隐藏候选框并关闭候选框窗口。
        self.candidate_tx.store(Arc::new(CandidateData::hidden(ThemeSnapshot::default())));
        if let Some(mut cw) = self.candidate_window.lock().take() {
            cw.shutdown();
        }

        // 先克隆 thread_mgr 引用再进行清理（避免 take 后丢失 COM 指针）。
        let tm_hold = self.thread_mgr.lock().clone();
        let mut state = self.cookies.lock();

        if let Some(ref tm) = tm_hold {
            // ── 主路径：ITfKeystrokeMgr::UnadviseKeyEventSink ──
            if state.using_keystroke_mgr && state.tid != 0 {
                if let Ok(kmgr) = tm.cast::<ITfKeystrokeMgr>() {
                    let _ = unsafe { kmgr.UnadviseKeyEventSink(state.tid) };
                    log::info!("[TSF] ITfKeystrokeMgr::UnadviseKeyEventSink 完成");
                }
            }

            // ── 回退路径 + ThreadEvent：ITfSource::UnadviseSink ──
            if let Ok(source) = tm.cast::<ITfSource>() {
                if state.key_event != 0 {
                    let _ = unsafe { source.UnadviseSink(state.key_event) };
                    state.key_event = 0;
                }
                if state.thread_event != 0 {
                    let _ = unsafe { source.UnadviseSink(state.thread_event) };
                    state.thread_event = 0;
                }
            }
        }

        // 清理所有持有的 COM 引用。
        *self.thread_mgr.lock() = None;
        *self.focus_doc_mgr.lock() = None;
        drop(state);

        // 释放自我持有的 IUnknown，避免强引用循环。
        self.release_self_unknown();

        // 清理 composition（如果有
        if let Some(comp) = self.composition.lock().take() {
            // 无法在 Deactivate 中获取有效 context 来 EndComposition，直接 drop
            drop(comp);
        }
        self.is_composing.store(false, Ordering::Release);

        // 清理状态机内部缓冲。
        self.sm.lock().reset();

        log::info!("[TSF] TIP deactivated");
        Ok(())
    }
}

impl ITfCompositionSink_Impl for TextService_Impl {
    fn OnCompositionTerminated(&self, _ecwrite: u32, _pcomp: Ref<'_, ITfComposition>) -> Result<()> {
        log::warn!("[TSF] Composition 被外部终止");
        *self.composition.lock() = None;
        self.is_composing.store(false, Ordering::Release);
        self.sm.lock().reset();
        Ok(())
    }
}

impl ITfThreadMgrEventSink_Impl for TextService_Impl {
    fn OnInitDocumentMgr(&self, _pdim: Ref<'_, ITfDocumentMgr>) -> Result<()> {
        Ok(())
    }

    fn OnUninitDocumentMgr(&self, _pdim: Ref<'_, ITfDocumentMgr>) -> Result<()> {
        Ok(())
    }

    fn OnSetFocus(
        &self,
        pdimfocus: Ref<'_, ITfDocumentMgr>,
        _pdimprevfocus: Ref<'_, ITfDocumentMgr>,
    ) -> Result<()> {
        // 跟踪当前焦点文档管理器（候选框定位参考）。
        let new_mgr = pdimfocus.as_ref().map(|m| m.clone());
        *self.focus_doc_mgr.lock() = new_mgr;
        Ok(())
    }

    fn OnPushContext(&self, _pic: Ref<'_, ITfContext>) -> Result<()> {
        Ok(())
    }

    fn OnPopContext(&self, _pic: Ref<'_, ITfContext>) -> Result<()> {
        Ok(())
    }
}

impl ITfKeyEventSink_Impl for TextService_Impl {
    fn OnSetFocus(&self, fforeground: BOOL) -> Result<()> {
        log::debug!("[TSF] KeyEventSink OnSetFocus foreground={:?}", fforeground.0);
        Ok(())
    }

    fn OnTestKeyDown(
        &self,
        _pic: Ref<'_, ITfContext>,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> Result<BOOL> {
        // 声明对 is_intercepted_key 识别的所有按键（字母/数字/退格/空格等）的兴趣
        if !key_filter::is_intercepted_key(wparam.0 as usize) {
            return Ok(BOOL(0));
        }
        let _ = lparam;
        Ok(BOOL(1))
    }

    fn OnTestKeyUp(
        &self,
        _pic: Ref<'_, ITfContext>,
        _wparam: WPARAM,
        _lparam: LPARAM,
    ) -> Result<BOOL> {
        Ok(BOOL(0))
    }

    fn OnKeyDown(
        &self,
        pic: Ref<'_, ITfContext>,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> Result<BOOL> {
        let Some(event) = key_filter::translate(wparam.0 as usize, lparam.0 as isize) else {
            return Ok(BOOL(0));
        };

        let t = self.sm.lock().handle(event);
        Ok(self.apply_transition(t, pic.as_ref()))
    }

    fn OnKeyUp(
        &self,
        _pic: Ref<'_, ITfContext>,
        _wparam: WPARAM,
        _lparam: LPARAM,
    ) -> Result<BOOL> {
        Ok(BOOL(0))
    }

    fn OnPreservedKey(
        &self,
        _pic: Ref<'_, ITfContext>,
        _rguid: *const GUID,
    ) -> Result<BOOL> {
        Ok(BOOL(0))
    }
}