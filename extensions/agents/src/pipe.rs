//! The notch's pipe (docs/extensions.md): frames of a `u32` little-endian length and that many bytes of JSON.
//! One thread owns it and reads only when `PeekNamedPipe` says a frame is there, so it can also write on its own
//! schedule without the blocked-read problem of a plain pipe handle.

use serde_json::Value;
use std::ffi::c_void;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::windows::io::AsRawHandle;
use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Pipes::{GetNamedPipeServerProcessId, PeekNamedPipe};
use windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION};

pub const MAX_FRAME: usize = 64 * 1024;

pub fn send(pipe: &mut File, v: Value) -> io::Result<()> {
    let _ = (pipe, v);
    todo!()
}

pub fn recv(pipe: &mut File) -> io::Result<Value> {
    let _ = pipe;
    todo!()
}

fn handle(pipe: &File) -> HANDLE {
    HANDLE(pipe.as_raw_handle() as *mut c_void)
}

/// Bytes waiting in the pipe, or None when it is closed.
pub fn available(pipe: &File) -> Option<u32> {
    let mut n = 0u32;
    unsafe { PeekNamedPipe(handle(pipe), None, 0, None, Some(&mut n), None) }.ok()?;
    Some(n)
}

pub fn open_pipe() -> io::Result<File> {
    let name = std::env::var("HESEWAKE_NOTCH_PIPE").map_err(|_| io::Error::new(io::ErrorKind::NotFound, "HESEWAKE_NOTCH_PIPE is not set"))?;
    OpenOptions::new().read(true).write(true).open(name)
}

/// The other end must be the notch (`hesewake-notch.exe`) before anything about sessions is sent
/// (docs/extensions.md, section 3).
pub fn server_is_notch(pipe: &File) -> bool {
    unsafe {
        let mut pid = 0u32;
        if GetNamedPipeServerProcessId(handle(pipe), &mut pid).is_err() {
            return false;
        }
        let Ok(p) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return false };
        let mut buf = [0u16; 1024];
        let mut n = buf.len() as u32;
        let r = QueryFullProcessImageNameW(p, PROCESS_NAME_FORMAT(0), PWSTR(buf.as_mut_ptr()), &mut n);
        let _ = CloseHandle(p);
        if r.is_err() {
            return false;
        }
        let path = String::from_utf16_lossy(&buf[..n as usize]);
        path.rsplit('\\').next().is_some_and(|f| f.eq_ignore_ascii_case("hesewake-notch.exe"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::tests::TempDir;
    use serde_json::json;
    use std::io::{Seek, SeekFrom};

    fn file(dir: &TempDir, name: &str) -> File {
        OpenOptions::new().read(true).write(true).create(true).truncate(true).open(dir.0.join(name)).unwrap()
    }

    #[test]
    fn frames_round_trip_and_bad_lengths_fail() {
        let dir = TempDir::new("frames");
        let mut f = file(&dir, "ok");
        let v = json!({"t": "slide", "title": "Coding |Agents", "rows": ["🌿"]});
        send(&mut f, v.clone()).unwrap();
        send(&mut f, json!({"t": "bye"})).unwrap();
        f.seek(SeekFrom::Start(0)).unwrap();
        assert_eq!(recv(&mut f).unwrap(), v);
        assert_eq!(recv(&mut f).unwrap()["t"], "bye");
        assert!(recv(&mut f).is_err(), "nothing more");
        for (name, bytes) in [("zero", 0u32.to_le_bytes().to_vec()), ("huge", ((MAX_FRAME + 1) as u32).to_le_bytes().to_vec()), ("short", vec![5, 0, 0, 0, b'{'])] {
            let mut f = file(&dir, name);
            f.write_all(&bytes).unwrap();
            f.seek(SeekFrom::Start(0)).unwrap();
            assert!(recv(&mut f).is_err(), "{name}");
        }
        let mut f = file(&dir, "big");
        send(&mut f, json!({"t": "x", "pad": "y".repeat(MAX_FRAME)})).unwrap();
        assert_eq!(f.metadata().unwrap().len(), 0, "a frame over the limit is not sent");
    }
}
