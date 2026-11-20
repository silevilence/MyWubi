//! Host adapters report observations here; no host commands are executed.
use crate::{
    recorder::Recorder, Change, Context, Display, Evidence, Outcome, PersonalConfig, Result, Source,
};
use core_engine::session::EditSessionOp;
use std::path::PathBuf;

pub struct Capture {
    recorder: Recorder,
    context: Context,
    display: Option<Display>,
    editor: String,
    last_commit: Option<PendingCommit>,
}
struct PendingCommit {
    id: String,
    text: String,
    before: Context,
    observed: bool,
}

impl PendingCommit {
    fn matches_insert(&self, next: &Context) -> bool {
        self.before.left.as_ref().is_some_and(|left| {
            next.left.as_deref() == Some(format!("{left}{}", self.text).as_str())
        }) && self.before.right.is_some()
            && self.before.right == next.right
            && next.selected.as_deref() == Some("")
            && !next.discontinuity
    }

    fn restored(&self, next: &Context) -> bool {
        self.observed
            && !next.discontinuity
            && self.before.left == next.left
            && self.before.right == next.right
            && self.before.selected.is_some()
            && self.before.selected == next.selected
            && self.before.selection == next.selection
    }
}

impl Capture {
    pub fn new(config: PathBuf) -> Result<Self> {
        Ok(Self {
            recorder: Recorder::new(config)?,
            context: Context::default(),
            display: None,
            editor: String::new(),
            last_commit: None,
        })
    }
    pub fn configure(&mut self, config: &PersonalConfig, allowed: bool, editor: &str) {
        let boundary = self.editor != editor || !allowed || !config.collect;
        self.recorder.configure(config, allowed, editor);
        if boundary {
            self.context = Context {
                discontinuity: true,
                ..Default::default()
            };
            self.display = None;
            self.last_commit = None;
            self.editor = editor.to_owned();
            self.recorder.record(
                "unknown",
                self.context.clone(),
                None,
                Change::Boundary {
                    reason: "editor_or_permission_changed".into(),
                },
                Evidence::Observed,
            );
        }
    }
    pub fn enabled(&self) -> bool {
        self.recorder.enabled()
    }
    pub fn context(&mut self, mut context: Context, external: bool) {
        if !self.enabled() {
            return;
        }
        context.revision = self.context.revision;
        if context == self.context {
            return;
        }
        context.revision = context.revision.saturating_add(1);
        if let Some(commit) = &mut self.last_commit {
            if commit.matches_insert(&context) {
                commit.observed = true;
            } else {
                let restored = external && commit.restored(&context);
                if external {
                    let operation = if restored && commit.before.selected.as_deref() != Some("") {
                        crate::EditKind::Replace
                    } else if restored {
                        crate::EditKind::Delete
                    } else {
                        crate::EditKind::Unknown
                    };
                    self.recorder.record(
                        "host",
                        self.context.clone(),
                        Some(context.clone()),
                        Change::Edit {
                            operation,
                            related: restored.then(|| commit.id.clone()),
                        },
                        if restored {
                            Evidence::Observed
                        } else {
                            Evidence::Unknown
                        },
                    );
                }
                self.last_commit = None;
            }
        } else if external {
            self.recorder.record(
                "host",
                self.context.clone(),
                Some(context.clone()),
                Change::Edit {
                    operation: crate::EditKind::Unknown,
                    related: None,
                },
                Evidence::Unknown,
            );
        }
        self.recorder.record(
            "host",
            context.clone(),
            None,
            Change::Context,
            Evidence::Observed,
        );
        self.context = context;
    }
    fn context_reference(&self) -> Context {
        Context {
            revision: self.context.revision,
            discontinuity: self.context.discontinuity,
            ..Default::default()
        }
    }
    pub fn display(&mut self, mode: &str, display: Display) {
        if !self.enabled() || self.display.as_ref() == Some(&display) {
            return;
        }
        self.recorder.record(
            mode,
            self.context_reference(),
            None,
            Change::Display {
                display: display.clone(),
            },
            Evidence::Observed,
        );
        self.display = Some(display);
    }
    pub fn operation(
        &mut self,
        mode: &str,
        operation: &EditSessionOp,
        outcome: Outcome,
        after: Option<Context>,
        method: &str,
    ) {
        if !self.enabled() {
            return;
        }
        match operation {
            EditSessionOp::CommitAndReplace { text, .. }
            | EditSessionOp::CommitThenUpdate { text, .. } => {
                let candidate = self
                    .display
                    .as_ref()
                    .and_then(|d| d.candidates.iter().find(|c| c.text == *text));
                let source = candidate
                    .map(|c| c.source.clone())
                    .unwrap_or(Source::Literal);
                let id = self.recorder.record(
                    mode,
                    self.context.clone(),
                    after.clone(),
                    Change::Commit {
                        text: text.clone(),
                        source,
                        method: method.into(),
                        outcome,
                        request: self.display.as_ref().map(|d| d.request.clone()),
                        candidate: candidate.map(|c| c.id.clone()),
                    },
                    Evidence::Observed,
                );
                if outcome != Outcome::Rejected {
                    self.last_commit = id
                        .filter(|_| {
                            outcome == Outcome::Confirmed
                                && !text.is_empty()
                                && self.context.left.is_some()
                                && self.context.right.is_some()
                                && !self.context.discontinuity
                        })
                        .map(|id| {
                            let mut commit = PendingCommit {
                                id,
                                text: text.clone(),
                                before: self.context.clone(),
                                observed: false,
                            };
                            commit.observed =
                                after.as_ref().is_some_and(|ctx| commit.matches_insert(ctx));
                            commit
                        });
                }
            }
            EditSessionOp::CompositionUpdate { spelling, .. } => {
                self.recorder.record(
                    mode,
                    self.context_reference(),
                    None,
                    Change::Preedit {
                        text: spelling.clone(),
                    },
                    Evidence::Observed,
                );
            }
            EditSessionOp::EndComposition { delete_text: true } => {
                self.recorder.record(
                    mode,
                    self.context_reference(),
                    None,
                    Change::Preedit {
                        text: String::new(),
                    },
                    Evidence::Observed,
                );
            }
            _ => (),
        }
        if let Some(after) = after {
            self.context = after;
        }
    }
    /// A date shortcut changes the host, but must not become a training commit or
    /// be attributed to a same-text ordinary candidate from the previous display.
    pub fn non_learning_commit(&mut self, outcome: Outcome) {
        if !self.enabled() || outcome == Outcome::Rejected {
            return;
        }
        self.last_commit = None;
        self.display = None;
        self.context = Context {
            revision: self.context.revision.saturating_add(1),
            discontinuity: true,
            ..Default::default()
        };
        self.recorder.record(
            "host",
            self.context.clone(),
            None,
            Change::Boundary {
                reason: "non_learning_commit".into(),
            },
            Evidence::Observed,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unlearned_commit_breaks_correction_identity_but_rejection_preserves_it() {
        let dir = tempfile::tempdir().unwrap();
        let mut capture = Capture::new(dir.path().join("config.toml")).unwrap();
        capture.configure(
            &PersonalConfig {
                collect: true,
                ..Default::default()
            },
            true,
            "editor",
        );
        capture.last_commit = Some(PendingCommit {
            id: "ordinary-id".into(),
            text: "2026-11-12".into(),
            before: Context::default(),
            observed: true,
        });
        capture.context.left = Some("before".into());
        capture.non_learning_commit(Outcome::Rejected);
        assert!(capture.last_commit.is_some());
        capture.non_learning_commit(Outcome::Confirmed);
        assert!(capture.last_commit.is_none());
        assert!(capture.display.is_none());
        assert!(capture.context.left.is_none());
        assert!(capture.context.discontinuity);
    }
}
