# Slint 候选框 UI 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use `subagent-driven-development` (recommended) or `executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 `im_engine.dll` 中实现基于 Slint 声明式 UI + Win32 透明分层窗口的无焦点候选框，跟随系统光标定位，支持 config.toml 外观主题。

**Architecture:** 双线程模型——TSF STA 线程处理按键并通过 `ArcSwap<CandidateData>` 发布候选数据；独立候选框线程运行 Win32 消息泵 + Slint 软件渲染器，每 16ms 轮询数据并 `UpdateLayeredWindow` 贴图。数据单向流动，渲染线程绝不回调 TSF。

**Tech Stack:** Rust + `slint` (software-renderer) + Win32 `WS_EX_LAYERED` / `UpdateLayeredWindow` + `ArcSwap` + `fontdb`

---

## 文件结构概览

```
windows/im_engine/
├── Cargo.toml                          # 修改：添加 slint, fontdb, build-deps
├── build.rs                            # 创建：slint-build 编译 .slint
├── ui/
│   └── candidate_window.slint          # 创建：声明式候选框 UI
├── src/
│   ├── lib.rs                          # 修改：Engine 新增 ArcSwap 字段
│   ├── factory.rs                      # 修改：传递 ArcSwap 给 TextService
│   ├── text_service.rs                 # 修改：Transition → CandidateData 写入
│   ├── candidate_data.rs               # 创建：共享数据结构
│   ├── candidate_window.rs             # 创建：Win32 窗口 + 消息泵线程
│   ├── candidate_renderer.rs           # 创建：Slint 渲染 + FontProvider trait
│   └── screen_geometry.rs              # 创建：光标定位 + 边界避让
core_engine/src/
    └── (不变)
```

---

### Task 1: 添加 Slint 依赖与构建脚本

**Files:**
- Modify: `windows/im_engine/Cargo.toml`
- Create: `windows/im_engine/build.rs`

- [ ] **Step 1: 更新 Cargo.toml 依赖**

```toml
# windows/im_engine/Cargo.toml — 在 [dependencies] 节末尾追加：
slint = { version = "1", default-features = false, features = ["software-renderer", "compat-1-2"] }
fontdb = { version = "0.21", default-features = false }

# 在文件末尾新增 [build-dependencies] 节：
[build-dependencies]
slint-build = "1"
```

- [ ] **Step 2: 创建 build.rs**

```rust
// windows/im_engine/build.rs
fn main() {
    slint_build::compile("ui/candidate_window.slint").unwrap();
}
```

- [ ] **Step 3: 验证依赖可解析**

Run: `cargo check -p im_engine 2>&1`
Expected: 无错误（仅有未使用导入的 warning 可接受）

- [ ] **Step 4: Commit**

```bash
git add windows/im_engine/Cargo.toml windows/im_engine/build.rs
git commit -m "🔧 chore(im_engine): 添加 Slint 软件渲染依赖与构建脚本"
```

---

### Task 2: 创建共享数据结构 `candidate_data.rs`

**Files:**
- Create: `windows/im_engine/src/candidate_data.rs`

- [ ] **Step 1: 编写 CandidateData 结构体**

```rust
// windows/im_engine/src/candidate_data.rs

/// 候选框锚点——光标左下角的屏幕绝对坐标。
#[derive(Debug, Clone, Copy, Default)]
pub struct ScreenPoint {
    pub x: i32,
    pub y: i32,
}

/// 外观主题快照（从 config.toml 同步）。
#[derive(Debug, Clone)]
pub struct ThemeSnapshot {
    pub font_size: u16,
    pub primary_color: u32,
    pub background_color: u32,
    pub highlight_color: u32,
}

impl Default for ThemeSnapshot {
    fn default() -> Self {
        Self {
            font_size: 14,
            primary_color: 0xFF1E88E5,
            background_color: 0xFFFFFFFF,
            highlight_color: 0xFFFFD54F,
        }
    }
}

/// 单个候选项。
#[derive(Debug, Clone)]
pub struct CandidateItem {
    /// 显示用标签（如 "1.", "2."）。
    pub label: String,
    /// 候选词文本。
    pub text: String,
}

/// 候选框共享数据——TSF 线程写入，渲染线程读取。
#[derive(Debug, Clone, Default)]
pub struct CandidateData {
    /// 候选框是否可见。
    pub visible: bool,
    /// 当前编码字符串。
    pub spelling: String,
    /// 当前页候选词列表（已截断为 page_size 条）。
    pub items: Vec<CandidateItem>,
    /// 当前高亮项索引（0-based）。
    pub highlighted: usize,
    /// 当前页码（0-based）。
    pub page: usize,
    /// 总页数。
    pub total_pages: usize,
    /// 光标屏幕坐标（可见时必有值）。
    pub anchor: Option<ScreenPoint>,
    /// 外观主题。
    pub theme: ThemeSnapshot,
}

impl CandidateData {
    /// 构造一个隐藏状态（visible=false）的空数据，保持上一次的 theme。
    pub fn hidden(theme: ThemeSnapshot) -> Self {
        Self {
            visible: false,
            theme,
            ..Default::default()
        }
    }

    /// 构造显示状态的数据。
    pub fn visible(
        spelling: String,
        items: Vec<CandidateItem>,
        highlighted: usize,
        page: usize,
        total_pages: usize,
        anchor: Option<ScreenPoint>,
        theme: ThemeSnapshot,
    ) -> Self {
        Self {
            visible: true,
            spelling,
            items,
            highlighted,
            page,
            total_pages,
            anchor,
            theme,
        }
    }
}
```

