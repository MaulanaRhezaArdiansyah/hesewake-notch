//! Indikator mikrofon/kamera yang sedang dipakai. Windows mencatat pemakaian per aplikasi di
//! `HKCU\...\CapabilityAccessManager\ConsentStore\{microphone,webcam}` (nilai QWORD `LastUsedTimeStart/Stop`): aplikasi
//! sedang memakai perangkat bila Start > 0 dan Stop == 0. Dipantau dengan `RegNotifyChangeKeyValue` (berbasis event,
//! tanpa polling); hanya nama aplikasi yang diambil.

use super::*;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::OnceLock;
use windows::core::PWSTR;
use windows::Win32::System::Registry::{
    RegCloseKey, RegEnumKeyExW, RegNotifyChangeKeyValue, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, KEY_NOTIFY, KEY_READ,
    REG_NOTIFY_CHANGE_LAST_SET, REG_NOTIFY_CHANGE_NAME,
};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject, INFINITE};

/// Pesan ke window: daftar aplikasi pemakai mic/kamera berubah.
pub(crate) const WM_PRIVACY: u32 = WM_APP + 5;

const ROOT: &str = r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore";
/// [0] = mikrofon, [1] = kamera.
static STATE: Mutex<[Vec<String>; 2]> = Mutex::new([Vec::new(), Vec::new()]);
static ENABLED: AtomicBool = AtomicBool::new(true);
static TARGET: AtomicIsize = AtomicIsize::new(0);

pub(crate) fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

/// (aplikasi pemakai mikrofon, aplikasi pemakai kamera).
pub(crate) fn snapshot() -> (Vec<String>, Vec<String>) {
    let s = STATE.lock().unwrap_or_else(|p| p.into_inner());
    (s[0].clone(), s[1].clone())
}

