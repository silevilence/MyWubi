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
//!   [`InputEvent`] 后驱动输入会话，依据处理结果
//!   决定是否“吃掉”按键。上屏文本由同步 EditSession 写入，
//!   候选框由 GDI 独立渲染。
//! * **焦点切换**：`ITfThreadMgrEventSink::OnSetFocus` 用于跟踪文档管理器焦点，
//!   为后续候选框定位提供基础（暂记当前 ITfDocumentMgr 指针）。

use core_engine::{config::SwitchKey, Config, Dictionary, InputEvent};
use parking_lot::Mutex;
use std::ffi::c_void;
use std::mem;
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use windows::core::{implement, w, IUnknown, Interface, Ref, Result, BOOL, GUID, HRESULT};
use windows::Win32::Foundation::{
    COLORREF, E_FAIL, E_INVALIDARG, LPARAM, POINT, RECT, S_FALSE, WPARAM,
};
use windows::Win32::Graphics::Gdi::{
    CreateBitmap, CreateCompatibleDC, CreateDIBSection, CreateFontW, DeleteDC, DeleteObject,
    DrawTextW, GdiFlush, SelectObject, SetBkMode, SetTextColor, ANTIALIASED_QUALITY, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DEFAULT_PITCH, DIB_RGB_COLORS,
    DT_CENTER, DT_SINGLELINE, DT_VCENTER, FF_DONTCARE, FW_NORMAL, HGDIOBJ, OUT_DEFAULT_PRECIS,
    RGBQUAD, TRANSPARENT,
};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use windows::Win32::System::Ole::{
    CONNECT_E_ADVISELIMIT, CONNECT_E_CANNOTCONNECT, CONNECT_E_NOCONNECTION,
};
use windows::Win32::System::Variant::{VARIANT, VARIANT_0, VARIANT_0_0, VARIANT_0_0_0, VT_I4};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    VK_CAPITAL, VK_LSHIFT, VK_RSHIFT, VK_SHIFT, VK_SPACE,
};
use windows::Win32::UI::TextServices::{
    CLSID_TF_LangBarItemMgr, IEnumTfDisplayAttributeInfo, IEnumTfDisplayAttributeInfo_Impl,
    ITfCategoryMgr, ITfCompartment, ITfCompartmentEventSink, ITfCompartmentEventSink_Impl,
    ITfCompartmentMgr, ITfComposition, ITfCompositionSink, ITfCompositionSink_Impl, ITfContext,
    ITfContextComposition, ITfDisplayAttributeInfo, ITfDisplayAttributeInfo_Impl,
    ITfDisplayAttributeProvider, ITfDisplayAttributeProvider_Impl, ITfDocumentMgr, ITfEditRecord,
    ITfEditSession, ITfEditSession_Impl, ITfFnGetPreferredTouchKeyboardLayout,
    ITfFnGetPreferredTouchKeyboardLayout_Impl, ITfFunction, ITfFunctionProvider,
    ITfFunctionProvider_Impl, ITfFunction_Impl, ITfKeyEventSink, ITfKeyEventSink_Impl,
    ITfKeystrokeMgr, ITfLangBarItem, ITfLangBarItemButton, ITfLangBarItemButton_Impl,
    ITfLangBarItemMgr, ITfLangBarItemSink, ITfLangBarItem_Impl, ITfMenu, ITfProperty, ITfRange,
    ITfSource, ITfSource_Impl, ITfTextEditSink, ITfTextEditSink_Impl, ITfTextInputProcessor,
    ITfTextInputProcessorEx, ITfTextInputProcessorEx_Impl, ITfTextInputProcessor_Impl,
    ITfThreadFocusSink, ITfThreadFocusSink_Impl, ITfThreadMgr, ITfThreadMgrEventSink,
    ITfThreadMgrEventSink_Impl, TKBLayoutType, TfLBIClick, GUID_COMPARTMENT_KEYBOARD_OPENCLOSE,
    GUID_LBI_INPUTMODE, GUID_PROP_ATTRIBUTE, GUID_TFCAT_DISPLAYATTRIBUTEPROVIDER, TF_AE_END,
    TF_ANCHOR_END, TF_ANCHOR_START, TF_CT_COLORREF, TF_DA_COLOR, TF_DA_COLOR_0,
    TF_DEFAULT_SELECTION, TF_DISPLAYATTRIBUTE, TF_ES_READWRITE, TF_ES_SYNC, TF_LANGBARITEMINFO,
    TF_LBI_CLK_LEFT, TF_LBI_ICON, TF_LBI_STATUS, TF_LBI_STATUS_HIDDEN, TF_LBI_STYLE_BTN_BUTTON,
    TF_LBI_STYLE_SHOWNINTRAY, TF_LBI_TEXT, TF_LBI_TOOLTIP, TF_LS_DOT, TF_LS_SOLID, TF_MOD_CONTROL,
    TF_MOD_ON_KEYUP, TF_PRESERVEDKEY, TF_SELECTION, TF_SELECTIONSTYLE, TKBLT_OPTIMIZED,
    TKBL_OPT_SIMPLIFIED_CHINESE_PINYIN,
};
use windows::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, HICON, ICONINFO};

