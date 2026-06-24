// windows/im_engine/src/candidate_renderer.rs
//! Slint 软件渲染候选框渲染桥接。
//!
//! 提供 `CandidateRenderer`，将上层候选数据 ([`CandidateItem`]) 映射到
//! Slint [`CandidateWindow`] 组件的属性，并通过 `Window::take_snapshot()`
//! 执行离屏软件渲染，返回 RGBA8 像素缓冲区。

use std::error::Error;
use std::fmt;
use std::rc::Rc;

use crate::candidate_data::{CandidateItem, ThemeSnapshot};

slint::include_modules!();

// ── FontProvider trait ──────────────────────────────────────────

/// 字体数据加载抽象。
///
/// `CandidateRenderer::new` 通过此 trait 获取字体字节数据。
///
/// **注意**: Slint 1.16 尚未公开 `register_font_from_memory`。
/// 字体注册功能预留待后续版本补齐，当前 `new()` 中不执行实际注册。
pub trait FontProvider {
    /// 返回字体文件的原始字节（如 `.ttf` 文件内容）。
    fn font_data(&self) -> &[u8];
}

// ── 方案 A: EmbeddedFontProvider ────────────────────────────────

/// 嵌入字体提供者 —— 持有编译期嵌入的 `&'static [u8]` 字体数据。
///
/// 适用于将字体文件通过 `include_bytes!` 直接编译进二进制。
pub struct EmbeddedFontProvider {
    data: &'static [u8],
}

impl EmbeddedFontProvider {
    /// 使用静态字体数据构造提供者。
    pub fn new(data: &'static [u8]) -> Self {
        Self { data }
    }
}

impl FontProvider for EmbeddedFontProvider {
    fn font_data(&self) -> &[u8] {
        self.data
    }
}

// ── 方案 B: GdiFontProvider（预留）────────────────────────────

// TODO(silev): 待方案 B 实现后启用 GdiFontProvider。当前仅保留结构声明，
// 方法标记 unimplemented!() 防止误用。
/// GDI 字体提供者（预留，尚未实现）。
///
/// 未来将通过 GDI `AddFontMemResourceEx` 等方式加载系统字体。
pub struct GdiFontProvider;

#[allow(unused)]
impl GdiFontProvider {
    /// 创建 GDI 字体提供者（当前不可用，调用时将 panic）。
    pub fn new() -> Self {
        unimplemented!("GdiFontProvider is not yet implemented")
    }
}

impl FontProvider for GdiFontProvider {
    fn font_data(&self) -> &[u8] {
        unimplemented!("GdiFontProvider::font_data is not yet implemented")
    }
}

// ── RenderError ──────────────────────────────────────────────────

/// 渲染过程中的自定义错误类型。
#[derive(Debug)]
pub enum RenderError {
    /// 初始化阶段错误（如创建组件失败）。
    Init(String),
    /// 渲染阶段错误（如截图失败）。
    Render(String),
    /// 无法获取软件渲染窗口句柄（通常因为 Slint 后端未正确配置）。
    NoWindow,
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RenderError::Init(msg) => write!(f, "初始化渲染器失败: {msg}"),
            RenderError::Render(msg) => write!(f, "渲染失败: {msg}"),
            RenderError::NoWindow => f.write_str("无法获取 Slint 软件渲染窗口"),
        }
    }
}

impl Error for RenderError {}

// ── CandidateRenderer ────────────────────────────────────────────

/// Slint 软件渲染候选框渲染器。
///
/// 封装 `CandidateWindow` 组件，通过 `Window::take_snapshot()`
/// 执行离屏渲染并返回 RGBA8 像素缓冲区。
pub struct CandidateRenderer {
    ui: CandidateWindow,
}

impl CandidateRenderer {
    /// 创建新的渲染器实例。
    ///
    /// 1. 创建 `CandidateWindow` 组件实例；
    /// 2. `FontProvider` 字体注册暂为预留（Slint 1.16 未提供公开 API）。
    pub fn new(font_provider: &dyn FontProvider) -> Result<Self, RenderError> {
        // ── 显式选择软件渲染后端 ──
        // TODO(silev): Slint 1.16 的 SoftwareRenderer 未实现 Platform trait，
        // set_platform(Box::new(SoftwareRenderer::new())) 无法直接编译。
        // 软件渲染在 renderer-software feature 下由后端选择器自动选中。
        // 待 Slint 官方提供独立软件渲染平台后端后改用显式初始化。

        // ── 字体注册 ──
        // TODO(silev): Slint 1.16 的 software-renderer 暂未公开
        // register_font_from_memory API。待后续版本支持后注册嵌入字体。
        let _ = font_provider;

        let ui = CandidateWindow::new()
            .map_err(|e| RenderError::Init(format!("创建 CandidateWindow 失败: {e}")))?;

        Ok(Self { ui })
    }

    /// 更新组件属性并执行渲染。
    ///
    /// # Parameters
    /// - `candidates` — 当前页候选词列表。
    /// - `highlighted` — 当前高亮项索引（0-based）。
    /// - `page` — 当前页码（0-based）。
    /// - `total_pages` — 总页数。
    /// - `theme` — 外观主题快照。
    ///
    /// # Returns
    /// `(pixel_buffer, width, height)`：
    /// - `pixel_buffer` — RGBA8 格式的原始像素数据。
    /// - `width`, `height` — 缓冲区尺寸。
    pub fn render(
        &self,
        candidates: &[CandidateItem],
        highlighted: usize,
        page: usize,
        total_pages: usize,
        theme: &ThemeSnapshot,
    ) -> Result<(Vec<u8>, u32, u32), RenderError> {
        // ── 构建候选项模型 ──
        let items: Vec<CandidateItemData> = candidates
            .iter()
            .enumerate()
            .map(|(i, c)| CandidateItemData {
                label: c.label.as_str().into(),
                text: c.text.as_str().into(),
                highlighted: i == highlighted,
            })
            .collect();
        let model = Rc::new(slint::VecModel::from(items));
        let model_rc = slint::ModelRc::from(model);

        // ── 更新组件属性 ──
        self.ui.set_candidates(model_rc);
        self.ui.set_page(page as i32);
        self.ui.set_total_pages(total_pages as i32);
        self.ui.set_bg_color(slint::Color::from_argb_encoded(theme.background_color));
        self.ui.set_primary(slint::Color::from_argb_encoded(theme.primary_color));
        self.ui.set_highlight(slint::Color::from_argb_encoded(theme.highlight_color));
        self.ui.set_font_size(theme.font_size as f32);

        // ── 执行离屏渲染并获取截图 ──
        let snapshot = self
            .ui
            .window()
            .take_snapshot()
            .map_err(|e| RenderError::Render(format!("截图失败: {e}")))?;

        let width = snapshot.width();
        let height = snapshot.height();
        let pixels = snapshot.as_bytes().to_vec();

        Ok((pixels, width, height))
    }

    /// 返回当前窗口的物理尺寸（像素单位）。
    pub fn size(&self) -> slint::PhysicalSize {
        self.ui.window().size()
    }
}
