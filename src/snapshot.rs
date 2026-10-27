//! Snapshot tampilan untuk review visual tanpa menjalankan exe. Tiap tes menyusun satu keadaan `App`
//! (tanpa window), menggambarnya lewat `render::draw` ke kanvas di memori, lalu memeriksa notch benar-benar
//! tergambar. Bila env `SNAPSHOT_DIR` diisi (dipakai CI), hasilnya disimpan sebagai PNG di atas latar abu-abu
//! supaya tepi notch hitam terlihat; CI mengunggahnya sebagai artifact `ui-snapshots`.
//!
//! Tambah keadaan baru dengan satu fungsi `#[test]` yang memanggil `shot`.

use super::*;
use tiny_skia::PixmapPaint;

/// App tanpa window, data pengguna, atau thread latar: semua slide aktif, splash sudah lewat.
/// Cuaca tidak pernah tampil (tidak ada data cuaca di tes), jadi notch idle selalu dalam keadaan "tanpa cuaca".
fn test_app(s: f32) -> App {
    let (tx, _rx) = channel();
    let mut cfg = Settings::default();
    for (_, on) in cfg.slides.iter_mut() {
        *on = true;
    }
    let past = Instant::now() - Duration::from_secs(10);
    App {
        hwnd: 0,
        wx: 0,
        wy: 0,
        mon: RECT::default(),
        mem_dc: 0,
        bmp: 0,
        bits: 0,
        scale: s,
        scale_forced: Some(s),
        gfx: Some(scale::new_gfx(s)),
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
        last_wheel: past,
        t: 0.0,
        last: Instant::now(),
        interval: 0,
        tick_n: 0,
        last_sec: 0,
        last_min: 0,
        hover: false,
        busy: false,
        minimal: false,
        toast: None,
        bt_shared: Arc::new(Mutex::new(Vec::new())),
        notifs: Vec::new(),
        notif_access: notif::Access::Unknown,
        notif_slot: Arc::new(Mutex::new(Vec::new())),
        notif_on: false,
        notif_started: false,
        notif_scroll: 0,
        nui: notifview::NotifUi::new(),
        bell_t: past,
        toast_q: Vec::new(),
        toast_end: past,
        ac_prev: None,
        low_armed: true,
        demo_toast: None,
        demo_slide: None,
        display_off: false,
        menu_open: false,
        peek_until: None,
        reminding: false,
        splash: None,
        // jam idle sudah selesai fade-in (fase jam, lihat `idle_phase`)
        idle_base: 5.0,
        idle_mark: None,
        was_idle: false,
        eye: EyeAnim::new(),
        eye_paths: build_eye_paths(),
        cur_phase: IdlePhase::Clock,
        shared: Arc::new(Mutex::new(Media::default())),
        art_shared: Arc::new(Mutex::new(None)),
        lyr_shared: Arc::new(Mutex::new(None)),
        lyrics: lyrics::Lyrics::None,
        lyric_idx: None,
        lyric_w: 0.0,
        lyric_mode: false,
        lyric_mode_t0: past,
        lyric_scroll: 0.0,
        lyric_sv: 0.0,
        lyric_t: Instant::now(),
        pos_at: Instant::now(),
        media: Media::default(),
        art: None,
        prev_art: None,
        art_t0: past,
        tint: music::Tint::default(),
        icon: None,
        art_key: String::new(),
        title_t0: 0.0,
        todos: vec![(true, "Skala DPI layar".into()), (false, "Mode Pomodoro".into()), (false, "Riwayat clipboard".into())],
        todo_scroll: 0,
        edit_mode: false,
        editing: false,
        edit_idx: None,
        input: String::new(),
        caret_on: true,
        edit_start: Instant::now(),
        prev_fg: 0,
        shelf: Vec::new(),
        shelf_flash: None,
        drop_pulse: None,
        note: String::new(),
        note_mode: false,
        nv: NoteView::default(),
        water: Water::parse("", &clock::today()),
        wui: WaterUi::new(),
        tm: timer::TimerState::new(timer::TMode::Countdown, 300),
        tx,
        priv_mic: Vec::new(),
        priv_cam: Vec::new(),
        priv_t: past,
        vol_t: past,
        bright_seen: false,
        vol_state: (0, false),
        order: slides::visible_order(&cfg.slides, false),
        settings_open: false,
        settings_prev: None,
        sui: settingsview::SettingsUi::new(),
        sys_ui: sysview::SysUi::new(),
        weather_idx: 0,
        cfg,
    }
}

