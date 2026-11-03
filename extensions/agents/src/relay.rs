//! From the hook process to the running extension: the extension owns a message-only window
//! (`HesewakeAgentsWindow`); the hook finds it, checks that it belongs to this same exe, and sends its `Msg` as a
//! `WM_COPYDATA`.

use crate::event::Msg;
use std::sync::mpsc::Sender;

/// `dwData` of the `WM_COPYDATA` that carries a `Msg` ("AGNT").
pub const COPY_ID: usize = 0x4147_4E54;

/// The running extension's window (0 if none). Only a window of this same exe counts.
pub fn find_ext() -> isize {
    todo!()
}

/// Is `hwnd` a window of a process running this same exe?
pub fn is_ours(hwnd: isize) -> bool {
    let _ = hwnd;
    todo!()
}

/// Send `bytes` to the extension window `hwnd`. False when there is no window or it doesn't answer within 500 ms.
pub fn send(hwnd: isize, bytes: &[u8]) -> bool {
    send_with(hwnd, COPY_ID, bytes)
}

fn send_with(hwnd: isize, id: usize, bytes: &[u8]) -> bool {
    let _ = (hwnd, id, bytes);
    todo!()
}

/// `--hook claude`: read the hook JSON from stdin and pass it to the running extension. Never fails, prints nothing.
pub fn run_hook() {
    todo!()
}

/// The extension's window, on a thread of its own: every `Msg` that arrives goes to the channel.
pub struct Receiver {
    hwnd: isize,
}

impl Receiver {
    pub fn start(tx: Sender<Msg>) -> std::io::Result<Receiver> {
        let _ = tx;
        todo!()
    }

    pub fn hwnd(&self) -> isize {
        self.hwnd
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{encode, sample};
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