use arc_swap::ArcSwap;

use crate::candidate_data::{CandidateData, ScreenPoint, ThemeSnapshot};
use crate::candidate_window::CandidateWindow;
use crate::guids::{CLSID_TEXT_SERVICE, GUID_PRESERVED_SHIFT};
use crate::input_session::{DisplayAttrKind, EditSessionOp, InputSession};
use crate::key_filter;
use crate::lifecycle::{remove_registration, Registration, TsfLifecycle};
use crate::screen_geometry::get_range_position;
use crate::RuntimeSnapshot;

// ── 类型定义 ────────────────────────────────────────────────────────

/// 编码输入态 display attribute 的 GUID.
const GUID_DISPLAY_ATTR_INPUT: GUID = GUID::from_u128(0x8a2e3b4c_1d5f_4a7b_9e6c_3f8d2b1a5e7d);
/// 有候选态 display attribute 的 GUID.
const GUID_DISPLAY_ATTR_CONVERTED: GUID = GUID::from_u128(0x6b1c8d9e_2f3a_4c5b_8d7e_1a2b3c4d5e6f);
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

fn preserved_key_for_switch_key(switch_key: SwitchKey) -> (TF_PRESERVEDKEY, &'static str) {
    match switch_key {
        SwitchKey::Shift => (
            TF_PRESERVEDKEY {
                uVKey: VK_SHIFT.0 as u32,
                uModifiers: TF_MOD_ON_KEYUP,
            },
            "中英文切换 (Shift)",
        ),
        SwitchKey::CapsLock => (
            TF_PRESERVEDKEY {
                uVKey: VK_CAPITAL.0 as u32,
                uModifiers: TF_MOD_ON_KEYUP,
            },
            "中英文切换 (CapsLock)",
        ),
        SwitchKey::CtrlSpace => (
            TF_PRESERVEDKEY {
                uVKey: VK_SPACE.0 as u32,
                uModifiers: TF_MOD_CONTROL,
            },
            "中英文切换 (Ctrl+Space)",
        ),
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
            -14,
            0,
            0,
            0,
            FW_NORMAL.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            ANTIALIASED_QUALITY,
            (DEFAULT_PITCH.0 | FF_DONTCARE.0).into(),
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
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: SIZE,
            bottom: SIZE,
        };
        let drawn = DrawTextW(
            dc,
            &mut utf16,
            &mut rect,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
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
        Ok(()) // 自定义属性不支持外部修改
    }

    fn Reset(&self) -> Result<()> {
        Ok(())
    }
}

/// 辅助函数：创建一个 TF_DA_COLOR（指定 RGB）。
fn make_color(r: u8, g: u8, b: u8) -> TF_DA_COLOR {
    TF_DA_COLOR {
        r#type: TF_CT_COLORREF,
        Anonymous: TF_DA_COLOR_0 {
            cr: COLORREF((r as u32) | ((g as u32) << 8) | ((b as u32) << 16)),
        },
    }
}

/// 创建一个 ComObject 包裹的 DisplayAttributeInfo，返回其 ITfDisplayAttributeInfo 接口。
fn make_display_attr_info(
    guid: GUID,
    desc: &'static str,
    attr: TF_DISPLAYATTRIBUTE,
) -> ITfDisplayAttributeInfo {
    let info = DisplayAttributeInfo {
        guid,
        description: desc,
        attr,
    };
    let obj = windows::core::ComObject::new(info);
    obj.to_interface()
}

