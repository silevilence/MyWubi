//! Bounded committed prefix, with explicit editor ownership and no persistence.
use crate::session::{EditSessionOp, EditToken, HostOutcome};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Coverage {
    Unavailable,
    SessionCommits,
    HostPrefix,
}

/// No Debug/Serialize: caller must deliberately select text for a local scoring request.
pub struct ContextBuffer {
    editor: Option<(u64, u64)>,
    enabled: bool,
    limit: usize,
    generation: u64,
    prefix: String,
    coverage: Coverage,
}

impl ContextBuffer {
    pub fn new(limit: usize) -> Self {
        Self {
            editor: None,
            enabled: false,
            limit: limit.min(4096),
            generation: 0,
            prefix: String::new(),
            coverage: Coverage::Unavailable,
        }
    }
    /// `allowed` must be false for passwords, sensitive input and a disabled feature.
    pub fn bind(&mut self, editor: u64, epoch: u64, allowed: bool) {
        if self.editor != Some((editor, epoch)) || self.enabled != allowed {
            self.invalidate();
        }
        self.editor = Some((editor, epoch));
        self.enabled = allowed;
    }
    pub fn invalidate(&mut self) {
        self.prefix.clear();
        self.coverage = Coverage::Unavailable;
        self.generation = self.generation.wrapping_add(1);
    }
    pub fn detach(&mut self) {
        self.invalidate();
        self.editor = None;
        self.enabled = false;
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn coverage(&self) -> Coverage {
        self.coverage
    }
    pub fn prefix(&self) -> &str {
        &self.prefix
    }
    /// Text must end immediately before owned preedit, or at the caret when idle.
    /// An unavailable read never revives invalidated text from another editor.
    pub fn observe(&mut self, editor: u64, epoch: u64, text: Option<&str>) -> bool {
        if !self.enabled || self.editor != Some((editor, epoch)) {
            return false;
        }
        if let Some(text) = text {
            let prefix = tail(text, self.limit);
            if self.prefix != prefix {
                self.generation = self.generation.wrapping_add(1);
            }
            self.prefix = prefix;
            self.coverage = Coverage::HostPrefix;
            true
        } else {
            false
        }
    }
    /// The session owner calls this only after accepting an acknowledgement as current.
    /// Unknown/partial writes discard history. A known rejection keeps confirmed text.
    pub fn acknowledged(&mut self, token: EditToken, op: &EditSessionOp, outcome: HostOutcome) {
        if !self.enabled || self.editor != Some((token.editor, token.epoch)) {
            return;
        }
        match outcome {
            HostOutcome::RejectedBeforeWrite => return,
            HostOutcome::Applied => {}
            _ => {
                self.invalidate();
                return;
            }
        }
        if let EditSessionOp::CommitAndReplace { text, replace_len }
        | EditSessionOp::CommitThenUpdate {
            text, replace_len, ..
        } = op
        {
            // Legacy replacement length may describe a host composition, not our prefix.
            // Without an authoritative read, do not guess which previous text survived.
            if *replace_len != 0 {
                self.invalidate();
            }
            self.prefix.push_str(text);
            self.prefix = tail(&self.prefix, self.limit);
            if self.coverage == Coverage::Unavailable {
                self.coverage = Coverage::SessionCommits;
            }
            self.generation = self.generation.wrapping_add(1);
        }
    }
}

fn tail(text: &str, limit: usize) -> String {
    let offset = text
        .char_indices()
        .rev()
        .nth(limit.saturating_sub(1))
        .map_or(0, |(i, _)| i);
    if limit == 0 {
        String::new()
    } else {
        text[offset..].into()
    }
}

/// Decode a bounded Windows/Android UTF-16 prefix. A truncated leading low surrogate
/// is dropped; any other malformed pair rejects the host read rather than inventing text.
pub fn decode_utf16_prefix(units: &[u16]) -> Result<String, std::string::FromUtf16Error> {
    let units = if units.first().is_some_and(|u| (0xdc00..=0xdfff).contains(u)) {
        &units[1..]
    } else {
        units
    };
    String::from_utf16(units)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn token() -> EditToken {
        EditToken {
            session: 1,
            generation: 1,
            editor: 2,
            epoch: 3,
            runtime: 1,
        }
    }
    #[test]
    fn only_acknowledged_commits_survive_failed_edits_and_sensitive_bindings() {
        let mut b = ContextBuffer::new(3);
        b.bind(2, 3, true);
        let op = EditSessionOp::CommitThenUpdate {
            attr: crate::candidates::PreeditKind::Input,
            text: "中😀文".into(),
            replace_len: 0,
            spelling: "xian".into(),
        };
        b.acknowledged(token(), &op, HostOutcome::RejectedBeforeWrite);
        assert_eq!(b.prefix(), "");
        b.acknowledged(token(), &op, HostOutcome::Applied);
        assert_eq!(b.prefix(), "中😀文");
        b.bind(2, 3, false);
        assert!(!b.observe(2, 3, Some("密码")));
        assert_eq!(b.prefix(), "");
    }
    #[test]
    fn edits_and_editor_changes_clear_old_state_and_utf16_boundaries_are_checked() {
        let mut b = ContextBuffer::new(2);
        b.bind(2, 3, true);
        b.observe(2, 3, Some("中文😀"));
        assert_eq!(b.prefix(), "文😀");
        b.invalidate();
        assert!(!b.observe(2, 3, None));
        assert_eq!(b.coverage(), Coverage::Unavailable);
        b.bind(4, 1, true);
        assert!(!b.observe(2, 3, Some("旧框")));
        assert_eq!(decode_utf16_prefix(&[0xde00, 0x4e2d]).unwrap(), "中");
        assert!(decode_utf16_prefix(&[0x4e2d, 0xd83d]).is_err());
    }
    #[test]
    fn uncertain_host_writes_never_become_confirmed_prefix() {
        let mut b = ContextBuffer::new(256);
        b.bind(2, 3, true);
        b.observe(2, 3, Some("已输入"));
        b.acknowledged(
            token(),
            &EditSessionOp::CommitAndReplace {
                text: "未知".into(),
                replace_len: 0,
            },
            HostOutcome::OutcomeUnknown,
        );
        assert_eq!(b.prefix(), "");
    }
}
