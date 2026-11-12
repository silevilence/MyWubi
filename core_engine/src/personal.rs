//! Portable observations, not a reconstructed document or a key logger.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const FORMAT: u32 = 1;
pub const CONTEXT_LIMIT: usize = 512;

/// Coordinates are relative to the observed window unless `absolute` is true.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Selection {
    pub start: i64,
    pub end: i64,
    pub unit: CoordinateUnit,
    pub absolute: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CoordinateUnit {
    Utf16,
    UnicodeScalar,
}

/// None means unreadable/unsupported; Some("") means an observed empty range.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Context {
    pub revision: u64,
    pub left: Option<String>,
    pub right: Option<String>,
    pub selected: Option<String>,
    pub selection: Option<Selection>,
    pub truncated: bool,
    pub discontinuity: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Evidence {
    Observed,
    Inferred,
    Unknown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    Confirmed,
    CommandAccepted,
    Rejected,
    Unknown,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Source {
    Code,
    Literal,
    DictionaryPrediction,
    ModelPrediction,
    PersonalPrediction,
    ExternalUnknown,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Candidate {
    pub id: String,
    pub text: String,
    pub source: Source,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Display {
    pub request: String,
    pub model: Option<String>,
    pub policy: String,
    pub candidates: Vec<Candidate>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Change {
    Preedit {
        text: String,
    },
    Display {
        display: Display,
    },
    Commit {
        text: String,
        source: Source,
        method: String,
        outcome: Outcome,
        request: Option<String>,
        candidate: Option<String>,
    },
    /// `related` refers to the original commit only when the host can establish it.
    Edit {
        operation: EditKind,
        related: Option<String>,
    },
    Boundary {
        reason: String,
    },
    Gap {
        dropped: u64,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EditKind {
    Insert,
    Delete,
    Replace,
    Move,
    Undo,
    Redo,
    Unknown,
}

/// IDs are random per producer incarnation plus a monotonic sequence. Imported
/// identities are preserved; a copied installation never resumes a producer ID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub format: u32,
    pub id: String,
    pub source: String,
    pub session: String,
    pub sequence: u64,
    pub time_ms: u64,
    pub mode: String,
    pub evidence: Evidence,
    pub before: Context,
    pub after: Option<Context>,
    pub change: Change,
    /// Additive extensions survive export/import even on older readers.
    #[serde(default, flatten)]
    pub extensions: BTreeMap<String, serde_json::Value>,
}
impl Event {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.format != FORMAT {
            return Err("unsupported event version");
        }
        if self.source.len() != 32
            || !self.source.bytes().all(|b| b.is_ascii_hexdigit())
            || self.id != format!("{}:{}", self.source, self.sequence)
            || self.session.is_empty()
            || self.session.len() > 128
        {
            return Err("invalid event identity");
        }
        for context in std::iter::once(&self.before).chain(self.after.iter()) {
            for text in [&context.left, &context.right, &context.selected]
                .into_iter()
                .flatten()
            {
                if text.chars().count() > CONTEXT_LIMIT {
                    return Err("context exceeds limit");
                }
            }
        }
        match &self.change {
            Change::Commit { text, .. } | Change::Preedit { text } if text.len() > 16384 => {
                Err("text exceeds limit")
            }
            Change::Display { display }
                if display.candidates.len() > 256
                    || display.candidates.iter().any(|c| c.text.len() > 16384) =>
            {
                Err("candidate display exceeds limit")
            }
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PersonalConfig {
    pub collect: bool,
    pub learn: bool,
    pub predict: bool,
    pub max_mb: u32,
    pub retention_days: u16,
}
impl Default for PersonalConfig {
    fn default() -> Self {
        Self {
            collect: false,
            learn: false,
            predict: false,
            max_mb: 64,
            retention_days: 90,
        }
    }
}