/// 构建「编码输入态」display attribute：灰色点线。
fn attr_input() -> TF_DISPLAYATTRIBUTE {
    TF_DISPLAYATTRIBUTE {
        crText: make_color(0x80, 0x80, 0x80),
        crBk: TF_DA_COLOR {
            r#type: TF_CT_COLORREF,
            ..Default::default()
        },
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
        crBk: TF_DA_COLOR {
            r#type: TF_CT_COLORREF,
            ..Default::default()
        },
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
            unsafe {
                rginfo
                    .add(offset)
                    .write(Some(self.items[start + offset].clone()));
            }
        }
        for offset in fetched..requested {
            unsafe {
                rginfo.add(offset).write(None);
            }
        }

        *index += fetched;
        if !pcfetched.is_null() {
            unsafe {
                *pcfetched = fetched as u32;
            }
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

/// ITfEditSession 的 COM 实现。由 run_edit 创建并传递给
/// ITfContext::RequestEditSession 调度。
#[implement(ITfEditSession)]
struct EditSession {
    op: EditSessionOp,
    context: ITfContext,
    anchor: Arc<Mutex<Option<ScreenPoint>>>,
    /// 指向 `_service_owner` 持有的 TextService COM 对象。
    service_ptr: *const TextService,
    /// 保持 TextService COM 对象存活直到同步 EditSession 完成。
    _service_owner: windows::core::IUnknown,
}

impl ITfEditSession_Impl for EditSession_Impl {
    fn DoEditSession(&self, ec: u32) -> Result<()> {
        // SAFETY: _service_owner keeps the COM allocation containing
        // service_ptr alive for the entire EditSession lifetime.
        let service = unsafe { &*self.service_ptr };
        *self.anchor.lock() = service.execute_edit_session(ec, &self.op, &self.context)?;
        Ok(())
    }
}

/// TSF 文本服务 COM 对象。
///
/// 一次实例对应一次 TIP 激活；`Activate` 时由系统通过 ClassFactory 创建并
/// 注入线程管理器，`Deactivate` 时自动反转连接并使本对象自洽回收。
///
/// 内部的输入会话 通过 `Mutex` 保护，多线程可见且无需发送数据
/// 跨线程锁竞争（TSF 单线程模型 + Mutex 互斥访问足够）。
#[implement(
    ITfTextInputProcessor,
    ITfThreadMgrEventSink,
    ITfKeyEventSink,
    ITfCompositionSink,
    ITfDisplayAttributeProvider,
    ITfTextInputProcessorEx,
    ITfThreadFocusSink,
    ITfTextEditSink,
    ITfFunctionProvider,
    ITfFunction,
    ITfFnGetPreferredTouchKeyboardLayout,
    ITfCompartmentEventSink,
    ITfLangBarItemButton,
    ITfSource
)]
pub struct TextService {
    /// 编码、候选显示与编辑结果的统一会话。
    input: InputSession,
    /// 可热替换的运行时快照（配置 + 码表 + 路径）。
    runtime: Arc<ArcSwap<RuntimeSnapshot>>,
    /// 注册记录、管理器与自身 owner 由生命周期模块共同持有。
    lifecycle: TsfLifecycle,
    /// 当前焦点所在文档管理器。
    focus_doc_mgr: Mutex<Option<ITfDocumentMgr>>,
    /// 候选框数据发布通道——TSF 按键处理通过 ArcSwap 推送 CandidateData。
    candidate_tx: Arc<ArcSwap<CandidateData>>,
    /// 候选框窗口实例（Activate 中启动，Deactivate 中关闭）。
    candidate_window: Mutex<Option<CandidateWindow>>,
    /// ── Phase 1 新增字段 ──
    /// 当前激活的 composition 对象。
    composition: Mutex<Option<ITfComposition>>,
    /// 编码输入态 display attribute 对应的 GUID atom。
    ga_input: Mutex<u32>,
    /// 候选态 display attribute 对应的 GUID atom。
    ga_converted: Mutex<u32>,
    /// 自己发起的 EndComposition 回调不应重置下一轮输入。
    ending_composition: AtomicBool,
    /// ── Phase 2 新增字段 ──
    /// ITfTextInputProcessorEx 的激活标志（TF_TMAE_COMLESS 等）。
    activate_flags: Mutex<u32>,
    /// 跟踪本轮 Shift 是否与其他按键组合使用，避免组合键误触发中英文切换。
    shift_toggle_tracker: Mutex<ShiftKeyToggleTracker>,
    /// ── Phase 3 新增字段 ──
    /// 语言栏按钮更新通知 sink。
    lang_bar_sink: Mutex<Option<ITfLangBarItemSink>>,
    /// 语言栏按钮是否可见。
    lang_bar_visible: AtomicBool,
}

impl TextService {
    /// 创建一个绑定码表与状态机的文本服务实例（不带 back-pointer）。
    pub fn new(
        dict: Arc<Dictionary>,
        page_size: usize,
        auto_commit_unique: bool,
        candidate_tx: Arc<ArcSwap<CandidateData>>,
    ) -> Self {
        let mut cfg = Config::default();
        cfg.basic.candidate_count = page_size.clamp(1, 10) as u8;
        cfg.basic.auto_commit_unique = auto_commit_unique;
        Self::from_config(dict, &cfg, candidate_tx)
    }

    /// 从配置创建独立会话。
    pub fn from_config(
        dict: Arc<Dictionary>,
        cfg: &Config,
        candidate_tx: Arc<ArcSwap<CandidateData>>,
    ) -> Self {
        let runtime = Arc::new(ArcSwap::from_pointee(RuntimeSnapshot {
            revision: 0,
            dict,
            config: cfg.clone(),
            config_path: "config.toml".into(),
            system_table_path: "tables/wubi86.dict".into(),
            user_table_path: "tables/user.dict".into(),
        }));
        Self::from_runtime(runtime, candidate_tx)
    }

