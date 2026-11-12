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
}
impl Capture {
    pub fn new(config: PathBuf) -> Result<Self> {
        Ok(Self {
            recorder: Recorder::new(config)?,
            context: Context::default(),
            display: None,
            editor: String::new(),
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
    pub fn context(&mut self, context: Context, external: bool) {
        if !self.enabled() {
            return;
        }
        if external && context != self.context {
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
        self.context = context;
    }
    pub fn display(&mut self, mode: &str, display: Display) {
        if !self.enabled() || self.display.as_ref() == Some(&display) {
            return;
        }
        self.recorder.record(
            mode,
            self.context.clone(),
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
                self.recorder.record(
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
            }
            EditSessionOp::CompositionUpdate { spelling, .. } => {
                self.recorder.record(
                    mode,
                    self.context.clone(),
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
                    self.context.clone(),
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