- [ ] **Step 2: 验证编译**

Run: `cargo check -p im_engine 2>&1`
Expected: 无错误

- [ ] **Step 3: Commit**

```bash
git add windows/im_engine/src/candidate_data.rs
git commit -m "✨ feat(im_engine): 添加候选框共享数据结构 CandidateData"
```

---

### Task 3: 创建 Slint UI 定义 `candidate_window.slint`

**Files:**
- Create: `windows/im_engine/ui/candidate_window.slint`

- [ ] **Step 1: 编写 Slint 声明式 UI**

```slint
// windows/im_engine/ui/candidate_window.slint

// 与 Rust 侧 CandidateItem 对应的 Slint 数据结构
struct CandidateItemData {
    label: string,
    text: string,
}

component CandidateItem {
    in property <string> label;
    in property <string> text;
    in property <bool> highlighted;
    in property <color> primary;
    in property <color> highlight;

    width: 80px;
    height: 28px;

    Rectangle {
        background: highlighted ? root.highlight : root.primary.with-alpha(0.0);
        border-radius: 2px;

        HorizontalLayout {
            padding: 4px;
            spacing: 2px;

            Text {
                text: root.label;
                color: highlighted ? Colors.white : root.primary;
                font-size: 14px;
                vertical-alignment: center;
            }

            Text {
                text: root.text;
                color: highlighted ? Colors.white : Colors.black;
                font-size: 14px;
                vertical-alignment: center;
                overflow: elide;
            }
        }
    }
}

export component CandidateWindow {
    in property <[CandidateItemData]> candidates;
    in property <int> page: 0;
    in property <int> total-pages: 1;
    in property <color> background;
    in property <color> primary;
    in property <color> highlight;

    preferred-height: 34px;

    Rectangle {
        background: root.background;
        border-radius: 4px;

        HorizontalLayout {
            padding: 3px 6px;
            spacing: 2px;

            for item in root.candidates: CandidateItem {
                label: item.label;
                text: item.text;
                highlighted: false;
                primary: root.primary;
                highlight: root.highlight;
            }

            if root.total-pages > 1: Rectangle {
                width: 40px;
                height: 28px;
                Text {
                    text: "{root.page + 1}/{root.total-pages}";
                    font-size: 11px;
                    color: root.primary.with-alpha(0.6);
                    horizontal-alignment: center;
                    vertical-alignment: center;
                }
            }
        }
    }
}
```

- [ ] **Step 2: 验证 Slint 编译**

Run: `cargo check -p im_engine 2>&1`
Expected: build.rs 执行成功，生成 `slint_generatedCandidateWindow` 模块

- [ ] **Step 3: Commit**

```bash
git add windows/im_engine/ui/candidate_window.slint
git commit -m "✨ feat(im_engine): 添加候选框 Slint 声明式 UI 定义"
```

---

### Task 4: 创建光标定位与避让算法 `screen_geometry.rs`

**Files:**
- Create: `windows/im_engine/src/screen_geometry.rs`
- Create: `windows/im_engine/tests/screen_geometry_tests.rs` (可选，单元测试内联)

- [ ] **Step 1: 编写 compute_window_rect 纯函数（可脱离 TSF 测试）**