    pub(crate) fn from_runtime(
        runtime: Arc<ArcSwap<RuntimeSnapshot>>,
        candidate_tx: Arc<ArcSwap<CandidateData>>,
    ) -> Self {
        let snapshot = runtime.load();
        let input = InputSession::new(
            Arc::clone(&snapshot.dict),
            snapshot.config.clone(),
            snapshot.revision,
            Arc::clone(&candidate_tx),
        );
        drop(snapshot);

        Self {
            input,
            runtime,
            lifecycle: TsfLifecycle::default(),
            focus_doc_mgr: Mutex::new(None),
            candidate_tx,
            candidate_window: Mutex::new(None),
            composition: Mutex::new(None),
            ga_input: Mutex::new(0),
            ga_converted: Mutex::new(0),
            ending_composition: AtomicBool::new(false),
            activate_flags: Mutex::new(0),
            shift_toggle_tracker: Mutex::new(ShiftKeyToggleTracker::default()),
            lang_bar_sink: Mutex::new(None),
            lang_bar_visible: AtomicBool::new(true),
        }
    }

    /// 仅供 IClassFactory 内部注入 self 弱强引用（见 [`crate::factory::TextServiceFactory`])。
    pub(crate) fn set_self_unknown(&self, unk: windows::core::IUnknown) {
        self.lifecycle.set_owner(unk);
    }

    /// 内部：取一份 IUnknown 副本（已 AddRef），用于 AdviseSink 注册。
    fn clone_self_unknown(&self) -> Option<windows::core::IUnknown> {
        self.lifecycle.owner()
    }

    fn theme_snapshot(&self) -> ThemeSnapshot {
        self.input.theme()
    }

    fn config_snapshot(&self) -> Config {
        self.input.config()
    }

    fn sync_runtime_if_needed(&self) {
        let snapshot = self.runtime.load_full();
        self.input.reload(&snapshot, |op| self.run_edit(op, None));
    }

    /// 工厂 QueryInterface 失败时，解除尚未激活对象的自身强引用。
    pub(crate) fn release_self_unknown(&self) {
        self.lifecycle.release_unactivated_owner();
    }

    /// 按键来自新 context 时，先结束旧文档的会话，失败则阻止本次输入。
    fn prepare_context(&self, context: Option<&ITfContext>) -> Result<bool> {
        let composition = self.composition.lock().clone();
        if let (Some(comp), Some(context)) = (composition, context) {
            let previous = unsafe { comp.GetRange()?.GetContext()? };
            if previous.cast::<IUnknown>()? != context.cast::<IUnknown>()? {
                self.input.lose_focus(|op| self.run_edit(op, None));
            }
        }
        Ok(self.input.recover(|op| self.run_edit(op, None)))
    }

    /// 将会话操作交给当前文档 adapter，并同时检查请求与执行 HRESULT。
    fn run_edit(
        &self,
        op: &EditSessionOp,
        context: Option<&ITfContext>,
    ) -> Result<Option<ScreenPoint>> {
        if matches!(op, EditSessionOp::NoOp) {
            return Ok(None);
        }
        let composition = self.composition.lock().clone();
        if matches!(op, EditSessionOp::EndComposition { .. }) && composition.is_none() {
            return Ok(None);
        }
        let context = if let Some(comp) = composition {
            unsafe { comp.GetRange()?.GetContext()? }
        } else if let Some(ctx) = context {
            ctx.clone()
        } else {
            self.get_focus_context()?.0
        };
        let service_owner = self
            .clone_self_unknown()
            .ok_or_else(|| windows::core::Error::from(E_FAIL))?;
        let tid = self.lifecycle.tid();
        if tid == 0 {
            return Err(E_FAIL.into());
        }
        let anchor = Arc::new(Mutex::new(None));
        let edit_session = EditSession {
            op: op.clone(),
            context: context.clone(),
            anchor: Arc::clone(&anchor),
            service_ptr: self as *const TextService,
            _service_owner: service_owner,
        };
        let com_obj = windows::core::ComObject::new(edit_session);
        let edit_session_com: ITfEditSession = com_obj.to_interface();
        unsafe {
            context.RequestEditSession(tid, &edit_session_com, TF_ES_SYNC | TF_ES_READWRITE)
        }?
        .ok()?;
        let position = *anchor.lock();
        Ok(position)
    }

    /// 在 TSF EditSession 回调中执行实际的 composition 操作。
    fn execute_edit_session(
        &self,
        ec: u32,
        op: &EditSessionOp,
        context: &ITfContext,
    ) -> Result<Option<ScreenPoint>> {
        match op {
            EditSessionOp::NoOp => Ok(None),
            EditSessionOp::CompositionUpdate { spelling, attr } => {
                self.edit_session_composition_update(ec, spelling, *attr, context)
            }
            EditSessionOp::CommitAndReplace { text, replace_len } => {
                self.edit_session_commit_and_replace(ec, text, *replace_len, context)?;
                Ok(None)
            }
            EditSessionOp::CommitThenUpdate {
                text,
                replace_len,
                spelling,
                attr,
            } => {
                self.edit_session_commit_and_replace(ec, text, *replace_len, context)?;
                self.edit_session_composition_update(ec, spelling, *attr, context)
            }
            EditSessionOp::EndComposition { delete_text } => {
                self.edit_session_end_composition(ec, *delete_text)?;
                Ok(None)
            }
        }
    }

