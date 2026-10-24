//! Pacing animasi: waitable timer resolusi tinggi + loop pesan sendiri, menggantikan WM_TIMER
//! (WM_TIMER berprioritas terendah dan beresolusi ~15,6 ms, jadi 16/33 ms sering meleset).

use super::*;
use std::sync::atomic::{AtomicIsize, Ordering};
use windows::Win32::System::Threading::{
    CancelWaitableTimer, CreateWaitableTimerExW, SetWaitableTimer, CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, INFINITE,
    TIMER_ALL_ACCESS,
};

static TIMER: AtomicIsize = AtomicIsize::new(0);

fn handle() -> Option<HANDLE> {
    let h = TIMER.load(Ordering::Relaxed);
    (h != 0).then(|| HANDLE(h as *mut c_void))
}

pub(crate) fn init() {
    unsafe {
        let h = CreateWaitableTimerExW(None, PCWSTR::null(), CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, TIMER_ALL_ACCESS.0)
            .or_else(|_| CreateWaitableTimerExW(None, PCWSTR::null(), 0, TIMER_ALL_ACCESS.0));
        if let Ok(h) = h {
            TIMER.store(h.0 as isize, Ordering::Relaxed);
        }
    }
}

pub(crate) fn available() -> bool {
    handle().is_some()
}

/// Nyalakan timer animasi berperiode `ms` (frame pertama tepat satu periode lagi).
pub(crate) fn set(ms: u32) {
    if let Some(h) = handle() {
        let due = -(ms as i64) * 10_000; // negatif = relatif, satuan 100 ns
        unsafe {
            let _ = SetWaitableTimer(h, &due, ms as i32, None, None, false);
        }
    }
}

pub(crate) fn cancel() {
    if let Some(h) = handle() {
        unsafe {
            let _ = CancelWaitableTimer(h);
        }
    }
}

/// Loop pesan: kuras semua pesan, lalu tidur sampai ada pesan baru atau timer animasi berbunyi.
pub(crate) unsafe fn run_loop() {
    let timer = handle();
    let handles = [timer.unwrap_or_default()];
    let mut msg = MSG::default();
    loop {
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            if msg.message == WM_QUIT {
                return;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let h: Option<&[HANDLE]> = if timer.is_some() { Some(&handles) } else { None };
        let r = MsgWaitForMultipleObjectsEx(h, INFINITE, QS_ALLINPUT, MWMO_INPUTAVAILABLE);
        if timer.is_some() && r == WAIT_OBJECT_0 {
            tick(1);
        }
    }
}
