//! Version 1 JSON facade shared by C and JNI. Handles are typed, never pointers.
use crate::{
    candidates::MAX_WINDOW, config::ValidationContext, data::*, runtime::*, session::*, *,
};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, LazyLock},
};

/// Maximum request size; text actions have the smaller semantic engine limit.
pub const MAX_REQUEST: usize = 16 * 1024 * 1024;
/// Wire version, independent of the application version.
pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    version: u32,
    request: Request,
}

/// Explicit operation tags. Serde rejects unknown variants and invalid scalar values.
#[derive(Deserialize)]
#[serde(tag = "op", deny_unknown_fields)]
enum Request {
    BuildRuntime {
        path: PathBuf,
    },
    CreateSession {
        runtime: u64,
        context: HostContext,
    },
    Destroy {
        handle: u64,
    },
    Prepare {
        session: u64,
        action: Action,
    },
    Ack {
        session: u64,
        token: EditToken,
        outcome: HostOutcome,
        owns_preedit: bool,
    },
    Recover {
        session: u64,
    },
    Close {
        session: u64,
    },
    State {
        session: u64,
    },
    Candidates {
        session: u64,
        offset: usize,
        count: usize,
    },
    IsCurrent {
        session: u64,
        token: EditToken,
    },
    Context {
        session: u64,
        context: HostContext,
        preserve_base: bool,
    },
    LoseFocus {
        session: u64,
    },
    ApplyRuntime {
        session: u64,
        runtime: u64,
    },
    ReadConfig {
        path: PathBuf,
    },
    ValidateConfig {
        config: Config,
    },
    SaveConfig {
        path: PathBuf,
        config: Config,
        expected: u64,
    },
    ReadTable {
        path: PathBuf,
    },
    ValidateTable {
        config: TableConfig,
        entries: Vec<Entry>,
    },
    SaveTable {
        original: PathBuf,
        target: PathBuf,
        config: TableConfig,
        entries: Vec<Entry>,
        expected: u64,
    },
    FileVersion {
        path: PathBuf,
    },
    ValidatePackage {
        root: PathBuf,
        main: PathBuf,
    },
    ReadUser {
        path: PathBuf,
    },
    EditUser {
        path: PathBuf,
        expected: u64,
        edit: UserOperation,
    },
    ExportUser {
        source: PathBuf,
        target: PathBuf,
        expected: u64,
    },
    SaveBundle {
        path: PathBuf,
        config: Config,
        expected: u64,
        table: Option<Box<TableWrite>>,
    },
}
#[derive(Deserialize)]
enum UserOperation {
    Add(Entry),
    Update(UserEntryId, Entry),
    Remove(UserEntryId),
    Import(PathBuf),
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TableWrite {
    original: PathBuf,
    target: PathBuf,
    config: TableConfig,
    entries: Vec<Entry>,
    expected: u64,
}

/// Stable error categories; details belong to this response, never global last_error.
#[derive(Debug, Serialize)]
pub struct ProtocolError {
    pub code: u32,
    pub message: String,
}
fn err(code: u32, message: impl Into<String>) -> ProtocolError {
    ProtocolError {
        code,
        message: message.into(),
    }
}
fn invalid() -> ProtocolError {
    err(3, "invalid, expired or wrong-kind handle")
}
fn data_error(e: DataError) -> ProtocolError {
    err(
        if matches!(e, DataError::Conflict(_)) {
            6
        } else {
            5
        },
        e.to_string(),
    )
}
fn session_error(e: SessionError) -> ProtocolError {
    err(
        if matches!(e, SessionError::Busy) {
            4
        } else {
            7
        },
        format!("{e:?}"),
    )
}

enum Object {
    Runtime(Arc<RuntimeSnapshot>),
    Session(Box<Session>),
    Version(PathBuf, FileVersion),
}
#[derive(Default)]
struct Registry {
    next: u64,
    objects: HashMap<u64, Object>,
}
impl Registry {
    fn insert(&mut self, value: Object) -> Result<u64, ProtocolError> {
        // Never reuse an incarnation, including after destruction. Low two bits encode kind.
        self.next = self
            .next
            .checked_add(1)
            .filter(|v| *v < (1 << 60))
            .ok_or_else(|| err(8, "handle space exhausted"))?;
        let kind = match &value {
            Object::Runtime(_) => 1,
            Object::Session(_) => 2,
            Object::Version(..) => 3,
        };
        let id = (self.next << 2) | kind;
        self.objects.insert(id, value);
        Ok(id)
    }
    fn runtime(&self, id: u64) -> Result<Arc<RuntimeSnapshot>, ProtocolError> {
        match self.objects.get(&id) {
            Some(Object::Runtime(v)) => Ok(v.clone()),
            _ => Err(invalid()),
        }
    }
    fn session(&mut self, id: u64) -> Result<&mut Session, ProtocolError> {
        match self.objects.get_mut(&id) {
            Some(Object::Session(v)) => Ok(v),
            _ => Err(invalid()),
        }
    }
    fn policy(&self, id: u64, path: &std::path::Path) -> Result<WritePolicy, ProtocolError> {
        match self.objects.get(&id) {
            Some(Object::Version(p, v)) if p == path => Ok(WritePolicy::Expected(v.clone())),
            _ => Err(invalid()),
        }
    }
    fn version(&mut self, path: PathBuf, version: FileVersion) -> Result<u64, ProtocolError> {
        self.insert(Object::Version(path, version))
    }
    fn execute(&mut self, request: Request) -> Result<Value, ProtocolError> {
        Ok(match request {
            Request::BuildRuntime { path } => {
                let config = Config::load(&path).map_err(|e| err(5, e.to_string()))?;
                let runtime = RuntimeBuilder::build(
                    path,
                    config,
                    BehaviorProfile::TouchV1,
                    LoadPolicy::Reload,
                )
                .map_err(|e| err(5, e.message))?;
                json!({"handle": self.insert(Object::Runtime(Arc::new(runtime)))?})
            }
            Request::CreateSession { runtime, context } => {
                let runtime = self.runtime(runtime)?;
                json!({"handle":self.insert(Object::Session(Box::new(Session::new(&runtime,BehaviorProfile::TouchV1,context))))?})
            }
            Request::Destroy { handle } => {
                self.objects.remove(&handle).ok_or_else(invalid)?;
                json!(null)
            }
            Request::Prepare { session, action } => {
                if matches!(action, Action::Legacy(_)) {
                    return Err(err(2, "Legacy actions are not part of TouchV1"));
                }
                json!(self
                    .session(session)?
                    .prepare(action)
                    .map_err(session_error)?)
            }
            Request::Ack {
                session,
                token,
                outcome,
                owns_preedit,
            } => {
                let ack =
                    self.session(session)?
                        .acknowledge_with_ownership(token, outcome, owns_preedit);
                json!({"status":ack.status,"follow_up":ack.follow_up})
            }
            Request::Recover { session } => {
                json!(self.session(session)?.recover().map_err(session_error)?)
            }
            Request::Close { session } => {
                self.session(session)?.close();
                json!(null)
            }
            Request::State { session } => {
                let s = self.session(session)?;
                json!({"status":s.status(),"view":s.snapshot(),"base":s.base_mode(),"temporary":s.temporary_mode(),"revision":s.revision(),"pinyin":s.capability(TemporaryMode::Pinyin)})
            }
            Request::Candidates {
                session,
                offset,
                count,
            } => {
                if count > MAX_WINDOW {
                    return Err(err(2, "candidate window exceeds 256"));
                }
                json!(self.session(session)?.candidate_window(offset, count))
            }
            Request::IsCurrent { session, token } => {
                json!(self.session(session)?.is_current(token))
            }
            Request::Context {
                session,
                context,
                preserve_base,
            } => {
                let s = self.session(session)?;
                if s.status() == SessionStatus::Closed {
                    return Err(err(7, "Closed"));
                }
                s.invalidate_context(context, preserve_base);
                json!(null)
            }
            Request::LoseFocus { session } => {
                json!(self.session(session)?.lose_focus().map_err(session_error)?)
            }
            Request::ApplyRuntime { session, runtime } => {
                let r = self.runtime(runtime)?;
                json!(self
                    .session(session)?
                    .apply_runtime(r)
                    .map_err(session_error)?)
            }
            Request::FileVersion { path } => {
                let v = FileVersion::read(&path).map_err(data_error)?;
                json!({"version":self.version(path,v)?})
            }
            Request::ReadConfig { path } => {
                let before = FileVersion::read(&path).map_err(data_error)?;
                let config = Config::load(&path).map_err(|e| err(5, e.to_string()))?;
                if before != FileVersion::read(&path).map_err(data_error)? {
                    return Err(err(6, "file changed during read"));
                }
                json!({"config":config,"inactive":config.inactive_fields(),"version":self.version(path,before)?})
            }
            Request::ValidateConfig { config } => {
                config
                    .validate_for(ValidationContext::TouchSave)
                    .map_err(|e| err(5, e.to_string()))?;
                json!(null)
            }
            Request::SaveConfig {
                path,
                config,
                expected,
            } => {
                let v = save_config(
                    &config,
                    &path,
                    ValidationContext::TouchSave,
                    self.policy(expected, &path)?,
                )
                .map_err(data_error)?;
                json!({"version":self.version(path,v)?})
            }
            Request::ReadTable { path } => {
                let before = FileVersion::read(&path).map_err(data_error)?;
                let (config, entries) = read_table(&path).map_err(|e| err(5, e.to_string()))?;
                if before != FileVersion::read(&path).map_err(data_error)? {
                    return Err(err(6, "file changed during read"));
                }
                json!({"config":config,"entries":entries,"version":self.version(path,before)?})
            }
            Request::ValidateTable { config, entries } => {
                json!(validate_table(&config, &entries, 256))
            }
            Request::SaveTable {
                original,
                target,
                config,
                entries,
                expected,
            } => {
                let (header, v) = crate::data::save_table(
                    &original,
                    &target,
                    &config,
                    &entries,
                    self.policy(expected, &target)?,
                )
                .map_err(data_error)?;
                json!({"config":header,"version":self.version(target,v)?})
            }
            Request::ValidatePackage { root, main } => {
                json!(validate_package(&root, &main).map_err(data_error)?)
            }
            Request::ReadUser { path } => {
                let before = FileVersion::read(&path).map_err(data_error)?;
                let user = UserDictionary::load(&path).map_err(|e| err(5, e.to_string()))?;
                if before != FileVersion::read(&path).map_err(data_error)? {
                    return Err(err(6, "file changed during read"));
                }
                json!({"entries":user.entries(),"version":self.version(path,before)?})
            }
            Request::EditUser {
                path,
                expected,
                edit,
            } => {
                let edit = match edit {
                    UserOperation::Add(e) => UserEdit::Add(e),
                    UserOperation::Update(id, e) => UserEdit::Update(id, e),
                    UserOperation::Remove(id) => UserEdit::Remove(id),
                    UserOperation::Import(p) => UserEdit::Import(p),
                };
                let result =
                    edit_user(&path, self.policy(expected, &path)?, edit).map_err(data_error)?;
                json!({"added":result.added,"updated":result.updated,"version":self.version(path,result.version)?})
            }
            Request::ExportUser {
                source,
                target,
                expected,
            } => {
                let v = export_user(&source, &target, self.policy(expected, &target)?)
                    .map_err(data_error)?;
                json!({"version":self.version(target,v)?})
            }
            Request::SaveBundle {
                path,
                config,
                expected,
                table,
            } => {
                let policy = self.policy(expected, &path)?;
                let draft = table
                    .as_ref()
                    .map(|t| {
                        Ok(TableDraft {
                            original: &t.original,
                            target: &t.target,
                            config: &t.config,
                            entries: &t.entries,
                            policy: self.policy(t.expected, &t.target)?,
                        })
                    })
                    .transpose()?;
                let report =
                    save_bundle(&config, &path, ValidationContext::TouchSave, policy, draft);
                json!({"saved":report.saved,"failure":report.failure.map(|(p,e)|json!({"path":p,"error":data_error(e)}))})
            }
        })
    }
}
static REGISTRY: LazyLock<Mutex<Registry>> = LazyLock::new(|| Mutex::new(Registry::default()));

/// Process one bounded UTF-8 JSON request. Contains unwind panics, not abort/OOM.
/// Calls are serialized; no lock is held across a host editor callback.
pub fn call(bytes: &[u8]) -> Vec<u8> {
    let result = std::panic::catch_unwind(|| {
        if bytes.len() > MAX_REQUEST {
            return Err(err(2, "request too large"));
        }
        let envelope: Envelope =
            serde_json::from_slice(bytes).map_err(|e| err(2, e.to_string()))?;
        if envelope.version != PROTOCOL_VERSION {
            return Err(err(1, "unsupported protocol version"));
        }
        REGISTRY.lock().execute(envelope.request)
    })
    .unwrap_or_else(|_| Err(err(8, "internal panic")));
    match result {
        Ok(value) => json!({"version":PROTOCOL_VERSION,"ok":true,"value":value}),
        Err(error) => json!({"version":PROTOCOL_VERSION,"ok":false,"error":error}),
    }
    .to_string()
    .into_bytes()
}

/// Caller-owned descriptor; data remains valid until its allocation handle is freed.
#[repr(C)]
pub struct Buffer {
    pub data: *const u8,
    pub len: u64,
    pub allocation: u64,
}
#[derive(Default)]
struct Buffers {
    next: u64,
    values: HashMap<u64, Box<[u8]>>,
}
static BUFFERS: LazyLock<Mutex<Buffers>> = LazyLock::new(|| Mutex::new(Buffers::default()));

/// Returns the supported protocol version.
#[no_mangle]
pub extern "C" fn mywubi_v1_version() -> u32 {
    PROTOCOL_VERSION
}

/// Executes a request and returns an independently owned response, including errors.
/// Returns 0 on transport success, 2 for invalid pointers/length, 8 for panic.
/// # Safety
/// `input` must point to `len` readable bytes (null only for zero length).
/// `output` must point to writable, aligned Buffer storage and not alias input.
/// The caller must release any previous allocation before reusing output.
#[no_mangle]
pub unsafe extern "C" fn mywubi_v1_call(input: *const u8, len: u64, output: *mut Buffer) -> u32 {
    if output.is_null()
        || !(output as usize).is_multiple_of(std::mem::align_of::<Buffer>())
        || len > MAX_REQUEST as u64
        || (input.is_null() && len != 0)
    {
        return 2;
    }
    // SAFETY: output validity is the caller's contract; initialize it even on failure.
    unsafe {
        output.write(Buffer {
            data: std::ptr::null(),
            len: 0,
            allocation: 0,
        });
    }
    std::panic::catch_unwind(|| {
        // SAFETY: readable input is the caller's contract, length has been bounded.
        let bytes = if len == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(input, len as usize) }
        };
        let response = call(bytes).into_boxed_slice();
        let mut buffers = BUFFERS.lock();
        let Some(generation) = buffers.next.checked_add(1).filter(|v| *v < (1 << 60)) else {
            return 8;
        };
        buffers.next = generation;
        // Kind 0 is reserved for response allocations, disjoint from registry objects.
        let id = generation << 2;
        let descriptor = Buffer {
            data: response.as_ptr(),
            len: response.len() as u64,
            allocation: id,
        };
        buffers.values.insert(id, response);
        // SAFETY: ownership transfers to the caller through the valid output descriptor.
        unsafe {
            output.write(descriptor);
        }
        0
    })
    .unwrap_or(8)
}
/// Release a response allocation once; stale/unknown IDs return 3 without dereferencing.
#[no_mangle]
pub extern "C" fn mywubi_v1_buffer_free(allocation: u64) -> u32 {
    if BUFFERS.lock().values.remove(&allocation).is_some() {
        0
    } else {
        3
    }
}
