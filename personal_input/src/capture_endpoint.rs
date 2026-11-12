//! JNI capture facade. Configure precedes observations and carries privacy policy.
use crate::{capture::Capture, Context, Display, Outcome, PersonalConfig};
use core_engine::session::EditSessionOp;
use std::{path::PathBuf, sync::Mutex};
#[derive(serde::Deserialize)]
#[serde(tag = "op")]
enum Command {
    Configure {
        config: PathBuf,
        settings: PersonalConfig,
        allowed: bool,
        session: String,
    },
    Context {
        context: Context,
        external: bool,
    },
    Operation {
        operation: EditSessionOp,
        outcome: Outcome,
        after: Option<Context>,
        method: String,
    },
    Display {
        display: Display,
        mode: String,
    },
    Close,
}
#[derive(Default)]
pub struct Endpoint(Mutex<Option<Capture>>);
impl Endpoint {
    pub fn exchange(&self, bytes: &[u8]) -> Vec<u8> {
        let result = (|| -> crate::Result<()> {
            if bytes.len() > 65536 {
                return Err(crate::Error::Invalid("采集请求超过限制"));
            }
            let command: Command = serde_json::from_slice(bytes)?;
            let mut slot = self
                .0
                .lock()
                .map_err(|_| crate::Error::Invalid("采集器不可用"))?;
            match command {
                Command::Configure {
                    config,
                    settings,
                    allowed,
                    session,
                } => {
                    if !settings.collect {
                        if let Some(capture) = slot.as_mut() {
                            capture.configure(&settings, false, &session);
                        }
                    } else {
                        if slot.is_none() {
                            *slot = Some(Capture::new(config)?);
                        }
                        if let Some(capture) = slot.as_mut() {
                            capture.configure(&settings, allowed, &session);
                        }
                    }
                }
                Command::Close => {
                    *slot = None;
                }
                other => {
                    if let Some(capture) = slot.as_mut().filter(|c| c.enabled()) {
                        match other {
                            Command::Context { context, external } => {
                                capture.context(context, external)
                            }
                            Command::Operation {
                                operation,
                                outcome,
                                after,
                                method,
                            } => capture.operation("android", &operation, outcome, after, &method),
                            Command::Display { display, mode } => capture.display(&mode, display),
                            _ => (),
                        }
                    }
                }
            }
            Ok(())
        })();
        if result.is_ok() {
            br#"{"ok":true}"#.to_vec()
        } else {
            br#"{"ok":false,"error":"capture unavailable"}"#.to_vec()
        }
    }
}
