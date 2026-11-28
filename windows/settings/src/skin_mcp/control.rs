//! Per-config Windows events let settings address an already open authoring window.
use std::{
    hash::{DefaultHasher, Hash, Hasher},
    path::Path,
    time::Duration,
};
use windows::{
    core::{Result, HSTRING},
    Win32::{
        Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, WAIT_OBJECT_0},
        System::Threading::{
            CreateEventW, OpenEventW, ResetEvent, SetEvent, WaitForSingleObject,
            EVENT_MODIFY_STATE, SYNCHRONIZATION_SYNCHRONIZE,
        },
    },
};

fn name(config: &Path, action: &str) -> HSTRING {
    let absolute = std::path::absolute(config).unwrap_or_else(|_| config.into());
    let path = absolute
        .parent()
        .zip(absolute.file_name())
        .and_then(|(parent, file)| parent.canonicalize().ok().map(|p| p.join(file)))
        .unwrap_or(absolute);
    let mut hash = DefaultHasher::new();
    path.to_string_lossy().to_lowercase().hash(&mut hash);
    HSTRING::from(format!(
        "Local\\MyWubi.SkinEditor.{:x}.{action}",
        hash.finish()
    ))
}
struct Event(HANDLE);
impl Drop for Event {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
impl Event {
    fn create(config: &Path, action: &str, manual: bool) -> Result<(Self, bool)> {
        unsafe {
            let event = CreateEventW(None, manual, false, &name(config, action))?;
            Ok((Self(event), GetLastError() != ERROR_ALREADY_EXISTS))
        }
    }
    fn signaled(&self) -> bool {
        unsafe { WaitForSingleObject(self.0, 0) == WAIT_OBJECT_0 }
    }
}

pub const POLL: Duration = Duration::from_millis(250);

pub fn signal(config: &Path, action: &str) -> bool {
    unsafe {
        OpenEventW(EVENT_MODIFY_STATE, false, &name(config, action))
            .map(Event)
            .and_then(|e| SetEvent(e.0))
            .is_ok()
    }
}
pub fn enabled(config: &Path) -> bool {
    unsafe {
        OpenEventW(SYNCHRONIZATION_SYNCHRONIZE, false, &name(config, "active"))
            .map(Event)
            .is_ok_and(|e| e.signaled())
    }
}
pub struct Controls {
    active: Event,
    enable: Event,
    disable: Event,
    focus: Event,
}
impl Controls {
    pub fn register(config: &Path) -> Result<Option<Self>> {
        let (active, first) = Event::create(config, "active", true)?;
        if !first {
            return Ok(None);
        }
        Ok(Some(Self {
            active,
            enable: Event::create(config, "enable", false)?.0,
            disable: Event::create(config, "disable", false)?.0,
            focus: Event::create(config, "focus", false)?.0,
        }))
    }
    pub fn poll(&self) -> (bool, bool, bool) {
        (
            self.enable.signaled(),
            self.disable.signaled(),
            self.focus.signaled(),
        )
    }
    pub fn publish(&self, active: bool) -> Result<()> {
        unsafe {
            if active {
                SetEvent(self.active.0)
            } else {
                ResetEvent(self.active.0)
            }
        }
    }
}