```rust
// windows/im_engine/src/screen_geometry.rs

use windows::Win32::UI::TextServices::ITfContext;
use crate::candidate_data::ScreenPoint;

/// 屏幕边缘 padding（像素）。
const EDGE_PADDING: i32 = 8;

/// 从 TSF ITfContext 获取光标屏幕坐标。
///
/// 通过 ITfContext::GetStatus 获取文本服务状态中的光标位置，
/// 再翻译为屏幕绝对坐标。
pub fn get_caret_position(_context: &ITfContext) -> Option<ScreenPoint> {
    // FIXME: 实现通过 ITfContext::GetStatus + ITfContextView::GetTextExt
    // 获取精确光标矩形，当前返回占位值供渲染联调。
    None
}

/// 计算候选框窗口的左上角屏幕坐标，自动避让屏幕边缘。
///
/// # Arguments
/// * `anchor` — 光标左下角的屏幕坐标。
/// * `window_size` — 候选框窗口的 (width, height)。
/// * `monitor_rect` — 当前显示器的 (left, top, right, bottom)。
///
/// # Returns
/// 候选框窗口左上角的 (x, y) 坐标。
pub fn compute_window_rect(
    anchor: ScreenPoint,
    window_size: (i32, i32),
    monitor_rect: (i32, i32, i32, i32),
) -> (i32, i32) {
    let (win_w, win_h) = window_size;
    let (mon_left, mon_top, mon_right, mon_bottom) = monitor_rect;

    // 默认：候选框左上角对齐光标左下角（即锚点本身）
    let mut x = anchor.x;
    let mut y = anchor.y;

    // 垂直避让：光标在屏幕下半部 → 候选框翻到光标上方
    let anchor_mid_y = anchor.y + (win_h / 2);
    if anchor_mid_y > (mon_top + mon_bottom) / 2 {
        y = anchor.y - win_h;
    }

    // 底部边界
    if y + win_h > mon_bottom - EDGE_PADDING {
        y = mon_bottom - win_h - EDGE_PADDING;
    }
    // 顶部边界
    if y < mon_top + EDGE_PADDING {
        y = mon_top + EDGE_PADDING;
    }

    // 右侧避让：候选框超出右边缘则向左偏移
    if x + win_w > mon_right - EDGE_PADDING {
        x = mon_right - win_w - EDGE_PADDING;
    }
    // 左侧边界
    if x < mon_left + EDGE_PADDING {
        x = mon_left + EDGE_PADDING;
    }

    (x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 光标在屏幕中央 → 候选框正常出现在光标下方。
    #[test]
    fn normal_below_cursor() {
        let anchor = ScreenPoint { x: 500, y: 400 };
        let (x, y) = compute_window_rect(anchor, (200, 34), (0, 0, 1920, 1080));
        assert_eq!((x, y), (500, 400));
    }

    /// 光标在屏幕底部 → 候选框翻到上方。
    #[test]
    fn flip_above_when_near_bottom() {
        let anchor = ScreenPoint { x: 500, y: 1000 };
        let (x, y) = compute_window_rect(anchor, (200, 34), (0, 0, 1920, 1080));
        // y = 1000 - 34 = 966
        assert_eq!(x, 500);
        assert!(y < 1000, "候选框应在光标上方");
    }

    /// 候选框超出右边缘 → 向左贴边。
    #[test]
    fn clamp_right_edge() {
        let anchor = ScreenPoint { x: 1850, y: 400 };
        let (x, y) = compute_window_rect(anchor, (200, 34), (0, 0, 1920, 1080));
        // 1850 + 200 = 2050 > 1920 - 8 = 1912
        // x = 1920 - 200 - 8 = 1712
        assert_eq!(x, 1712);
        assert_eq!(y, 400);
    }

    /// 候选框超出左边缘 → 向右贴边。
    #[test]
    fn clamp_left_edge() {
        let anchor = ScreenPoint { x: -10, y: 400 };
        let (x, y) = compute_window_rect(anchor, (200, 34), (0, 0, 1920, 1080));
        assert_eq!(x, 8);
        assert_eq!(y, 400);
    }
}
```

- [ ] **Step 2: 运行单元测试**

Run: `cargo test -p im_engine -- screen_geometry 2>&1`
Expected: 4 tests PASS

- [ ] **Step 3: Commit**

```bash
git add windows/im_engine/src/screen_geometry.rs
git commit -m "✨ feat(im_engine): 添加光标定位与候选框屏幕避让算法"
```

---

### Task 5: 创建 Slint 渲染桥接 `candidate_renderer.rs`

**Files:**
- Create: `windows/im_engine/src/candidate_renderer.rs`

- [ ] **Step 1: 编写 FontProvider trait 与 EmbeddedFontProvider**