    fn edit_session_composition_update(
        &self,
        ec: u32,
        spelling: &str,
        attr: DisplayAttrKind,
        context: &ITfContext,
    ) -> Result<Option<ScreenPoint>> {
        let current = self.composition.lock().clone();
        let comp = if let Some(comp) = current {
            comp
        } else {
            let ctx_comp: ITfContextComposition = context.cast()?;
            let selection = self.get_selection_range(context, ec)?;
            let sink = self
                .clone_self_unknown()
                .ok_or_else(|| windows::core::Error::from(E_FAIL))?
                .cast::<ITfCompositionSink>()?;
            let comp = unsafe { ctx_comp.StartComposition(ec, &selection, &sink) }?;
            *self.composition.lock() = Some(comp.clone());
            comp
        };
        let range = unsafe { comp.GetRange() }?;
        let wide: Vec<u16> = spelling.encode_utf16().collect();
        unsafe { range.SetText(ec, 0, &wide) }?;
        self.set_range_display_attribute(context, ec, &range, attr)?;
        Ok(get_range_position(context, ec, &range))
    }

    fn edit_session_commit_and_replace(
        &self,
        ec: u32,
        text: &str,
        replace_len: usize,
        context: &ITfContext,
    ) -> Result<()> {
        let comp = self.composition.lock().clone();
        let range = if let Some(comp) = &comp {
            unsafe { comp.GetRange() }?
        } else {
            let range = self.get_selection_range(context, ec)?;
            if replace_len > 0 {
                let mut shifted = 0;
                unsafe {
                    range.ShiftStart(ec, -(replace_len as i32), &mut shifted, ptr::null())?;
                    range.Collapse(ec, TF_ANCHOR_START)?;
                    range.ShiftEnd(ec, replace_len as i32, &mut shifted, ptr::null())?;
                }
            }
            range
        };
        let wide: Vec<u16> = text.encode_utf16().collect();
        unsafe { range.SetText(ec, 0, &wide) }?;
        unsafe { range.Collapse(ec, TF_ANCHOR_END) }?;
        if let Some(comp) = comp {
            self.end_composition(ec, &comp)?;
        }
        self.set_selection_to_range_end(context, ec, &range)
    }

    fn edit_session_end_composition(&self, ec: u32, delete_text: bool) -> Result<()> {
        let comp = self.composition.lock().clone();
        if let Some(comp) = comp {
            if delete_text {
                unsafe { comp.GetRange()?.SetText(ec, 0, &[]) }?;
            }
            self.end_composition(ec, &comp)?;
        }
        Ok(())
    }

