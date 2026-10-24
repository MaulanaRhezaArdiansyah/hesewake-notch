//! Toast Caps/Num/Scroll Lock: hook keyboard low-level (`WH_KEYBOARD_LL`) yang hanya meneruskan penekanan tombol kunci
//! ke window utama. Isi hook sengaja minimal (batas waktu hook singkat); keadaan On/Off dibaca di UI thread.

use super::*;
use std::sync::atomic::{AtomicIsize, Ordering};
use windows::Win32::UI::WindowsAndMessaging::{CallNextHookEx, SetWindowsHookExW, UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT, WH_KEYBOARD_LL};

/// Pesan ke window: tombol kunci dilepas (`wParam` = virtual-key).
pub(crate) const WM_LOCKKEY: u32 = WM_APP + 4;

const VK_CAPITAL: u32 = 0x14;
const VK_NUMLOCK: u32 = 0x90;
const VK_SCROLL: u32 = 0x91;

static HOOK: AtomicIsize = AtomicIsize::new(0);
static TARGET: AtomicIsize = AtomicIsize::new(0);

/// Pasang/lepas hook sesuai pengaturan. Hook hanya terpasang selama fitur aktif.
pub(crate) fn set_enabled(hwnd: isize, on: bool) {
    TARGET.store(hwnd, Ordering::Relaxed);
    let cur = HOOK.load(Ordering::Relaxed);
    unsafe {
        if on && cur == 0 {
            let hmod = GetModuleHandleW(None).unwrap_or_default();
            if let Ok(h) = SetWindowsHookExW(WH_KEYBOARD_LL, Some(proc), HINSTANCE(hmod.0), 0) {
                HOOK.store(h.0 as isize, Ordering::Relaxed);
            }
        } else if !on && cur != 0 {
            let _ = UnhookWindowsHookEx(HHOOK(cur as *mut c_void));
            HOOK.store(0, Ordering::Relaxed);
        }
    }
}

unsafe extern "system" fn proc(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    // HC_ACTION = 0; tombol dilepas (WM_KEYUP / WM_SYSKEYUP)
    if code == 0 && (wp.0 as u32 == WM_KEYUP || wp.0 as u32 == WM_SYSKEYUP) {
        let k = &*(lp.0 as *const KBDLLHOOKSTRUCT);
        if matches!(k.vkCode, VK_CAPITAL | VK_NUMLOCK | VK_SCROLL) {
            let _ = PostMessageW(HWND(TARGET.load(Ordering::Relaxed) as *mut c_void), WM_LOCKKEY, WPARAM(k.vkCode as usize), LPARAM(0));
        }
    }
    CallNextHookEx(None, code, wp, lp)
}

/// Jenis tombol dari virtual-key (None bila bukan tombol kunci).
pub(crate) fn kind_of(vk: u32) -> Option<crate::toast::HudKind> {
    use crate::toast::HudKind;
    match vk {
        VK_CAPITAL => Some(HudKind::Caps),
        VK_NUMLOCK => Some(HudKind::Num),
        VK_SCROLL => Some(HudKind::Scroll),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toast::HudKind;

    #[test]
    fn maps_lock_keys_only() {
        assert_eq!(kind_of(0x14), Some(HudKind::Caps));
        assert_eq!(kind_of(0x90), Some(HudKind::Num));
        assert_eq!(kind_of(0x91), Some(HudKind::Scroll));
        assert_eq!(kind_of(0x41), None);
    }
}