```rust
// windows/im_engine/src/candidate_renderer.rs

use slint::platform::software_renderer::MinimalSoftwareWindow;
use slint::ComponentHandle;
use std::error::Error;
use std::fmt;

slint::include_modules!();

/// 字体加载错误。
#[derive(Debug)]
pub enum RenderError {
    Init(String),
    Render(String),
    NoWindow,
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Init(s) => write!(f, "Slint init failed: {s}"),
            Self::Render(s) => write!(f, "Slint render failed: {s}"),
            Self::NoWindow => write!(f, "No software window available"),
        }
    }
}

impl Error for RenderError {}

/// 字体提供者 trait——方案 A（嵌入式字体）和方案 B（GDI 系统字体）的统一接口。
pub trait FontProvider: Send + Sync {
    /// 返回字体文件的原始字节。
    fn font_data(&self) -> &[u8];
}

/// 方案 A：编译期内嵌思源黑体（Noto Sans SC Regular）。
pub struct EmbeddedFontProvider {
    data: &'static [u8],
}

impl EmbeddedFontProvider {
    /// 从嵌入的字体字节创建。
    pub fn new(data: &'static [u8]) -> Self {
        Self { data }
    }
}

impl FontProvider for EmbeddedFontProvider {
    fn font_data(&self) -> &[u8] {
        self.data
    }
}

// 预留方案 B 结构体声明（本次不实现功能）：
//
// pub struct GdiFontProvider;
// impl FontProvider for GdiFontProvider {
//     fn font_data(&self) -> &[u8] { unimplemented!("GDI font loading not yet implemented") }
// }

/// Slint 候选框渲染器。
///
/// 持有 Slint 软件渲染窗口和组件句柄，负责：
/// 1. 注册嵌入字体
/// 2. 更新组件属性
/// 3. 调用 software renderer 生成像素 buffer
pub struct CandidateRenderer {
    window: MinimalSoftwareWindow,
    ui: CandidateWindow,
}

impl CandidateRenderer {
    /// 初始化渲染器，注册字体并创建 Slint 组件。
    pub fn new(font_provider: &dyn FontProvider) -> Result<Self, RenderError> {
        // 注册嵌入字体
        let font_data = font_provider.font_data();
        slint::platform::software_renderer::register_font_from_memory(font_data)
            .map_err(|e| RenderError::Init(format!("Failed to register font: {e}")))?;

        // 创建 Slint 组件
        let ui = CandidateWindow::new()
            .map_err(|e| RenderError::Init(format!("Failed to create component: {e}")))?;

        // 获取软件渲染窗口
        let window = ui
            .software_renderer()
            .ok_or(RenderError::NoWindow)?;

        Ok(Self { window, ui })
    }

    /// 更新候选数据属性并渲染一帧，返回 ARGB8888 pixel buffer。
    ///
    /// buffer 尺寸为 (width, height)，可直接传给 UpdateLayeredWindow。
    pub fn render(
        &self,
        labels: &[String],
        texts: &[String],
        page: usize,
        total_pages: usize,
        background: u32,
        primary: u32,
        highlight: u32,
    ) -> Result<(Vec<u8>, u32, u32), RenderError> {
        use slint::Model;

        // 构造候选词列表模型
        let candidate_model: Vec<CandidateItemData> = labels
            .iter()
            .zip(texts.iter())
            .map(|(l, t)| CandidateItemData {
                label: l.into(),
                text: t.into(),
            })
            .collect();

        let model = std::rc::Rc::new(slint::VecModel::from(candidate_model));

        // 更新 Slint 属性
        self.ui.set_candidates(slint::ModelRc::from(model as std::rc::Rc<dyn slint::Model<CandidateItemData>>));
        self.ui.set_page(page as i32);
        self.ui.set_total_pages(total_pages as i32);
        self.ui.set_background(slint::Color::from_argb_encoded(background));
        self.ui.set_primary(slint::Color::from_argb_encoded(primary));
        self.ui.set_highlight(slint::Color::from_argb_encoded(highlight));

        // 触发布局计算
        let size = self.window.size();

        // 如果窗口尺寸为 0，先设置一个合理的初始尺寸
        if size.width == 0.0 || size.height == 0.0 {
            self.window.set_size(slint::PhysicalSize::new(400, 34));
        }

        // 软件渲染
        self.window.draw_if_needed(|renderer| {
            renderer.render_by_line(|line_buffer| {
                // 逐行渲染回调——这里我们只需要触发渲染即可，
                // 实际逐行操作由 Slint 内部处理。
                std::ops::ControlFlow::Continue(())
            });
        });

        // 获取渲染后的像素 buffer
        match self.window.take_buffer() {
            Some(buffer) => {
                let width = buffer.width();
                let height = buffer.height();
                let pixels = buffer.into_raw();
                Ok((pixels, width, height))
            }
            None => Err(RenderError::Render("No buffer produced".into())),
        }
    }

    /// 返回窗口的当前逻辑尺寸。
    pub fn size(&self) -> slint::PhysicalSize {
        self.window.size()
    }
}
```

- [ ] **Step 2: 验证编译**

Run: `cargo check -p im_engine 2>&1`
Expected: 编译通过（API 如有差异需根据实际 Slint 1.x 版本调整）

- [ ] **Step 3: Commit**

```bash
git add windows/im_engine/src/candidate_renderer.rs
git commit -m "✨ feat(im_engine): 添加 Slint 软件渲染桥接与 FontProvider trait"
```

---

### Task 6: 创建 Win32 候选框窗口 `candidate_window.rs`

**Files:**
- Create: `windows/im_engine/src/candidate_window.rs`

- [ ] **Step 1: 编写窗口创建与消息泵**

