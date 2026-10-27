#![windows_subsystem = "windows"]
//! Hesewake Notch for Windows 10/11 (flush with the top edge of the screen). Based on Poch by Commonly Works (MIT).
//! - Layered per-pixel alpha window (UpdateLayeredWindow), software rendering (tiny-skia)
//! - Slides: Music (with clock), Water, To-do, Ledge (file shelf, max 5 items), Scrap (mini notepad). Endless carousel: arrow buttons, swipe/scroll, drag, dots
//! - Scrap: scrollable note (max 3,333 chars), caret + selection, clipboard, live character counter
//! - Busy mode: Adobe apps / maximized / fullscreen window -> notch shrinks to a thin line, hover to open
//! - Media: Windows GSMTC (Spotify, browsers, ...). Artwork from GSMTC, fallback to iTunes Search API
//! - Hotkey Ctrl+Alt+H (or "h" when focused): toggle clock <-> eye
//! - Idle: 10 min clock (time), then 15 min "blinking eye" (shapes baked from eye.svg) with a running
//!   blue-shades glow that fades to transparent upwards, then loop (only counts while idle)
//! - Startup: the eye + glow show for 1 s right after launch, then cross-fade into the clock
//! - Data: %APPDATA%\hesewake-notch\todo.txt, water.txt, shelf.txt and notepad.txt (todo/water migrated automatically from the old %APPDATA%\notch folder)
//! - Font: Instrument Sans (embedded in the exe, see assets/)
//! - Memory: small embedded fonts, bounded glyph cache, working set trimmed when idle
//! - Ledge: drag-out via OLE (copy default, Shift+drag = move)

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tiny_skia::{
    Color, FillRule, FilterQuality, GradientStop, LineCap, LinearGradient, Mask, Paint, Path, PathBuilder, Pattern,
    Pixmap, Point, Rect, SpreadMode, Stroke, Transform,
};
use windows::core::{w, Result, HSTRING, PCWSTR, PWSTR};
use windows::Foundation::Uri;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSessionManager as SessionManager,
    GlobalSystemMediaTransportControlsSessionMediaProperties as MediaProps,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as PlayStatus,
};
use windows::Storage::Streams::DataReader;
use windows::Web::Http::HttpClient;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Ole::OleInitialize;
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, QueryFullProcessImageNameW, SetProcessWorkingSetSize, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, ReleaseCapture, SetCapture, SetFocus, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT,
};
use windows::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows::Win32::UI::Shell::{DragFinish, DragQueryFileW, ShellExecuteW, HDROP};
use windows::Win32::UI::WindowsAndMessaging::*;

// ---------------------------------------------------------------- modul (satu file per fitur)
mod anim;
mod audio;
mod bt;
mod busy;
mod cjk;
mod clock;
mod config;
mod data;
mod dragdrop;
mod eye;
mod gfx;
mod icon;
mod icons;
mod input;
mod keys;
mod lyricview;
mod lyrics;
mod monitors;
mod music;
mod nav;
mod net;
mod notif;
mod notif_img;
mod notifview;
mod note;
mod pace;
mod perf;
mod privacy;
mod render;
mod scale;
mod settings;
mod settingsview;
mod shelf;
mod slides;
mod sys;
mod sysview;
mod weather;
mod todo;
mod timer;
mod toast;
mod water;
mod window;

