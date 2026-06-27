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

use core_engine::{Config, Dictionary, InputEvent, StateMachine, Transition};
use parking_lot::Mutex;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use windows::core::{implement, Interface, Ref, Result, BOOL, GUID, HRESULT};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::TextServices::{
    ITfCategoryMgr, ITfComposition, ITfCompositionSink, ITfCompositionSink_Impl, ITfContext,
    ITfContextComposition, ITfDisplayAttributeInfo, ITfDisplayAttributeInfo_Impl,
    ITfDisplayAttributeProvider, ITfDisplayAttributeProvider_Impl,
    ITfDocumentMgr, ITfEditSession, ITfEditSession_Impl, ITfEditRecord,
    ITfFnGetPreferredTouchKeyboardLayout, ITfFnGetPreferredTouchKeyboardLayout_Impl,
    ITfFunction, ITfFunction_Impl, ITfFunctionProvider, ITfFunctionProvider_Impl,
    ITfKeyEventSink, ITfKeyEventSink_Impl, IEnumTfDisplayAttributeInfo,
    ITfKeystrokeMgr, ITfRange, ITfSource,
    ITfTextEditSink, ITfTextEditSink_Impl,
    ITfTextInputProcessor, ITfTextInputProcessorEx, ITfTextInputProcessorEx_Impl,
    ITfTextInputProcessor_Impl, ITfThreadFocusSink, ITfThreadFocusSink_Impl,
    ITfThreadMgr,
    ITfThreadMgrEventSink, ITfThreadMgrEventSink_Impl,
    TF_DA_COLOR, TF_DA_COLOR_0,
    TF_DISPLAYATTRIBUTE, TF_ES_ASYNC, TF_ES_READWRITE, TF_DEFAULT_SELECTION, TF_LS_DOT,
    TF_LS_SOLID, TF_SELECTION, TF_CT_COLORREF, GUID_TFCAT_DISPLAYATTRIBUTEPROVIDER,
    TF_PRESERVEDKEY, TF_MOD_ON_KEYUP,
    TKBLayoutType, TKBLT_OPTIMIZED, TKBL_OPT_SIMPLIFIED_CHINESE_PINYIN,
};
use windows::Win32::UI::Input::KeyboardAndMouse::VK_SHIFT;

use windows::Win32::Foundation::COLORREF;

use arc_swap::ArcSwap;

use crate::candidate_data::{CandidateData, CandidateItem, ThemeSnapshot};
use crate::guids::CLSID_TEXT_SERVICE;
use crate::candidate_window::CandidateWindow;
use crate::key_filter;
use crate::screen_geometry::get_caret_position;

// ── 类型定义 ────────────────────────────────────────────────────────

/// 编码输入态 display attribute 的 GUID。
const GUID_DISPLAY_ATTR_INPUT: GUID = GUID::from_u128(0x8a2e3b4c_1d5f_4a7b_9e6c_3f8d2b1a5e7d);
/// 有候选态 display attribute 的 GUID。
const GUID_DISPLAY_ATTR_CONVERTED: GUID = GUID::from_u128(0x6b1c8d9e_2f3a_4c5b_8d7e_1a2b3c4d5e6f);
/// PreservedKey（Shift 切换）的 GUID。
const GUID_PRESERVED_SHIFT: GUID = GUID::from_u128(0x1a2b3c4d_5e6f_7a8b_9c0d_1e2f3a4b5c6d);


/// 自定义 DisplayAttributeInfo 实现。每个实例对应用户自定义的 display attribute。
#[implement(ITfDisplayAttributeInfo)]
struct DisplayAttributeInfo {
    /// 对应 GUID_DISPLAY_ATTR_INPUT 或 GUID_DISPLAY_ATTR_CONVERTED。
    guid: GUID,
    /// 显示名称。
    description: &'static str,
    /// TF_DISPLAYATTRIBUTE 数据。
    attr: TF_DISPLAYATTRIBUTE,
}

impl ITfDisplayAttributeInfo_Impl for DisplayAttributeInfo_Impl {
    fn GetGUID(&self) -> Result<GUID> {
        Ok(self.guid)
    }