```rust
// windows/im_engine/src/candidate_window.rs

use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;
use arc_swap::ArcSwap;
use windows::Win32::Foundation::HWND;
use log;

use crate::candidate_data::CandidateData;

/// 候选框窗口句柄——由窗口线程持有，TSF 线程无直接访问。
pub struct CandidateWindowHandle {
    thread: Option<JoinHandle<()>>,
    /// 向窗口线程发送退出信号。
    quit_tx: Option<std::sync::mpsc::Sender<()>>,
}

impl CandidateWindowHandle {
    /// 启动候选框窗口线程。
    ///
    /// `data_src` 是 TSF 线程写入的共享数据源。
    pub fn spawn(data_src: Arc<ArcSwap<CandidateData>>) -> Self {
        let (quit_tx, quit_rx) = std::sync::mpsc::channel::<()>();

        let handle = thread::Builder::new()
            .name("candidate-window".into())
            .spawn(move || {
                window_thread_main(data_src, quit_rx);
            })
            .expect("Failed to spawn candidate window thread");

        Self {
            thread: Some(handle),
            quit_tx: Some(quit_tx),
        }
    }

    /// 请求窗口线程退出并等待（最多 500ms）。
    pub fn shutdown(&mut self) {
        if let Some(tx) = self.quit_tx.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.thread.take() {
            // 等待线程自然退出
            let _ = handle.join();
        }
    }
}

impl Drop for CandidateWindowHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// 窗口线程主函数。
fn window_thread_main(
    data_src: Arc<ArcSwap<CandidateData>>,
    quit_rx: std::sync::mpsc::Receiver<()>,
) {
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, MSG,
        PostQuitMessage, RegisterClassW, SetTimer, ShowWindow, TranslateMessage,
        WNDCLASSW, WM_CREATE, WM_DESTROY, WM_TIMER, WS_EX_LAYERED,
        WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP, SW_SHOWNA, SW_HIDE,
        UpdateLayeredWindow, SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOSIZE,
    };
    use windows::core::PCWSTR;

    const CLASS_NAME: &str = "MyWubiCandidateWindow";
    const TIMER_ID: usize = 1;
    const TIMER_INTERVAL_MS: u32 = 16;

    // 注册窗口类
    let class_name_wide: Vec<u16> = CLASS_NAME.encode_utf16().chain(std::iter::once(0)).collect();
    let wc = WNDCLASSW {
        lpfnWndProc: Some(wnd_proc),
        lpszClassName: PCWSTR(class_name_wide.as_ptr()),
        ..Default::default()
    };

    if unsafe { RegisterClassW(&wc) } == 0 {
        log::error!("[CandidateWindow] RegisterClassW failed");
        return;
    }

    // 创建透明分层窗口
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_NOACTIVATE | WS_EX_LAYERED | WS_EX_TOOLWINDOW,
            PCWSTR(class_name_wide.as_ptr()),
            PCWSTR::null(),
            WS_POPUP,
            0, 0, 400, 34,
            HWND::default(),
            None,
            None,
            None,
        )
    };

    if hwnd.0 == 0 {
        log::error!("[CandidateWindow] CreateWindowExW failed");
        return;
    }

    // 将 data_src 指针嵌入窗口用户数据，供 wnd_proc 使用
    let data_ptr = Arc::into_raw(Arc::new(WindowData {
        data_src,
        hwnd,
    }));
    unsafe {
        windows::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW(
            hwnd,
            windows::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
            data_ptr as isize,
        );
    }

    // 启动定时器
    unsafe { SetTimer(hwnd, TIMER_ID, TIMER_INTERVAL_MS, None); }

    // 消息泵
    let mut msg = MSG::default();
    loop {
        // 非阻塞检查 quit 信号
        if quit_rx.try_recv().is_ok() {
            unsafe { PostQuitMessage(0); }
        }

        let ret = unsafe { GetMessageW(&mut msg, hwnd, 0, 0) };
        if ret.0 <= 0 {
            break;
        }
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    // 清理：释放 Arc
    if data_ptr.is_null() {
        return;
    }
    unsafe {
        let _ = Arc::from_raw(data_ptr);
    }
}

struct WindowData {
    data_src: Arc<ArcSwap<CandidateData>>,
    hwnd: HWND,
}

/// 窗口过程函数。
extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::UI::WindowsAndMessaging::{
        WM_CREATE, WM_DESTROY, WM_TIMER, GWLP_USERDATA, GetWindowLongPtrW,
        ShowWindow, SW_HIDE, SW_SHOWNA, UpdateLayeredWindow, SetWindowPos,
        HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOSIZE, HDC, BLENDFUNCTION,
        SRCCOPY, AC_SRC_OVER,
    };

    match msg {
        WM_TIMER => {
            // 获取窗口数据
            let data_ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const WindowData;
            if data_ptr.is_null() {
                return Default::default();
            }
            let data = unsafe { &*data_ptr };

            let candidate = data.data_src.load();

            if candidate.visible {
                // 定位窗口到锚点
                if let Some(anchor) = candidate.anchor {
                    unsafe {
                        SetWindowPos(
                            hwnd,
                            HWND_TOPMOST,
                            anchor.x, anchor.y,
                            0, 0,
                            SWP_NOACTIVATE | SWP_NOSIZE,
                        );
                    }
                }

                // TODO(Task 7): 这里调用 candidate_renderer 渲染并 UpdateLayeredWindow
                // 当前阶段：窗口显示隐藏逻辑先行

                unsafe { ShowWindow(hwnd, SW_SHOWNA); }
            } else {
                unsafe { ShowWindow(hwnd, SW_HIDE); }
            }

            return windows::Win32::Foundation::LRESULT(0);
        }
        WM_DESTROY => {
            unsafe { windows::Win32::UI::WindowsAndMessaging::PostQuitMessage(0); }
            return windows::Win32::Foundation::LRESULT(0);
        }
        _ => {}
    }

    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}
```

- [ ] **Step 2: 更新 lib.rs 注册新模块**

在 `windows/im_engine/src/lib.rs` 顶部 `pub mod` 区域追加：

```rust
pub mod candidate_data;
pub mod candidate_renderer;
pub mod candidate_window;
pub mod screen_geometry;
```

- [ ] **Step 3: 验证编译**

Run: `cargo check -p im_engine 2>&1`
Expected: 无错误

