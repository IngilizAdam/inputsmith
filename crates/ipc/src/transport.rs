//! Local IPC transport: Windows named pipes, Unix domain sockets elsewhere.
//!
//! A [`Conn`] can be cloned so one thread reads while others write.

use std::io;
use std::sync::Arc;

use crate::frame::{self, Decoder};
use crate::message::Message;

/// Base name of the agent endpoint; the full path adds a per-user suffix.
pub const AGENT_ENDPOINT: &str = "inputsmith_agent";

/// Full endpoint path for the current user.
pub fn agent_path() -> String {
    crate::endpoint_path(AGENT_ENDPOINT, &current_user())
}

fn current_user() -> String {
    std::env::var("USERNAME").or_else(|_| std::env::var("USER")).unwrap_or_else(|_| "user".into())
}

#[cfg(windows)]
mod imp {
    use std::io;
    use std::mem::zeroed;
    use std::ptr::{null, null_mut};

    use windows_sys::Win32::Foundation::{
        CloseHandle, ERROR_BROKEN_PIPE, ERROR_IO_PENDING, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, GENERIC_READ,
        GENERIC_WRITE, GetLastError, HANDLE, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, OPEN_EXISTING, PIPE_ACCESS_DUPLEX, ReadFile,
        WriteFile,
    };
    use windows_sys::Win32::System::IO::{GetOverlappedResult, OVERLAPPED};
    use windows_sys::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE,
        PIPE_UNLIMITED_INSTANCES, PIPE_WAIT, WaitNamedPipeW,
    };
    use windows_sys::Win32::System::Threading::{CreateEventW, INFINITE, WaitForSingleObject};

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    pub struct Handle(HANDLE);
    unsafe impl Send for Handle {}
    unsafe impl Sync for Handle {}

    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe { CloseHandle(self.0) };
        }
    }

    /// Runs one overlapped operation to completion.
    fn overlapped(h: HANDLE, op: impl FnOnce(*mut OVERLAPPED) -> i32) -> io::Result<u32> {
        let ev = unsafe { CreateEventW(null(), 1, 0, null()) };
        if ev.is_null() {
            return Err(io::Error::last_os_error());
        }
        let ev = Handle(ev);
        let mut ov: OVERLAPPED = unsafe { zeroed() };
        ov.hEvent = ev.0;
        if op(&mut ov) == 0 {
            let e = unsafe { GetLastError() };
            if e != ERROR_IO_PENDING {
                return Err(io::Error::from_raw_os_error(e as i32));
            }
            unsafe { WaitForSingleObject(ev.0, INFINITE) };
        }
        let mut n = 0u32;
        if unsafe { GetOverlappedResult(h, &ov, &mut n, 1) } == 0 {
            let e = unsafe { GetLastError() };
            if e == ERROR_BROKEN_PIPE {
                return Ok(0);
            }
            return Err(io::Error::from_raw_os_error(e as i32));
        }
        Ok(n)
    }

    pub fn read(h: &Handle, buf: &mut [u8]) -> io::Result<usize> {
        let p = buf.as_mut_ptr();
        let len = buf.len() as u32;
        Ok(overlapped(h.0, |ov| unsafe { ReadFile(h.0, p, len, null_mut(), ov) })? as usize)
    }

    pub fn write_all(h: &Handle, mut buf: &[u8]) -> io::Result<()> {
        while !buf.is_empty() {
            let p = buf.as_ptr();
            let len = buf.len() as u32;
            let n = overlapped(h.0, |ov| unsafe { WriteFile(h.0, p, len, null_mut(), ov) })?;
            if n == 0 {
                return Err(io::ErrorKind::BrokenPipe.into());
            }
            buf = &buf[n as usize..];
        }
        Ok(())
    }

    pub struct Listener {
        path: Vec<u16>,
        /// The instance waiting for the next client; always present so
        /// clients can connect between accepts.
        next: Option<Handle>,
    }

    impl Listener {
        pub fn bind(path: &str) -> io::Result<Self> {
            let path = wide(path);
            // FIRST_PIPE_INSTANCE fails if another server already owns the name.
            let next = Some(create(&path, true)?);
            Ok(Self { path, next })
        }

        pub fn accept(&mut self) -> io::Result<Handle> {
            let h = match self.next.take() {
                Some(h) => h,
                None => create(&self.path, false)?,
            };
            let r = overlapped(h.0, |ov| unsafe { ConnectNamedPipe(h.0, ov) });
            self.next = Some(create(&self.path, false)?);
            match r {
                Ok(_) => Ok(h),
                Err(e) if e.raw_os_error() == Some(ERROR_PIPE_CONNECTED as i32) => Ok(h),
                Err(e) => Err(e),
            }
        }
    }

    fn create(path: &[u16], first: bool) -> io::Result<Handle> {
        {
            let mut flags = PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED;
            if first {
                flags |= FILE_FLAG_FIRST_PIPE_INSTANCE;
            }
            let h = unsafe {
                CreateNamedPipeW(
                    path.as_ptr(),
                    flags,
                    PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                    PIPE_UNLIMITED_INSTANCES,
                    64 * 1024,
                    64 * 1024,
                    0,
                    null(),
                )
            };
            if h == INVALID_HANDLE_VALUE {
                return Err(io::Error::last_os_error());
            }
            Ok(Handle(h))
        }
    }

    pub fn connect(path: &str) -> io::Result<Handle> {
        let w = wide(path);
        for _ in 0..50 {
            let h = unsafe {
                CreateFileW(
                    w.as_ptr(),
                    GENERIC_READ | GENERIC_WRITE,
                    0,
                    null(),
                    OPEN_EXISTING,
                    FILE_FLAG_OVERLAPPED,
                    null_mut(),
                )
            };
            if h != INVALID_HANDLE_VALUE {
                return Ok(Handle(h));
            }
            let e = unsafe { GetLastError() };
            if e != ERROR_PIPE_BUSY {
                return Err(io::Error::from_raw_os_error(e as i32));
            }
            unsafe { WaitNamedPipeW(w.as_ptr(), 200) };
        }
        Err(io::ErrorKind::TimedOut.into())
    }
}

