//! Stable query identities and platform-neutral presentation, without UI labels or coordinates.

/// Unique per engine incarnation, and per changed buffer/mode/runtime/editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueryRevision {
    pub engine: u64,
    pub version: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CandidateId(pub usize);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CandidateHint {
    CodeSuffix(String),
    ReverseCodes(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub id: CandidateId,
    pub text: String,
    pub exact: bool,
    pub hints: Vec<CandidateHint>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateWindow {
    pub query_revision: QueryRevision,
    pub view_revision: u64,
    pub total: usize,
    pub offset: usize,
    pub candidates: Vec<Candidate>,
}

/// Query windows never mutate the desktop page size or page position.
pub const MAX_WINDOW: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreeditKind {
    Input,
    Converted,
}

/// Presentation text is distinct from candidate identity and committed text.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PresentationItem {
    pub text: String,
    pub hint: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionView {
    pub visible: bool,
    pub spelling: String,
    pub items: Vec<PresentationItem>,
    pub page: usize,
    pub total_pages: usize,
    pub mode: crate::InputMode,
    pub preedit_kind: Option<PreeditKind>,
}

/// None means keep the previously published view, including platform anchor fallback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewUpdate {
    Unchanged,
    Replace(SessionView),
    /// Clear platform presentation metadata (for example the previous desktop anchor).
    Reset(SessionView),
}