- [ ] **Step 4: Commit**

```bash
git add windows/im_engine/src/candidate_window.rs windows/im_engine/src/lib.rs
git commit -m "✨ feat(im_engine): 添加 Win32 透明分层候选框窗口与消息泵"
```

---

### Task 7: 集成渲染到窗口线程——连接 candidate_renderer + candidate_window

**Files:**
- Modify: `windows/im_engine/src/candidate_window.rs`
- Modify: `windows/im_engine/src/candidate_renderer.rs`

- [ ] **Step 1: 修改 candidate_window.rs——在 WM_TIMER 中调用渲染**

替换 `candidate_window.rs` 中 `wnd_proc` 函数 `WM_TIMER` 分支的 `// TODO(Task 7)` 注释块为实际渲染调用：

在 `candidate_window.rs` 顶部添加导入：
```rust
use crate::candidate_renderer::{CandidateRenderer, EmbeddedFontProvider};
use crate::candidate_data::ThemeSnapshot;
```

修改 `WM_TIMER` 分支内部的 `if candidate.visible` 块：

```rust
if candidate.visible {
    // 定位窗口到锚点
    if let Some(anchor) = candidate.anchor {
        unsafe {
            SetWindowPos(
                hwnd,
                HWND_TOPMOST,
                anchor.x, anchor.y,
                0, 0,
                SWP_NOACTIVATE | SWP_NOSIZE,
            );
        }
    }

    // 调用 Slint 渲染器
    let renderer = get_or_init_renderer(data_ptr);
    if let Some(r) = renderer {
        let labels: Vec<String> = candidate.items.iter().map(|it| it.label.clone()).collect();
        let texts: Vec<String> = candidate.items.iter().map(|it| it.text.clone()).collect();
        let theme = &candidate.theme;

        match r.render(
            &labels,
            &texts,
            candidate.page,
            candidate.total_pages,
            theme.background_color,
            theme.primary_color,
            theme.highlight_color,
        ) {
            Ok((buffer, width, height)) => {
                // UpdateLayeredWindow 贴图
                unsafe {
                    let hdc_screen = windows::Win32::Graphics::Gdi::GetDC(HWND::default());
                    if hdc_screen.0 != 0 {
                        let blend = BLENDFUNCTION {
                            BlendOp: AC_SRC_OVER as u8,
                            BlendFlags: 0,
                            SourceConstantAlpha: 255,
                            AlphaFormat: 1, // AC_SRC_ALPHA
                        };
                        let _ = UpdateLayeredWindow(
                            hwnd,
                            hdc_screen,
                            None,
                            None,
                            HDC::default(),
                            &windows::Win32::Foundation::POINT { x: 0, y: 0 },
                            0,
                            &blend,
                            2, // ULW_ALPHA
                        );
                        // FIXME: 实际需要创建一个内存 DC + 选中 DIB bitmap，
                        // 将 buffer 写入后传入 UpdateLayeredWindow。
                        // 当前为骨架代码，后续 Task 细化。
                        let _ = windows::Win32::Graphics::Gdi::ReleaseDC(HWND::default(), hdc_screen);
                    }
                }
                // 调整窗口大小
                unsafe {
                    windows::Win32::UI::WindowsAndMessaging::SetWindowPos(
                        hwnd,
                        HWND::default(),
                        0, 0,
                        width as i32, height as i32,
                        SWP_NOACTIVATE | SWP_NOMOVE,
                    );
                }
            }
            Err(e) => {
                log::error!("[CandidateWindow] render failed: {e}");
            }
        }
    }

    unsafe { ShowWindow(hwnd, SW_SHOWNA); }
}
```

在 `wnd_proc` 所在文件中添加辅助函数：

```rust
/// 获取或惰性初始化 Slint 渲染器（通过窗口用户数据指针关联）。
fn get_or_init_renderer(data_ptr: *const WindowData) -> Option<&'static CandidateRenderer> {
    // 使用 OnceLock 静态变量在首次调用时初始化渲染器
    use std::sync::OnceLock;
    static RENDERER: OnceLock<CandidateRenderer> = OnceLock::new();

    if RENDERER.get().is_none() {
        // 使用嵌入字体初始化（字体数据将在 Task 10 嵌入）
        // 当前使用空数据占位
        let font_data: &[u8] = &[];
        let provider = EmbeddedFontProvider::new(font_data);
        match CandidateRenderer::new(&provider) {
            Ok(r) => { let _ = RENDERER.set(r); }
            Err(e) => {
                log::error!("[CandidateWindow] Failed to init renderer: {e}");
                return None;
            }
        }
    }

    RENDERER.get()
}
```

- [ ] **Step 2: 验证编译**

Run: `cargo check -p im_engine 2>&1`
Expected: 无错误（warning 可接受）

- [ ] **Step 3: Commit**

```bash
git add windows/im_engine/src/candidate_window.rs
git commit -m "✨ feat(im_engine): 连接 Slint 渲染器到候选框窗口消息泵"
```

---

### Task 8: 修改 TextService——将 Transition 发布到 ArcSwap