#[cfg(unix)]
mod imp {
    use std::io::{self, Read, Write};
    use std::os::unix::net::{UnixListener, UnixStream};

    pub struct Handle(UnixStream);

    pub fn read(h: &Handle, buf: &mut [u8]) -> io::Result<usize> {
        (&h.0).read(buf)
    }

    pub fn write_all(h: &Handle, buf: &[u8]) -> io::Result<()> {
        (&h.0).write_all(buf)
    }

    pub struct Listener(UnixListener);

    impl Listener {
        pub fn bind(path: &str) -> io::Result<Self> {
            // A stale socket from a crashed agent would block binding.
            if UnixStream::connect(path).is_err() {
                let _ = std::fs::remove_file(path);
            }
            let l = UnixListener::bind(path)?;
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
            Ok(Self(l))
        }

        pub fn accept(&mut self) -> io::Result<Handle> {
            Ok(Handle(self.0.accept()?.0))
        }
    }

    pub fn connect(path: &str) -> io::Result<Handle> {
        Ok(Handle(UnixStream::connect(path)?))
    }
}

/// Accepts client connections.
pub struct Listener(imp::Listener);

impl Listener {
    pub fn bind(path: &str) -> io::Result<Self> {
        Ok(Self(imp::Listener::bind(path)?))
    }

    pub fn accept(&mut self) -> io::Result<Conn> {
        Ok(Conn(Arc::new(self.0.accept()?)))
    }
}

/// A framed, message-oriented connection. Clones share the underlying handle.
#[derive(Clone)]
pub struct Conn(Arc<imp::Handle>);

impl Conn {
    pub fn connect(path: &str) -> io::Result<Self> {
        Ok(Self(Arc::new(imp::connect(path)?)))
    }

    pub fn send(&self, msg: &Message) -> io::Result<()> {
        imp::write_all(&self.0, &frame::encode(msg.to_json().as_bytes()))
    }

    /// Returns a blocking iterator over incoming messages.
    pub fn messages(&self) -> Incoming {
        Incoming { conn: self.clone(), dec: Decoder::default(), buf: vec![0; 64 * 1024] }
    }
}

pub struct Incoming {
    conn: Conn,
    dec: Decoder,
    buf: Vec<u8>,
}

impl Iterator for Incoming {
    type Item = io::Result<Message>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.dec.next_frame() {
                Ok(Some(payload)) => {
                    return Some(
                        Message::from_json(&payload).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e)),
                    );
                }
                Ok(None) => {}
                Err(e) => return Some(Err(e)),
            }
            match imp::read(&self.conn.0, &mut self.buf) {
                Ok(0) => return None,
                Ok(n) => self.dec.push(&self.buf[..n]),
                Err(e) => return Some(Err(e)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::Verb;

    #[test]
    fn roundtrip_over_local_endpoint() {
        let path = crate::endpoint_path("inputsmith_test", &std::process::id().to_string());
        let mut listener = Listener::bind(&path).unwrap();
        let server = std::thread::spawn(move || {
            let conn = listener.accept().unwrap();
            for m in conn.messages() {
                let m = m.unwrap();
                conn.send(&m.reply(crate::Status::Ok)).unwrap();
                break;
            }
        });
        let client = Conn::connect(&path).unwrap();
        let req = Message { id: 1, ..Message::new(Verb::Get, "/ping") };
        client.send(&req).unwrap();
        let resp = client.messages().next().unwrap().unwrap();
        assert_eq!(resp.id, 1);
        assert_eq!(resp.status, Some(crate::Status::Ok));
        server.join().unwrap();
    }
}