use anim::{schedule, target, tick};
use busy::{is_busy};
use clock::{CLOCK_FADE_IN, DAYS_S, MONTHS, now_tm, today};
use config::{ACCENT, SLIDE_ACCENT, TEXT_DIM, TEXT_FAINT, CL, CW, CX, DROP_AMP, DROP_ANIM_SECS, EXP_H, EXP_W, H, IDLE, LYRIC_SIZE, LX, NAVY, PLAYING, RX, THIN, W};
use data::{data_path, migrate_old_data, trim_memory, write_data};
use eye::{EYE_REST_MS, EyeAnim, HOTKEY_ID, HOTKEY_PLAIN_H, IDLE_CYCLE_SECS, IdlePhase, SPLASH_FADE, SPLASH_SECS, build_eye_paths, splash_tick, toggle_idle_phase};
use gfx::{Box4, Gfx, acc, sacc, dummy_path, ellipsize, in_rect, notch_path, notch_path_w, paint, pixmap_from_rgba, decode_limited, rrect, ty, union_box, wrap_lines};
use icon::{load_icon};
use input::{click, context_menu};
use music::{ArtData, Cmd, MARQUEE_GAP, MARQUEE_PAUSE, MARQUEE_SPEED, MCX, MCY, MW, MX, Media, media_thread};
use nav::{NAV_L, NAV_R, cyc, dot_x};
use note::{NOTE_H, NOTE_Y, NoteView, load_note, save_note};
use render::present;
use shelf::{SH_GAP, SH_H, SH_W, SH_Y, ShelfItem, load_shelf, open_path, save_shelf};
use todo::{ROW_H, ROW_P, load_todos, rows_geom, save_todos};
use toast::{HudKind, Toast, ToastKind};
use settings::Settings;
use slides::SlideId;
use water::{Water, WaterUi, check_reminder};
use window::{wndproc};

// ---------------------------------------------------------------- state
struct App {
    hwnd: isize,
    wx: i32,
    /// Tepi atas window (koordinat layar): bukan 0 bila notch di monitor lain.
    wy: i32,
    /// Monitor yang sedang dipakai notch (koordinat layar).
    mon: RECT,
    mem_dc: isize,
    /// Bitmap window (DIB) yang sedang dipilih ke `mem_dc`; dibuat ulang saat skala berubah.
    bmp: isize,
    bits: usize,
    /// Skala DPI aktif (lihat scale.rs) dan nilai paksa dari `--scale=`.
    scale: f32,
    scale_forced: Option<f32>,
    gfx: Option<Gfx>,
    w: f32,
    h: f32,
    vw: f32,
    vh: f32,
    slide: i32,
    sp: f32,
    sv: f32,
    drag: Option<(f32, f32)>,
    drag_dx: f32,
    shelf_drag: Option<(usize, f32, f32)>,
    last_wheel: Instant,
    t: f32,
    last: Instant,
    interval: u32,
    tick_n: u32,
    last_sec: u16,
    last_min: u16,
    hover: bool,
    busy: bool,
    /// Mode "Minimal notch": garis tipis saat tidak ada musik, buka dengan hover.
    minimal: bool,
    toast: Option<Toast>,
    bt_shared: bt::BtSlot,
    notifs: Vec<notif::NotifItem>,
    notif_access: notif::Access,
    notif_slot: notif::NotifSlot,
    notif_on: bool,
    notif_started: bool,
    notif_scroll: usize,
    nui: notifview::NotifUi,
    bell_t: Instant,
    toast_q: Vec<ToastKind>,
    toast_end: Instant,
    ac_prev: Option<bool>,
    low_armed: bool,
    demo_toast: Option<ToastKind>,
    demo_slide: Option<usize>,
    /// Layar mati: timer animasi dan poll dihentikan.
    display_off: bool,
    menu_open: bool,
    peek_until: Option<Instant>,
    reminding: bool,
    splash: Option<Instant>,
    idle_base: f32,
    idle_mark: Option<Instant>,
    was_idle: bool,
    eye: EyeAnim,
    eye_paths: Vec<Path>,
    cur_phase: IdlePhase,
    shared: Arc<Mutex<Media>>,
    art_shared: Arc<Mutex<Option<ArtData>>>,
    lyr_shared: lyrics::LyricsSlot,
    lyrics: lyrics::Lyrics,
    lyric_idx: Option<usize>,
    lyric_w: f32,
    lyric_mode: bool,
    lyric_mode_t0: Instant,
    lyric_scroll: f32,
    lyric_sv: f32,
    lyric_t: Instant,
    pos_at: Instant,
    media: Media,
    art: Option<Pixmap>,
    /// Cover lagu sebelumnya (untuk cross-fade) dan waktu cover terbaru datang.
    prev_art: Option<Pixmap>,
    art_t0: Instant,
    tint: music::Tint,
    icon: Option<Pixmap>,
    art_key: String,
    title_t0: f32,
    todos: Vec<(bool, String)>,
    todo_scroll: usize,
    edit_mode: bool,
    editing: bool,
    edit_idx: Option<usize>,
    input: String,
    caret_on: bool,
    edit_start: Instant,
    prev_fg: isize,
    shelf: Vec<ShelfItem>,
    shelf_flash: Option<Instant>,
    drop_pulse: Option<Instant>,
    note: String,
    note_mode: bool,
    nv: NoteView,
    water: Water,
    wui: WaterUi,
    /// Stopwatch / hitung mundur (slide Timer).
    tm: timer::TimerState,
    tx: Sender<Cmd>,
    /// Pengaturan (settings.txt) dan urutan slide yang sedang tampil.
    cfg: Settings,
    /// Aplikasi yang sedang memakai mikrofon / kamera, dan waktu perubahan terakhir (animasi titik).
    priv_mic: Vec<String>,
    priv_cam: Vec<String>,
    priv_t: Instant,
    /// Indikator volume di slide Music setelah scroll: waktu terakhir dan (persen, bisu).
    vol_t: Instant,
    /// Peristiwa kecerahan pertama (saat pendaftaran) hanya dicatat, tidak ditampilkan.
    bright_seen: bool,
    vol_state: (u8, bool),
    order: Vec<SlideId>,
    /// Slide Settings sedang dibuka (ada di carousel) dan slide asal sebelum dibuka.
    settings_open: bool,
    settings_prev: Option<SlideId>,
    sui: settingsview::SettingsUi,
    sys_ui: sysview::SysUi,
    /// Indeks tahap cuaca terakhir yang digambar (memicu transisi suhu <-> ikon).
    weather_idx: u64,
}

