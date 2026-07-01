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
use std::ffi::c_void;
use std::mem;
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use windows::core::{implement, w, Interface, Ref, Result, BOOL, GUID, HRESULT};
use windows::Win32::Foundation::{
    COLORREF, E_FAIL, E_INVALIDARG, LPARAM, POINT, RECT, S_FALSE, WPARAM,
};
use windows::Win32::Graphics::Gdi::{
    CreateBitmap, CreateCompatibleDC, CreateDIBSection, CreateFontW, DeleteDC, DeleteObject,
    DrawTextW, GdiFlush, SelectObject, SetBkMode, SetTextColor, ANTIALIASED_QUALITY, BI_RGB,
    BITMAPINFO, BITMAPINFOHEADER, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DEFAULT_PITCH,
    DIB_RGB_COLORS, DT_CENTER, DT_SINGLELINE, DT_VCENTER, FF_DONTCARE, FW_NORMAL, HGDIOBJ,
    OUT_DEFAULT_PRECIS, RGBQUAD, TRANSPARENT,
};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use windows::Win32::System::Ole::{
    CONNECT_E_ADVISELIMIT, CONNECT_E_CANNOTCONNECT, CONNECT_E_NOCONNECTION,
};
use windows::Win32::System::Variant::{VARIANT, VARIANT_0, VARIANT_0_0, VARIANT_0_0_0, VT_I4};
use windows::Win32::UI::TextServices::{
    CLSID_TF_LangBarItemMgr, GUID_PROP_ATTRIBUTE, IEnumTfDisplayAttributeInfo,
    IEnumTfDisplayAttributeInfo_Impl, ITfCategoryMgr, ITfCompartment, ITfCompartmentEventSink,
    ITfCompartmentEventSink_Impl,
    ITfCompartmentMgr, ITfComposition, ITfCompositionSink, ITfCompositionSink_Impl, ITfContext,
    ITfContextComposition, ITfDisplayAttributeInfo, ITfDisplayAttributeInfo_Impl,
    ITfDisplayAttributeProvider, ITfDisplayAttributeProvider_Impl,
    ITfDocumentMgr, ITfEditSession, ITfEditSession_Impl, ITfEditRecord,
    ITfFnGetPreferredTouchKeyboardLayout, ITfFnGetPreferredTouchKeyboardLayout_Impl,
    ITfFunction, ITfFunction_Impl, ITfFunctionProvider, ITfFunctionProvider_Impl,
    ITfKeyEventSink, ITfKeyEventSink_Impl, ITfKeystrokeMgr, ITfLangBarItem,
    ITfLangBarItemButton, ITfLangBarItemButton_Impl, ITfLangBarItem_Impl, ITfLangBarItemMgr,
    ITfLangBarItemSink, ITfMenu, ITfProperty, ITfRange, ITfSource, ITfSource_Impl,
    ITfTextEditSink, ITfTextEditSink_Impl,
    ITfTextInputProcessor, ITfTextInputProcessorEx, ITfTextInputProcessorEx_Impl,
    ITfTextInputProcessor_Impl, ITfThreadFocusSink, ITfThreadFocusSink_Impl,
    ITfThreadMgr,
    ITfThreadMgrEventSink, ITfThreadMgrEventSink_Impl,
    TF_DA_COLOR, TF_DA_COLOR_0,
    TF_DISPLAYATTRIBUTE, TF_ES_ASYNC, TF_ES_READWRITE, TF_DEFAULT_SELECTION, TF_LS_DOT,
    TF_LS_SOLID, TF_SELECTION, TF_CT_COLORREF, GUID_TFCAT_DISPLAYATTRIBUTEPROVIDER,
    GUID_COMPARTMENT_KEYBOARD_OPENCLOSE, GUID_LBI_INPUTMODE, TF_LANGBARITEMINFO, TF_LBI_CLK_LEFT,
    TF_LBI_ICON, TF_LBI_STATUS, TF_LBI_STATUS_HIDDEN, TF_LBI_STYLE_BTN_BUTTON,
    TF_LBI_STYLE_SHOWNINTRAY, TF_LBI_TEXT, TF_LBI_TOOLTIP, TF_MOD_ON_KEYUP, TF_PRESERVEDKEY,
    TKBLayoutType, TfLBIClick, TKBLT_OPTIMIZED, TKBL_OPT_SIMPLIFIED_CHINESE_PINYIN,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_LSHIFT, VK_RSHIFT, VK_SHIFT};
use windows::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, HICON, ICONINFO};

use arc_swap::ArcSwap;

use crate::candidate_data::{CandidateData, CandidateItem, ScreenPoint, ThemeSnapshot};
use crate::guids::CLSID_TEXT_SERVICE;
use crate::candidate_window::CandidateWindow;
use crate::key_filter;
use crate::screen_geometry::get_range_position;
use crate::RuntimeSnapshot;

// ── 类型定义 ────────────────────────────────────────────────────────

