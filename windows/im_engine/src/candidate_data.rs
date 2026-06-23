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