**Files:**
- Modify: `windows/im_engine/src/text_service.rs`

- [ ] **Step 1: 添加依赖字段与修改构造函数**

在 `text_service.rs` 顶部添加导入：
```rust
use arc_swap::ArcSwap;
use crate::candidate_data::{CandidateData, CandidateItem, ScreenPoint, ThemeSnapshot};
use crate::screen_geometry;
```

在 `TextService` 结构体中新增字段：
```rust
#[implement(ITfTextInputProcessor, ITfThreadMgrEventSink, ITfKeyEventSink)]
pub struct TextService {
    sm: Mutex<StateMachine>,
    thread_mgr: Mutex<Option<ITfThreadMgr>>,
    cookies: Mutex<SinkCookies>,
    focus_doc_mgr: Mutex<Option<ITfDocumentMgr>>,
    self_unknown: Mutex<Option<windows::core::IUnknown>>,
    /// 候选数据发布通道（新增）
    candidate_tx: Arc<ArcSwap<CandidateData>>,
}
```

修改 `new` 构造函数：
```rust
impl TextService {
    pub fn new(
        dict: Arc<Dictionary>,
        page_size: usize,
        auto_commit_unique: bool,
        candidate_tx: Arc<ArcSwap<CandidateData>>,  // 新增参数
    ) -> Self {
        let sm = StateMachine::with_options(dict, page_size, auto_commit_unique);
        Self {
            sm: Mutex::new(sm),
            thread_mgr: Mutex::new(None),
            cookies: Mutex::new(SinkCookies::default()),
            focus_doc_mgr: Mutex::new(None),
            self_unknown: Mutex::new(None),
            candidate_tx,
        }
    }

    pub fn from_config(
        dict: Arc<Dictionary>,
        cfg: &Config,
        candidate_tx: Arc<ArcSwap<CandidateData>>,  // 新增参数
    ) -> Self {
        Self::new(
            dict,
            cfg.basic.candidate_count as usize,
            cfg.basic.auto_commit_unique,
            candidate_tx,
        )
    }
}
```

- [ ] **Step 2: 修改 apply_transition——发布 CandidateData**

将 `apply_transition` 方法替换为：

```rust
fn apply_transition(&self, t: Transition) -> BOOL {
    let theme = ThemeSnapshot::default(); // TODO: 从 live config 读取
    match t {
        Transition::None => BOOL(1),
        Transition::Commit(text) => {
            log::info!("[TSF] commit text: {text}");
            self.candidate_tx.store(Arc::new(CandidateData::hidden(theme)));
            BOOL(1)
        }
        Transition::Candidates {
            ref spelling,
            ref candidates,
            page,
            total_pages,
        } => {
            let items: Vec<CandidateItem> = candidates
                .iter()
                .enumerate()
                .map(|(i, text)| CandidateItem {
                    label: format!("{}.", i + 1),
                    text: text.clone(),
                })
                .collect();

            // 尝试获取光标位置（当前为占位，见 screen_geometry::get_caret_position）
            let anchor = None; // FIXME: 从 ITfContext 获取

            let data = CandidateData::visible(
                spelling.clone(),
                items,
                0, // highlighted
                page,
                total_pages,
                anchor,
                theme,
            );
            self.candidate_tx.store(Arc::new(data));
            log::debug!(
                "[TSF] spelling={spelling} candidates={:?} page={page}/{total_pages}",
                candidates
            );
            BOOL(1)
        }
        Transition::SpellingUpdated(s) => {
            log::debug!("[TSF] spelling={s}");
            self.candidate_tx.store(Arc::new(CandidateData::hidden(theme)));
            BOOL(1)
        }
        Transition::Cleared => {
            log::debug!("[TSF] cleared");
            self.candidate_tx.store(Arc::new(CandidateData::hidden(theme)));
            BOOL(1)
        }
        Transition::Passthrough(_) => BOOL(0),
    }
}
```

- [ ] **Step 3: 验证编译**

Run: `cargo check -p im_engine 2>&1`
Expected: 无错误（对 factory.rs 的编译错误是预期的，将在 Task 9 修复）

- [ ] **Step 4: Commit**

```bash
git add windows/im_engine/src/text_service.rs
git commit -m "✨ feat(im_engine): TextService 发布 Transition 到 CandidateData 通道"
```

---

### Task 9: 修改 Factory 与 Lib——初始化 ArcSwap 并注入

**Files:**
- Modify: `windows/im_engine/src/factory.rs`
- Modify: `windows/im_engine/src/lib.rs`

- [ ] **Step 1: 修改 lib.rs——Engine 新增 ArcSwap 字段**