/// Buka slide `id` di notch besar.
fn open_slide(a: &mut App, id: SlideId) {
    a.hover = true;
    a.slide = a.order.iter().position(|&s| s == id).expect("slide aktif") as i32;
}

/// Toast yang animasi munculnya sudah selesai.
fn show_toast(a: &mut App, kind: ToastKind) {
    a.toast = Some(Toast { kind, t0: Instant::now() - Duration::from_millis(1500), dur: 60.0 });
}

/// Gambar satu keadaan pada skala `s`, cek ada isinya, dan simpan sebagai `<name>@<persen>.png` bila diminta.
fn shot(name: &str, s: f32, setup: impl FnOnce(&mut App)) {
    let mut a = test_app(s);
    setup(&mut a);
    let (w, h) = anim::target(&a);
    (a.w, a.h) = (w, h);
    a.sp = a.slide as f32;
    render::draw(&mut a);
    let g = a.gfx.as_ref().expect("gfx");
    let opaque = g.pm.data().chunks_exact(4).filter(|p| p[3] == 255).count();
    assert!(opaque > (w * h * s * s) as usize / 2, "{name}: notch hampir kosong ({opaque} piksel)");

    let Some(dir) = std::env::var_os("SNAPSHOT_DIR") else { return };
    let mut out = Pixmap::new(g.pm.width(), g.pm.height()).expect("ukuran kanvas");
    out.fill(Color::from_rgba8(118, 124, 134, 255));
    out.draw_pixmap(0, 0, g.pm.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
    std::fs::create_dir_all(&dir).expect("buat folder snapshot");
    let file = std::path::Path::new(&dir).join(format!("{name}@{}.png", (s * 100.0).round() as u32));
    out.save_png(&file).expect("simpan png");
}

const SCALES: [f32; 2] = [1.0, 1.5];

#[test]
fn snap_idle_clock() {
    for s in SCALES {
        shot("idle-clock", s, |_| {});
    }
}

#[test]
fn snap_idle_eye() {
    shot("idle-eye", 1.0, |a| {
        a.cfg.idle_display = 2;
        // jalankan animasi 3 detik: mata sudah selesai muncul dan terbuka
        for _ in 0..180 {
            a.eye.update(1.0 / 60.0);
        }
        a.t = 3.0;
    });
}

#[test]
fn snap_playing() {
    shot("playing", 1.0, |a| {
        a.media = Media { title: "Almost Easy".into(), artist: "Avenged Sevenfold".into(), playing: true, pos: 60, dur: 240, pos_ms: 60_000, source: "Demo".into(), sources: 1 };
    });
}

#[test]
fn snap_timer_slide() {
    for s in SCALES {
        shot("timer-countdown", s, |a| open_slide(a, SlideId::Timer));
    }
    shot("timer-stopwatch", 1.0, |a| {
        a.tm.set_mode(timer::TMode::Stopwatch);
        a.tm.start(Instant::now() - Duration::from_millis(83_400));
        open_slide(a, SlideId::Timer);
    });
}

#[test]
fn snap_timer_chip_idle() {
    for s in SCALES {
        shot("timer-chip", s, |a| a.tm.start(Instant::now() - Duration::from_secs(61)));
    }
}

#[test]
fn snap_timer_toast() {
    shot("toast-timer", 1.0, |a| show_toast(a, ToastKind::Timer { label: "5 min timer".into() }));
}

#[test]
fn snap_slide_todo() {
    shot("slide-todo", 1.0, |a| open_slide(a, SlideId::Todo));
}

#[test]
fn snap_slide_water() {
    shot("slide-water", 1.0, |a| open_slide(a, SlideId::Water));
}

#[test]
fn snap_slide_system() {
    shot("slide-system", 1.0, |a| open_slide(a, SlideId::System));
}

#[test]
fn snap_slide_music_empty() {
    shot("slide-music-empty", 1.0, |a| open_slide(a, SlideId::Music));
}

/// Buka Settings pada tab `tab`. Knob toggle disetel ke keadaan sebenarnya selagi slide belum terlihat,
/// sama seperti saat aplikasi membuka Settings.
fn open_settings_tab(a: &mut App, tab: u8) {
    a.open_settings();
    a.sui.tab = tab;
    a.settings_ui_step(0.0);
    a.hover = true;
}

#[test]
fn snap_settings() {
    for tab in 0..4u8 {
        shot(&format!("settings-tab{tab}"), 1.0, |a| open_settings_tab(a, tab));
    }
}