static APP: Mutex<Option<App>> = Mutex::new(None);

fn with_app<F: FnOnce(&mut App)>(f: F) {
    use std::sync::TryLockError;
    // Kalau pernah ada panic saat lock dipegang, mutex jadi "poisoned": tetap pakai datanya, jangan mati total.
    let mut g = match APP.try_lock() {
        Ok(g) => g,
        Err(TryLockError::Poisoned(p)) => p.into_inner(),
        Err(TryLockError::WouldBlock) => return,
    };
    if let Some(a) = g.as_mut() {
        f(a);
    }
}

impl App {
    fn expanded(&self) -> bool {
        self.hover
            || self.editing
            // menekan tombol di toast Water (Snooze/Drank) bukan gestur geser: toast tidak boleh dibatalkan sebelum klik selesai
            || (self.drag.is_some() && !self.water_toast())
            || self.peek_until.map_or(false, |t| Instant::now() < t)
    }

    fn idle(&self) -> bool {
        !self.expanded() && !self.thin() && !self.media.playing
    }

    /// Notch menyusut jadi garis tipis: app penuh layar/Adobe (busy) atau mode Minimal saat tidak ada musik.
    fn thin(&self) -> bool {
        self.busy || (self.minimal && !self.media.playing)
    }

    fn ready(&self) -> bool {
        self.h >= EXP_H - 30.0
    }
}

/// `--demo-toast=charging|unplugged|low|bt|bt-off`: tampilkan satu toast setelah start (uji visual tanpa colok kabel).
const DEMO_NOTIF_ID: u32 = 999_999;

/// `--demo-playing`: lagu palsu sedang diputar (uji tata letak notch kecil tanpa pemutar media sungguhan).
pub(crate) fn demo_playing() -> bool {
    std::env::args().any(|a| a == "--demo-playing")
}

