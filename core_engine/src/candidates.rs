//! Stable query identities and platform-neutral presentation, without UI labels or coordinates.

/// Unique per engine incarnation, and per changed buffer/mode/runtime/editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct QueryRevision {
    pub engine: u64,
    pub version: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CandidateId(pub usize);

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CandidateHint {
    CodeSuffix(String),
    ReverseCodes(Vec<String>),
    /// Human-readable expression name; never part of committed text or an input code.
    ExpressionName(String),
    /// Search quality, including partial matches; display only.
    ExpressionMatch(String),
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Candidate {
    /// Rendering metadata only; never inferred from the spelling or output characters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expression_kind: Option<crate::expressions::ExpressionKind>,
    pub id: CandidateId,
    pub text: String,
    pub exact: bool,
    pub hints: Vec<CandidateHint>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CandidateWindow {
    pub query_revision: QueryRevision,
    pub view_revision: u64,
    pub total: usize,
    pub offset: usize,
    pub candidates: Vec<Candidate>,
}

/// Query windows never mutate the desktop page size or page position.
pub const MAX_WINDOW: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PreeditKind {
    Input,
    Converted,
}

/// Presentation text is distinct from candidate identity and committed text.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PresentationItem {
    pub text: String,
    pub hint: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SessionView {
    /// Internal query only: this text must never become host preedit.
    #[serde(default)]
    pub expression: Option<ExpressionQueryView>,
    #[serde(default)]
    pub prediction: bool,
    pub visible: bool,
    pub spelling: String,
    pub items: Vec<PresentationItem>,
    pub page: usize,
    pub total_pages: usize,
    pub mode: crate::InputMode,
    pub preedit_kind: Option<PreeditKind>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ExpressionPhase {
    Editing,
    Results,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ExpressionInput {
    #[default]
    Words,
    Literal,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExpressionQueryView {
    pub kind: crate::expressions::ExpressionKind,
    #[serde(default)]
    pub input: ExpressionInput,
    pub phase: ExpressionPhase,
    pub text: String,
    pub words: Vec<String>,
    pub message: Option<String>,
}

/// None means keep the previously published view, including platform anchor fallback.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ViewUpdate {
    Unchanged,
    Replace(SessionView),
    /// Clear platform presentation metadata (for example the previous desktop anchor).
    Reset(SessionView),
}
