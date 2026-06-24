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

use windows::core::{implement, Interface, Ref, Result, BOOL, GUID, HRESULT};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::TextServices::{
    ITfContext, ITfDocumentMgr, ITfKeyEventSink, ITfKeyEventSink_Impl, ITfSource,
    ITfTextInputProcessor, ITfTextInputProcessor_Impl, ITfThreadMgr,
    ITfThreadMgrEventSink, ITfThreadMgrEventSink_Impl,
};

use arc_swap::ArcSwap;

use crate::candidate_data::{CandidateData, CandidateItem, ThemeSnapshot};
use crate::candidate_window::CandidateWindow;
use crate::key_filter;
use crate::screen_geometry::get_caret_position;

/// 单次会话缓存的 cookie（来自 `ITfSource::AdviseSink`）。
#[derive(Default, Clone, Copy)]
struct SinkCookies {
    key_event: u32,
    thread_event: u32,
}

/// TSF 文本服务 COM 对象。
///
/// 一次实例对应一次 TIP 激活；`Activate` 时由系统通过 ClassFactory 创建并
/// 注入线程管理器，`Deactivate` 时自动反转连接并使本对象自洽回收。
///
/// 内部的 [`StateMachine`] 通过 `Mutex` 保护，多线程可见且无需发送数据
/// 跨线程锁竞争（TSF 单线程模型 + Mutex 互斥访问足够）。
#[implement(ITfTextInputProcessor, ITfThreadMgrEventSink, ITfKeyEventSink)]
pub struct TextService {
    /// 跨平台核心状态机。
    sm: Mutex<StateMachine>,
    /// 当前激活的线程管理器（用于后续插入文本）。
    thread_mgr: Mutex<Option<ITfThreadMgr>>,
    /// AdviseSink 返回的 cookie，Deactivate 时用于 Unadvise。
    cookies: Mutex<SinkCookies>,
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
}

impl TextService {
    /// 创建一个绑定码表与状态机的文本服务实例（不带 back-pointer）。
    pub fn new(dict: Arc<Dictionary>, page_size: usize, auto_commit_unique: bool, candidate_tx: Arc<ArcSwap<CandidateData>>) -> Self {
        let sm = StateMachine::with_options(dict, page_size, auto_commit_unique);
        Self {
            sm: Mutex::new(sm),
            thread_mgr: Mutex::new(None),
            cookies: Mutex::new(SinkCookies::default()),
            focus_doc_mgr: Mutex::new(None),
            self_unknown: Mutex::new(None),
            candidate_tx,
            candidate_window: Mutex::new(None),
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
    /// `context` 为可选的 TSF `ITfContext`，用于在候选框弹出时获取光标屏幕坐标。
    fn apply_transition(&self, t: Transition, context: Option<&ITfContext>) -> BOOL {
        let theme = ThemeSnapshot::default();
        match t {
            Transition::None => BOOL(1),
            Transition::Commit(text) => {
                log::info!("[TSF] commit text: {text}");
                self.candidate_tx.store(Arc::new(CandidateData::hidden(theme)));
                BOOL(1)
            }
            Transition::Candidates { spelling, candidates, page, total_pages } => {
                // 防御：编码串为空则隐藏候选框
                if spelling.is_empty() {
                    self.candidate_tx.store(Arc::new(CandidateData::hidden(theme)));
                    return BOOL(1);
                }
                let items: Vec<CandidateItem> = candidates.iter().enumerate().map(|(i, text)| {
                    CandidateItem { label: format!("{}.", i + 1), text: text.clone() }
                }).collect();

                // 从 TSF 上下文获取光标屏幕坐标（若无上下文则 None，候选框仍工作但不跟随光标）
                let anchor = context.and_then(|ctx| get_caret_position(ctx));
                let data = CandidateData::visible(
                    spelling.clone(), items, 0, page, total_pages, anchor, theme,
                );
                self.candidate_tx.store(Arc::new(data));
                BOOL(1)
            }
            Transition::SpellingUpdated(s) => {
                log::debug!("[TSF] spelling={s}");
                // 更新编码串但不改变候选框可见性——避免打字过程中闪烁
                let mut data = (**self.candidate_tx.load()).clone();
                data.spelling = s.clone();
                // 如果之前没有候选，保持隐藏；如果有，保持可见但清空候选列表
                if data.items.is_empty() {
                    data.visible = false;
                }
                self.candidate_tx.store(Arc::new(data));
                BOOL(1)
            }
            Transition::Cleared => {
                self.candidate_tx.store(Arc::new(CandidateData::hidden(theme)));
                BOOL(1)
            }
            Transition::Passthrough(_) => {
                // 非拦截按键时隐藏候选框，恢复应用焦点
                self.candidate_tx.store(Arc::new(CandidateData::hidden(theme)));
                BOOL(0)
            },
        }
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

        // 把本对象的 IUnknown 作为 ITfKeyEventSink / ITfThreadMgrEventSink 注册到
        // 线程管理器（它通过 QueryInterface 派生出 ITfSource）。
        let mut cookies = self.cookies.lock();
        if let Some(punk_self) = self.clone_self_unknown() {
            match tm.cast::<ITfSource>() {
                Ok(source) => {
                    let iid_key = <ITfKeyEventSink as windows::core::Interface>::IID;
                    let iid_thread = <ITfThreadMgrEventSink as windows::core::Interface>::IID;
                    match unsafe { source.AdviseSink(&iid_key, &punk_self) } {
                        Ok(c) => cookies.key_event = c,
                        Err(e) => log::error!("[TSF] AdviseSink ITfKeyEventSink 失败: {e}"),
                    }
                    match unsafe { source.AdviseSink(&iid_thread, &punk_self) } {
                        Ok(c) => cookies.thread_event = c,
                        Err(e) => log::error!("[TSF] AdviseSink ITfThreadMgrEventSink 失败: {e}"),
                    }
                }
                Err(e) => log::error!("[TSF] ITfThreadMgr::cast::<ITfSource>() 失败: {e}"),
            }
        } else {
            // 工厂未注入 self_unknown，跳过 AdviseSink（系统不会回调按键）。
            log::warn!("[TSF] Activate: self_unknown 未注入，跳过 AdviseSink");
        }

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

        let mut cookies = self.cookies.lock();
        if let Some(tm) = self.thread_mgr.lock().take() {
            if let Ok(source) = tm.cast::<ITfSource>() {
                if cookies.key_event != 0 {
                    let _ = unsafe { source.UnadviseSink(cookies.key_event) };
                    cookies.key_event = 0;
                }
                if cookies.thread_event != 0 {
                    let _ = unsafe { source.UnadviseSink(cookies.thread_event) };
                    cookies.thread_event = 0;
                }
            }
        }
        *self.focus_doc_mgr.lock() = None;

        // 释放自我持有的 IUnknown，避免强引用循环。
        self.release_self_unknown();

        // 清理状态机内部缓冲。
        self.sm.lock().reset();

        log::info!("[TSF] TIP deactivated");
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