/// `--demo-priv=cam,mic`: nyalakan titik kamera dan/atau mikrofon sejak start.
fn demo_priv(dev: &str) -> Vec<String> {
    let on = std::env::args().find_map(|a| a.strip_prefix("--demo-priv=").map(|v| v.split(',').any(|d| d == dev)));
    if on == Some(true) { vec!["Demo".to_string()] } else { Vec::new() }
}

/// `--demo-timer=count|stop`: mulai hitung mundur / stopwatch sejak start (uji tampilan chip di notch idle).
fn demo_timer(mut t: timer::TimerState) -> timer::TimerState {
    match std::env::args().find_map(|a| a.strip_prefix("--demo-timer=").map(str::to_string)).as_deref() {
        Some("stop") => t.set_mode(timer::TMode::Stopwatch),
        Some("count") => t.set_mode(timer::TMode::Countdown),
        _ => return t,
    }
    t.start(Instant::now());
    t
}

fn demo_toast() -> Option<ToastKind> {
    let arg = std::env::args().find_map(|a| a.strip_prefix("--demo-toast=").map(str::to_string))?;
    Some(match arg.as_str() {
        "charging" => ToastKind::Charging { pct: 70 },
        "unplugged" => ToastKind::Unplugged { pct: 70 },
        "low" => ToastKind::LowBattery { pct: 20 },
        "bt" => ToastKind::Bluetooth { name: "AirBuds Pro".into(), pct: Some(70), connected: true },
        "notif" => ToastKind::Notification { id: DEMO_NOTIF_ID },
        "caps" => ToastKind::Hud { kind: HudKind::Caps, on: true },
        "caps-off" => ToastKind::Hud { kind: HudKind::Caps, on: false },
        "num" => ToastKind::Hud { kind: HudKind::Num, on: true },
        "volume" => ToastKind::Hud { kind: HudKind::Volume(46, false), on: true },
        "brightness" => ToastKind::Hud { kind: HudKind::Brightness(72), on: true },
        "mute" => ToastKind::Hud { kind: HudKind::Volume(0, true), on: true },
        "water" => ToastKind::Water,
        "timer" => ToastKind::Timer { label: "5 min timer".into() },
        "mic" => ToastKind::Privacy { cam: false, app: "Zoom".into() },
        "cam" => ToastKind::Privacy { cam: true, app: "chrome".into() },
        "bt-off" => ToastKind::Bluetooth { name: "AirBuds Pro".into(), pct: None, connected: false },
        _ => ToastKind::Bluetooth { name: "soundcore R50i".into(), pct: None, connected: true },
    })
}

