//! UI snapshots for visual review without running the exe. Each test builds one `App` state (no window),
//! draws it with `render::draw` into an in-memory canvas, and checks that the notch was actually drawn.
//! When the `SNAPSHOT_DIR` env var is set (CI does this), the result is saved as a PNG over a grey background
//! so the black notch edges stay visible; CI uploads them as the `ui-snapshots` artifact.
//!
//! Add a new state with one `#[test]` function that calls `shot`.

use super::*;
use tiny_skia::PixmapPaint;

/// An App with no window, user data or background threads: every slide enabled, splash already over.
/// Weather never shows (tests have no weather data), so the idle notch is always in its "no weather" state.
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
        prayer: None,
        prayer_fired: None,
        splash: None,
        // the idle clock has finished fading in (clock phase, see `idle_phase`)
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
        todos: vec![(true, "Display DPI scaling".into()), (false, "Pomodoro mode".into()), (false, "Clipboard history".into())],
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
        clips: clipboard::History::default(),
        clip_scroll: 0,
        clip_flash: None,
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

/// Open slide `id` in the expanded notch.
fn open_slide(a: &mut App, id: SlideId) {
    a.hover = true;
    a.slide = a.order.iter().position(|&s| s == id).expect("slide is enabled") as i32;
}

/// A toast whose pop-in animation has finished.
fn show_toast(a: &mut App, kind: ToastKind) {
    a.toast = Some(Toast { kind, t0: Instant::now() - Duration::from_millis(1500), dur: 60.0 });
}

