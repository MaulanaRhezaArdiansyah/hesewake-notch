//! From the hook process to the running extension: the extension owns a message-only window
//! (`HesewakeAgentsWindow`); the hook finds it, checks that it belongs to this same exe, and sends its `Msg` as a
//! `WM_COPYDATA`.

use crate::claude_hook::read_hook;
use crate::event::{decode, encode, Msg, MAX_MSG};
use crate::procs::{agent_pid, ancestors, image_path, parent_map, terminal_window};
use std::cell::RefCell;
use std::ffi::c_void;
use std::sync::mpsc::Sender;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, FindWindowExW, GetMessageW, GetWindowThreadProcessId, PostMessageW, PostQuitMessage, RegisterClassW,
    SendMessageTimeoutW, HWND_MESSAGE, MSG, SMTO_ABORTIFHUNG, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE, WM_COPYDATA, WM_DESTROY, WNDCLASSW,
};

/// `dwData` of the `WM_COPYDATA` that carries a `Msg` ("AGNT").
pub const COPY_ID: usize = 0x4147_4E54;

/// The running extension's window (0 if none). Only a window of this same exe counts.
pub fn find_ext() -> isize {
    let mut prev = HWND::default();
    for _ in 0..16 {
        let Ok(h) = (unsafe { FindWindowExW(HWND_MESSAGE, prev, class(), PCWSTR::null()) }) else { return 0 };
        if h.0.is_null() {
            return 0;
        }
        if is_ours(h.0 as isize) {
            return h.0 as isize;
        }
        prev = h;
    }
    0
}

/// Is `hwnd` a window of a process running this same exe?
pub fn is_ours(hwnd: isize) -> bool {
    if hwnd == 0 {
        return false;
    }
    let Ok(me) = std::env::current_exe() else { return false };
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(HWND(hwnd as *mut c_void), Some(&mut pid)) };
    pid != 0 && image_path(pid).is_some_and(|p| p.eq_ignore_ascii_case(&me.to_string_lossy()))
}

/// Send `bytes` to the extension window `hwnd`. False when there is no window or it doesn't answer within 500 ms.
pub fn send(hwnd: isize, bytes: &[u8]) -> bool {
    send_with(hwnd, COPY_ID, bytes)
}

fn send_with(hwnd: isize, id: usize, bytes: &[u8]) -> bool {
    if hwnd == 0 {
        return false;
    }
    let cds = COPYDATASTRUCT { dwData: id, cbData: bytes.len() as u32, lpData: bytes.as_ptr() as *mut c_void };
    let mut result = 0usize;
    let sent = unsafe {
        SendMessageTimeoutW(HWND(hwnd as *mut c_void), WM_COPYDATA, WPARAM(0), LPARAM(&cds as *const _ as isize), SMTO_ABORTIFHUNG, 500, Some(&mut result))
    };
    sent.0 != 0 && result == 1
}

/// `--hook claude`: read the hook JSON from stdin and pass it to the running extension. Never fails, prints nothing.
pub fn run_hook() {
    let Some(mut m) = read_hook(std::io::stdin()) else { return };
    let ext = find_ext();
    if ext == 0 {
        return;
    }
    let chain = ancestors(&parent_map(), unsafe { GetCurrentProcessId() });
    m.pid = agent_pid(&chain);
    m.hwnd = terminal_window(&chain, &m.project);
    send(ext, &encode(&m));
}

fn class() -> PCWSTR {
    w!("HesewakeAgentsWindow")
}