    fn end_composition(&self, ec: u32, comp: &ITfComposition) -> Result<()> {
        struct EndGuard<'a>(&'a AtomicBool);
        impl Drop for EndGuard<'_> {
            fn drop(&mut self) {
                self.0.store(false, Ordering::Release);
            }
        }
        self.ending_composition.store(true, Ordering::Release);
        let _guard = EndGuard(&self.ending_composition);
        unsafe { comp.EndComposition(ec) }?;
        let old = self.composition.lock().take();
        drop(old);
        Ok(())
    }

    /// 获取当前焦点文档管理器的顶 context.
    fn get_focus_context(&self) -> Result<(ITfContext, ITfDocumentMgr)> {
        let tm = self
            .lifecycle
            .manager()
            .ok_or_else(|| windows::core::Error::from(HRESULT(-1)))?;
        let doc_mgr: ITfDocumentMgr = unsafe { tm.GetFocus() }?;
        let ctx: ITfContext = unsafe { doc_mgr.GetBase() }?;
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

    fn set_selection_to_range_end(
        &self,
        ctx: &ITfContext,
        ec: u32,
        range: &ITfRange,
    ) -> Result<()> {
        let mut selection = [TF_SELECTION {
            range: mem::ManuallyDrop::new(Some(range.clone())),
            style: TF_SELECTIONSTYLE {
                ase: TF_AE_END,
                fInterimChar: BOOL(0),
            },
        }];
        let result = unsafe { ctx.SetSelection(ec, &selection) };
        unsafe { mem::ManuallyDrop::drop(&mut selection[0].range) };
        result
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
            .lifecycle
            .manager()
            .ok_or_else(|| windows::core::Error::from(E_INVALIDARG))?;
        let manager: ITfCompartmentMgr = thread_mgr.cast()?;
        unsafe { manager.GetCompartment(&GUID_COMPARTMENT_KEYBOARD_OPENCLOSE) }
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
        let tid = self.lifecycle.tid();
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

    fn register_compartment_sink(&self, punk: &windows::core::IUnknown) -> Result<()> {
        let compartment = self.keyboard_openclose_compartment()?;
        let source: ITfSource = compartment.cast()?;
        let sink: ITfCompartmentEventSink = punk.cast()?;
        let cookie =
            unsafe { source.AdviseSink(&<ITfCompartmentEventSink as Interface>::IID, &sink) }?;
        self.lifecycle.record(Registration::Compartment(cookie));

        match self.read_compartment_mode() {
            Ok(chinese) => self.apply_ime_mode(chinese),
            Err(error) => {
                log::debug!("[TSF] 键盘开关 compartment 尚无可读值: {error}");
                let chinese = self.input.chinese();
                self.write_compartment_mode(chinese)?;
            }
        }
        Ok(())
    }

    fn apply_ime_mode(&self, chinese: bool) {
        self.input
            .set_chinese(chinese, |op| self.run_edit(op, None));
        self.notify_lang_bar();
    }

    /// 反转中英文模式。
    pub fn toggle_ime_mode(&self) {
        let next = !self.input.chinese();
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

    /// 两个按键回调共用同一组模式、修饰键与编码规则。
    fn translated_key(&self, wparam: WPARAM, lparam: LPARAM) -> Option<InputEvent> {
        self.sync_runtime_if_needed();
        self.track_shift_toggle_keydown(wparam);
        if !self.input.recover(|op| self.run_edit(op, None))
            || !self.input.chinese()
            || key_filter::is_system_modifier_pressed()
        {
            return None;
        }
        let config = self.config_snapshot();
        key_filter::translate(wparam.0, lparam.0, self.input.selecting(), &config.hotkey)
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
        let chinese = self.input.chinese();
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
        let chinese = self.input.chinese();
        create_lang_bar_icon(lang_bar_display(chinese).0)
    }

    fn GetText(&self) -> Result<windows::core::BSTR> {
        let chinese = self.input.chinese();
        Ok(windows::core::BSTR::from(lang_bar_display(chinese).0))
    }
}

impl ITfTextInputProcessor_Impl for TextService_Impl {
    fn Activate(&self, ptim: Ref<'_, ITfThreadMgr>, tid: u32) -> Result<()> {
        // ITfTextInputProcessorEx::ActivateEx 的退化调用。
        self.ActivateEx(ptim, tid, 0)
    }

    fn Deactivate(&self) -> Result<()> {
        self.input.lose_focus(|op| self.run_edit(op, None));
        let old = self.composition.lock().take();
        drop(old);
        self.input.terminated();
        if let Some(mut cw) = self.candidate_window.lock().take() {
            cw.shutdown();
        }

        let result = self.lifecycle.deactivate(remove_registration);
        if !self.lifecycle.has_language_bar() {
            self.lang_bar_sink.lock().take();
        }
        *self.focus_doc_mgr.lock() = None;

        // 清理状态机内部缓冲。
        *self.ga_input.lock() = 0;
        *self.ga_converted.lock() = 0;

        result?;
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
        if self.lifecycle.owner().is_none() {
            // TSF 可在 Deactivate 后复用同一个 COM 对象；工厂不会再次注入 owner。
            // 当前 COM 调用保证 self 仍存活，重新取得引用供本轮订阅使用。
            use windows::core::IUnknownImpl;
            self.lifecycle.set_owner(self.to_interface());
        }
        if !self.lifecycle.begin(tm.clone(), tid) {
            log::warn!("[TSF] ActivateEx: TIP 已激活，忽略重复激活");
            return Ok(());
        }
        let tm = tm.clone();
        // 保存激活标志（如 TF_TMAE_COMLESS）
        *self.activate_flags.lock() = dwflags;

        if let Some(punk_self) = self.clone_self_unknown() {
            // ── 主路径：通过 ITfKeystrokeMgr 注册按键事件 sink ──
            // 这是 Microsoft 官方 TSF 示例推荐的方式，比 ITfSource::AdviseSink
            // 更可靠，确保 OnKeyDown/OnTestKeyDown 被正确调用。
            let kmgr_ok = match tm.cast::<ITfKeystrokeMgr>() {
                Ok(kmgr) => match punk_self.cast::<ITfKeyEventSink>() {
                    Ok(key_sink) => {
                        match unsafe { kmgr.AdviseKeyEventSink(tid, &key_sink, true) } {
                            Ok(()) => {
                                self.lifecycle.record(Registration::KeyEvent(tid));
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
                },
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
                            self.lifecycle.record(Registration::SourceKey(c));
                            log::info!("[TSF] 回退: ITfSource::AdviseSink ITfKeyEventSink 成功");
                        }
                        Err(e) => {
                            log::error!("[TSF] 回退: AdviseSink ITfKeyEventSink 仍然失败: {e}")
                        }
                    }
                }
            }

            // ── ITfThreadMgrEventSink：始终通过 ITfSource ──
            if let Ok(source) = tm.cast::<ITfSource>() {
                let iid_thread = <ITfThreadMgrEventSink as windows::core::Interface>::IID;
                match unsafe { source.AdviseSink(&iid_thread, &punk_self) } {
                    Ok(c) => self.lifecycle.record(Registration::ThreadEvent(c)),
                    Err(e) => log::error!("[TSF] AdviseSink ITfThreadMgrEventSink 失败: {e}"),
                }
            }
        } else {
            log::warn!("[TSF] Activate: self_unknown 未注入，跳过所有 AdviseSink");
        }
        let saved_using_kmgr = self.lifecycle.using_keystroke_mgr();
        let saved_tid = tid;

        // 获取 self_unknown 引用于后续注册（clone 保持引用计数）
        let punk_self_later = self.clone_self_unknown();

        if let Some(ref punk) = punk_self_later {
            if !self.lifecycle.has_language_bar() {
                match self.register_language_bar(punk) {
                    Ok(()) => self.lifecycle.record(Registration::LanguageBar),
                    Err(error) => log::warn!("[TSF] 注册语言栏按钮失败: {error}"),
                }
            }
            if let Err(error) = self.register_compartment_sink(punk) {
                log::warn!("[TSF] 注册键盘开关 compartment sink 失败: {error}");
            }
        }

        // 注册自定义 DisplayAttribute 类别。
        self.register_display_attribute_categories();
        self.initialize_display_attribute_atoms();

        // 注册 PreservedKey（中英文切换）
        if saved_tid != 0 && saved_using_kmgr {
            if let Ok(kmgr) = tm.cast::<ITfKeystrokeMgr>() {
                let cfg = self.config_snapshot();
                let (preserved_key, desc) = preserved_key_for_switch_key(cfg.basic.switch_key);
                let desc: Vec<u16> = format!("{desc}\0").encode_utf16().collect();
                if unsafe {
                    kmgr.PreserveKey(saved_tid, &GUID_PRESERVED_SHIFT, &preserved_key, &desc)
                }
                .is_ok()
                {
                    self.lifecycle
                        .record(Registration::PreservedKey(preserved_key));
                }
            }
        }

        // 注册 ITfThreadFocusSink（线程焦点变化通知）
        if let Some(ref punk) = punk_self_later {
            if let Ok(source) = tm.cast::<ITfSource>() {
                let iid_thread_focus = <ITfThreadFocusSink as Interface>::IID;
                if let Ok(cookie) = unsafe { source.AdviseSink(&iid_thread_focus, punk) } {
                    self.lifecycle.record(Registration::ThreadFocus(cookie));
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
                            self.lifecycle.record(Registration::TextEdit(ctx, cookie));
                            log::debug!("[TSF] ITfTextEditSink 注册成功");
                        }
                    }
                }
            }
        }

        let activated = self.lifecycle.finish_activation(remove_registration);
        if !matches!(activated, Ok(true)) {
            if !self.lifecycle.has_language_bar() {
                self.lang_bar_sink.lock().take();
            }
            self.input.terminated();
            *self.ga_input.lock() = 0;
            *self.ga_converted.lock() = 0;
            activated?;
            return Ok(());
        }

        log::info!("[TSF] TIP activated (tid={tid})");
        Ok(())
    }
}