pub(crate) fn start(hwnd: isize) {
    static ONCE: OnceLock<()> = OnceLock::new();
    if ONCE.set(()).is_err() {
        return;
    }
    TARGET.store(hwnd, Ordering::Relaxed);
    for (idx, dev) in ["microphone", "webcam"].into_iter().enumerate() {
        std::thread::spawn(move || watch(idx, dev));
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

unsafe fn open(parent: HKEY, path: &str, access: windows::Win32::System::Registry::REG_SAM_FLAGS) -> Option<HKEY> {
    let mut k = HKEY::default();
    let w = wide(path);
    RegOpenKeyExW(parent, PCWSTR(w.as_ptr()), 0, access, &mut k).is_ok().then_some(k)
}

unsafe fn subkeys(k: HKEY) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0u32;
    loop {
        let mut buf = [0u16; 512];
        let mut len = buf.len() as u32;
        if RegEnumKeyExW(k, i, PWSTR(buf.as_mut_ptr()), &mut len, None, PWSTR::null(), None, None).is_err() {
            break;
        }
        out.push(String::from_utf16_lossy(&buf[..len as usize]));
        i += 1;
    }
    out
}

unsafe fn qword(k: HKEY, name: &str) -> Option<u64> {
    let w = wide(name);
    let mut buf = [0u8; 8];
    let mut len = 8u32;
    RegQueryValueExW(k, PCWSTR(w.as_ptr()), None, None, Some(buf.as_mut_ptr()), Some(&mut len)).is_ok().then(|| u64::from_le_bytes(buf))
}

/// Aplikasi di bawah `parent\name` yang sedang memakai perangkat (Start > 0 dan Stop == 0).
unsafe fn is_in_use(parent: HKEY, name: &str) -> bool {
    let Some(k) = open(parent, name, KEY_READ) else { return false };
    let r = matches!((qword(k, "LastUsedTimeStart"), qword(k, "LastUsedTimeStop")), (Some(s), Some(0)) if s > 0);
    let _ = RegCloseKey(k);
    r
}

unsafe fn apps_in_use(root: HKEY) -> Vec<String> {
    let mut out = Vec::new();
    for name in subkeys(root) {
        if name == "NonPackaged" {
            if let Some(np) = open(root, "NonPackaged", KEY_READ) {
                for n in subkeys(np) {
                    if is_in_use(np, &n) {
                        out.push(friendly(&n));
                    }
                }
                let _ = RegCloseKey(np);
            }
        } else if is_in_use(root, &name) {
            out.push(friendly(&name));
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Nama pendek: `C:#...#chrome.exe` -> `chrome`; `5319275A.WhatsAppDesktop_cv1g...` -> `WhatsAppDesktop`.
pub(crate) fn friendly(raw: &str) -> String {
    if raw.contains('#') || raw.contains('\\') {
        let last = raw.rsplit(['#', '\\']).next().unwrap_or(raw);
        return last.rsplit_once('.').map_or(last, |(n, _)| n).to_string();
    }
    let fam = raw.split('_').next().unwrap_or(raw);
    fam.rsplit('.').next().unwrap_or(fam).to_string()
}

fn watch(idx: usize, dev: &'static str) {
    unsafe {
        let Some(root) = open(HKEY_CURRENT_USER, &format!(r"{ROOT}\{dev}"), KEY_NOTIFY | KEY_READ) else { return };
        let Ok(ev) = CreateEventW(None, false, false, None) else { return };
        loop {
            let apps = apps_in_use(root);
            let changed = {
                let mut s = STATE.lock().unwrap_or_else(|p| p.into_inner());
                let c = s[idx] != apps;
                s[idx] = apps;
                c
            };
            if changed && ENABLED.load(Ordering::Relaxed) {
                let _ = PostMessageW(HWND(TARGET.load(Ordering::Relaxed) as *mut c_void), WM_PRIVACY, WPARAM(idx), LPARAM(0));
            }
            // tidur sampai ada perubahan di bawah kunci ini (nama atau nilai)
            if RegNotifyChangeKeyValue(root, true, REG_NOTIFY_CHANGE_LAST_SET | REG_NOTIFY_CHANGE_NAME, ev, true).is_err() {
                return;
            }
            WaitForSingleObject(ev, INFINITE);
            // beri jeda singkat agar penulisan Start/Stop selesai sebelum dibaca
            std::thread::sleep(Duration::from_millis(60));
        }
    }
}

impl App {
    /// Terapkan perubahan daftar pemakai; toast saat aplikasi baru mulai memakai mic/kamera.
    pub(crate) fn privacy_apply(&mut self) {
        let (mic, cam) = snapshot();
        let new_mic: Vec<String> = mic.iter().filter(|a| !self.priv_mic.contains(a)).cloned().collect();
        let new_cam: Vec<String> = cam.iter().filter(|a| !self.priv_cam.contains(a)).cloned().collect();
        if mic != self.priv_mic || cam != self.priv_cam {
            self.priv_t = Instant::now();
        }
        self.priv_mic = mic;
        self.priv_cam = cam;
        if !self.cfg.privacy_dot {
            return;
        }
        if let Some(app) = new_cam.first() {
            self.push_toast(toast::ToastKind::Privacy { cam: true, app: app.clone() });
        } else if let Some(app) = new_mic.first() {
            self.push_toast(toast::ToastKind::Privacy { cam: false, app: app.clone() });
        }
    }

    pub(crate) fn privacy_animating(&self) -> bool {
        self.cfg.privacy_dot && self.priv_t.elapsed().as_secs_f32() < 0.5
    }

    /// Titik di pojok kanan atas notch kecil: oranye = mikrofon, hijau = kamera.
    pub(crate) fn draw_privacy_dots(&self, g: &mut Gfx, w: f32, ca: f32) {
        if !self.cfg.privacy_dot || ca <= 0.05 {
            return;
        }
        let e = self.priv_t.elapsed().as_secs_f32();
        let pop = (1.0 - (-9.0 * e).exp() * (16.0 * e).cos()).clamp(0.0, 1.2);
        let x0 = CX - w / 2.0;
        let mut x = x0 + w - 9.0;
        for (on, rgb) in [(!self.priv_cam.is_empty(), (48u8, 209u8, 88u8)), (!self.priv_mic.is_empty(), (255u8, 159u8, 10u8))] {
            if on {
                g.circle(x, 7.0, 3.3 * pop.min(1.15), &paint(rgb.0, rgb.1, rgb.2, ca));
                x -= 9.0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn friendly_names() {
        assert_eq!(friendly(r"C:#Program Files (x86)#Google#Chrome#Application#chrome.exe"), "chrome");
        assert_eq!(friendly(r"C:\Users\x\AppData\Roaming\Zoom\bin\Zoom.exe"), "Zoom");
        assert_eq!(friendly("5319275A.WhatsAppDesktop_cv1g1gvanyjgm"), "WhatsAppDesktop");
        assert_eq!(friendly("MSTeams_8wekyb3d8bbwe"), "MSTeams");
        assert_eq!(friendly("Microsoft.WindowsCamera_8wekyb3d8bbwe"), "WindowsCamera");
    }
}