    fn GetDescription(&self) -> Result<windows::core::BSTR> {
        Ok(windows::core::BSTR::from(self.description))
    }

    fn GetAttributeInfo(&self, pda: *mut TF_DISPLAYATTRIBUTE) -> Result<()> {
        unsafe { *pda = self.attr.clone() };
        Ok(())
    }

    fn SetAttributeInfo(&self, _pda: *const TF_DISPLAYATTRIBUTE) -> Result<()> {
        Ok(())  // 自定义属性不支持外部修改
    }

    fn Reset(&self) -> Result<()> {
        Ok(())
    }
}

/// 辅助函数：创建一个 TF_DA_COLOR（指定 RGB）。
fn make_color(r: u8, g: u8, b: u8) -> TF_DA_COLOR {
    TF_DA_COLOR {
        r#type: TF_CT_COLORREF,
            Anonymous: TF_DA_COLOR_0 { cr: COLORREF((r as u32) | ((g as u32) << 8) | ((b as u32) << 16)) },
    }
}

/// 创建一个 ComObject 包裹的 DisplayAttributeInfo，返回其 ITfDisplayAttributeInfo 接口。
fn make_display_attr_info(guid: GUID, desc: &'static str, attr: TF_DISPLAYATTRIBUTE) -> ITfDisplayAttributeInfo {
    let info = DisplayAttributeInfo { guid, description: desc, attr };
    let obj = windows::core::ComObject::new(info);
    obj.to_interface()
}

/// 构建「编码输入态」display attribute：灰色点线。
fn attr_input() -> TF_DISPLAYATTRIBUTE {
    TF_DISPLAYATTRIBUTE {
        crText: make_color(0x80, 0x80, 0x80),
        crBk: TF_DA_COLOR { r#type: TF_CT_COLORREF, ..Default::default() },
        lsStyle: TF_LS_DOT,
        fBoldLine: false.into(),
        crLine: make_color(0x80, 0x80, 0x80),
        bAttr: Default::default(),
    }
}

/// 构建「有候选态」display attribute：黑色实线。
fn attr_converted() -> TF_DISPLAYATTRIBUTE {
    TF_DISPLAYATTRIBUTE {
        crText: make_color(0x00, 0x00, 0x00),
        crBk: TF_DA_COLOR { r#type: TF_CT_COLORREF, ..Default::default() },
        lsStyle: TF_LS_SOLID,
        fBoldLine: false.into(),
        crLine: make_color(0x00, 0x00, 0x00),
        bAttr: Default::default(),
    }
}

/// 异步 ITfEditSession 的操作描述。由 apply_transition 根据 Transition
/// 构建，由 EditSession::DoEditSession 消费。
#[derive(Debug, Clone)]
#[allow(dead_code)]
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
             ITfCompositionSink, ITfDisplayAttributeProvider,
             ITfTextInputProcessorEx, ITfThreadFocusSink,
             ITfTextEditSink, ITfFunctionProvider,
             ITfFunction, ITfFnGetPreferredTouchKeyboardLayout)]
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
    /// 当前候选框主题快照。
    theme: ThemeSnapshot,
    /// ── Phase 1 新增字段 ──
    /// 当前激活的 composition 对象。
    composition: Mutex<Option<ITfComposition>>,
    /// 当前是否处于 composing 状态。
    is_composing: AtomicBool,
    /// ── Phase 2 新增字段 ──
    /// ITfTextInputProcessorEx 的激活标志（TF_TMAE_COMLESS 等）。
    activate_flags: Mutex<u32>,
    /// true=中文模式（拦截字母键），false=英文模式（Passthrough）。
    ime_mode: Mutex<bool>,
    /// ── Phase 3 新增字段 ──
    /// ITfThreadFocusSink 的 AdviseSink cookie。
    thread_focus_cookie: Mutex<u32>,
    /// 注册了 ITfTextEditSink 的上下文（用于反注册）。
    edit_sink_context: Mutex<Option<ITfContext>>,
    /// ITfTextEditSink 的 AdviseSink cookie。
    edit_sink_cookie: Mutex<u32>,
}