thread_local! {
    /// Where the window of this thread passes what it receives.
    static TX: RefCell<Option<Sender<Msg>>> = const { RefCell::new(None) };
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_COPYDATA if lp.0 != 0 => {
            let cds = &*(lp.0 as *const COPYDATASTRUCT);
            if cds.dwData != COPY_ID || cds.cbData as usize > MAX_MSG || cds.lpData.is_null() {
                return LRESULT(0);
            }
            // copied before the sender's buffer goes away
            let bytes = std::slice::from_raw_parts(cds.lpData as *const u8, cds.cbData as usize).to_vec();
            match decode(&bytes) {
                Some(m) => {
                    TX.with(|t| {
                        if let Some(tx) = t.borrow().as_ref() {
                            let _ = tx.send(m);
                        }
                    });
                    LRESULT(1)
                }
                None => LRESULT(0),
            }
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

/// The extension's window, on a thread of its own: every `Msg` that arrives goes to the channel.
pub struct Receiver {
    hwnd: isize,
}

impl Receiver {
    pub fn start(tx: Sender<Msg>) -> std::io::Result<Receiver> {
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<isize>();
        std::thread::spawn(move || unsafe {
            TX.with(|t| *t.borrow_mut() = Some(tx));
            let Ok(hinst) = GetModuleHandleW(None) else {
                let _ = ready_tx.send(0);
                return;
            };
            let wc = WNDCLASSW { lpfnWndProc: Some(wndproc), hInstance: hinst.into(), lpszClassName: class(), ..Default::default() };
            // a second Receiver in the same process finds the class already there
            RegisterClassW(&wc);
            let hwnd = CreateWindowExW(WINDOW_EX_STYLE(0), class(), PCWSTR::null(), WINDOW_STYLE(0), 0, 0, 0, 0, HWND_MESSAGE, None, hinst, None);
            let Ok(hwnd) = hwnd else {
                let _ = ready_tx.send(0);
                return;
            };
            let _ = ready_tx.send(hwnd.0 as isize);
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                DispatchMessageW(&msg);
            }
        });
        match ready_rx.recv() {
            Ok(h) if h != 0 => Ok(Receiver { hwnd: h }),
            _ => Err(std::io::Error::new(std::io::ErrorKind::Other, "could not create the window")),
        }
    }

    #[cfg(test)]
    pub fn hwnd(&self) -> isize {
        self.hwnd
    }
}

impl Drop for Receiver {
    fn drop(&mut self) {
        unsafe {
            let _ = PostMessageW(HWND(self.hwnd as *mut c_void), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::sample;
    use std::sync::mpsc::channel;
    use std::time::{Duration, Instant};

    #[test]
    fn a_message_reaches_the_extension() {
        let (tx, rx) = channel();
        let r = Receiver::start(tx).expect("window");
        assert!(send(r.hwnd(), &encode(&sample())));
        assert_eq!(rx.recv_timeout(Duration::from_secs(2)).ok(), Some(sample()));
        assert_ne!(find_ext(), 0, "the hook finds a window of this exe");
    }

    #[test]
    fn other_ids_and_garbage_are_dropped() {
        let (tx, rx) = channel();
        let r = Receiver::start(tx).expect("window");
        send_with(r.hwnd(), 0x434C_4155, &encode(&sample()));
        send(r.hwnd(), b"xx");
        send(r.hwnd(), &vec![b' '; 17 * 1024]);
        assert!(rx.recv_timeout(Duration::from_millis(300)).is_err());
    }

    #[test]
    fn hook_without_extension_is_quick() {
        let t = Instant::now();
        assert!(!send(0, &encode(&sample())));
        assert!(t.elapsed() < Duration::from_millis(600));
    }

    /// Only a window owned by this same exe gets the hook's data, not any window that copies the class name.
    #[test]
    fn only_our_own_exe_is_the_extension() {
        let (tx, _rx) = channel();
        let r = Receiver::start(tx).expect("window");
        assert!(is_ours(r.hwnd()), "a window of this process");
        let desktop = unsafe { windows::Win32::UI::WindowsAndMessaging::GetDesktopWindow() };
        assert!(!is_ours(desktop.0 as isize), "a window of another process");
        assert!(!is_ours(0));
    }

    #[test]
    fn dropping_the_receiver_closes_its_window() {
        let (tx, _rx) = channel();
        let r = Receiver::start(tx).expect("window");
        let h = r.hwnd();
        drop(r);
        let t = Instant::now();
        while crate::procs::window_alive(h) && t.elapsed() < Duration::from_secs(2) {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!crate::procs::window_alive(h));
    }
}