/// 编码输入态 display attribute 的 GUID.
const GUID_DISPLAY_ATTR_INPUT: GUID = GUID::from_u128(0x8a2e3b4c_1d5f_4a7b_9e6c_3f8d2b1a5e7d);
/// 有候选态 display attribute 的 GUID.
const GUID_DISPLAY_ATTR_CONVERTED: GUID = GUID::from_u128(0x6b1c8d9e_2f3a_4c5b_8d7e_1a2b3c4d5e6f);
/// PreservedKey（Shift 切换）的 GUID.
const GUID_PRESERVED_SHIFT: GUID = GUID::from_u128(0x1a2b3c4d_5e6f_7a8b_9c0d_1e2f3a4b5c6d);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DisplayAttrKind {
    Input,
    Converted,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct ShiftKeyToggleTracker {
    used_with_other_key: bool,
}

impl ShiftKeyToggleTracker {
    fn on_key_down(&mut self, vk: u16, shift_pressed: bool) {
        if is_shift_vk(vk) {
            self.used_with_other_key = false;
        } else if shift_pressed {
            self.used_with_other_key = true;
        }
    }

    fn consume_preserved_key(&mut self) -> bool {
        let should_toggle = !self.used_with_other_key;
        self.used_with_other_key = false;
        should_toggle
    }
}

fn is_shift_vk(vk: u16) -> bool {
    matches!(vk, v if v == VK_SHIFT.0 || v == VK_LSHIFT.0 || v == VK_RSHIFT.0)
}

fn lang_bar_display(chinese: bool) -> (&'static str, &'static str) {
    if chinese {
        ("中", "中文模式")
    } else {
        ("英", "英文模式")
    }
}

fn fixed_utf16<const N: usize>(text: &str) -> [u16; N] {
    let mut output = [0; N];
    if N > 0 {
        for (slot, value) in output[..N - 1].iter_mut().zip(text.encode_utf16()) {
            *slot = value;
        }
    }
    output
}

fn create_lang_bar_icon(text: &str) -> Result<HICON> {
    const SIZE: i32 = 16;

    // SAFETY: Every GDI handle created here is restored/deleted before return.
    // CreateIconIndirect copies both bitmaps, so the returned HICON owns no
    // reference to the temporary color and mask bitmaps.
    unsafe {
        let dc = CreateCompatibleDC(None);
        if dc.is_invalid() {
            return Err(E_FAIL.into());
        }

        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: SIZE,
                biHeight: -SIZE,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            bmiColors: [RGBQUAD::default(); 1],
        };
        let mut bits: *mut c_void = ptr::null_mut();
        let color = match CreateDIBSection(Some(dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
            Ok(bitmap) if !bitmap.is_invalid() && !bits.is_null() => bitmap,
            Ok(bitmap) => {
                if !bitmap.is_invalid() {
                    let _ = DeleteObject(HGDIOBJ(bitmap.0));
                }
                let _ = DeleteDC(dc);
                return Err(E_FAIL.into());
            }
            Err(error) => {
                let _ = DeleteDC(dc);
                return Err(error);
            }
        };
        let old_bitmap = SelectObject(dc, HGDIOBJ(color.0));

        let font = CreateFontW(
            -14, 0, 0, 0, FW_NORMAL.0 as i32, 0, 0, 0,
            DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS,
            ANTIALIASED_QUALITY, (DEFAULT_PITCH.0 | FF_DONTCARE.0).into(),
            w!("Microsoft YaHei UI"),
        );
        if font.is_invalid() {
            let _ = SelectObject(dc, old_bitmap);
            let _ = DeleteObject(HGDIOBJ(color.0));
            let _ = DeleteDC(dc);
            return Err(E_FAIL.into());
        }
        let old_font = SelectObject(dc, HGDIOBJ(font.0));
        let _ = SetBkMode(dc, TRANSPARENT);
        let _ = SetTextColor(dc, COLORREF(0x00ff0000));

        let pixels = std::slice::from_raw_parts_mut(bits.cast::<u8>(), (SIZE * SIZE * 4) as usize);
        pixels.fill(0);
        let mut utf16: Vec<u16> = text.encode_utf16().collect();
        let mut rect = RECT { left: 0, top: 0, right: SIZE, bottom: SIZE };
        let drawn = DrawTextW(dc, &mut utf16, &mut rect, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
        let _ = GdiFlush();

        for pixel in pixels.chunks_exact_mut(4) {
            pixel[3] = pixel[0].max(pixel[1]).max(pixel[2]);
        }

        let mask_bits = [0u8; 32];
        let mask = CreateBitmap(SIZE, SIZE, 1, 1, Some(mask_bits.as_ptr().cast()));
        let icon = if drawn != 0 && !mask.is_invalid() {
            CreateIconIndirect(&ICONINFO {
                fIcon: true.into(),
                hbmColor: color,
                hbmMask: mask,
                ..Default::default()
            })
        } else {
            Err(E_FAIL.into())
        };

        let _ = SelectObject(dc, old_font);
        let _ = SelectObject(dc, old_bitmap);
        let _ = DeleteObject(HGDIOBJ(font.0));
        let _ = DeleteObject(HGDIOBJ(color.0));
        if !mask.is_invalid() {
            let _ = DeleteObject(HGDIOBJ(mask.0));
        }
        let _ = DeleteDC(dc);
        icon
    }
}

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

fn make_display_attr_infos() -> Vec<ITfDisplayAttributeInfo> {
    vec![
        make_display_attr_info(GUID_DISPLAY_ATTR_INPUT, "编码输入态", attr_input()),
        make_display_attr_info(GUID_DISPLAY_ATTR_CONVERTED, "候选态", attr_converted()),
    ]
}

#[implement(IEnumTfDisplayAttributeInfo)]
struct DisplayAttributeInfoEnum {
    items: Vec<ITfDisplayAttributeInfo>,
    index: Mutex<usize>,
}

impl IEnumTfDisplayAttributeInfo_Impl for DisplayAttributeInfoEnum_Impl {
    fn Clone(&self) -> Result<IEnumTfDisplayAttributeInfo> {
        let cloned = DisplayAttributeInfoEnum {
            items: self.items.clone(),
            index: Mutex::new(*self.index.lock()),
        };
        Ok(windows::core::ComObject::new(cloned).to_interface())
    }

    fn Next(
        &self,
        ulcount: u32,
        rginfo: *mut Option<ITfDisplayAttributeInfo>,
        pcfetched: *mut u32,
    ) -> Result<()> {
        if rginfo.is_null() || (ulcount != 1 && pcfetched.is_null()) {
            return Err(E_INVALIDARG.into());
        }

        let requested = ulcount as usize;
        let mut index = self.index.lock();
        let start = *index;
        let fetched = self.items.len().saturating_sub(start).min(requested);

        for offset in 0..fetched {
            unsafe { rginfo.add(offset).write(Some(self.items[start + offset].clone())); }
        }
        for offset in fetched..requested {
            unsafe { rginfo.add(offset).write(None); }
        }

        *index += fetched;
        if !pcfetched.is_null() {
            unsafe { *pcfetched = fetched as u32; }
        }

        if fetched == requested {
            Ok(())
        } else {
            Err(S_FALSE.into())
        }
    }

    fn Reset(&self) -> Result<()> {
        *self.index.lock() = 0;
        Ok(())
    }

    fn Skip(&self, ulcount: u32) -> Result<()> {
        let requested = ulcount as usize;
        let mut index = self.index.lock();
        let skipped = self.items.len().saturating_sub(*index).min(requested);
        *index += skipped;
        if skipped == requested {
            Ok(())
        } else {
            Err(S_FALSE.into())
        }
    }
}

fn make_i32_variant(value: i32) -> VARIANT {
    VARIANT {
        Anonymous: VARIANT_0 {
            Anonymous: std::mem::ManuallyDrop::new(VARIANT_0_0 {
                vt: VT_I4,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: VARIANT_0_0_0 { lVal: value },
            }),
        },
    }
}

/// 异步 ITfEditSession 的操作描述。由 apply_transition 根据 Transition
/// 构建，由 EditSession::DoEditSession 消费。
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
enum EditSessionOp {
    /// 创建或更新 composition range 上的编码文本。
    CompositionUpdate { spelling: String, attr: DisplayAttrKind },
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
    /// 指向 `_service_owner` 持有的 TextService COM 对象。
    service_ptr: *const TextService,
    /// 保持 TextService COM 对象存活直到异步 EditSession 完成。
    _service_owner: windows::core::IUnknown,
}

impl ITfEditSession_Impl for EditSession_Impl {
    fn DoEditSession(&self, ec: u32) -> Result<()> {
        // SAFETY: _service_owner keeps the COM allocation containing
        // service_ptr alive for the entire EditSession lifetime.
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
             ITfFunction, ITfFnGetPreferredTouchKeyboardLayout,
             ITfCompartmentEventSink, ITfLangBarItemButton,
             ITfSource)]
pub struct TextService {
    /// 跨平台核心状态机。
    sm: Mutex<StateMachine>,
    /// 可热替换的运行时快照（配置 + 码表 + 路径）。
    runtime: Arc<ArcSwap<RuntimeSnapshot>>,
    /// 当前实例已同步到的运行时版本。
    runtime_revision: AtomicU64,
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
    theme: Mutex<ThemeSnapshot>,
    /// 全局配置（用于翻页热键等动态映射）。
    cfg: Mutex<Config>,
    /// ── Phase 1 新增字段 ──
    /// 当前激活的 composition 对象。
    composition: Mutex<Option<ITfComposition>>,
    /// 编码输入态 display attribute 对应的 GUID atom。
    ga_input: Mutex<u32>,
    /// 候选态 display attribute 对应的 GUID atom。
    ga_converted: Mutex<u32>,
    /// 当前是否处于 composing 状态。
    is_composing: AtomicBool,
    /// ── Phase 2 新增字段 ──
    /// ITfTextInputProcessorEx 的激活标志（TF_TMAE_COMLESS 等）。
    activate_flags: Mutex<u32>,
    /// true=中文模式（拦截字母键），false=英文模式（Passthrough）。
    ime_mode: Mutex<bool>,
    /// 跟踪本轮 Shift 是否与其他按键组合使用，避免组合键误触发中英文切换。
    shift_toggle_tracker: Mutex<ShiftKeyToggleTracker>,
    /// ── Phase 3 新增字段 ──
    /// ITfThreadFocusSink 的 AdviseSink cookie。
    thread_focus_cookie: Mutex<u32>,
    /// 注册了 ITfTextEditSink 的上下文（用于反注册）。
    edit_sink_context: Mutex<Option<ITfContext>>,
    /// ITfTextEditSink 的 AdviseSink cookie。
    edit_sink_cookie: Mutex<u32>,
    /// 语言栏按钮更新通知 sink。
    lang_bar_sink: Mutex<Option<ITfLangBarItemSink>>,
    /// 语言栏按钮是否可见。
    lang_bar_visible: AtomicBool,
    /// 语言栏按钮是否已注册到系统。
    lang_bar_registered: AtomicBool,
    /// 键盘开关 compartment 的 AdviseSink cookie。
    compartment_sink_cookie: Mutex<Option<u32>>,
}

fn should_intercept_test_key(event: Option<InputEvent>, spelling_empty: bool) -> bool {
    match event {
        Some(InputEvent::Char(_)) => true,
        Some(_) => !spelling_empty,
        None => false,
    }
}

fn resolve_candidate_anchor(current_anchor: Option<ScreenPoint>, font_size: u16) -> Option<ScreenPoint> {
    current_anchor
        .or_else(|| crate::screen_geometry::get_caret_position_win32(font_size))
        .or_else(|| crate::screen_geometry::get_cursor_position())
}

fn build_spelling_only_candidate_data(
    mut current: CandidateData,
    spelling: String,
    anchor: Option<ScreenPoint>,
) -> CandidateData {
    current.visible = !spelling.is_empty();
    current.spelling = spelling;
    current.items.clear();
    current.highlighted = 0;
    current.page = 0;
    current.total_pages = 0;
    current.anchor = anchor.or(current.anchor);
    current
}

fn update_candidate_anchor(
    mut current: CandidateData,
    anchor: Option<ScreenPoint>,
) -> CandidateData {
    if let Some(anchor) = anchor {
        current.anchor = Some(anchor);
    }
    current
}

fn transition_to_edit_session_op(transition: &Transition) -> EditSessionOp {
    match transition {
        Transition::None => EditSessionOp::NoOp,
        Transition::Commit(text) => EditSessionOp::CommitAndReplace { text: text.clone() },
        Transition::Candidates { spelling, .. } if !spelling.is_empty() => {
            EditSessionOp::CompositionUpdate {
                spelling: spelling.clone(),
                attr: DisplayAttrKind::Converted,
            }
        }
        Transition::SpellingUpdated(spelling) if !spelling.is_empty() => {
            EditSessionOp::CompositionUpdate {
                spelling: spelling.clone(),
                attr: DisplayAttrKind::Input,
            }
        }
        Transition::Cleared => EditSessionOp::EndComposition { delete_text: true },
        Transition::Candidates { .. } | Transition::SpellingUpdated(_) | Transition::Passthrough(_) => {
            EditSessionOp::NoOp
        }
    }
}

impl TextService {
    /// 创建一个绑定码表与状态机的文本服务实例（不带 back-pointer）。
    pub fn new(dict: Arc<Dictionary>, page_size: usize, auto_commit_unique: bool, candidate_tx: Arc<ArcSwap<CandidateData>>) -> Self {
        Self::with_theme(
            dict,
            page_size,
            auto_commit_unique,
            core_engine::config::PunctuationMode::BufferedCommit,
            candidate_tx,
            ThemeSnapshot::default(),
            Config::default(),
        )
    }

    fn with_theme(
        dict: Arc<Dictionary>,
        page_size: usize,
        auto_commit_unique: bool,
        punctuation_mode: core_engine::config::PunctuationMode,
        candidate_tx: Arc<ArcSwap<CandidateData>>,
        theme: ThemeSnapshot,
        cfg: Config,
    ) -> Self {
        let sm = StateMachine::with_behavior(dict, page_size, auto_commit_unique, punctuation_mode);
        Self {
            sm: Mutex::new(sm),
            runtime: Arc::new(ArcSwap::from_pointee(RuntimeSnapshot {
                revision: 0,
                dict: Dictionary::from_entries(Vec::new(), None, Default::default())
                    .expect("empty dictionary should construct"),
                config: cfg.clone(),
                config_path: std::path::PathBuf::from("config.toml"),
                system_table_path: std::path::PathBuf::from("tables/wubi86.dict"),
            })),
            runtime_revision: AtomicU64::new(0),
            thread_mgr: Mutex::new(None),
            cookies: Mutex::new(SinkState::default()),
            focus_doc_mgr: Mutex::new(None),
            self_unknown: Mutex::new(None),
            candidate_tx,
            candidate_window: Mutex::new(None),
            theme: Mutex::new(theme),
            cfg: Mutex::new(cfg),
            edit_sink_context: Mutex::new(None),
            edit_sink_cookie: Mutex::new(0),
            composition: Mutex::new(None),
            ga_input: Mutex::new(0),
            ga_converted: Mutex::new(0),
            is_composing: AtomicBool::new(false),
            activate_flags: Mutex::new(0),
            ime_mode: Mutex::new(true),
            shift_toggle_tracker: Mutex::new(ShiftKeyToggleTracker::default()),
            thread_focus_cookie: Mutex::new(0),
            lang_bar_sink: Mutex::new(None),
            lang_bar_visible: AtomicBool::new(true),
            lang_bar_registered: AtomicBool::new(false),
            compartment_sink_cookie: Mutex::new(None),
        }
    }

    /// 从 [`Config`] 选择默认参数。
    pub fn from_config(dict: Arc<Dictionary>, cfg: &Config, candidate_tx: Arc<ArcSwap<CandidateData>>) -> Self {
        Self::with_theme(
            dict,
            cfg.basic.candidate_count as usize,
            cfg.basic.auto_commit_unique,
            cfg.basic.punctuation_mode,
            candidate_tx,
            ThemeSnapshot::from_config(cfg),
            cfg.clone(),
        )
    }

    pub(crate) fn from_runtime(
        runtime: Arc<ArcSwap<RuntimeSnapshot>>,
        candidate_tx: Arc<ArcSwap<CandidateData>>,
    ) -> Self {
        let snapshot = runtime.load();
        let cfg = snapshot.config.clone();
        let revision = snapshot.revision;
        let theme = ThemeSnapshot::from_config(&cfg);
        let sm = StateMachine::with_behavior(
            Arc::clone(&snapshot.dict),
            cfg.basic.candidate_count as usize,
            cfg.basic.auto_commit_unique,
            cfg.basic.punctuation_mode,
        );
        drop(snapshot);

        Self {
            sm: Mutex::new(sm),
            runtime,
            runtime_revision: AtomicU64::new(revision),
            thread_mgr: Mutex::new(None),
            cookies: Mutex::new(SinkState::default()),
            focus_doc_mgr: Mutex::new(None),
            self_unknown: Mutex::new(None),
            candidate_tx,
            candidate_window: Mutex::new(None),
            theme: Mutex::new(theme),
            cfg: Mutex::new(cfg),
            edit_sink_context: Mutex::new(None),
            edit_sink_cookie: Mutex::new(0),
            composition: Mutex::new(None),
            ga_input: Mutex::new(0),
            ga_converted: Mutex::new(0),
            is_composing: AtomicBool::new(false),
            activate_flags: Mutex::new(0),
            ime_mode: Mutex::new(true),
            shift_toggle_tracker: Mutex::new(ShiftKeyToggleTracker::default()),
            thread_focus_cookie: Mutex::new(0),
            lang_bar_sink: Mutex::new(None),
            lang_bar_visible: AtomicBool::new(true),
            lang_bar_registered: AtomicBool::new(false),
            compartment_sink_cookie: Mutex::new(None),
        }
    }

    /// 仅供 IClassFactory 内部注入 self 弱强引用（见 [`crate::factory::TextServiceFactory`])。
    pub(crate) fn set_self_unknown(&self, unk: windows::core::IUnknown) {
        *self.self_unknown.lock() = Some(unk);
    }

    /// 内部：取一份 IUnknown 副本（已 AddRef），用于 AdviseSink 注册。
    fn clone_self_unknown(&self) -> Option<windows::core::IUnknown> {
        self.self_unknown.lock().clone()
    }

    fn theme_snapshot(&self) -> ThemeSnapshot {
        self.theme.lock().clone()
    }

    fn config_snapshot(&self) -> Config {
        self.cfg.lock().clone()
    }

    fn sync_runtime_if_needed(&self) {
        let snapshot = self.runtime.load();
        if self.runtime_revision.load(Ordering::Acquire) == snapshot.revision {
            return;
        }

        let cfg = snapshot.config.clone();
        let theme = ThemeSnapshot::from_config(&cfg);
        let sm = StateMachine::with_behavior(
            Arc::clone(&snapshot.dict),
            cfg.basic.candidate_count as usize,
            cfg.basic.auto_commit_unique,
            cfg.basic.punctuation_mode,
        );
        let revision = snapshot.revision;
        drop(snapshot);

        *self.sm.lock() = sm;
        *self.cfg.lock() = cfg;
        *self.theme.lock() = theme.clone();
        self.candidate_tx
            .store(Arc::new(CandidateData::hidden(theme)));
        self.runtime_revision.store(revision, Ordering::Release);
        log::info!("[TSF] 已同步最新配置/码表 revision={revision}");
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
        let theme = self.theme_snapshot();
        match &t {
            Transition::None => {}
            Transition::Commit(text) => {
                log::info!("[TSF] commit text: {text}");
                self.candidate_tx.store(Arc::new(CandidateData::hidden(theme.clone())));
            }
            Transition::Candidates { spelling, candidates, page, total_pages } => {
                if spelling.is_empty() {
                    self.candidate_tx.store(Arc::new(CandidateData::hidden(theme.clone())));
                } else {
                    let current_anchor = self.candidate_tx.load().anchor;
                    let items: Vec<CandidateItem> = candidates.iter().enumerate().map(|(i, text)| {
                        CandidateItem { label: format!("{}. ", i + 1), text: text.clone() }
                    }).collect();
                    let anchor = resolve_candidate_anchor(current_anchor, theme.font_size);
                    self.candidate_tx.store(Arc::new(CandidateData::visible(
                        spelling.clone(), items, 0, *page, *total_pages, anchor, theme.clone(),
                    )));
                }
            }
            Transition::SpellingUpdated(s) => {
                log::debug!("[TSF] spelling={s}");
                let current = self.candidate_tx.load();
                let data = build_spelling_only_candidate_data(
                    (**current).clone(),
                    s.clone(),
                    resolve_candidate_anchor(current.anchor, theme.font_size),
                );
                self.candidate_tx.store(Arc::new(data));
            }
            Transition::Cleared => {
                self.candidate_tx.store(Arc::new(CandidateData::hidden(theme.clone())));
            }
            Transition::Passthrough(_) => {
                self.candidate_tx.store(Arc::new(CandidateData::hidden(theme.clone())));
            }
        }

        let op = transition_to_edit_session_op(&t);

        // 异步调度 EditSession 更新 composition（候选框已同步更新）
        if !matches!(op, EditSessionOp::NoOp) {
            if let Some(ctx) = context {
                self.schedule_edit_session(op, ctx);
            }
        }

        match t {
            Transition::Passthrough(_) => BOOL(0),
            _ => BOOL(1),
        }
    }

    /// 异步调度一条 composition 操作到 TSF 编辑会话。
    fn schedule_edit_session(&self, op: EditSessionOp, context: &ITfContext) {
        let Some(service_owner) = self.clone_self_unknown() else {
            log::warn!("[TSF] schedule_edit_session: self_unknown 缺失，跳过");
            return;
        };
        let tid = self.cookies.lock().tid;
        if tid == 0 {
            log::warn!("[TSF] schedule_edit_session: tid==0，跳过");
            return;
        }
        let edit_session = EditSession {
            op,
            service_ptr: self as *const TextService,
            _service_owner: service_owner,
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
            EditSessionOp::CompositionUpdate { spelling, attr } => {
                self.edit_session_composition_update(ec, spelling, *attr)
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

    fn edit_session_composition_update(
        &self,
        ec: u32,
        spelling: &str,
        attr: DisplayAttrKind,
    ) -> Result<()> {
        // 使用 get_focus_context 获取的文档 context 创建 composition
        let (edit_ctx, _doc_mgr) = self.get_focus_context()?;
        let mut comp_guard = self.composition.lock();
        if comp_guard.is_none() {
            let ctx_comp: ITfContextComposition = edit_ctx.cast()?;
            let selection = self.get_selection_range(&edit_ctx, ec)?;
            let sink = self
                .clone_self_unknown()
                .ok_or_else(|| windows::core::Error::from(HRESULT(-1)))?
                .cast::<ITfCompositionSink>()?;
            let new_comp = unsafe { ctx_comp.StartComposition(ec, &selection, &sink) }?;
            *comp_guard = Some(new_comp);
            self.is_composing.store(true, Ordering::Release);
        }
        if let Some(ref comp) = *comp_guard {
            let range: ITfRange = unsafe { comp.GetRange()? };
            let wide: Vec<u16> = spelling.encode_utf16().collect();
            unsafe { range.SetText(ec, 0, &wide) }?;
            self.set_range_display_attribute(&edit_ctx, ec, &range, attr)?;

            let current = self.candidate_tx.load();
            let updated = update_candidate_anchor(
                (**current).clone(),
                get_range_position(&edit_ctx, ec, &range),
            );
            self.candidate_tx.store(Arc::new(updated));
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

    /// 获取当前焦点文档管理器的顶 context.
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

    /// 获取当前光标处的文本 range.
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

    fn initialize_display_attribute_atoms(&self) {
        use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};

        *self.ga_input.lock() = 0;
        *self.ga_converted.lock() = 0;

        let category_mgr: Result<ITfCategoryMgr> = unsafe {
            CoCreateInstance(
                &windows::Win32::UI::TextServices::CLSID_TF_CategoryMgr,
                None,
                CLSCTX_INPROC_SERVER,
            )
        };

        let Ok(cat) = category_mgr else {
            log::warn!("[TSF] 无法初始化 DisplayAttribute GUID atom");
            return;
        };

        match unsafe { cat.RegisterGUID(&GUID_DISPLAY_ATTR_INPUT) } {
            Ok(atom) => *self.ga_input.lock() = atom,
            Err(e) => log::warn!("[TSF] 注册编码态 GUID atom 失败: {e}"),
        }
        match unsafe { cat.RegisterGUID(&GUID_DISPLAY_ATTR_CONVERTED) } {
            Ok(atom) => *self.ga_converted.lock() = atom,
            Err(e) => log::warn!("[TSF] 注册候选态 GUID atom 失败: {e}"),
        }
    }

    fn display_attr_atom(&self, attr: DisplayAttrKind) -> Option<u32> {
        let atom = match attr {
            DisplayAttrKind::Input => *self.ga_input.lock(),
            DisplayAttrKind::Converted => *self.ga_converted.lock(),
        };
        (atom != 0).then_some(atom)
    }

    fn set_range_display_attribute(
        &self,
        context: &ITfContext,
        ec: u32,
        range: &ITfRange,
        attr: DisplayAttrKind,
    ) -> Result<()> {
        let Some(atom) = self.display_attr_atom(attr) else {
            return Ok(());
        };
        let atom = i32::try_from(atom).map_err(|_| windows::core::Error::from(HRESULT(-1)))?;
        let property: ITfProperty = unsafe { context.GetProperty(&GUID_PROP_ATTRIBUTE) }?;
        let value = make_i32_variant(atom);
        unsafe { property.SetValue(ec, range, &value) }?;
        Ok(())
    }

    fn keyboard_openclose_compartment(&self) -> Result<ITfCompartment> {
        let thread_mgr = self
            .thread_mgr
            .lock()
            .clone()
            .ok_or_else(|| windows::core::Error::from(E_INVALIDARG))?;
        let manager: ITfCompartmentMgr = thread_mgr.cast()?;
        unsafe { manager.GetCompartment(&GUID_COMPARTMENT_KEYBOARD_OPENCLOSE) }
    }

    fn unregister_compartment_sink(&self, thread_mgr: &ITfThreadMgr) -> Result<()> {
        let Some(cookie) = *self.compartment_sink_cookie.lock() else {
            return Ok(());
        };
        thread_mgr
            .cast::<ITfCompartmentMgr>()
            .and_then(|manager| unsafe {
                manager.GetCompartment(&GUID_COMPARTMENT_KEYBOARD_OPENCLOSE)
            })
            .and_then(|compartment| compartment.cast::<ITfSource>())
            .and_then(|source| unsafe { source.UnadviseSink(cookie) })?;
        let mut current = self.compartment_sink_cookie.lock();
        if *current == Some(cookie) {
            *current = None;
        }
        Ok(())
    }

    fn read_compartment_mode(&self) -> Result<bool> {
        let value = unsafe { self.keyboard_openclose_compartment()?.GetValue() }?;
        let inner = unsafe { &value.Anonymous.Anonymous };
        if inner.vt != VT_I4 {
            return Err(E_INVALIDARG.into());
        }
        Ok(unsafe { inner.Anonymous.lVal != 0 })
    }

    fn write_compartment_mode(&self, chinese: bool) -> Result<()> {
        let tid = self.cookies.lock().tid;
        let value = make_i32_variant(i32::from(chinese));
        unsafe { self.keyboard_openclose_compartment()?.SetValue(tid, &value) }
    }

    fn notify_lang_bar_flags(&self, flags: u32) {
        let sink = self.lang_bar_sink.lock().clone();
        if let Some(sink) = sink {
            if let Err(error) = unsafe { sink.OnUpdate(flags) } {
                log::warn!("[TSF] 语言栏更新通知失败: {error}");
            }
        }
    }

    fn notify_lang_bar(&self) {
        self.notify_lang_bar_flags(TF_LBI_ICON | TF_LBI_TEXT | TF_LBI_TOOLTIP | TF_LBI_STATUS);
    }

    fn register_language_bar(&self, punk: &windows::core::IUnknown) -> Result<()> {
        let manager: ITfLangBarItemMgr =
            unsafe { CoCreateInstance(&CLSID_TF_LangBarItemMgr, None, CLSCTX_INPROC_SERVER) }?;
        let item: ITfLangBarItem = punk.cast()?;
        unsafe { manager.AddItem(&item) }
    }

    fn remove_language_bar(&self, punk: &windows::core::IUnknown) -> Result<()> {
        let manager: ITfLangBarItemMgr =
            unsafe { CoCreateInstance(&CLSID_TF_LangBarItemMgr, None, CLSCTX_INPROC_SERVER) }?;
        let item: ITfLangBarItem = punk.cast()?;
        unsafe { manager.RemoveItem(&item) }
    }

    fn register_compartment_sink(&self, punk: &windows::core::IUnknown) -> Result<()> {
        if self.compartment_sink_cookie.lock().is_some() {
            return Ok(());
        }
        let compartment = self.keyboard_openclose_compartment()?;
        let source: ITfSource = compartment.cast()?;
        let sink: ITfCompartmentEventSink = punk.cast()?;
        let cookie =
            unsafe { source.AdviseSink(&<ITfCompartmentEventSink as Interface>::IID, &sink) }?;
        *self.compartment_sink_cookie.lock() = Some(cookie);

        match self.read_compartment_mode() {
            Ok(chinese) => self.apply_ime_mode(chinese),
            Err(error) => {
                log::debug!("[TSF] 键盘开关 compartment 尚无可读值: {error}");
                let chinese = *self.ime_mode.lock();
                self.write_compartment_mode(chinese)?;
            }
        }
        Ok(())
    }

    fn apply_ime_mode(&self, chinese: bool) {
        let mut current = self.ime_mode.lock();
        if *current == chinese {
            return;
        }
        *current = chinese;
        drop(current);

        if chinese {
            log::debug!("[TSF] 切换到中文模式");
        } else {
            self.sm.lock().reset();
            self.end_active_composition();
            self.candidate_tx
                .store(Arc::new(CandidateData::hidden(self.theme_snapshot())));
            log::debug!("[TSF] 切换到英文模式");
        }
        self.notify_lang_bar();
    }

    /// 反转中英文模式。
    pub fn toggle_ime_mode(&self) {
        let next = !*self.ime_mode.lock();
        if let Err(error) = self.write_compartment_mode(next) {
            log::warn!("[TSF] 写入键盘开关 compartment 失败: {error}");
        }
        self.apply_ime_mode(next);
    }

    fn track_shift_toggle_keydown(&self, wparam: WPARAM) {
        self.shift_toggle_tracker
            .lock()
            .on_key_down(wparam.0 as u16, key_filter::is_shift_pressed());
    }

    /// 结束当前活跃的 TSF composition（若存在）。
    ///
    /// 切换到英文模式时调用，避免残留的 composition 吞掉后续按键。
    fn end_active_composition(&self) {
        if self.is_composing.load(Ordering::Acquire) {
            if let Ok((ctx, _)) = self.get_focus_context() {
                self.schedule_edit_session(EditSessionOp::EndComposition { delete_text: true }, &ctx);
            }
        }
    }
}

impl ITfSource_Impl for TextService_Impl {
    #[expect(
        clippy::not_unsafe_ptr_arg_deref,
        reason = "windows-rs fixes this COM trait method as safe; TSF supplies riid and the method checks it for null before dereferencing"
    )]
    fn AdviseSink(&self, riid: *const GUID, punk: Ref<'_, windows::core::IUnknown>) -> Result<u32> {
        let punk = punk
            .as_ref()
            .ok_or_else(|| windows::core::Error::from(E_INVALIDARG))?;
        if riid.is_null() {
            return Err(E_INVALIDARG.into());
        }
        if unsafe { *riid } != <ITfLangBarItemSink as Interface>::IID {
            return Err(CONNECT_E_CANNOTCONNECT.into());
        }
        if self.lang_bar_sink.lock().is_some() {
            return Err(CONNECT_E_ADVISELIMIT.into());
        }
        let sink: ITfLangBarItemSink = punk
            .cast()
            .map_err(|_| windows::core::Error::from(CONNECT_E_CANNOTCONNECT))?;

        let mut current = self.lang_bar_sink.lock();
        if current.is_some() {
            return Err(CONNECT_E_ADVISELIMIT.into());
        }
        *current = Some(sink);
        Ok(1)
    }

    fn UnadviseSink(&self, dwcookie: u32) -> Result<()> {
        if dwcookie != 1 {
            return Err(CONNECT_E_NOCONNECTION.into());
        }
        if self.lang_bar_sink.lock().take().is_none() {
            return Err(CONNECT_E_NOCONNECTION.into());
        }
        Ok(())
    }
}

impl ITfLangBarItem_Impl for TextService_Impl {
    #[expect(
        clippy::not_unsafe_ptr_arg_deref,
        reason = "windows-rs fixes this COM trait method as safe; TSF supplies pinfo and the method checks it for null before writing"
    )]
    fn GetInfo(&self, pinfo: *mut TF_LANGBARITEMINFO) -> Result<()> {
        if pinfo.is_null() {
            return Err(E_INVALIDARG.into());
        }
        let info = TF_LANGBARITEMINFO {
            clsidService: CLSID_TEXT_SERVICE,
            guidItem: GUID_LBI_INPUTMODE,
            dwStyle: TF_LBI_STYLE_BTN_BUTTON | TF_LBI_STYLE_SHOWNINTRAY,
            ulSort: 0,
            szDescription: fixed_utf16("MyWubi 中英文切换"),
        };
        // SAFETY: pinfo was checked non-null and TSF supplies writable storage
        // for exactly one TF_LANGBARITEMINFO value.
        unsafe { pinfo.write(info) };
        Ok(())
    }

    fn GetStatus(&self) -> Result<u32> {
        Ok(if self.lang_bar_visible.load(Ordering::Acquire) {
            0
        } else {
            TF_LBI_STATUS_HIDDEN
        })
    }

    fn Show(&self, fshow: BOOL) -> Result<()> {
        let visible = fshow.as_bool();
        if self.lang_bar_visible.swap(visible, Ordering::AcqRel) != visible {
            self.notify_lang_bar_flags(TF_LBI_STATUS);
        }
        Ok(())
    }

    fn GetTooltipString(&self) -> Result<windows::core::BSTR> {
        let chinese = *self.ime_mode.lock();
        Ok(windows::core::BSTR::from(lang_bar_display(chinese).1))
    }
}

impl ITfLangBarItemButton_Impl for TextService_Impl {
    fn OnClick(&self, click: TfLBIClick, _pt: &POINT, _prcarea: *const RECT) -> Result<()> {
        if click == TF_LBI_CLK_LEFT {
            self.toggle_ime_mode();
        }
        Ok(())
    }

    fn InitMenu(&self, _pmenu: Ref<'_, ITfMenu>) -> Result<()> {
        Ok(())
    }

    fn OnMenuSelect(&self, _wid: u32) -> Result<()> {
        Ok(())
    }

    fn GetIcon(&self) -> Result<HICON> {
        let chinese = *self.ime_mode.lock();
        create_lang_bar_icon(lang_bar_display(chinese).0)
    }

    fn GetText(&self) -> Result<windows::core::BSTR> {
        let chinese = *self.ime_mode.lock();
        Ok(windows::core::BSTR::from(lang_bar_display(chinese).0))
    }
}

impl ITfTextInputProcessor_Impl for TextService_Impl {
    fn Activate(&self, ptim: Ref<'_, ITfThreadMgr>, tid: u32) -> Result<()> {
        // ITfTextInputProcessorEx::ActivateEx 的退化调用。
        self.ActivateEx(ptim, tid, 0)
    }

    fn Deactivate(&self) -> Result<()> {
        // 删除 composition（如果有）
        self.composition.lock().take();
        self.is_composing.store(false, Ordering::Release);

        // 隐藏候选框并关闭候选框窗口。
        self.candidate_tx
            .store(Arc::new(CandidateData::hidden(self.theme_snapshot())));
        if let Some(mut cw) = self.candidate_window.lock().take() {
            cw.shutdown();
        }

        // 先克隆 thread_mgr 引用再进行清理（避免 take 后丢失 COM 指针）。
        let tm_hold = self.thread_mgr.lock().clone();

        let compartment_unadvise_error = tm_hold
            .as_ref()
            .and_then(|tm| self.unregister_compartment_sink(tm).err());
        if let Some(ref error) = compartment_unadvise_error {
            log::warn!("[TSF] 反注册键盘开关 compartment sink 失败: {error}");
        }

        let mut lang_bar_remove_error = None;
        if self.lang_bar_registered.load(Ordering::Acquire) {
            let result = self
                .clone_self_unknown()
                .ok_or_else(|| windows::core::Error::from(E_FAIL))
                .and_then(|punk| self.remove_language_bar(&punk));
            match result {
                Ok(()) => {
                    self.lang_bar_sink.lock().take();
                    self.lang_bar_registered.store(false, Ordering::Release);
                }
                Err(error) => {
                    log::warn!("[TSF] 移除语言栏按钮失败: {error}");
                    lang_bar_remove_error = Some(error);
                }
            }
        } else {
            self.lang_bar_sink.lock().take();
        }

        let state = mem::take(&mut *self.cookies.lock());

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
                }
                if state.thread_event != 0 {
                    let _ = unsafe { source.UnadviseSink(state.thread_event) };
                }
            }
        }

        // 清理所有持有的 COM 引用。
        // （thread_mgr 还可能需要用于反注册，先清理 sink 再释放 thread_mgr）
        // 反注册 ITfThreadFocusSink
        let fc = mem::take(&mut *self.thread_focus_cookie.lock());
        if let Some(ref tm) = tm_hold {
            if fc != 0 {
                if let Ok(source) = tm.cast::<ITfSource>() {
                    let _ = unsafe { source.UnadviseSink(fc) };
                }
            }
        }

        // 反注册 ITfTextEditSink
        let ec = mem::take(&mut *self.edit_sink_cookie.lock());
        let edit_sink_context = self.edit_sink_context.lock().take();
        if ec != 0 {
            if let Some(ctx) = edit_sink_context.as_ref() {
                if let Ok(source) = ctx.cast::<ITfSource>() {
                    let _ = unsafe { source.UnadviseSink(ec) };
                }
            }
        }

        if compartment_unadvise_error.is_none() {
            *self.thread_mgr.lock() = None;
        }
        *self.focus_doc_mgr.lock() = None;

        // 清理状态机内部缓冲。
        self.sm.lock().reset();
        *self.ga_input.lock() = 0;
        *self.ga_converted.lock() = 0;

        if let Some(error) = compartment_unadvise_error.or(lang_bar_remove_error) {
            return Err(error);
        }

        // 语言栏未注册或已成功移除，可以释放自我持有避免循环引用。
        self.release_self_unknown();
        log::info!("[TSF] TIP deactivated");
        Ok(())
    }
}