fn should_intercept_test_key(event: Option<InputEvent>, spelling_empty: bool) -> bool {
    match event {
        Some(InputEvent::Char(_)) => true,
        Some(_) => !spelling_empty,
        None => false,
    }
}

impl TextService {
    /// 创建一个绑定码表与状态机的文本服务实例（不带 back-pointer）。
    pub fn new(dict: Arc<Dictionary>, page_size: usize, auto_commit_unique: bool, candidate_tx: Arc<ArcSwap<CandidateData>>) -> Self {
        Self::with_theme(
            dict,
            page_size,
            auto_commit_unique,
            candidate_tx,
            ThemeSnapshot::default(),
        )
    }

    fn with_theme(
        dict: Arc<Dictionary>,
        page_size: usize,
        auto_commit_unique: bool,
        candidate_tx: Arc<ArcSwap<CandidateData>>,
        theme: ThemeSnapshot,
    ) -> Self {
        let sm = StateMachine::with_options(dict, page_size, auto_commit_unique);
        Self {
            sm: Mutex::new(sm),
            thread_mgr: Mutex::new(None),
            cookies: Mutex::new(SinkState::default()),
            focus_doc_mgr: Mutex::new(None),
            self_unknown: Mutex::new(None),
            candidate_tx,
            candidate_window: Mutex::new(None),
            theme,
            composition: Mutex::new(None),
            is_composing: AtomicBool::new(false),
            activate_flags: Mutex::new(0),
            ime_mode: Mutex::new(true),
            thread_focus_cookie: Mutex::new(0),
            edit_sink_context: Mutex::new(None),
            edit_sink_cookie: Mutex::new(0),
        }
    }

    /// 从 [`Config`] 选择默认参数。
    pub fn from_config(dict: Arc<Dictionary>, cfg: &Config, candidate_tx: Arc<ArcSwap<CandidateData>>) -> Self {
        Self::with_theme(
            dict,
            cfg.basic.candidate_count as usize,
            cfg.basic.auto_commit_unique,
            candidate_tx,
            ThemeSnapshot::from_config(cfg),
        )
    }