fn main() -> Result<()> {
    // Karena windows_subsystem = "windows", panic tidak terlihat. Catat ke file supaya penyebab crash bisa dilacak.
    std::panic::set_hook(Box::new(|info| {
        let line = format!("{:?} | {}\n", std::time::SystemTime::now(), info);
        let p = data::data_path("crash.log");
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p) {
            let _ = f.write_all(line.as_bytes());
        }
    }));
    perf::init();
    pace::init();
    if !sys::single_instance() {
        return Ok(());
    }
    // Pindahkan data dan autostart dari nama lama sebelum ada yang membaca pengaturan.
    migrate_old_data();
    sys::migrate_autostart();
    unsafe {
        // Per-monitor: Windows tidak meregangkan window kita; skala digambar sendiri (scale.rs).
        if SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2).is_err() {
            let _ = SetProcessDPIAware();
        }
        let hinst = GetModuleHandleW(None)?;
        let class = w!("HesewakeNotchWindow");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinst.into(),
            lpszClassName: class,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            ..Default::default()
        };
        RegisterClassW(&wc);

        let cfg0 = Settings::load();
        let mon = monitors::resolve(cfg0.monitor);
        let scale_forced = scale::forced_from_args(std::env::args());
        let s0 = scale::effective(cfg0.notch_size, monitors::dpi_of(&mon), scale_forced);
        scale::set_current(s0);
        let (cw0, ch0) = scale::canvas(s0);
        let (wx, wy) = monitors::origin(&mon, cw0);
        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_ACCEPTFILES,
            class,
            w!("Hesewake Notch"),
            WS_POPUP,
            wx,
            wy,
            cw0,
            ch0,
            None,
            None,
            hinst,
            None,
        )?;

        let mods = if HOTKEY_PLAIN_H {
            MOD_NOREPEAT
        } else {
            HOT_KEY_MODIFIERS(MOD_ALT.0 | MOD_CONTROL.0 | MOD_NOREPEAT.0)
        };
        if RegisterHotKey(hwnd, HOTKEY_ID, mods, 'H' as u32).is_err() {
            perf::note("hotkey: Ctrl+Alt+H sudah dipakai aplikasi lain (mis. instance lain), tombol pintas jam/mata nonaktif");
        }
        sys::register_display_notifications(hwnd);
        sys::tray_add(hwnd);

        let screen = GetDC(None);
        let mem = CreateCompatibleDC(screen);
        let (bmp, bits) = scale::make_surface(mem, s0).ok_or_else(windows::core::Error::from_win32)?;
        SelectObject(mem, bmp);
        ReleaseDC(None, screen);

        // OLE dibutuhkan oleh DoDragDrop (drag-out Ledge). Aman dipanggil sekali di UI thread.
        let _ = OleInitialize(None);

        let shared = Arc::new(Mutex::new(Media::default()));
        let art_shared: Arc<Mutex<Option<ArtData>>> = Arc::new(Mutex::new(None));
        let lyr_shared: lyrics::LyricsSlot = Arc::new(Mutex::new(None));
        let (tx, rx) = channel();
        {
            let sh = shared.clone();
            let ar = art_shared.clone();
            let ly = lyr_shared.clone();
            let h = hwnd.0 as isize;
            std::thread::spawn(move || media_thread(h, sh, ar, ly, rx));
        }

        let bt_shared: bt::BtSlot = Arc::new(Mutex::new(Vec::new()));
        {
            let (slot, h) = (bt_shared.clone(), hwnd.0 as isize);
            std::thread::spawn(move || bt::bt_thread(h, slot));
        }

        let notif_slot: notif::NotifSlot = Arc::new(Mutex::new(Vec::new()));
        let cfg = cfg0;
        let notif_on = cfg.notifications;
        notif::set_avatar(cfg.notif_avatar);
        keys::set_enabled(hwnd.0 as isize, cfg.lock_toast);
        privacy::set_enabled(cfg.privacy_dot);
        privacy::start(hwnd.0 as isize);
        audio::start_watch(hwnd.0 as isize);
        {
            let (_, lat, lon) = weather::CITIES[cfg.weather_city];
            weather::start(hwnd.0 as isize, cfg.weather_on, lat, lon);
        }
        let demo_notifs = std::env::args().any(|a| a == "--demo-notifs");
        if notif_on && !demo_notifs {
            notif::start(hwnd.0 as isize, notif_slot.clone());
        }

        let st = now_tm();
        let mut app = App {
            hwnd: hwnd.0 as isize,
            wx,
            wy,
            mon,
            mem_dc: mem.0 as isize,
            bmp: bmp.0 as isize,
            bits: bits as usize,
            scale: s0,
            scale_forced,
            gfx: Some(scale::new_gfx(s0)),
            w: IDLE.0,
            h: IDLE.1,
            vw: 0.0,
            vh: 0.0,
            slide: 0,
            sp: 0.0,
            sv: 0.0,
            drag: None,
            drag_dx: 0.0,
            shelf_drag: None,
            last_wheel: Instant::now(),
            t: 0.0,
            last: Instant::now(),
            interval: 0,
            tick_n: 0,
            last_sec: st.wSecond,
            last_min: st.wMinute,
            hover: false,
            busy: is_busy(hwnd, cfg.busy_on_maximized, &mon),
            minimal: cfg.minimal,
            toast: None,
            bt_shared: bt_shared.clone(),
            notifs: if demo_notifs { notif::demo_items() } else { Vec::new() },
            notif_access: if demo_notifs { notif::Access::Allowed } else { notif::Access::Unknown },
            notif_slot: notif_slot.clone(),
            notif_on,
            notif_started: notif_on,
            notif_scroll: 0,
            nui: notifview::NotifUi::new(),
            bell_t: Instant::now() - Duration::from_secs(10),
            toast_q: Vec::new(),
            toast_end: Instant::now() - Duration::from_secs(10),
            ac_prev: sys::power_status().map(|p| p.0),
            low_armed: true,
            demo_toast: demo_toast(),
            demo_slide: std::env::args().find_map(|a| a.strip_prefix("--demo-slide=").and_then(|n| if n == "settings" { Some(usize::MAX) } else { n.parse().ok() })),
            display_off: false,
            menu_open: false,
            peek_until: None,
            reminding: false,
            splash: Some(Instant::now()),
            idle_base: 0.0,
            idle_mark: None,
            was_idle: false,
            eye: {
                let mut e = EyeAnim::new();
                e.hold = 0.18; // shorter closed-eyes hold so the pop fits inside the 1 s splash
                e
            },
            eye_paths: build_eye_paths(),
            cur_phase: IdlePhase::Clock,
            shared,
            art_shared,
            lyr_shared,
            lyrics: lyrics::Lyrics::None,
            lyric_idx: None,
            lyric_w: 0.0,
            lyric_mode: false,
            lyric_mode_t0: Instant::now() - Duration::from_secs(10),
            lyric_scroll: 0.0,
            lyric_sv: 0.0,
            lyric_t: Instant::now(),
            pos_at: Instant::now(),
            media: if demo_playing() {
                Media { title: "Almost Easy".into(), artist: "Avenged Sevenfold".into(), playing: true, pos: 60, dur: 240, pos_ms: 60_000, source: "Demo".into(), sources: 1 }
            } else {
                Media::default()
            },
            art: None,
            prev_art: None,
            art_t0: Instant::now() - Duration::from_secs(10),
            tint: music::Tint::default(),
            icon: load_icon(),
            art_key: String::new(),
            title_t0: 0.0,
            todos: load_todos(),
            todo_scroll: 0,
            edit_mode: false,
            editing: false,
            edit_idx: None,
            input: String::new(),
            caret_on: true,
            edit_start: Instant::now(),
            prev_fg: 0,
            shelf: load_shelf(),
            shelf_flash: None,
            drop_pulse: None,
            note: load_note(),
            note_mode: false,
            nv: NoteView::default(),
            water: Water::load(),
            wui: WaterUi::new(),
            tm: demo_timer(timer::TimerState::load()),
            tx,
            priv_mic: demo_priv("mic"),
            priv_cam: demo_priv("cam"),
            priv_t: Instant::now() - Duration::from_secs(10),
            vol_t: Instant::now() - Duration::from_secs(10),
            bright_seen: false,
            vol_state: (0, false),
            order: slides::visible_order(&cfg.slides, false),
            settings_open: false,
            settings_prev: None,
            sui: settingsview::SettingsUi::new(),
            sys_ui: sysview::SysUi::new(),
            weather_idx: 0,
            cfg,
        };
        let (tw, th) = target(&app);
        app.w = tw;
        app.h = th;
        present(&mut app);
        *APP.lock().unwrap() = Some(app);

        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        SetTimer(hwnd, 2, 100, None);
        trim_memory();

        pace::run_loop();
    }
    Ok(())
}