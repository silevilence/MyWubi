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
    last_commit: Option<(String, String, String, Option<String>, bool)>,
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
        if external && context != self.context {
            let corrected = self
                .last_commit
                .as_ref()
                .filter(|(_, text, left, right, observed)| {
                    *observed
                        && context.left.as_deref() == Some(left.as_str())
                        && context.right == *right
                        && !text.is_empty()
                })
                .map(|(id, _, _, _, _)| id.clone());
            self.recorder.record(
                "host",
                self.context.clone(),
                Some(context.clone()),
                Change::Edit {
                    operation: if corrected.is_some() {
                        crate::EditKind::Delete
                    } else {
                        crate::EditKind::Unknown
                    },
                    related: corrected.clone(),
                },
                if corrected.is_some() {
                    Evidence::Observed
                } else {
                    Evidence::Unknown
                },
            );
            if corrected.is_some() {
                self.last_commit = None;
            }
        }
        if let Some((_, text, left, right, observed)) = self.last_commit.as_mut() {
            if context.left.as_deref() == Some(format!("{left}{text}").as_str())
                && context.right == *right
            {
                *observed = true;
            }
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
                if outcome == Outcome::Confirmed {
                    if let (Some(id), Some(left)) = (id, self.context.left.clone()) {
                        self.last_commit =
                            Some((id, text.clone(), left, self.context.right.clone(), false));
                    }
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
}