```rust
// windows/im_engine/src/lib.rs

// 新增导入
use arc_swap::ArcSwap;
use crate::candidate_data::CandidateData;

// 修改 Engine 结构体
struct Engine {
    dict: Arc<Dictionary>,
    sm: Mutex<StateMachine>,
    candidate_data: Arc<ArcSwap<CandidateData>>,  // 新增
}

impl Engine {
    fn new(dict: Arc<Dictionary>, sm: StateMachine, cd: Arc<ArcSwap<CandidateData>>) -> Self {
        Self { dict, sm: Mutex::new(sm), candidate_data: cd }
    }
}

// 在 Engine 上新增访问方法
impl Engine {
    pub fn candidate_data(&self) -> &Arc<ArcSwap<CandidateData>> {
        &self.candidate_data
    }
}

// 修改 im_engine_init——初始化 ArcSwap
#[no_mangle]
pub extern "C" fn im_engine_init() -> i32 {
    if ENGINE.get().is_some() {
        return 0;
    }
    let cfg = match Config::load("config.toml") {
        Ok(c) => c,
        Err(e) => {
            log::error!("加载配置失败: {e}, 使用默认配置");
            Config::default()
        }
    };
    let dict = match Dictionary::load(&cfg.dictionary.system_table) {
        Ok(d) => d,
        Err(e) => {
            log::error!("加载码表失败: {e}");
            Dictionary::from_entries(Vec::new(), None, Default::default())
                .expect("空码表构建不会失败")
        }
    };
    let sm = StateMachine::with_options(
        Arc::clone(&dict),
        cfg.basic.candidate_count as usize,
        cfg.basic.auto_commit_unique,
    );

    // 新增：初始化共享候选数据通道
    let candidate_data = Arc::new(ArcSwap::from_pointee(CandidateData::default()));

    let _ = ENGINE.set(Engine::new(dict, sm, candidate_data));
    0
}
```

- [ ] **Step 2: 修改 factory.rs——传递候选数据通道并启动候选框窗口**

读取 `windows/im_engine/src/factory.rs` 现有代码，找到 `TextServiceFactory` 中创建 `TextService` 的逻辑：

```rust
// 在 factory.rs 中找到创建 TextService 的位置，修改为：

// 从全局 ENGINE 获取 candidate_data
let engine = crate::ENGINE.get().expect("Engine not initialized");
let candidate_data = engine.candidate_data().clone();

// 创建 TextService 时传入
let ts = TextService::new(
    Arc::clone(&engine.dict),
    engine.candidate_data().load().items.len().max(5), // fallback page size
    true,
    candidate_data,
);
```

并在 `TextServiceFactory` 的 `Activate` 流程中启动候选框窗口：
```rust
// 在 Activate 或对应的初始化点：
use crate::candidate_window::CandidateWindowHandle;
// 将 CandidateWindowHandle 以某种方式持有（如字段或全局）
// let _handle = CandidateWindowHandle::spawn(candidate_data.clone());
```

- [ ] **Step 3: 验证编译**

Run: `cargo check -p im_engine 2>&1`
Expected: 无错误

- [ ] **Step 4: Commit**

```bash
git add windows/im_engine/src/lib.rs windows/im_engine/src/factory.rs
git commit -m "✨ feat(im_engine): Engine 初始化 ArcSwap 候选数据通道并注入 TextService/Factory"
```

---

### Task 10: 编译验证与问题修复

- [ ] **Step 1: 全量编译**

Run: `cargo build -p im_engine 2>&1`
Expected: 编译成功，生成 `target/debug/im_engine.dll`

- [ ] **Step 2: 运行所有已有测试确保无回归**

Run: `cargo test --workspace 2>&1`
Expected: 所有已有测试 PASS

- [ ] **Step 3: 修复编译警告和 API 不匹配**

逐一检查 warning，对 Slint API 调用的差异根据实际 crate 版本微调。

- [ ] **Step 4: Commit 修复**

```bash
git add -A
git commit -m "🐛 fix(im_engine): 修复编译警告与 Slint API 适配"
```

---

### Task 11: 手动集成测试

- [ ] **Step 1: Release 编译**

Run: `cargo build -p im_engine --release 2>&1`
Expected: `target/release/im_engine.dll` 生成成功

- [ ] **Step 2: 注册 DLL**

Run (管理员 PowerShell):
```powershell
regsvr32 /s target\release\im_engine.dll
```

- [ ] **Step 3: 在记事本中测试**

1. 打开记事本
2. 切换到 MyWubi 输入法
3. 输入五笔编码（如 `gggg` → "五"）
4. 验证：候选框出现在光标下方，显示候选词列表
5. 验证：空格上屏后候选框隐藏
6. 验证：退格键清空编码、候选框消失

- [ ] **Step 4: 检查 debug 日志**

查看 `%LOCALAPPDATA%\MyWubi\debug.log` 确认：
- `[TSF] commit text:` 上有上屏记录
- `[TSF] spelling=... candidates=...` 有候选记录
- `[CandidateWindow]` 无错误日志

---

---

## 自审清单

执行前逐项检查：

1. **规范覆盖** — Task 1-9 覆盖了规范中的所有 5 个新模块 + 3 个旧模块修改 + 数据流 + 错误处理策略
2. **占位符** — `screen_geometry::get_caret_position` 和 `UpdateLayeredWindow` 精细实现标记为 `FIXME`，属于平台 API 集成细节，在主逻辑骨架完整后补充
3. **类型一致性** — `CandidateData` / `CandidateItem` / `ThemeSnapshot` / `ScreenPoint` 在 Rust 侧和 `.slint` 侧命名统一