/// Draw one state at scale `s`, check it has content, and save it as `<name>@<percent>.png` when asked.
fn shot(name: &str, s: f32, setup: impl FnOnce(&mut App)) {
    let mut a = test_app(s);
    setup(&mut a);
    let (w, h) = anim::target(&a);
    (a.w, a.h) = (w, h);
    a.sp = a.slide as f32;
    render::draw(&mut a);
    let g = a.gfx.as_ref().expect("gfx");
    let opaque = g.pm.data().chunks_exact(4).filter(|p| p[3] == 255).count();
    assert!(opaque > (w * h * s * s) as usize / 2, "{name}: notch is almost empty ({opaque} pixels)");

    let Some(dir) = std::env::var_os("SNAPSHOT_DIR") else { return };
    let mut out = Pixmap::new(g.pm.width(), g.pm.height()).expect("canvas size");
    out.fill(Color::from_rgba8(118, 124, 134, 255));
    out.draw_pixmap(0, 0, g.pm.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
    std::fs::create_dir_all(&dir).expect("create snapshot folder");
    let file = std::path::Path::new(&dir).join(format!("{name}@{}.png", (s * 100.0).round() as u32));
    out.save_png(&file).expect("save png");
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
        // run 3 seconds of animation: the eyes have appeared and are open
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
    shot("toast-timer", 1.0, |a| show_toast(a, ToastKind::Timer { title: "Time's up".into(), label: "5 min timer".into() }));
}

/// A Pomodoro that has finished `done` focus sessions and is in phase `phase`, running for `secs` seconds.
fn pomo(a: &mut App, done: u32, phase: timer::Phase, secs: u64) {
    a.tm.set_mode(timer::TMode::Pomodoro);
    a.tm.done = done;
    a.tm.phase = phase;
    a.tm.start(Instant::now() - Duration::from_secs(secs));
}

#[test]
fn snap_pomodoro() {
    for s in SCALES {
        shot("pomo-focus", s, |a| {
            pomo(a, 2, timer::Phase::Focus, 9 * 60 + 12);
            open_slide(a, SlideId::Timer);
        });
        shot("pomo-chip-focus", s, |a| pomo(a, 2, timer::Phase::Focus, 9 * 60 + 12));
    }
    shot("pomo-short-break", 1.0, |a| {
        pomo(a, 3, timer::Phase::Short, 70);
        open_slide(a, SlideId::Timer);
    });
    shot("pomo-long-break", 1.0, |a| {
        pomo(a, 4, timer::Phase::Long, 70);
        open_slide(a, SlideId::Timer);
    });
    shot("pomo-paused-fresh", 1.0, |a| {
        a.tm.set_mode(timer::TMode::Pomodoro);
        open_slide(a, SlideId::Timer);
    });
    shot("pomo-chip-break", 1.0, |a| pomo(a, 1, timer::Phase::Short, 70));
    shot("toast-pomo-focus-done", 1.0, |a| {
        show_toast(a, ToastKind::Timer { title: "Focus #4 done".into(), label: "Long break \u{b7} 15 min".into() })
    });
    shot("toast-pomo-break-over", 1.0, |a| show_toast(a, ToastKind::Timer { title: "Break over".into(), label: "Focus \u{b7} 25 min".into() }));
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

/// Open Settings on tab `tab`. Toggle knobs are snapped to their real state while the slide is still hidden,
/// just like when the app opens Settings.
fn open_settings_tab(a: &mut App, tab: u8) {
    a.open_settings();
    a.sui.tab = tab;
    a.settings_ui_step(0.0);
    a.hover = true;
}

#[test]
fn snap_settings() {
    for tab in 0..5u8 {
        shot(&format!("settings-tab{tab}"), 1.0, |a| open_settings_tab(a, tab));
    }
}

/// Settings › Extras with a long city name: the stepper grows to the left instead of cutting the name.
#[test]
fn snap_settings_long_city() {
    for (name, city) in [("settings-city-short", "Jakarta"), ("settings-city-long", "Balikpapan"), ("settings-city-longest", "Bandar Lampung")] {
        let i = weather::CITIES.iter().position(|c| c.0 == city).unwrap();
        shot(name, 1.0, |a| {
            a.cfg.weather_city = i;
            open_settings_tab(a, 4);
        });
    }
}

/// Every stepper value is shown in full (no ellipsis) and never runs into its row's label: all cities, the
/// lyric timing extremes, every notch size and idle display.
#[test]
fn stepper_values_fit_without_cutting() {
    let _lang = i18n::restore_on_drop();
    let mut a = test_app(1.0);
    let mut g = Gfx::new(1.0);
    let mut check = |a: &App, tab: u8| {
        for r in a.settings_rows(tab) {
            let settingsview::RowKind::Stepper(v) = &r.kind else { continue };
            let f = settingsview::stepper_fit(&mut g, &r, v);
            assert_eq!(&f.value, v, "{:?}: value was cut", r.label);
            let label_end = 12.0 + g.width_ex(r.label, 12.0, true).max(g.width(r.sub, 11.0));
            assert!(f.minus_x - 9.0 >= label_end + 4.0 - 0.01, "{:?} = {v:?}: the - button overlaps the label", r.label);
            assert!(f.size >= 9.0, "{:?} = {v:?}: text too small", r.label);
        }
    };
    for i in 0..weather::CITIES.len() {
        a.cfg.weather_city = i;
        check(&a, 4);
    }
    for lead in [settings::LEAD_MIN, settings::LEAD_MAX] {
        a.cfg.lyric_lead_ms = lead;
        check(&a, 1);
    }
    for lang in 0..i18n::LANGS.len() {
        i18n::set(lang);
        for n in 0..scale::SIZES.len() {
            a.cfg.notch_size = n;
            a.cfg.idle_display = n % settings::IDLE_DISPLAY_LABELS.len();
            check(&a, 1);
        }
        for m in 0..monitors::mode_count() {
            a.cfg.monitor = m;
            check(&a, 1);
        }
        for ac in 0..accent::ACCENTS.len() {
            a.cfg.accent = ac;
            check(&a, 4);
        }
        for l in 0..i18n::LANGS.len() {
            a.cfg.lang = l;
            check(&a, 2);
        }
    }
}

/// Short values keep the old stepper; "Bandar Lampung" moves the "-" button left and clicking it still steps back.
#[test]
fn moved_minus_button_is_still_clickable() {
    let mut a = test_app(1.0);
    let row = |a: &App| a.settings_rows(4).into_iter().position(|r| r.label == "City").unwrap();
    let i = row(&a);
    let minus_x = |a: &mut App| {
        let r = a.settings_rows(4).swap_remove(i);
        let settingsview::RowKind::Stepper(v) = &r.kind else { unreachable!() };
        settingsview::stepper_fit(a.gfx.as_mut().unwrap(), &r, v).minus_x
    };
    a.cfg.weather_city = 0; // Jakarta
    assert_eq!(minus_x(&mut a), settingsview::STEP_PLUS_X - settingsview::STEP_MIN_SPAN);

    let long = weather::CITIES.iter().position(|c| c.0 == "Bandar Lampung").unwrap();
    a.cfg.weather_city = long;
    let mx = minus_x(&mut a);
    assert!(mx < settingsview::STEP_PLUS_X - settingsview::STEP_MIN_SPAN, "the - button should move left");
    let (cx, cy) = settingsview::cell_pos(i);
    a.settings_click_rows(4, cx + mx, cy + 17.0);
    assert_eq!(a.cfg.weather_city, long - 1, "clicking the moved - button steps back");
    a.cfg.weather_city = long;
    a.settings_click_rows(4, cx + settingsview::STEP_PLUS_X, cy + 17.0);
    assert_eq!(a.cfg.weather_city, (long + 1) % weather::CITIES.len(), "+ still steps forward");
}

/// Every slide name fits in its Settings > Slides cell instead of running into the chevrons.
#[test]
fn slide_names_fit_settings_cells() {
    let _lang = i18n::restore_on_drop();
    let mut g = Gfx::new(1.0);
    for lang in 0..i18n::LANGS.len() {
        i18n::set(lang);
        for id in SlideId::USER {
            let w = g.width_ex(id.short_label(), 11.0, true);
            assert!(w <= settingsview::SLIDE_NAME_W, "{:?} is {w:.1} px wide", id.short_label());
        }
    }
}

/// Every Settings row's label and sub-label fit to the left of its control (no ellipsis needed).
#[test]
fn settings_text_fits_cells() {
    let _lang = i18n::restore_on_drop();
    let a = test_app(1.0);
    let mut g = Gfx::new(1.0);
    for lang in 0..i18n::LANGS.len() {
        i18n::set(lang);
        for tab in 1..=4u8 {
            for r in a.settings_rows(tab) {
                let w = settingsview::text_w(&r.kind);
                let (lw, sw) = (g.width_ex(r.label, 12.0, true), g.width(r.sub, 11.0));
                assert!(lw <= w, "tab {tab}: label {:?} {lw:.1} px > {w} px", r.label);
                assert!(sw <= w, "tab {tab}: sub {:?} {sw:.1} px > {w} px", r.sub);
            }
        }
    }
}

/// The Settings tabs sit right of the title and inside the notch, in every language.
#[test]
fn settings_tabs_fit_next_to_the_title() {
    let _lang = i18n::restore_on_drop();
    let mut g = Gfx::new(1.0);
    for lang in 0..i18n::LANGS.len() {
        i18n::set(lang);
        let tabs = settingsview::tab_rects(&mut g);
        let title_end = LX + g.width_ex(i18n::tr("Settings"), 15.0, true);
        assert!(tabs[0].0 >= title_end + 8.0, "lang {lang}: tabs start at {:.1}, title ends at {title_end:.1}", tabs[0].0);
        let (x, w) = tabs[tabs.len() - 1];
        assert!(x + w <= RX + 0.01, "lang {lang}: tabs end at {:.1} > {RX}", x + w);
    }
}

/// The Language row in Settings > Features switches the whole UI right away, and back.
#[test]
fn language_row_switches_the_ui() {
    let _lang = i18n::restore_on_drop();
    let mut a = test_app(1.0);
    let i = a.settings_rows(2).iter().position(|r| r.label == "Language").expect("Language row in Features");
    let (cx, cy) = settingsview::cell_pos(i);
    a.settings_click_rows(2, cx + settingsview::STEP_PLUS_X, cy + 17.0);
    assert_eq!(a.cfg.lang, i18n::ID);
    assert_eq!(i18n::current(), i18n::ID, "applied right away");
    assert_eq!(a.settings_rows(2)[i].label, "Bahasa");
    assert_eq!(a.settings_rows(4)[1].label, "Kota");
    assert_eq!(SlideId::Todo.label(), "Tugas");
    assert_eq!(clock::day_short(1), "Sen");
    assert_eq!(clock::month(4), "Mei");
    assert_eq!(timer::fmt_preset(90 * 60), "1 j 30 mnt");
    a.settings_click_rows(2, cx + settingsview::STEP_PLUS_X, cy + 17.0);
    assert_eq!(a.cfg.lang, i18n::EN);
    assert_eq!(a.settings_rows(4)[1].label, "City");
    assert_eq!(clock::day_short(1), "Mon");
    assert_eq!(timer::fmt_preset(90 * 60), "1 h 30 min");
}

/// The main screens in Indonesian, for review.
#[test]
fn snap_indonesian() {
    let _lang = i18n::restore_on_drop();
    let id = |a: &mut App| {
        a.cfg.lang = i18n::ID;
        i18n::set(i18n::ID);
    };
    for tab in 0..5u8 {
        shot(&format!("id-settings-tab{tab}"), 1.0, |a| {
            id(a);
            open_settings_tab(a, tab);
        });
    }
    for (name, slide) in [("id-todo", SlideId::Todo), ("id-water", SlideId::Water), ("id-system", SlideId::System), ("id-notifs", SlideId::Notifications), ("id-music", SlideId::Music), ("id-clipboard", SlideId::Clipboard), ("id-ledge", SlideId::Ledge), ("id-scrap", SlideId::Scrap)] {
        shot(name, 1.0, |a| {
            id(a);
            open_slide(a, slide);
        });
    }
    shot("id-timer", 1.0, |a| {
        id(a);
        a.tm.start(Instant::now() - Duration::from_secs(100));
        open_slide(a, SlideId::Timer);
    });
    shot("id-pomodoro", 1.0, |a| {
        id(a);
        pomo(a, 2, timer::Phase::Focus, 9 * 60 + 12);
        open_slide(a, SlideId::Timer);
    });
    shot("id-clipboard-full", 1.0, |a| {
        id(a);
        with_clips(a, &CLIPS);
        open_slide(a, SlideId::Clipboard);
    });
    shot("id-toast-prayer", 1.0, |a| {
        id(a);
        show_toast(a, ToastKind::Prayer { idx: 2, at: 15 * 60 + 12 });
    });
    shot("id-toast-battery", 1.0, |a| {
        id(a);
        show_toast(a, ToastKind::LowBattery { pct: 15 });
    });
    shot("id-toast-water", 1.0, |a| {
        id(a);
        show_toast(a, ToastKind::Water);
    });
    shot("id-toast-mic", 1.0, |a| {
        id(a);
        show_toast(a, ToastKind::Privacy { cam: false, app: "Zoom".into() });
    });
}

/// Fake prayer times: prayer `idx` is `mins` minutes from now, the others are outside the countdown window.
fn prayer_in(a: &mut App, idx: usize, mins: u16) {
    let st = now_tm();
    let now = st.wHour * 60 + st.wMinute;
    let mut t = [0u16; 5];
    for (i, v) in t.iter_mut().enumerate() {
        *v = if i < idx { 0 } else if i == idx { now + mins } else { now + mins + 600 };
    }
    a.prayer = Some((0, t));
}

#[test]
fn snap_prayer() {
    for s in SCALES {
        shot("prayer-idle", s, |a| prayer_in(a, 2, 12));
    }
    shot("prayer-playing", 1.0, |a| {
        prayer_in(a, 3, 5);
        a.media = Media { title: "Almost Easy".into(), artist: "Avenged Sevenfold".into(), playing: true, pos: 60, dur: 240, pos_ms: 60_000, source: "Demo".into(), sources: 1 };
    });
    shot("prayer-with-timer", 1.0, |a| {
        prayer_in(a, 4, 9);
        a.tm.start(Instant::now() - Duration::from_secs(61));
    });
    shot("toast-prayer", 1.0, |a| show_toast(a, ToastKind::Prayer { idx: 2, at: 15 * 60 + 12 }));
    // the "Prayer times" row in Settings > Extras: see settings-tab4
}

#[test]
fn snap_accents() {
    for (i, ac) in accent::ACCENTS.iter().enumerate() {
        let n = ac.name.to_lowercase();
        shot(&format!("accent-{n}-settings"), 1.0, |a| {
            a.cfg.accent = i;
            open_settings_tab(a, 4);
        });
        shot(&format!("accent-{n}-todo"), 1.0, |a| {
            a.cfg.accent = i;
            open_slide(a, SlideId::Todo);
        });
        shot(&format!("accent-{n}-timer"), 1.0, |a| {
            a.cfg.accent = i;
            a.tm.start(Instant::now() - Duration::from_secs(100));
            open_slide(a, SlideId::Timer);
        });
    }
}

/// The accent setting really changes the render, and every accent produces a different image.
#[test]
fn accent_changes_the_rendered_slide() {
    let render = |i: usize| {
        let mut a = test_app(1.0);
        a.cfg.accent = i;
        open_slide(&mut a, SlideId::Todo);
        (a.w, a.h) = anim::target(&a);
        a.sp = a.slide as f32;
        render::draw(&mut a);
        a.gfx.as_ref().expect("gfx").pm.data().to_vec()
    };
    let shots: Vec<_> = (0..accent::ACCENTS.len()).map(render).collect();
    for i in 0..shots.len() {
        for j in i + 1..shots.len() {
            assert_ne!(shots[i], shots[j], "accents {i} and {j} render the same");
        }
    }
}

#[test]
fn snap_slide_hotkeys() {
    // Settings > Slides with each slide's shortcut, and Features with the "Slide hotkeys" toggle
    shot("hotkeys-settings-slides", 1.0, |a| open_settings_tab(a, 0));
    shot("hotkeys-settings-features", 1.0, |a| open_settings_tab(a, 2));
    shot("hotkeys-settings-slides-off", 1.0, |a| {
        a.cfg.slide_hotkeys = false;
        open_settings_tab(a, 0);
    });
    // Ctrl+Alt+3 from the idle notch
    shot("hotkeys-open-third", 1.0, |a| {
        assert!(a.slide_hotkey(2));
    });
}

/// Ctrl+Alt+N opens the Nth slide, the same key closes it again, and a missing slide is ignored.
#[test]
fn slide_hotkey_opens_and_closes() {
    let mut a = test_app(1.0);
    assert!(!a.expanded());
    assert!(a.slide_hotkey(2));
    assert!(a.expanded());
    assert_eq!(a.cur_id(), a.order[2]);
    assert!(a.slide_hotkey(0), "another hotkey switches slides");
    assert_eq!(a.cur(), 0);
    assert!(a.expanded());
    assert!(a.slide_hotkey(0), "same hotkey again closes");
    assert!(!a.expanded());
    assert!(!a.slide_hotkey(a.order.len()), "no slide at that position");
}

/// Fill the clipboard history, oldest first: (text, seconds ago).
fn with_clips(a: &mut App, clips: &[(&str, u64)]) {
    let now = Instant::now();
    for (s, ago) in clips {
        a.clips.push(s, now - Duration::from_secs(*ago));
    }
}

const CLIPS: [(&str, u64); 4] = [
    ("https://github.com/dirgaastya/hesewake-notch/issues/13", 3 * 86400),
    ("fn main() {\r\n    println!(\"hello\");\r\n}", 2 * 3600),
    ("Meeting notes: ship 0.6 after the clipboard slide lands, then look at the ICS calendar idea and the update check", 600),
    ("rhezaardiansyah", 20),
];

/// Middle of visible row `vi` on the Clipboard slide.
fn clip_row_y(vi: usize) -> f32 {
    clipboard::ROW_Y0 + vi as f32 * clipboard::ROW_P + clipboard::ROW_H / 2.0
}

fn set_slide(a: &mut App, id: SlideId, on: bool) {
    a.cfg.slides.iter_mut().find(|(s, _)| *s == id).expect("slide in config").1 = on;
}

#[test]
fn snap_clipboard() {
    shot("clipboard-empty", 1.0, |a| open_slide(a, SlideId::Clipboard));
    for s in SCALES {
        shot("clipboard", s, |a| {
            with_clips(a, &CLIPS);
            open_slide(a, SlideId::Clipboard);
        });
    }
    shot("clipboard-copied", 1.0, |a| {
        with_clips(a, &CLIPS);
        open_slide(a, SlideId::Clipboard);
        let hit = a.clip_hit(LX + 60.0, clip_row_y(2)).expect("third row");
        a.clip_apply(hit);
    });
    shot("clipboard-full-scrolled", 1.0, |a| {
        for i in 0..clipboard::MAX_ITEMS {
            a.clips.push(&format!("Copied text number {}", i + 1), Instant::now() - Duration::from_secs(60 * (10 - i as u64)));
        }
        open_slide(a, SlideId::Clipboard);
        a.clip_scroll = clipboard::MAX_ITEMS - 3;
    });
}

/// Clicking an entry gives its full text back for the clipboard and moves it to the top; the clipboard update that
/// follows does not add it twice.
#[test]
fn clipboard_click_copies_back_and_moves_to_top() {
    let mut a = test_app(1.0);
    with_clips(&mut a, &CLIPS);
    open_slide(&mut a, SlideId::Clipboard);
    let hit = a.clip_hit(LX + 60.0, clip_row_y(2));
    assert_eq!(hit, Some(clipboard::ClipHit::Copy(2)));
    assert_eq!(a.clip_apply(hit.unwrap()).as_deref(), Some(CLIPS[1].0), "full text, newlines kept");
    assert_eq!(a.clips.items()[0].text, CLIPS[1].0, "moved to the top");
    assert!(a.clip_flash.is_some(), "the row shows \"Copied\"");
    assert!(!a.on_clipboard(Some(CLIPS[1].0.into()), Instant::now()), "our own copy is not added again");
    assert_eq!(a.clips.len(), CLIPS.len());
    assert_eq!(a.clip_hit(LX + 60.0, EXP_H - 10.0), None, "below the rows");
}

#[test]
fn clipboard_remove_and_clear_buttons() {
    let mut a = test_app(1.0);
    with_clips(&mut a, &CLIPS);
    open_slide(&mut a, SlideId::Clipboard);
    let x_btn = LX + clipboard::row_w(a.clips.len()) - 15.0;
    assert_eq!(a.clip_hit(x_btn, clip_row_y(0)), Some(clipboard::ClipHit::Remove(0)));
    assert_eq!(a.clip_apply(clipboard::ClipHit::Remove(0)), None, "nothing to copy");
    assert_eq!(a.clips.items()[0].text, CLIPS[2].0);
    let (cx, cy, cw, ch) = clipboard::CLEAR_BTN;
    assert_eq!(a.clip_hit(cx + cw / 2.0, cy + ch / 2.0), Some(clipboard::ClipHit::Clear));
    a.clip_apply(clipboard::ClipHit::Clear);
    assert!(a.clips.is_empty());
    assert_eq!(a.clip_hit(cx + cw / 2.0, cy + ch / 2.0), None, "no Clear button on an empty list");
}

#[test]
fn clipboard_scrolls_with_the_wheel() {
    let mut a = test_app(1.0);
    for i in 0..5 {
        a.clips.push(&format!("clip {i}"), Instant::now());
    }
    open_slide(&mut a, SlideId::Clipboard);
    assert!(a.clip_wheel(true));
    assert_eq!(a.clip_scroll, 1);
    assert_eq!(a.clip_hit(LX + 60.0, clip_row_y(0)), Some(clipboard::ClipHit::Copy(1)), "rows follow the scroll");
    for _ in 0..5 {
        a.clip_wheel(true);
    }
    assert_eq!(a.clip_scroll, 2, "stops at the last full page");
    assert!(!a.clip_wheel(true));
    assert!(a.clip_wheel(false));
    assert_eq!(a.clip_scroll, 1);
    a.clip_apply(clipboard::ClipHit::Clear);
    assert_eq!(a.clip_scroll, 0);
}

/// Nothing is recorded while the Clipboard slide is off, and turning it off forgets what was kept.
#[test]
fn clipboard_is_only_recorded_while_the_slide_is_on() {
    let mut a = test_app(1.0);
    set_slide(&mut a, SlideId::Clipboard, false);
    assert!(!a.on_clipboard(Some("secret".into()), Instant::now()));
    assert!(a.clips.is_empty());
    set_slide(&mut a, SlideId::Clipboard, true);
    assert!(a.on_clipboard(Some("hello".into()), Instant::now()));
    assert!(!a.on_clipboard(None, Instant::now()), "no text (or not allowed): nothing changes");
    assert_eq!(a.clips.len(), 1);
    set_slide(&mut a, SlideId::Clipboard, false);
    a.on_clipboard(Some("another".into()), Instant::now());
    assert!(a.clips.is_empty(), "turned off: history forgotten");
}
