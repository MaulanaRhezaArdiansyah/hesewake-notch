//! The setup window: server address, database, login and API key. Connect tries them against Odoo on a thread of its
//! own (the window stays responsive), and when they work it saves the key in Credential Manager and hands the settings
//! to the main loop, which saves them and refreshes. The window has its own thread and message loop; one at a time.

use crate::cfg::{parse_url, OdooCfg};
use crate::{cred, tickets};
use std::cell::RefCell;
use std::ffi::c_void;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::mpsc::Sender;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{BOOL, COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateFontW, DeleteObject, GetMonitorInfoW, GetSysColor, GetSysColorBrush, InvalidateRect, MonitorFromPoint, SetBkMode, SetTextColor, COLOR_GRAYTEXT,
    COLOR_WINDOW, HBRUSH, HDC, HFONT, MONITORINFO, MONITOR_DEFAULTTOPRIMARY, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForMonitor, GetDpiForWindow, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::*;

/// The open window (its handle), -1 while it is being made, 0 when there is none.
static OPEN: AtomicIsize = AtomicIsize::new(0);

const ID_OK: i32 = 1; // IDOK: Enter
const ID_CANCEL: i32 = 2; // IDCANCEL: Esc
const WM_DONE: u32 = WM_APP + 1;
const EM_SETCUEBANNER: u32 = 0x1501;
const EM_LIMITTEXT: u32 = 0x00C5;

/// The client area at 96 dpi.
const WIDTH: i32 = 400;
const MARGIN: i32 = 20;

/// What the Connect thread found.
struct Done {
    cfg: OdooCfg,
    /// A key typed in the window: saved only when it worked.
    new_key: Option<String>,
    result: Result<tickets::TestOk, String>,
}

struct Win {
    id_lang: bool,
    cfg: OdooCfg,
    had_key: bool,
    tx: Sender<OdooCfg>,
    /// Each control with its place at 96 dpi.
    ctls: Vec<(HWND, RECT)>,
    title: HWND,
    hint: HWND,
    statics: Vec<HWND>,
    /// URL, database, login, key.
    edits: [HWND; 4],
    status: HWND,
    buttons: [HWND; 2],
    font: HFONT,
    title_font: HFONT,
    busy: bool,
    error: bool,
}

thread_local! {
    static WIN: RefCell<Option<Win>> = const { RefCell::new(None) };
}

/// Runs `f` on the window's state, unless a message arrived while it was already in use (then nothing).
fn with<R>(f: impl FnOnce(&mut Win) -> R) -> Option<R> {
    WIN.with(|w| w.try_borrow_mut().ok().and_then(|mut w| w.as_mut().map(f)))
}

fn tx(id_lang: bool, en: &'static str, id: &'static str) -> &'static str {
    if id_lang {
        id
    } else {
        en
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// Open the window, or bring the open one forward.
pub fn open(cfg: OdooCfg, id_lang: bool, done: Sender<OdooCfg>) {
    match OPEN.compare_exchange(0, -1, Ordering::SeqCst, Ordering::SeqCst) {
        Ok(_) => {
            std::thread::spawn(move || unsafe { run(cfg, id_lang, done) });
        }
        Err(h) if h > 0 => unsafe {
            let hwnd = HWND(h as *mut c_void);
            let _ = ShowWindow(hwnd, SW_RESTORE);
            let _ = SetForegroundWindow(hwnd);
            let _ = FlashWindow(hwnd, BOOL(1));
        },
        Err(_) => {}
    }
}

unsafe fn run(cfg: OdooCfg, id_lang: bool, done: Sender<OdooCfg>) {
    if let Some(hwnd) = make(cfg, id_lang, done) {
        OPEN.store(hwnd.0 as isize, Ordering::SeqCst);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            if IsDialogMessageW(hwnd, &msg).as_bool() {
                continue;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    WIN.with(|w| {
        if let Some(w) = w.borrow_mut().take() {
            let _ = DeleteObject(w.font);
            let _ = DeleteObject(w.title_font);
        }
    });
    OPEN.store(0, Ordering::SeqCst);
}

const STYLE: WINDOW_STYLE = WINDOW_STYLE(WS_OVERLAPPED.0 | WS_CAPTION.0 | WS_SYSMENU.0 | WS_MINIMIZEBOX.0);
// topmost: the notch started this process, so Windows may not let it come to the front by itself
const EX_STYLE: WINDOW_EX_STYLE = WINDOW_EX_STYLE(WS_EX_TOPMOST.0 | WS_EX_CONTROLPARENT.0);

/// Where each control goes, top to bottom, at 96 dpi, and the height of the client area.
fn rows() -> (Vec<RECT>, i32) {
    let inner = WIDTH - 2 * MARGIN;
    let r = |y: i32, h: i32| RECT { left: MARGIN, top: y, right: MARGIN + inner, bottom: y + h };
    let mut v = Vec::new();
    let mut y = MARGIN;
    v.push(r(y, 26)); // title
    y += 32;
    v.push(r(y, 48)); // hint
    y += 58;
    for _ in 0..4 {
        v.push(r(y, 18)); // label
        v.push(r(y + 20, 26)); // edit
        y += 58;
    }
    v.push(r(y, 36)); // status
    y += 44;
    let bw = 100;
    v.push(RECT { left: WIDTH - MARGIN - 2 * bw - 8, top: y, right: WIDTH - MARGIN - bw - 8, bottom: y + 30 }); // Connect
    v.push(RECT { left: WIDTH - MARGIN - bw, top: y, right: WIDTH - MARGIN, bottom: y + 30 }); // Cancel
    (v, y + 30 + MARGIN)
}

fn scale(v: i32, dpi: u32) -> i32 {
    (v as i64 * dpi as i64 / 96) as i32
}

unsafe fn make(cfg: OdooCfg, id_lang: bool, done: Sender<OdooCfg>) -> Option<HWND> {
    let hinst = HINSTANCE(GetModuleHandleW(None).ok()?.0);
    static CLASS: std::sync::Once = std::sync::Once::new();
    CLASS.call_once(|| {
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinst,
            lpszClassName: w!("HesewakeOdooSetup"),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            hIcon: LoadIconW(hinst, PCWSTR(1 as *const u16)).unwrap_or_default(),
            hbrBackground: HBRUSH((COLOR_WINDOW.0 + 1) as isize as *mut c_void),
            ..Default::default()
        };
        RegisterClassW(&wc);
    });

    // centred on the screen with the mouse (where the click that opened it was)
    let mut pt = POINT::default();
    let _ = GetCursorPos(&mut pt);
    let mon = MonitorFromPoint(pt, MONITOR_DEFAULTTOPRIMARY);
    let (mut dpi, mut dpi_y) = (96u32, 96u32);
    let _ = GetDpiForMonitor(mon, MDT_EFFECTIVE_DPI, &mut dpi, &mut dpi_y);
    let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
    let _ = GetMonitorInfoW(mon, &mut mi);
    let (_, height) = rows();
    let mut r = RECT { left: 0, top: 0, right: scale(WIDTH, dpi), bottom: scale(height, dpi) };
    let _ = AdjustWindowRectExForDpi(&mut r, STYLE, BOOL(0), EX_STYLE, dpi);
    let (w, h) = (r.right - r.left, r.bottom - r.top);
    let work = mi.rcWork;
    let x = work.left + ((work.right - work.left) - w) / 2;
    let y = work.top + ((work.bottom - work.top) - h) / 2;
    let caption = wide(tx(id_lang, "Odoo for Hesewake Notch", "Odoo untuk Hesewake Notch"));
    let hwnd = CreateWindowExW(EX_STYLE, w!("HesewakeOdooSetup"), PCWSTR(caption.as_ptr()), STYLE, x, y, w, h, None, None, hinst, None).ok()?;

    let ctl = |class: PCWSTR, text: &str, style: u32, id: i32| -> HWND {
        let t = wide(text);
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            PCWSTR(t.as_ptr()),
            WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | style),
            0,
            0,
            0,
            0,
            hwnd,
            HMENU(id as isize as *mut c_void),
            hinst,
            None,
        )
        .unwrap_or_default()
    };
    let t = |en, id| tx(id_lang, en, id);
    let had_key = cred::has_key();
    let (places, _) = rows();
    let mut ctls = Vec::new();
    let mut statics = Vec::new();
    let mut places = places.into_iter();
    let mut add = |h: HWND, ctls: &mut Vec<(HWND, RECT)>| {
        if let Some(p) = places.next() {
            ctls.push((h, p));
        }
        h
    };

    let title = add(ctl(w!("STATIC"), t("Connect to Odoo", "Sambungkan ke Odoo"), 0, -1), &mut ctls);
    let hint = add(
        ctl(
            w!("STATIC"),
            t(
                "Make an API key in Odoo: your profile > Preferences > Account Security > New API Key. The key stays in Windows Credential Manager.",
                "Buat API key di Odoo: profil > Preferences > Account Security > New API Key. Key disimpan di Windows Credential Manager.",
            ),
            0,
            -1,
        ),
        &mut ctls,
    );
    statics.extend([title, hint]);
    let fields: [(&str, &str, &str, u32, usize); 4] = [
        (t("Server address", "Alamat server"), cfg.url.as_str(), "https://company.odoo.com", 0, 300),
        (t("Database", "Database"), cfg.db.as_str(), "", 0, 100),
        (t("Login (email)", "Login (email)"), cfg.login.as_str(), "", 0, 200),
        (t("API key", "API key"), "", "", ES_PASSWORD as u32, 200),
    ];
    let mut edits = [HWND::default(); 4];
    for (i, (label, value, cue, style, max)) in fields.into_iter().enumerate() {
        let l = add(ctl(w!("STATIC"), label, 0, -1), &mut ctls);
        statics.push(l);
        let e = add(ctl(w!("EDIT"), value, WS_TABSTOP.0 | WS_BORDER.0 | ES_AUTOHSCROLL as u32 | style, 101 + i as i32), &mut ctls);
        SendMessageW(e, EM_LIMITTEXT, WPARAM(max), LPARAM(0));
        let cue = if i == 3 && had_key { t("Saved. Leave empty to keep it", "Tersimpan. Kosongkan untuk tetap memakainya") } else { cue };
        if !cue.is_empty() {
            let c = wide(cue);
            SendMessageW(e, EM_SETCUEBANNER, WPARAM(1), LPARAM(c.as_ptr() as isize));
        }
        edits[i] = e;
    }
    let status = add(ctl(w!("STATIC"), "", 0, -1), &mut ctls);
    statics.push(status);
    let ok = add(ctl(w!("BUTTON"), t("Connect", "Sambungkan"), WS_TABSTOP.0 | BS_DEFPUSHBUTTON as u32, ID_OK), &mut ctls);
    let cancel = add(ctl(w!("BUTTON"), t("Cancel", "Batal"), WS_TABSTOP.0 | BS_PUSHBUTTON as u32, ID_CANCEL), &mut ctls);

    WIN.with(|w| {
        *w.borrow_mut() = Some(Win {
            id_lang,
            cfg,
            had_key,
            tx: done,
            ctls,
            title,
            hint,
            statics,
            edits,
            status,
            buttons: [ok, cancel],
            font: HFONT::default(),
            title_font: HFONT::default(),
            busy: false,
            error: false,
        })
    });
    layout(hwnd);
    let _ = ShowWindow(hwnd, SW_SHOW);
    let _ = SetForegroundWindow(hwnd);
    // the first empty box, else the key
    let first = edits.iter().take(3).find(|e| text(**e).is_empty()).copied().unwrap_or(edits[3]);
    let _ = SetFocus(first);
    Some(hwnd)
}

/// Fonts and places for the window's dpi (again when it moves to a screen with another one).
unsafe fn layout(hwnd: HWND) {
    let dpi = GetDpiForWindow(hwnd).max(96);
    let font = |pt: i32, weight: i32| {
        CreateFontW(-(pt * dpi as i32 / 72), 0, 0, 0, weight, 0, 0, 0, 1, 0, 0, 5, 0, w!("Segoe UI"))
    };
    let (body, head) = (font(9, 400), font(13, 600));
    let Some((ctls, title, old)) = with(|w| {
        let old = [w.font, w.title_font];
        w.font = body;
        w.title_font = head;
        (w.ctls.clone(), w.title, old)
    }) else {
        let _ = DeleteObject(body);
        let _ = DeleteObject(head);
        return;
    };
    for (h, r) in ctls {
        let _ = MoveWindow(h, scale(r.left, dpi), scale(r.top, dpi), scale(r.right - r.left, dpi), scale(r.bottom - r.top, dpi), BOOL(1));
        let f = if h == title { head } else { body };
        SendMessageW(h, WM_SETFONT, WPARAM(f.0 as usize), LPARAM(1));
    }
    for f in old {
        if !f.is_invalid() {
            let _ = DeleteObject(f);
        }
    }
}

unsafe fn text(h: HWND) -> String {
    let mut buf = vec![0u16; 512];
    let n = GetWindowTextW(h, &mut buf).max(0) as usize;
    String::from_utf16_lossy(&buf[..n]).trim().to_string()
}

unsafe fn set_status(msg: &str, error: bool) {
    let Some(status) = with(|w| {
        w.error = error;
        w.status
    }) else {
        return;
    };
    let t = wide(msg);
    let _ = SetWindowTextW(status, PCWSTR(t.as_ptr()));
    let _ = InvalidateRect(status, None, BOOL(1));
}

unsafe fn set_busy(busy: bool) {
    let Some((edits, buttons)) = with(|w| {
        w.busy = busy;
        (w.edits, w.buttons)
    }) else {
        return;
    };
    for h in edits.iter().chain(&buttons[..1]) {
        let _ = EnableWindow(*h, BOOL::from(!busy));
    }
}

unsafe fn connect(hwnd: HWND) {
    let Some((edits, id_lang, cfg, had_key, busy)) = with(|w| (w.edits, w.id_lang, w.cfg.clone(), w.had_key, w.busy)) else { return };
    if busy {
        return;
    }
    let t = |en, id| tx(id_lang, en, id);
    let [url, db, login, key] = edits.map(|e| text(e));
    let problem = if url.is_empty() {
        Some((t("Enter the server address", "Isi alamat server"), 0))
    } else if parse_url(&url).is_err() {
        Some((t("The address must start with https://", "Alamat harus diawali https://"), 0))
    } else if db.is_empty() {
        Some((t("Enter the database name", "Isi nama database"), 1))
    } else if login.is_empty() {
        Some((t("Enter your login", "Isi login"), 2))
    } else if key.is_empty() && !had_key {
        Some((t("Enter your API key", "Isi API key"), 3))
    } else {
        None
    };
    if let Some((msg, field)) = problem {
        set_status(msg, true);
        let _ = SetFocus(edits[field]);
        return;
    }
    let (use_key, new_key) = if key.is_empty() {
        match cred::get_key() {
            Some(k) => (k, None),
            None => {
                set_status(t("Enter your API key", "Isi API key"), true);
                return;
            }
        }
    } else {
        (key.clone(), Some(key))
    };
    let probe = cfg.for_account(url.trim_end_matches('/'), &db, &login);
    set_busy(true);
    set_status(t("Connecting\u{2026}", "Menyambung\u{2026}"), false);
    let target = hwnd.0 as isize;
    std::thread::spawn(move || {
        let result = tickets::run_test(&probe, &use_key);
        let p = Box::into_raw(Box::new(Done { cfg: probe, new_key, result }));
        if PostMessageW(HWND(target as *mut c_void), WM_DONE, WPARAM(0), LPARAM(p as isize)).is_err() {
            // the window was closed meanwhile
            drop(Box::from_raw(p));
        }
    });
}

unsafe fn finish(hwnd: HWND, d: Done) {
    let id_lang = with(|w| w.id_lang).unwrap_or(false);
    let ok = match d.result {
        Ok(ok) => ok,
        Err(e) => {
            set_busy(false);
            let msg = if e.contains("Wrong login") || e.contains("Access Denied") {
                tx(id_lang, "Wrong login or API key", "Login atau API key salah").to_string()
            } else {
                e
            };
            set_status(&msg, true);
            return;
        }
    };
    if let Some(k) = &d.new_key {
        if let Err(e) = cred::set_key(&d.cfg.login, k) {
            set_busy(false);
            set_status(&e, true);
            return;
        }
    }
    let mut cfg = d.cfg;
    tickets::apply_test(&mut cfg, &ok);
    with(|w| {
        let _ = w.tx.send(cfg);
    });
    let _ = DestroyWindow(hwnd);
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_COMMAND => {
            match (wp.0 & 0xFFFF) as i32 {
                ID_OK => connect(hwnd),
                ID_CANCEL => {
                    let _ = DestroyWindow(hwnd);
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_DONE => {
            let d = Box::from_raw(lp.0 as *mut Done);
            finish(hwnd, *d);
            LRESULT(0)
        }
        WM_CTLCOLORSTATIC => {
            let ctl = HWND(lp.0 as *mut c_void);
            let Some((mine, hint, status, error)) = with(|w| (w.statics.contains(&ctl), w.hint, w.status, w.error)) else {
                return DefWindowProcW(hwnd, msg, wp, lp);
            };
            if !mine {
                return DefWindowProcW(hwnd, msg, wp, lp);
            }
            let dc = HDC(wp.0 as *mut c_void);
            SetBkMode(dc, TRANSPARENT);
            if ctl == status && error {
                SetTextColor(dc, COLORREF(0x001C_2BC4)); // red (BGR)
            } else if ctl == hint {
                SetTextColor(dc, COLORREF(GetSysColor(COLOR_GRAYTEXT)));
            }
            LRESULT(GetSysColorBrush(COLOR_WINDOW).0 as isize)
        }
        WM_DPICHANGED => {
            let r = &*(lp.0 as *const RECT);
            let _ = SetWindowPos(hwnd, None, r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_NOZORDER | SWP_NOACTIVATE);
            layout(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_fit_the_window() {
        let (v, h) = rows();
        // title, hint, four labels and boxes, status, two buttons
        assert_eq!(v.len(), 2 + 8 + 1 + 2);
        assert!(v.iter().all(|r| r.left >= MARGIN && r.right <= WIDTH - MARGIN && r.bottom <= h - MARGIN));
        assert!(v.windows(2).take(10).all(|p| p[0].bottom <= p[1].top));
        assert_eq!(scale(WIDTH, 144), 600);
    }
}