impl ITfTextInputProcessorEx_Impl for TextService_Impl {
    fn ActivateEx(&self, ptim: Ref<'_, ITfThreadMgr>, tid: u32, dwflags: u32) -> Result<()> {
        let tm = match ptim.as_ref() {
            Some(tm) => tm,
            None => {
                log::error!("[TSF] ActivateEx: ptim 为空");
                return Err(HRESULT(-1).into());
            }
        };
        if self.thread_mgr.lock().is_some() {
            log::warn!("[TSF] ActivateEx: TIP 已激活，忽略重复激活");
            return Ok(());
        }
        let tm = tm.clone();
        *self.thread_mgr.lock() = Some(tm.clone());
        // 保存激活标志（如 TF_TMAE_COMLESS）
        *self.activate_flags.lock() = dwflags;

        {
            let mut state = self.cookies.lock();
            state.tid = tid;
            state.using_keystroke_mgr = false;
        }
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
                                    self.cookies.lock().using_keystroke_mgr = true;
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
                            let mut state = self.cookies.lock();
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
                    Ok(c) => self.cookies.lock().thread_event = c,
                    Err(e) => log::error!("[TSF] AdviseSink ITfThreadMgrEventSink 失败: {e}"),
                }
            }
        } else {
            log::warn!("[TSF] Activate: self_unknown 未注入，跳过所有 AdviseSink");
        }
        let saved_using_kmgr = self.cookies.lock().using_keystroke_mgr;
        let saved_tid = tid;

        // 获取 self_unknown 引用于后续注册（clone 保持引用计数）
        let punk_self_later = self.clone_self_unknown();

        if let Some(ref punk) = punk_self_later {
            if !self.lang_bar_registered.load(Ordering::Acquire) {
                match self.register_language_bar(punk) {
                    Ok(()) => self.lang_bar_registered.store(true, Ordering::Release),
                    Err(error) => log::warn!("[TSF] 注册语言栏按钮失败: {error}"),
                }
            }
            if let Err(error) = self.register_compartment_sink(punk) {
                log::warn!("[TSF] 注册键盘开关 compartment sink 失败: {error}");
            }
        }

        // 启动候选框窗口线程。
        {
            let mut cw = self.candidate_window.lock();
            if cw.is_none() {
                *cw = Some(CandidateWindow::spawn(Arc::clone(&self.candidate_tx)));
            }
        }

        // 注册自定义 DisplayAttribute 类别。
        self.register_display_attribute_categories();
        self.initialize_display_attribute_atoms();

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
        let enum_obj = DisplayAttributeInfoEnum {
            items: make_display_attr_infos(),
            index: Mutex::new(0),
        };
        Ok(windows::core::ComObject::new(enum_obj).to_interface())
    }

    fn GetDisplayAttributeInfo(&self, guid: *const GUID) -> Result<ITfDisplayAttributeInfo> {
        if guid.is_null() {
            return Err(E_INVALIDARG.into());
        }
        let guid_safe = unsafe { &*guid };
        if *guid_safe == GUID_DISPLAY_ATTR_INPUT {
            Ok(make_display_attr_info(GUID_DISPLAY_ATTR_INPUT, "编码输入态", attr_input()))
        } else if *guid_safe == GUID_DISPLAY_ATTR_CONVERTED {
            Ok(make_display_attr_info(GUID_DISPLAY_ATTR_CONVERTED, "候选态", attr_converted()))
        } else {
            Err(E_INVALIDARG.into())
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
        let theme = self.theme_snapshot();
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

impl ITfCompartmentEventSink_Impl for TextService_Impl {
    fn OnChange(&self, rguid: *const GUID) -> Result<()> {
        if rguid.is_null() || unsafe { *rguid } != GUID_COMPARTMENT_KEYBOARD_OPENCLOSE {
            return Ok(());
        }
        match self.read_compartment_mode() {
            Ok(chinese) => self.apply_ime_mode(chinese),
            Err(error) => log::warn!("[TSF] 读取键盘开关 compartment 失败: {error}"),
        }
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
        self.sync_runtime_if_needed();
        self.track_shift_toggle_keydown(wparam);

        // 英文模式：所有按键透传
        if !*self.ime_mode.lock() {
            return Ok(BOOL(0));
        }

        // Ctrl/Alt/Win 按下时放行，避免拦截 Ctrl+C 等系统组合键
        if key_filter::is_system_modifier_pressed() {
            return Ok(BOOL(0));
        }

        let spelling_empty = self.sm.lock().spelling().is_empty();
        let cfg = self.config_snapshot();
        Ok(BOOL(should_intercept_test_key(
            key_filter::translate(
                wparam.0 as usize,
                lparam.0 as isize,
                &cfg.hotkey.page_next,
                &cfg.hotkey.page_prev,
            ),
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
        self.sync_runtime_if_needed();
        self.track_shift_toggle_keydown(wparam);

        // Ctrl/Alt/Win 按下时放行，避免拦截 Ctrl+C 等系统组合键
        if key_filter::is_system_modifier_pressed() {
            return Ok(BOOL(0));
        }

        let cfg = self.config_snapshot();
        let Some(event) = key_filter::translate(
            wparam.0 as usize,
            lparam.0 as isize,
            &cfg.hotkey.page_next,
            &cfg.hotkey.page_prev,
        ) else { return Ok(BOOL(0)); };
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
            if self.shift_toggle_tracker.lock().consume_preserved_key() {
                self.toggle_ime_mode();
                Ok(BOOL(1))
            } else {
                Ok(BOOL(0))
            }
        } else {
            Ok(BOOL(0))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn com_text_service() -> windows::core::ComObject<TextService> {
        let dict = Dictionary::from_entries(Vec::new(), None, Default::default()).unwrap();
        let candidate_tx = Arc::new(ArcSwap::from_pointee(CandidateData::hidden(
            ThemeSnapshot::default(),
        )));
        windows::core::ComObject::new(TextService::new(dict, 5, false, candidate_tx))
    }

    #[test]
    fn compartment_sink_starts_unregistered() {
        let service = com_text_service();

        assert!(service.compartment_sink_cookie.lock().is_none());
    }

    #[test]
    fn language_bar_item_uses_sdk_input_mode_guid() {
        let service = com_text_service();
        let button: ITfLangBarItemButton = service.to_interface();
        let item: ITfLangBarItem = button.cast().unwrap();
        let mut info = TF_LANGBARITEMINFO::default();

        unsafe { item.GetInfo(&mut info) }.unwrap();

        assert_eq!(
            info.guidItem,
            windows::Win32::UI::TextServices::GUID_LBI_INPUTMODE
        );
    }

    #[test]
    fn advise_sink_rejects_wrong_iid_with_cannot_connect() {
        let service = com_text_service();
        let source: ITfSource = service.to_interface();
        let unknown: windows::core::IUnknown = service.to_interface();

        let error = unsafe { source.AdviseSink(&GUID::from_u128(0), &unknown) }.unwrap_err();

        assert_eq!(
            error.code(),
            windows::Win32::System::Ole::CONNECT_E_CANNOTCONNECT
        );
    }

    #[test]
    fn advise_sink_maps_sink_query_failure_to_cannot_connect() {
        let service = com_text_service();
        let source: ITfSource = service.to_interface();
        let unknown: windows::core::IUnknown = service.to_interface();

        let error = unsafe {
            source.AdviseSink(&<ITfLangBarItemSink as Interface>::IID, &unknown)
        }
        .unwrap_err();

        assert_eq!(
            error.code(),
            windows::Win32::System::Ole::CONNECT_E_CANNOTCONNECT
        );
    }

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

    #[test]
    fn chinese_mode_uses_chinese_language_bar_text() {
        assert_eq!(lang_bar_display(true), ("中", "中文模式"));
    }

    #[test]
    fn english_mode_uses_english_language_bar_text() {
        assert_eq!(lang_bar_display(false), ("英", "英文模式"));
    }

    #[test]
    fn fixed_utf16_fills_description_and_keeps_trailing_nul() {
        let value = fixed_utf16::<8>("MyWubi");

        assert_eq!(value, [77, 121, 87, 117, 98, 105, 0, 0]);
    }

    #[test]
    fn switching_to_english_clears_input_state_and_hides_candidates() {
        let dict = Dictionary::from_entries(
            vec![core_engine::dictionary::Entry {
                code: "a".into(),
                word: "工".into(),
                weight: 1,
            }],
            None,
            Default::default(),
        )
        .unwrap();
        let theme = ThemeSnapshot::default();
        let candidate_tx = Arc::new(ArcSwap::from_pointee(CandidateData::visible(
            "a".into(),
            vec![CandidateItem {
                label: "1. ".into(),
                text: "工".into(),
            }],
            0,
            0,
            1,
            None,
            theme,
        )));
        let service = TextService::new(dict, 5, false, Arc::clone(&candidate_tx));
        service.sm.lock().handle(InputEvent::Char('a'));

        service.apply_ime_mode(false);

        assert!(!*service.ime_mode.lock());
        assert!(service.sm.lock().spelling().is_empty());
        assert!(!candidate_tx.load().visible);
    }

    #[test]
    fn shift_toggle_tracker_toggles_for_shift_alone() {
        let mut tracker = ShiftKeyToggleTracker::default();

        tracker.on_key_down(VK_SHIFT.0, false);

        assert!(tracker.consume_preserved_key());
    }

    #[test]
    fn shift_toggle_tracker_ignores_shift_letter_combo() {
        let mut tracker = ShiftKeyToggleTracker::default();

        tracker.on_key_down(VK_SHIFT.0, false);
        tracker.on_key_down(b'A' as u16, true);

        assert!(!tracker.consume_preserved_key());
    }

    #[test]
    fn shift_toggle_tracker_resets_after_combo() {
        let mut tracker = ShiftKeyToggleTracker::default();

        tracker.on_key_down(VK_SHIFT.0, false);
        tracker.on_key_down(b'A' as u16, true);
        assert!(!tracker.consume_preserved_key());

        tracker.on_key_down(VK_SHIFT.0, false);
        assert!(tracker.consume_preserved_key());
    }

    #[test]
    fn spelling_update_clears_stale_candidates_and_keeps_spelling_visible() {
        let theme = ThemeSnapshot::default();
        let current = CandidateData::visible(
            "a".into(),
            vec![CandidateItem { label: "1. ".into(), text: "工".into() }],
            0,
            0,
            1,
            None,
            theme.clone(),
        );

        let updated = build_spelling_only_candidate_data(
            current,
            "aaa".into(),
            Some(crate::candidate_data::ScreenPoint { x: 10, y: 20 }),
        );

        assert!(updated.visible);
        assert_eq!(updated.spelling, "aaa");
        assert!(updated.items.is_empty());
        assert_eq!(updated.highlighted, 0);
        assert_eq!(updated.page, 0);
        assert_eq!(updated.total_pages, 0);
        assert_eq!(updated.anchor.unwrap().x, 10);
        assert_eq!(updated.anchor.unwrap().y, 20);
    }

    #[test]
    fn spelling_update_shows_spelling_even_without_candidates() {
        let theme = ThemeSnapshot::default();
        let current = CandidateData::hidden(theme.clone());

        let updated = build_spelling_only_candidate_data(current, "aaa".into(), None);

        assert!(updated.visible);
        assert_eq!(updated.spelling, "aaa");
        assert!(updated.items.is_empty());
        assert_eq!(updated.theme.font_size, theme.font_size);
    }

    #[test]
    fn spelling_update_preserves_existing_anchor_when_new_anchor_missing() {
        let theme = ThemeSnapshot::default();
        let current = CandidateData::visible(
            "a".into(),
            vec![CandidateItem { label: "1. ".into(), text: "工".into() }],
            0,
            0,
            1,
            Some(crate::candidate_data::ScreenPoint { x: 32, y: 64 }),
            theme,
        );

        let updated = build_spelling_only_candidate_data(current, "aaa".into(), None);

        assert_eq!(updated.anchor.unwrap().x, 32);
        assert_eq!(updated.anchor.unwrap().y, 64);
    }

    #[test]
    fn precise_anchor_update_overrides_fallback_anchor() {
        let theme = ThemeSnapshot::default();
        let current = CandidateData::visible(
            "gg".into(),
            vec![CandidateItem { label: "1. ".into(), text: "工".into() }],
            0,
            0,
            1,
            Some(crate::candidate_data::ScreenPoint { x: 5, y: 10 }),
            theme,
        );

        let updated = update_candidate_anchor(
            current,
            Some(crate::candidate_data::ScreenPoint { x: 120, y: 240 }),
        );

        assert_eq!(updated.anchor.unwrap().x, 120);
        assert_eq!(updated.anchor.unwrap().y, 240);
        assert_eq!(updated.spelling, "gg");
        assert_eq!(updated.items.len(), 1);
    }

    #[test]
    fn spelling_update_maps_to_input_composition_update() {
        let op = transition_to_edit_session_op(&Transition::SpellingUpdated("gg".into()));

        assert_eq!(
            op,
            EditSessionOp::CompositionUpdate {
                spelling: "gg".into(),
                attr: DisplayAttrKind::Input,
            }
        );
    }

    #[test]
    fn candidates_map_to_converted_composition_update() {
        let op = transition_to_edit_session_op(&Transition::Candidates {
            spelling: "gg".into(),
            candidates: vec!["工".into()],
            page: 0,
            total_pages: 1,
        });

        assert_eq!(
            op,
            EditSessionOp::CompositionUpdate {
                spelling: "gg".into(),
                attr: DisplayAttrKind::Converted,
            }
        );
    }
}