    fn current_theme(&self) -> ThemeSnapshot {
        self.theme.clone()
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
        let theme = self.current_theme();
        let op = match &t {
            Transition::None => EditSessionOp::NoOp,
            Transition::Commit(text) => {
                log::info!("[TSF] commit text: {text}");
                self.candidate_tx.store(Arc::new(CandidateData::hidden(theme.clone())));
                EditSessionOp::CommitAndReplace { text: text.clone() }
            }
            Transition::Candidates { spelling, candidates, page, total_pages } => {
                if spelling.is_empty() {
                    self.candidate_tx.store(Arc::new(CandidateData::hidden(theme.clone())));
                    return BOOL(1);
                }
                let items: Vec<CandidateItem> = candidates.iter().enumerate().map(|(i, text)| {
                    CandidateItem { label: format!("{}. ", i + 1), text: text.clone() }
                }).collect();
                // 优先使用 Win32 GetCaretPos 获取光标位置（无需 TSF edit cookie），
                // 若失败则尝试 TSF 上下文方法获取。
                let anchor = crate::screen_geometry::get_caret_position_win32(theme.font_size)
                    .or_else(|| crate::screen_geometry::get_cursor_position())
                    .or_else(|| context.and_then(|ctx| get_caret_position(ctx)));
                self.candidate_tx.store(Arc::new(CandidateData::visible(
                    spelling.clone(), items, 0, *page, *total_pages, anchor, theme.clone(),
                )));
                // 候选框显示编码和候选词，不插入 composition 文字（Wubi 输入法典型行为）
                EditSessionOp::NoOp
            }
            Transition::SpellingUpdated(s) => {
                log::debug!("[TSF] spelling={s}");
                let mut data = (**self.candidate_tx.load()).clone();
                data.spelling = s.clone();
                if data.items.is_empty() {
                    data.visible = false;
                }
                self.candidate_tx.store(Arc::new(data));
                // 不插入 composition 文字，候选框会显示当前编码
                EditSessionOp::NoOp
            }
            Transition::Cleared => {
                self.candidate_tx.store(Arc::new(CandidateData::hidden(theme.clone())));
                EditSessionOp::EndComposition { delete_text: true }
            }
            Transition::Passthrough(_) => {
                self.candidate_tx.store(Arc::new(CandidateData::hidden(theme.clone())));
                // Passthrough 不应调度任何 EditSession，避免干扰应用自身按键处理
                return BOOL(0);
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
        if let Err(e) = unsafe { context.RequestEditSession(tid, &edit_session_com, TF_ES_ASYNC | TF_ES_READWRITE) } {
            log::error!("[TSF] RequestEditSession 失败: {e}");
        }
    }

    /// 在 TSF EditSession 回调中执行实际的 composition 操作。
    fn execute_edit_session(&self, ec: u32, op: &EditSessionOp) -> Result<()> {
        let result = match op {
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
        };
        if let Err(ref e) = result {
            log::error!("[TSF] EditSession 操作失败: {e}");
        }
        result
    }

    fn edit_session_composition_update(&self, ec: u32, spelling: &str) -> Result<()> {
        // 使用 get_focus_context 获取的文档 context 创建 composition
        let (edit_ctx, _doc_mgr) = self.get_focus_context()?;
        let mut comp_guard = self.composition.lock();
        if comp_guard.is_none() {
            let ctx_comp: ITfContextComposition = edit_ctx.cast()?;
            let selection = self.get_selection_range(&edit_ctx, ec)?;
            let new_comp = unsafe { ctx_comp.StartComposition(ec, &selection, None) }?;
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
            unsafe { comp.EndComposition(ec) }?;
        } else {
            let (edit_ctx, _doc_mgr) = self.get_focus_context()?;
            let range = self.get_selection_range(&edit_ctx, ec)?;
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
                unsafe { range.SetText(ec, 0, &[]) }?;
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

    /// 注册本 TIP 的自定义 display attribute 类别，使 TSF 能调用
    /// ITfDisplayAttributeProvider::GetDisplayAttributeInfo。
    fn register_display_attribute_categories(&self) {
        use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};

        let category_mgr: Result<ITfCategoryMgr> = unsafe {
            CoCreateInstance(
                &windows::Win32::UI::TextServices::CLSID_TF_CategoryMgr,
                None,
                CLSCTX_INPROC_SERVER,
            )
        };
        match category_mgr {
            Ok(cat) => {
                let guids = [GUID_DISPLAY_ATTR_INPUT, GUID_DISPLAY_ATTR_CONVERTED];
                for guid in &guids {
                    let _ = unsafe {
                        cat.RegisterCategory(
                            &CLSID_TEXT_SERVICE,
                            &GUID_TFCAT_DISPLAYATTRIBUTEPROVIDER,
                            guid,
                        )
                    };
                }
            }
            Err(e) => log::warn!("[TSF] 无法创建 ITfCategoryMgr: {e}，DisplayAttribute 不可用"),
        }
    }

    /// 切换到中文模式。
    pub fn set_chinese_mode(&self) {
        *self.ime_mode.lock() = true;
        log::debug!("[TSF] 切换到中文模式");
    }

    /// 切换到英文模式。
    pub fn set_english_mode(&self) {
        *self.ime_mode.lock() = false;
        self.sm.lock().reset();
        let theme = self.current_theme();
        self.candidate_tx.store(Arc::new(CandidateData::hidden(theme)));
        log::debug!("[TSF] 切换到英文模式");
    }

    /// 反转中英文模式。
    pub fn toggle_ime_mode(&self) {
        let mut mode = self.ime_mode.lock();
        let old = *mode;
        *mode = !*mode;
        if !*mode {
            self.sm.lock().reset();
            log::debug!("[TSF] Exit中文→英文");
        };
        if !old {
            log::debug!("[TSF] 英文→中文");
        }
    }
}

impl ITfTextInputProcessor_Impl for TextService_Impl {
    fn Activate(&self, ptim: Ref<'_, ITfThreadMgr>, tid: u32) -> Result<()> {
        // ITfTextInputProcessorEx::ActivateEx 的退化调用。
        self.ActivateEx(ptim, tid, 0)
    }

    fn Deactivate(&self) -> Result<()> {
        // 删除 composition（如果有）
        if let Some(comp) = self.composition.lock().take() {
            drop(comp);
        }
        self.is_composing.store(false, Ordering::Release);

        // 隐藏候选框并关闭候选框窗口。
        self.candidate_tx.store(Arc::new(CandidateData::hidden(self.current_theme())));
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
        // （thread_mgr 还可能需要用于反注册，先清理 sink 再释放 thread_mgr）
        if let Some(ref tm) = tm_hold {
            // 反注册 ITfThreadFocusSink
            let fc = *self.thread_focus_cookie.lock();
            if fc != 0 {
                if let Ok(source) = tm.cast::<ITfSource>() {
                    let _ = unsafe { source.UnadviseSink(fc) };
                }
            }
        }
        *self.thread_focus_cookie.lock() = 0;

        // 反注册 ITfTextEditSink
        let ec = *self.edit_sink_cookie.lock();
        if ec != 0 {
            if let Some(ctx) = self.edit_sink_context.lock().as_ref() {
                if let Ok(source) = ctx.cast::<ITfSource>() {
                    let _ = unsafe { source.UnadviseSink(ec) };
                }
            }
        }
        *self.edit_sink_cookie.lock() = 0;
        self.edit_sink_context.lock().take();

        *self.thread_mgr.lock() = None;
        *self.focus_doc_mgr.lock() = None;
        drop(state);

        // 释放自我持有的 IUnknown，避免强引用循环。
        self.release_self_unknown();

        // 清理状态机内部缓冲。
        self.sm.lock().reset();

        log::info!("[TSF] TIP deactivated");
        Ok(())
    }
}

impl ITfTextInputProcessorEx_Impl for TextService_Impl {
    fn ActivateEx(&self, ptim: Ref<'_, ITfThreadMgr>, tid: u32, dwflags: u32) -> Result<()> {
        let tm: ITfThreadMgr = match ptim.as_ref() {
            Some(tm) => tm.clone(),
            None => {
                log::error!("[TSF] ActivateEx: ptim 为空");
                return Err(HRESULT(-1).into());
            }
        };
        *self.thread_mgr.lock() = Some(tm.clone());
        // 保存激活标志（如 TF_TMAE_COMLESS）
        *self.activate_flags.lock() = dwflags;

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
        // 在释放 state 之前提取需要的值
        let saved_tid = state.tid;
        let saved_using_kmgr = state.using_keystroke_mgr;
        drop(state);

        // 获取 self_unknown 引用于后续注册（clone 保持引用计数）
        let punk_self_later = self.clone_self_unknown();

        // 启动候选框窗口线程。
        {
            let mut cw = self.candidate_window.lock();
            if cw.is_none() {
                *cw = Some(CandidateWindow::spawn(Arc::clone(&self.candidate_tx)));
            }
        }

        // 注册自定义 DisplayAttribute 类别。
        self.register_display_attribute_categories();

        // 注册 PreservedKey（Shift 切换中英文）
        if saved_tid != 0 && saved_using_kmgr {
            if let Ok(kmgr) = tm.cast::<ITfKeystrokeMgr>() {
                let shift_key = TF_PRESERVEDKEY {
                    uVKey: VK_SHIFT.0 as u32,
                    uModifiers: TF_MOD_ON_KEYUP,
                };
                let desc: Vec<u16> = "中英文切换 (Shift)\0".encode_utf16().collect();
                let _ = unsafe { kmgr.PreserveKey(saved_tid, &GUID_PRESERVED_SHIFT, &shift_key, &desc) };
            }
        }

        // 注册 ITfThreadFocusSink（线程焦点变化通知）
        if let Some(ref punk) = punk_self_later {
            if let Ok(source) = tm.cast::<ITfSource>() {
                let iid_thread_focus = <ITfThreadFocusSink as Interface>::IID;
                if let Ok(cookie) = unsafe { source.AdviseSink(&iid_thread_focus, punk) } {
                    *self.thread_focus_cookie.lock() = cookie;
                    log::debug!("[TSF] ITfThreadFocusSink 注册成功");
                }
            }
        }

        // 注册 ITfTextEditSink（监听外部文本编辑）
        if let Some(ref punk) = punk_self_later {
            if let Ok(doc_mgr) = unsafe { tm.GetFocus() } {
                if let Ok(ctx) = unsafe { doc_mgr.GetBase() } {
                    if let Ok(source) = ctx.cast::<ITfSource>() {
                        let iid_edit = <ITfTextEditSink as Interface>::IID;
                        if let Ok(cookie) = unsafe { source.AdviseSink(&iid_edit, punk) } {
                            *self.edit_sink_cookie.lock() = cookie;
                            *self.edit_sink_context.lock() = Some(ctx);
                            log::debug!("[TSF] ITfTextEditSink 注册成功");
                        }
                    }
                }
            }
        }

        log::info!("[TSF] TIP activated (tid={tid})");
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

impl ITfDisplayAttributeProvider_Impl for TextService_Impl {
    fn EnumDisplayAttributeInfo(&self) -> Result<IEnumTfDisplayAttributeInfo> {
        // windows-rs 0.61 未导出 IEnumTfDisplayAttributeInfo_Impl trait，
        // 无法实现自定义枚举器。TSF 通过 RegisterCategory 注册后仍可通过
        // GetDisplayAttributeInfo 直接获取属性信息。
        Err(windows::core::Error::from(HRESULT(-1)))
    }

    fn GetDisplayAttributeInfo(&self, guid: *const GUID) -> Result<ITfDisplayAttributeInfo> {
        let guid_safe = unsafe { &*guid };
        if *guid_safe == GUID_DISPLAY_ATTR_INPUT {
            Ok(make_display_attr_info(GUID_DISPLAY_ATTR_INPUT, "编码输入态", attr_input()))
        } else if *guid_safe == GUID_DISPLAY_ATTR_CONVERTED {
            Ok(make_display_attr_info(GUID_DISPLAY_ATTR_CONVERTED, "候选态", attr_converted()))
        } else {
            Err(HRESULT(-1).into())
        }
    }
}

impl ITfThreadFocusSink_Impl for TextService_Impl {
    fn OnSetThreadFocus(&self) -> Result<()> {
        log::debug!("[TSF] 线程焦点获得");
        Ok(())
    }

    fn OnKillThreadFocus(&self) -> Result<()> {
        log::debug!("[TSF] 线程焦点丢失");
        // 清理输入状态
        self.sm.lock().reset();
        let theme = self.current_theme();
        self.candidate_tx.store(Arc::new(CandidateData::hidden(theme)));
        if self.is_composing.load(Ordering::Acquire) {
            // 异步清除 composition（需要有效的 context，Deactivate 会兜底）
            if let Some(tm) = self.thread_mgr.lock().as_ref() {
                if let Ok(doc_mgr) = unsafe { tm.GetFocus() } {
                    if let Ok(ctx) = unsafe { doc_mgr.GetBase() } {
                        self.schedule_edit_session(EditSessionOp::EndComposition { delete_text: false }, &ctx);
                    }
                }
            }
        }
        Ok(())
    }
}

impl ITfTextEditSink_Impl for TextService_Impl {
    fn OnEndEdit(&self, _pic: Ref<'_, ITfContext>, _ecreadonly: u32, _peditrecord: Ref<'_, ITfEditRecord>) -> Result<()> {
        // 如果 composition 被外部编辑破坏，ITfCompositionSink::OnCompositionTerminated
        // 会收到通知并清理状态。此处仅做日志监控。
        if self.is_composing.load(Ordering::Acquire) {
            log::trace!("[TSF] OnEndEdit — composing 中检测到文本变更");
        }
        Ok(())
    }
}

// ── Phase 4: ITfFunction + ITfFnGetPreferredTouchKeyboardLayout ──

impl ITfFunction_Impl for TextService_Impl {
    fn GetDisplayName(&self) -> Result<windows::core::BSTR> {
        Ok(windows::core::BSTR::from("MyWubi"))
    }
}

impl ITfFnGetPreferredTouchKeyboardLayout_Impl for TextService_Impl {
    fn GetLayout(&self, ptkblayouttype: *mut TKBLayoutType, pwpreferredlayoutid: *const u16) -> Result<()> {
        unsafe {
            *ptkblayouttype = TKBLT_OPTIMIZED;
            *(pwpreferredlayoutid as *mut u16) = TKBL_OPT_SIMPLIFIED_CHINESE_PINYIN as u16;
        }
        Ok(())
    }
}

impl ITfFunctionProvider_Impl for TextService_Impl {
    fn GetType(&self) -> Result<GUID> {
        Ok(crate::guids::CLSID_TEXT_SERVICE)
    }

    fn GetDescription(&self) -> Result<windows::core::BSTR> {
        Ok(windows::core::BSTR::from("MyWubi 形码输入法"))
    }

    fn GetFunction(&self, rguid: *const windows_core::GUID, riid: *const windows_core::GUID) -> Result<windows_core::IUnknown> {
        let guid = unsafe { &*rguid };
        let iid = unsafe { &*riid };
        // 标准 TSF 中, 通过查询 IID_ITfFnGetPreferredTouchKeyboardLayout
        // 来返回触摸键盘布局接口。TextService 同时实现了该接口。
        if *guid == GUID::zeroed()
            && *iid == <ITfFnGetPreferredTouchKeyboardLayout as Interface>::IID
        {
            if let Some(unk) = self.clone_self_unknown() {
                let result: Result<ITfFnGetPreferredTouchKeyboardLayout> = unk.cast();
                if let Ok(layout) = result {
                    return Ok(layout.into());
                }
            }
        }
        Err(HRESULT(-1).into())
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
        // 英文模式：所有按键透传
        if !*self.ime_mode.lock() {
            return Ok(BOOL(0));
        }

        let _ = lparam;
        let spelling_empty = self.sm.lock().spelling().is_empty();
        Ok(BOOL(should_intercept_test_key(
            key_filter::translate(wparam.0 as usize, lparam.0 as isize),
            spelling_empty,
        ) as i32))
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

        let spelling_empty = self.sm.lock().spelling().is_empty();
        if !should_intercept_test_key(Some(event.clone()), spelling_empty) {
            return Ok(BOOL(0));
        }

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
        rguid: *const GUID,
    ) -> Result<BOOL> {
        let guid = unsafe { &*rguid };
        if *guid == GUID_PRESERVED_SHIFT {
            self.toggle_ime_mode();
            Ok(BOOL(1))
        } else {
            Ok(BOOL(0))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_snapshot_comes_from_config_appearance() {
        let mut cfg = Config::default();
        cfg.appearance.font_size = 18;
        cfg.appearance.primary_color = 0xFF102030;
        cfg.appearance.background_color = 0xFFF0E0D0;
        cfg.appearance.highlight_color = 0xFFABCDEF;

        let theme = ThemeSnapshot::from_config(&cfg);

        assert_eq!(theme.font_size, 18);
        assert_eq!(theme.primary_color, 0xFF102030);
        assert_eq!(theme.background_color, 0xFFF0E0D0);
        assert_eq!(theme.highlight_color, 0xFFABCDEF);
    }

    #[test]
    fn idle_backspace_is_not_intercepted_in_test_keydown() {
        assert!(!should_intercept_test_key(Some(InputEvent::Backspace), true));
    }

    #[test]
    fn composing_backspace_is_intercepted_in_test_keydown() {
        assert!(should_intercept_test_key(Some(InputEvent::Backspace), false));
    }

    #[test]
    fn character_input_is_still_intercepted() {
        assert!(should_intercept_test_key(Some(InputEvent::Char('g')), true));
    }
}