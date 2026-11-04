//! Bounded local named-pipe transport, with no model runtime dependency.
#![cfg(windows)]
use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Write},
    os::windows::{
        fs::OpenOptionsExt,
        io::{AsRawHandle, FromRawHandle},
    },
    time::{Duration, Instant},
};
use windows::{
    core::{PCWSTR, PWSTR},
    Win32::{
        Foundation::*,
        Security::{Authorization::*, *},
        Storage::FileSystem::*,
        System::{Pipes::*, Threading::*},
    },
};
const MAX: usize = 65536;
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
pub fn user_sid() -> io::Result<String> {
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token)?;
        let result = (|| {
            let mut size = 0;
            let _ = GetTokenInformation(token, TokenUser, None, 0, &mut size);
            let mut data = vec![0usize; (size as usize).div_ceil(std::mem::size_of::<usize>())];
            GetTokenInformation(
                token,
                TokenUser,
                Some(data.as_mut_ptr().cast()),
                size,
                &mut size,
            )?;
            let user = &*(data.as_ptr().cast::<TOKEN_USER>());
            let mut sid = PWSTR::null();
            ConvertSidToStringSidW(user.User.Sid, &mut sid)?;
            let result = sid.to_string().map_err(io::Error::other);
            LocalFree(Some(HLOCAL(sid.0.cast())));
            result
        })();
        let _ = CloseHandle(token);
        result
    }
}
pub fn pipe_name() -> io::Result<String> {
    Ok(format!(r"\\.\pipe\MyWubi-Prediction-v1-{}", user_sid()?))
}
fn transient(error: &io::Error) -> bool {
    matches!(error.raw_os_error(), Some(232 | 536 | 231))
        || error.kind() == io::ErrorKind::WouldBlock
}
fn transfer(
    file: &mut File,
    mut data: &mut [u8],
    write: bool,
    deadline: Instant,
) -> io::Result<()> {
    while !data.is_empty() {
        if Instant::now() >= deadline {
            return Err(io::ErrorKind::TimedOut.into());
        }
        let count = if write {
            file.write(data)
        } else {
            file.read(data)
        };
        match count {
            // Windows nonblocking byte pipes can report success with zero bytes
            // while the peer is still connected. The deadline bounds both this
            // state and an orderly peer disconnect.
            Ok(0) => std::thread::sleep(Duration::from_millis(2)),
            Ok(n) => data = &mut data[n..],
            Err(e) if transient(&e) => std::thread::sleep(Duration::from_millis(2)),
            Err(e) => return Err(e),
        }
    }
    Ok(())
}
fn send(file: &mut File, bytes: &[u8], deadline: Instant) -> io::Result<()> {
    if bytes.len() > MAX {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let mut frame = (bytes.len() as u32).to_le_bytes().to_vec();
    frame.extend_from_slice(bytes);
    transfer(file, &mut frame, true, deadline)
}
fn receive(file: &mut File, deadline: Instant) -> io::Result<Vec<u8>> {
    let mut size = [0; 4];
    transfer(file, &mut size, false, deadline)?;
    let size = u32::from_le_bytes(size) as usize;
    if size > MAX {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let mut bytes = vec![0; size];
    transfer(file, &mut bytes, false, deadline)?;
    Ok(bytes)
}
/// Call only on a transport thread, with a finite deadline.
pub fn exchange(bytes: &[u8], timeout: Duration) -> io::Result<Vec<u8>> {
    let deadline = Instant::now() + timeout;
    let name = pipe_name()?;
    let mut file = loop {
        match OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(SECURITY_SQOS_PRESENT.0 | SECURITY_IDENTIFICATION.0)
            .open(&name)
        {
            Ok(file) => break file,
            Err(e) if e.raw_os_error() == Some(231) && Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(2))
            }
            Err(e) => return Err(e),
        }
    };
    unsafe {
        SetNamedPipeHandleState(HANDLE(file.as_raw_handle()), Some(&PIPE_NOWAIT), None, None)?;
    }
    send(&mut file, bytes, deadline)?;
    let response = receive(&mut file, deadline)?;
    transfer(&mut file, &mut [1], true, deadline)?;
    Ok(response)
}
pub struct Server {
    file: File,
}
impl Server {
    /// Protected current-user DACL, first-instance protection, no remote clients.
    /// Restricted hosts that cannot connect retain the dictionary baseline.
    pub fn new() -> io::Result<Self> {
        unsafe {
            let sddl = wide(&format!("D:P(A;;GA;;;{})", user_sid()?));
            let mut descriptor = PSECURITY_DESCRIPTOR::default();
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                PCWSTR(sddl.as_ptr()),
                SDDL_REVISION_1,
                &mut descriptor,
                None,
            )?;
            let security = SECURITY_ATTRIBUTES {
                nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: descriptor.0,
                bInheritHandle: false.into(),
            };
            let name = wide(&pipe_name()?);
            let pipe = CreateNamedPipeW(
                PCWSTR(name.as_ptr()),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_NOWAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                MAX as u32 + 4,
                MAX as u32 + 4,
                200,
                Some(&security),
            );
            let error = io::Error::last_os_error();
            LocalFree(Some(HLOCAL(descriptor.0)));
            if pipe.is_invalid() {
                return Err(error);
            }
            Ok(Self {
                file: File::from_raw_handle(pipe.0),
            })
        }
    }
    pub fn poll(&mut self, handler: impl FnOnce(u32, &[u8]) -> Vec<u8>) -> io::Result<bool> {
        let handle = HANDLE(self.file.as_raw_handle());
        unsafe {
            if let Err(error) = ConnectNamedPipe(handle, None) {
                if error.code() == ERROR_PIPE_LISTENING.to_hresult() {
                    return Ok(false);
                }
                if error.code() != ERROR_PIPE_CONNECTED.to_hresult() {
                    let _ = DisconnectNamedPipe(handle);
                    return Err(error.into());
                }
            }
        }
        let result = (|| {
            let mut pid = 0;
            unsafe {
                GetNamedPipeClientProcessId(handle, &mut pid)?;
            }
            let deadline = Instant::now() + Duration::from_secs(1);
            let request = receive(&mut self.file, deadline)?;
            let response = handler(pid, &request);
            send(&mut self.file, &response, deadline)?;
            transfer(&mut self.file, &mut [0], false, deadline)?;
            Ok(true)
        })();
        unsafe {
            let _ = DisconnectNamedPipe(handle);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_pipe_is_single_instance_framed_and_round_trips_without_blocking_forever() {
        let mut server = Server::new().unwrap();
        assert!(Server::new().is_err());
        let client =
            std::thread::spawn(|| exchange(b"bounded fixture", Duration::from_secs(2)).unwrap());
        let end = Instant::now() + Duration::from_secs(3);
        loop {
            if server
                .poll(|pid, bytes| {
                    assert_eq!(pid, std::process::id());
                    bytes.to_vec()
                })
                .unwrap()
            {
                break;
            }
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(client.join().unwrap(), b"bounded fixture");
    }
}