impl ITfCompositionSink_Impl for TextService_Impl {
    fn OnCompositionTerminated(
        &self,
        _ecwrite: u32,
        _pcomp: Ref<'_, ITfComposition>,
    ) -> Result<()> {
        if self.ending_composition.load(Ordering::Acquire) {
            return Ok(());
        }
        let mut current = self.composition.lock();
        if let (Some(comp), Some(terminated)) = (current.as_ref(), _pcomp.as_ref()) {
            if Interface::as_raw(comp) != Interface::as_raw(terminated) {
                return Ok(());
            }
        }
        let old = current.take();
        drop(current);
        drop(old);
        self.input.terminated();
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
            Ok(make_display_attr_info(
                GUID_DISPLAY_ATTR_INPUT,
                "编码输入态",
                attr_input(),
            ))
        } else if *guid_safe == GUID_DISPLAY_ATTR_CONVERTED {
            Ok(make_display_attr_info(
                GUID_DISPLAY_ATTR_CONVERTED,
                "候选态",
                attr_converted(),
            ))
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
        self.input.lose_focus(|op| self.run_edit(op, None));
        Ok(())
    }
}

impl ITfTextEditSink_Impl for TextService_Impl {
    fn OnEndEdit(
        &self,
        _pic: Ref<'_, ITfContext>,
        _ecreadonly: u32,
        _peditrecord: Ref<'_, ITfEditRecord>,
    ) -> Result<()> {
        // 如果 composition 被外部编辑破坏，ITfCompositionSink::OnCompositionTerminated
        // 会收到通知并清理状态。此处仅做日志监控。
        if self.composition.lock().is_some() {
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
    fn GetLayout(
        &self,
        ptkblayouttype: *mut TKBLayoutType,
        pwpreferredlayoutid: *const u16,
    ) -> Result<()> {
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

    fn GetFunction(
        &self,
        rguid: *const windows_core::GUID,
        riid: *const windows_core::GUID,
    ) -> Result<windows_core::IUnknown> {
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
        if pdimfocus.as_ref() != _pdimprevfocus.as_ref() {
            self.input.lose_focus(|op| self.run_edit(op, None));
        }
        let new_mgr = pdimfocus.as_ref().cloned();
        let old = std::mem::replace(&mut *self.focus_doc_mgr.lock(), new_mgr);
        drop(old);
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
        log::debug!(
            "[TSF] KeyEventSink OnSetFocus foreground={:?}",
            fforeground.0
        );
        if !fforeground.as_bool() {
            self.input.lose_focus(|op| self.run_edit(op, None));
        }
        Ok(())
    }

    fn OnTestKeyDown(
        &self,
        _pic: Ref<'_, ITfContext>,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> Result<BOOL> {
        if !self.prepare_context(_pic.as_ref())? {
            return Ok(BOOL(0));
        }
        Ok(BOOL(
            self.translated_key(wparam, lparam)
                .is_some_and(|event| self.input.test_key(&event, false)) as i32,
        ))
    }

    fn OnTestKeyUp(
        &self,
        _pic: Ref<'_, ITfContext>,
        _wparam: WPARAM,
        _lparam: LPARAM,
    ) -> Result<BOOL> {
        Ok(BOOL(0))
    }

    fn OnKeyDown(&self, pic: Ref<'_, ITfContext>, wparam: WPARAM, lparam: LPARAM) -> Result<BOOL> {
        if !self.prepare_context(pic.as_ref())? {
            return Ok(BOOL(0));
        }
        let Some(event) = self.translated_key(wparam, lparam) else {
            return Ok(BOOL(0));
        };
        // GetWnd 可能在 WinRT 自定义编辑控件中返回空句柄；回退到 TSF 线程焦点。
        let owner = unsafe {
            pic.as_ref()
                .and_then(|context| context.GetActiveView().ok())
                .and_then(|view| view.GetWnd().ok())
                .filter(|hwnd| !hwnd.is_invalid())
                .unwrap_or_else(|| windows::Win32::UI::Input::KeyboardAndMouse::GetFocus())
        };
        {
            let mut window = self.candidate_window.lock();
            if window.as_ref().is_some_and(|window| !window.is_running()) {
                window.take();
            }
            if let Some(window) = window.as_ref() {
                window.set_owner(owner);
            } else {
                *window = Some(CandidateWindow::spawn(
                    Arc::clone(&self.candidate_tx),
                    owner,
                ));
            }
        }
        let theme = self.theme_snapshot();
        let anchor = self
            .candidate_tx
            .load()
            .anchor
            .or_else(|| crate::screen_geometry::get_caret_position_win32(theme.font_size))
            .or_else(crate::screen_geometry::get_cursor_position);
        Ok(BOOL(
            self.input
                .handle(event, false, anchor, |op| self.run_edit(op, pic.as_ref()))
                as i32,
        ))
    }

    fn OnKeyUp(&self, _pic: Ref<'_, ITfContext>, _wparam: WPARAM, _lparam: LPARAM) -> Result<BOOL> {
        Ok(BOOL(0))
    }

    fn OnPreservedKey(&self, _pic: Ref<'_, ITfContext>, rguid: *const GUID) -> Result<BOOL> {
        let guid = unsafe { &*rguid };
        if *guid == GUID_PRESERVED_SHIFT {
            let cfg = self.config_snapshot();
            if cfg.basic.switch_key != SwitchKey::Shift
                || self.shift_toggle_tracker.lock().consume_preserved_key()
            {
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

        assert!(service.lifecycle.manager().is_none());
    }

    #[test]
    fn same_com_service_can_activate_after_deactivation() {
        // TSF (including SearchApp) may retain the COM object across profile switches.
        std::thread::spawn(|| unsafe {
            use windows::Win32::System::Com::{
                CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED,
            };
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap();
            {
                let thread_mgr_clsid = GUID::from_u128(0x529a9e6b_6587_4f23_ab9e_9c7d683e3c50);
                let manager: ITfThreadMgr =
                    CoCreateInstance(&thread_mgr_clsid, None, CLSCTX_INPROC_SERVER).unwrap();
                let tid = manager.Activate().unwrap();
                let service = com_text_service();
                service.set_self_unknown(service.to_interface());
                let processor: ITfTextInputProcessorEx = service.to_interface();
                for _ in 0..2 {
                    processor.ActivateEx(&manager, tid, 0).unwrap();
                    assert!(service.lifecycle.manager().is_some());
                    processor.Deactivate().unwrap();
                    assert!(service.lifecycle.owner().is_none());
                }
                manager.Deactivate().unwrap();
            }
            CoUninitialize();
        })
        .join()
        .unwrap();
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

        let error = unsafe { source.AdviseSink(&<ITfLangBarItemSink as Interface>::IID, &unknown) }
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
}
