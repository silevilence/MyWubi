// windows/im_engine/src/candidate_data.rs

use core_engine::Config;

/// 候选框锚点——光标左下角的屏幕绝对坐标。
#[derive(Debug, Clone, Copy, Default)]
pub struct ScreenPoint {
    pub x: i32,
    pub y: i32,
}

/// 外观主题快照（从 config.toml 同步）。
#[derive(Debug, Clone)]
pub struct ThemeSnapshot {
    pub images: std::sync::Arc<crate::skin_images::Cache>,
    pub parameters: Option<std::sync::Arc<core_engine::skin::parameters::Parameters>>,
    pub font_size: u16,
    pub primary_color: u32,
    pub background_color: u32,
    pub highlight_color: u32,
}

impl Default for ThemeSnapshot {
    fn default() -> Self {
        Self {
            images: Default::default(),
            parameters: None,
            font_size: 14,
            primary_color: 0xFF1E88E5,
            background_color: 0xFFFFFFFF,
            highlight_color: 0xFFFFD54F,
        }
    }
}

impl ThemeSnapshot {
    pub fn from_config(cfg: &Config) -> Self {
        Self {
            images: Default::default(),
            parameters: None,
            font_size: cfg.appearance.font_size,
            primary_color: cfg.appearance.primary_color,
            background_color: cfg.appearance.background_color,
            highlight_color: cfg.appearance.highlight_color,
        }
    }

    pub fn from_parameters(
        parameters: std::sync::Arc<core_engine::skin::parameters::Parameters>,
    ) -> Self {
        Self {
            images: Default::default(),
            font_size: parameters.number("font_size") as u16,
            primary_color: parameters.color("code"),
            background_color: parameters.color("background"),
            highlight_color: parameters.color("selected_background"),
            parameters: Some(parameters),
        }
    }
}

/// 单个候选项。
#[derive(Debug, Clone)]
pub struct CandidateItem {
    pub expression_kind: Option<core_engine::expressions::ExpressionKind>,
    /// Non-text state for unavailable mode choices; never append errors to words.
    pub disabled: bool,
    /// 显示用标签（如 "1.", "2."）。
    pub label: String,
    /// 候选词文本。
    pub text: String,
    /// 候选词对应的后续编码提示。
    pub hint: String,
}

/// 候选框共享数据——TSF 线程写入，渲染线程读取。
pub const FIND_EXPRESSIONS: usize = usize::MAX - 1;
pub const EDIT_EXPRESSION: usize = usize::MAX - 2;
pub const CANCEL_EXPRESSION: usize = usize::MAX - 3;
pub const PREVIOUS_PAGE: usize = usize::MAX - 4;
pub const NEXT_PAGE: usize = usize::MAX - 5;
pub const WORDS_EXPRESSION: usize = usize::MAX - 6;
pub const LITERAL_EXPRESSION: usize = usize::MAX - 7;
pub const CLEAR_EXPRESSION: usize = usize::MAX - 8;
#[derive(Debug, Clone)]
pub struct CandidateControl {
    pub id: usize,
    pub label: String,
}

#[derive(Debug, Clone, Default)]
pub struct CandidateData {
    pub controls: Vec<CandidateControl>,
    pub selection_query: Option<core_engine::candidates::QueryRevision>,
    pub selection_token: Option<u64>,
    pub selection_ids: Vec<usize>,
    pub mode_label: String,
    pub expression: Option<core_engine::candidates::ExpressionQueryView>,
    pub notices: Vec<String>,
    /// 候选框是否可见。
    pub visible: bool,
    /// 当前编码字符串；不拼接模式名、预测状态或其它说明文字。
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
            controls: Vec::new(),
            selection_query: None,
            selection_token: None,
            selection_ids: Vec::new(),
            mode_label: String::new(),
            expression: None,
            notices: Vec::new(),
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
