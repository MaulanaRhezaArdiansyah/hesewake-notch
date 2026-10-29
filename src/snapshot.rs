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
        cal_link: None,
        cal_status: calendar::CalStatus::Off,
        cal: Vec::new(),
        cal_fired: Vec::new(),
        cal_paste_err: None,
        cal_confirm: None,
        agenda_scroll: 0,
        update: None,
        update_last: None,
        update_busy: false,
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
        outs: output::Outputs::default(),
        out_open: false,
        out_scroll: 0,
        out_t: past,
        out_acc: 0,
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
    for tab in 0..4u8 {
        shot(&format!("settings-tab{tab}"), 1.0, |a| open_settings_tab(a, tab));
    }
}

/// Settings › Widgets with a long city name: the stepper grows to the left instead of cutting the name.
#[test]
fn snap_settings_long_city() {
    for (name, city) in [("settings-city-short", "Jakarta"), ("settings-city-long", "Balikpapan"), ("settings-city-longest", "Bandar Lampung")] {
        let i = weather::CITIES.iter().position(|c| c.0 == city).unwrap();
        shot(name, 1.0, |a| {
            a.cfg.weather_city = i;
            open_settings_tab(a, 3);
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

// ---------------------------------------------------------------- calendar

/// Jakarta time, UTC+7.
const WIB: i32 = 7 * 3600;

fn event(title: &str, start: i64, mins: i64, link: Option<&str>) -> ics::Event {
    ics::Event { summary: title.into(), start, end: start + mins * 60, all_day: false, link: link.map(str::to_string) }
}

fn now_s() -> i64 {
    notif::now_unix() as i64
}

#[test]
fn meeting_countdown_in_the_last_ten_minutes() {
    let mut a = test_app(1.0);
    let now = now_s();
    a.cal = vec![
        ics::Event { all_day: true, ..event("Holiday", now + 60, 24 * 60, None) },
        event("Standup", now + 8 * 60 + 30, 15, Some("https://meet.google.com/abc")),
        event("Review", now + 9 * 60, 60, None),
    ];
    let (e, m) = a.meeting_countdown(now).expect("countdown");
    assert_eq!((e.summary.as_str(), m), ("Standup", 9), "the soonest timed meeting, minutes rounded up; all-day ignored");
    a.cal = vec![event("Later", now + 11 * 60, 30, None)];
    assert!(a.meeting_countdown(now).is_none(), "more than 10 minutes away");
    a.cal = vec![event("Running", now - 60, 30, None)];
    assert!(a.meeting_countdown(now).is_none(), "already started");
}

#[test]
fn meeting_toast_once_when_it_starts() {
    let mut a = test_app(1.0);
    let start = now_s() - 20;
    a.cal = vec![event("Standup", start, 15, Some("https://meet.google.com/abc")), ics::Event { all_day: true, ..event("Holiday", start, 1440, None) }];
    assert!(a.meeting_minute(start + 20));
    let Some(Toast { kind: ToastKind::Meeting { title, at, link }, .. }) = &a.toast else { panic!("meeting toast") };
    assert_eq!((title.as_str(), *at, link.as_deref()), ("Standup", start, Some("https://meet.google.com/abc")));
    a.toast = None;
    assert!(!a.meeting_minute(start + 60), "only once per meeting");
    assert!(a.toast.is_none() && a.toast_q.is_empty(), "no toast for the all-day event");
    assert!(!a.meeting_minute(start + 10 * 60), "long after the start: no toast");
}

#[test]
fn meeting_toast_join_opens_the_link() {
    let mut a = test_app(1.0);
    let link = "https://meet.google.com/abc-defg-hij";
    show_toast(&mut a, ToastKind::Meeting { title: "Standup".into(), at: now_s(), link: Some(link.into()) });
    let (w, h) = a.toast_size();
    let (bx, by, bw, bh) = toast::meeting_join_rect(w, h);
    assert!(a.toast_click(bx + bw / 2.0, by + bh / 2.0));
    assert_eq!(sys::opened(), [link]);
    assert!(a.toast.is_none(), "the toast closes");
}

#[test]
fn calendar_button_pastes_and_removes_the_link() {
    let mut a = test_app(1.0);
    a.cal_button(Some("not a link".into()));
    assert_eq!(a.cal_link, None);
    assert!(a.cal_paste_err.is_some(), "tells that the clipboard has no ICS link");
    a.cal_button(Some(" webcal://calendar.google.com/x/basic.ics ".into()));
    assert_eq!(a.cal_link.as_deref(), Some("https://calendar.google.com/x/basic.ics"));
    assert_eq!(a.cal_status, calendar::CalStatus::Loading);
    a.cal_update(calendar::CalStatus::Ready, vec![event("Standup", now_s() + 600, 15, None)]);
    assert_eq!(a.cal.len(), 1);
    a.cal_button(None);
    assert!(a.cal_link.is_some(), "first press only asks to confirm");
    a.cal_button(None);
    assert_eq!(a.cal_link, None, "second press removes it");
    assert_eq!(a.cal_status, calendar::CalStatus::Off);
    assert!(a.cal.is_empty());
}

#[test]
fn agenda_lists_what_is_left_today_and_join_opens_the_link() {
    let mut a = test_app(1.0);
    // 28 Oct 2026 10:00 WIB
    let now = ics::unix_utc(2026, 10, 28, 3, 0, 0);
    let at = |h: u32, m: u32| ics::unix_utc(2026, 10, 28, h, m, 0) - WIB as i64;
    a.cal = vec![
        event("Done already", at(8, 0), 30, None),
        event("Design review", at(9, 45), 30, Some("https://zoom.us/j/1")),
        event("Lunch", at(12, 0), 60, None),
        event("Sprint planning", at(14, 0), 60, Some("https://meet.google.com/xyz")),
        event("Tomorrow", at(24 + 9, 0), 30, None),
        ics::Event { all_day: true, ..event("Company holiday", at(0, 0), 24 * 60, None) },
    ];
    let names: Vec<_> = a.agenda(now, WIB).iter().map(|e| e.summary.clone()).collect();
    assert_eq!(names, ["Company holiday", "Design review", "Lunch", "Sprint planning"], "all-day first, ended and tomorrow left out");
    open_slide(&mut a, SlideId::Agenda);
    let (x, y, w, h) = calendar::join_rect(1);
    assert_eq!(a.agenda_hit(x + w / 2.0, y + h / 2.0, now, WIB), Some(calendar::AgendaHit::Join(1)));
    let (x, y, w, h) = calendar::join_rect(2);
    assert_eq!(a.agenda_hit(x + w / 2.0, y + h / 2.0, now, WIB), None, "Lunch has no link, so no button");
}

#[test]
fn countdown_shows_whichever_comes_first() {
    let mut a = test_app(1.0);
    a.cfg.prayer_on = true;
    prayer_in(&mut a, 2, 12);
    a.cal = vec![event("Standup", now_s() + 5 * 60 - 10, 15, None)];
    assert_eq!(a.upcoming(), Some(calendar::Upcoming::Meeting("Standup".into(), 5)));
    a.cal = vec![event("Later", now_s() + 14 * 60, 15, None)];
    assert_eq!(a.upcoming(), Some(calendar::Upcoming::Prayer(2, 12)), "the meeting is not within 10 minutes yet");
    a.cfg.prayer_on = false;
    assert_eq!(a.upcoming(), None);
}

/// Settings > Slides holds 10 slides in rows of 3: the wheel brings the 4th row into view, and its cells work.
#[test]
fn slides_grid_scrolls_to_the_tenth_slide() {
    let mut a = test_app(1.0);
    open_settings_tab(&mut a, 0);
    assert_eq!(a.cfg.slides.len(), 10);
    assert!(settingsview::slide_cells_visible(9, 0).is_none(), "the 10th slide starts below the grid");
    assert!(a.settings_slides_wheel(true));
    assert_eq!(a.sui.slides_scroll, 1);
    assert!(!a.settings_slides_wheel(true), "only one extra row");
    let (x, y) = settingsview::slide_cells_visible(9, 1).expect("visible after scrolling");
    assert!(settingsview::slide_cells_visible(0, 1).is_none(), "the first row scrolled out");
    let was = a.cfg.slides[9].1;
    a.settings_click(x + settingsview::SC_W - 10.0, y + 10.0);
    assert_eq!(a.cfg.slides[9].1, !was, "its toggle works");
    assert!(a.settings_slides_wheel(false));
    assert_eq!(a.sui.slides_scroll, 0);
}

/// A day with meetings: one running with a Meet link, then two more today.
fn with_meetings(a: &mut App) {
    let now = now_s();
    a.cal_link = Some("https://calendar.google.com/x/basic.ics".into());
    a.cal_status = calendar::CalStatus::Ready;
    a.cal = vec![
        event("Design review", now - 10 * 60, 45, Some("https://meet.google.com/abc-defg-hij")),
        event("Lunch with the team", now + 90 * 60, 60, None),
        event("Sprint planning for the clipboard and calendar work", now + 3 * 3600, 60, Some("https://zoom.us/j/1")),
    ];
}

#[test]
fn snap_calendar() {
    for s in SCALES {
        shot("meeting-idle", s, |a| a.cal = vec![event("Standup", now_s() + 8 * 60, 15, None)]);
    }
    shot("meeting-playing", 1.0, |a| {
        a.cal = vec![event("Standup", now_s() + 8 * 60, 15, None)];
        a.media = Media { title: "Almost Easy".into(), artist: "Avenged Sevenfold".into(), playing: true, pos: 60, dur: 240, pos_ms: 60_000, source: "Demo".into(), sources: 1 };
    });
    shot("toast-meeting", 1.0, |a| {
        show_toast(a, ToastKind::Meeting { title: "Daily Standup".into(), at: now_s(), link: Some("https://meet.google.com/abc".into()) })
    });
    shot("toast-meeting-nolink", 1.0, |a| show_toast(a, ToastKind::Meeting { title: "1:1 with Rina".into(), at: now_s(), link: None }));
    shot("agenda", 1.0, |a| {
        with_meetings(a);
        open_slide(a, SlideId::Agenda);
    });
    shot("agenda-no-link", 1.0, |a| open_slide(a, SlideId::Agenda));
    shot("agenda-loading", 1.0, |a| {
        a.cal_link = Some("https://x.com/a.ics".into());
        a.cal_status = calendar::CalStatus::Loading;
        open_slide(a, SlideId::Agenda);
    });
    shot("agenda-failed", 1.0, |a| {
        a.cal_link = Some("https://x.com/a.ics".into());
        a.cal_status = calendar::CalStatus::Failed;
        open_slide(a, SlideId::Agenda);
    });
    shot("agenda-done-for-today", 1.0, |a| {
        a.cal_link = Some("https://x.com/a.ics".into());
        a.cal_status = calendar::CalStatus::Ready;
        a.cal = vec![event("Standup", now_s() + 30 * 3600, 15, None)];
        open_slide(a, SlideId::Agenda);
    });
    shot("settings-calendar-off", 1.0, |a| open_settings_tab(a, 3));
    shot("settings-calendar-synced", 1.0, |a| {
        with_meetings(a);
        open_settings_tab(a, 3);
    });
    shot("settings-slides-scrolled", 1.0, |a| {
        open_settings_tab(a, 0);
        a.sui.slides_scroll = 1;
    });
    let id = |a: &mut App| {
        a.cfg.lang = i18n::ID;
        i18n::set(i18n::ID);
    };
    let _lang = i18n::restore_on_drop();
    shot("id-agenda", 1.0, |a| {
        id(a);
        with_meetings(a);
        open_slide(a, SlideId::Agenda);
    });
    shot("id-toast-meeting", 1.0, |a| {
        id(a);
        show_toast(a, ToastKind::Meeting { title: "Daily Standup".into(), at: now_s(), link: Some("https://meet.google.com/abc".into()) })
    });
    shot("id-settings-calendar", 1.0, |a| {
        id(a);
        with_meetings(a);
        open_settings_tab(a, 3);
    });
}

/// The Calendar row in Settings > Widgets: Paste reads the clipboard, and every state's text fits, in both languages.
#[test]
fn calendar_row_pastes_and_fits_in_every_state() {
    let _lang = i18n::restore_on_drop();
    let mut a = test_app(1.0);
    let row = |a: &App| a.settings_rows(3).into_iter().position(|r| r.label == i18n::tr("Calendar")).expect("Calendar row");
    let i = row(&a);
    let (cx, cy) = settingsview::cell_pos(i);
    calendar::TEST_CLIPBOARD.with(|c| *c.borrow_mut() = Some("https://calendar.google.com/x/basic.ics".into()));
    a.settings_click_rows(3, cx + settingsview::CELL_W - 30.0, cy + 17.0);
    assert_eq!(a.cal_link.as_deref(), Some("https://calendar.google.com/x/basic.ics"), "Paste took the link from the clipboard");

    let mut g = Gfx::new(1.0);
    for lang in 0..i18n::LANGS.len() {
        i18n::set(lang);
        let states: [(&str, fn(&mut App)); 6] = [
            ("no link", |a| a.cal_link = None),
            ("nothing on the clipboard", |a| {
                a.cal_link = None;
                a.cal_paste_err = Some(Instant::now());
            }),
            ("loading", |a| {
                a.cal_link = Some("https://x.com/a.ics".into());
                a.cal_status = calendar::CalStatus::Loading;
            }),
            ("ready", |a| {
                a.cal_status = calendar::CalStatus::Ready;
                a.cal = (0..12).map(|k| event("Meeting", now_s() + k * 60, 10, None)).collect();
            }),
            ("failed", |a| a.cal_status = calendar::CalStatus::Failed),
            ("confirm remove", |a| a.cal_confirm = Some(Instant::now())),
        ];
        for (name, set) in states {
            set(&mut a);
            let r = a.settings_rows(3).swap_remove(row(&a));
            let w = settingsview::text_w(&r.kind);
            assert!(g.width_ex(r.label, 12.0, true) <= w && g.width(r.sub, 11.0) <= w, "lang {lang}, {name}: {:?} / {:?}", r.label, r.sub);
            let settingsview::RowKind::Button(b) = &r.kind else { panic!("{name}: a button") };
            assert!(g.width_ex(b, 11.0, true) <= 46.0, "lang {lang}, {name}: button {b:?}");
        }
    }
}

/// Short values keep the old stepper; "Bandar Lampung" moves the "-" button left and clicking it still steps back.
#[test]
fn moved_minus_button_is_still_clickable() {
    let mut a = test_app(1.0);
    let row = |a: &App| a.settings_rows(3).into_iter().position(|r| r.label == "City").unwrap();
    let i = row(&a);
    let minus_x = |a: &mut App| {
        let r = a.settings_rows(3).swap_remove(i);
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
    a.settings_click_rows(3, cx + mx, cy + 17.0);
    assert_eq!(a.cfg.weather_city, long - 1, "clicking the moved - button steps back");
    a.cfg.weather_city = long;
    a.settings_click_rows(3, cx + settingsview::STEP_PLUS_X, cy + 17.0);
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
        for tab in 1..=3u8 {
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

/// The Language row in Settings > General switches the whole UI right away, and back.
#[test]
fn language_row_switches_the_ui() {
    let _lang = i18n::restore_on_drop();
    let mut a = test_app(1.0);
    let i = a.settings_rows(1).iter().position(|r| r.label == "Language").expect("Language row in General");
    let (cx, cy) = settingsview::cell_pos(i);
    a.settings_click_rows(1, cx + settingsview::STEP_PLUS_X, cy + 17.0);
    assert_eq!(a.cfg.lang, i18n::ID);
    assert_eq!(i18n::current(), i18n::ID, "applied right away");
    assert_eq!(a.settings_rows(1)[i].label, "Bahasa");
    assert_eq!(a.settings_rows(3)[1].label, "Kota");
    assert_eq!(SlideId::Todo.label(), "Tugas");
    assert_eq!(clock::day_short(1), "Sen");
    assert_eq!(clock::month(4), "Mei");
    assert_eq!(timer::fmt_preset(90 * 60), "1 j 30 mnt");
    a.settings_click_rows(1, cx + settingsview::STEP_PLUS_X, cy + 17.0);
    assert_eq!(a.cfg.lang, i18n::EN);
    assert_eq!(a.settings_rows(3)[1].label, "City");
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
    for tab in 0..4u8 {
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
    // the "Prayer times" row in Settings > Extras: see settings-tab3
}

#[test]
fn snap_accents() {
    for (i, ac) in accent::ACCENTS.iter().enumerate() {
        let n = ac.name.to_lowercase();
        shot(&format!("accent-{n}-settings"), 1.0, |a| {
            a.cfg.accent = i;
            open_settings_tab(a, 1);
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
    // Settings > Slides with each slide's shortcut, and General with the "Slide hotkeys" toggle
    shot("hotkeys-settings-slides", 1.0, |a| open_settings_tab(a, 0));
    shot("hotkeys-settings-general", 1.0, |a| open_settings_tab(a, 1));
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

/// Busy mode (fullscreen video) with music playing: right after a toast such as the volume HUD closes, the notch
/// shrinks back to the thin line. The cover and the equalizer must not be drawn into that line, because the timer
/// can stop before the toast's grace period ends and leave them on screen.
#[test]
fn thin_line_stays_empty_right_after_a_toast() {
    let mut a = test_app(1.0);
    a.media = Media { title: "Netflix".into(), artist: String::new(), playing: true, pos: 60, dur: 240, pos_ms: 60_000, source: "Demo".into(), sources: 1 };
    a.busy = true;
    a.toast_end = Instant::now();
    assert!(a.toast_hold(), "still inside the grace period after the toast");
    (a.w, a.h) = anim::target(&a);
    assert_eq!((a.w, a.h), THIN);
    render::draw(&mut a);
    let g = a.gfx.as_ref().expect("gfx");
    let lit = g.pm.data().chunks_exact(4).filter(|p| p[0] > 8 || p[1] > 8 || p[2] > 8).count();
    assert_eq!(lit, 0, "the thin line shows {lit} coloured pixels");
}

// ---------------------------------------------------------------- update check

fn release(v: &str) -> update::Release {
    update::Release { version: v.into(), url: format!("https://github.com/dirgaastya/hesewake-notch/releases/tag/v{v}") }
}

fn update_note(a: &App) -> Option<update::Note> {
    match &a.toast {
        Some(Toast { kind: ToastKind::Update(n), .. }) => Some(n.clone()),
        _ => None,
    }
}

#[test]
fn update_toast_once_per_version() {
    let mut a = test_app(1.0);
    a.on_update(false, Some(release("99.0.0")));
    assert_eq!(update_note(&a), Some(update::Note::Available(release("99.0.0"))));
    assert_eq!(a.cfg.update_seen, "99.0.0");
    assert_eq!(a.update, Some(release("99.0.0")), "kept for the menu");
    a.toast = None;
    a.on_update(false, Some(release("99.0.0")));
    assert_eq!(update_note(&a), None, "the same version is told once");
    a.on_update(false, Some(release("99.0.1")));
    assert_eq!(update_note(&a), Some(update::Note::Available(release("99.0.1"))), "a newer one again");
}

#[test]
fn update_quiet_when_current_or_offline() {
    let mut a = test_app(1.0);
    a.on_update(false, Some(release(env!("CARGO_PKG_VERSION"))));
    assert_eq!((update_note(&a), a.update.clone()), (None, None), "same version: nothing");
    a.on_update(false, Some(release("0.0.1")));
    assert_eq!(update_note(&a), None, "older: nothing");
    a.on_update(false, None);
    assert_eq!(update_note(&a), None, "offline: silent");
}

#[test]
fn update_check_from_the_menu_always_answers() {
    let mut a = test_app(1.0);
    a.on_update(true, Some(release(env!("CARGO_PKG_VERSION"))));
    assert_eq!(update_note(&a), Some(update::Note::UpToDate));
    a.toast = None;
    a.on_update(true, None);
    assert_eq!(update_note(&a), Some(update::Note::Failed));
    a.toast = None;
    a.cfg.update_seen = "99.0.0".into();
    a.on_update(true, Some(release("99.0.0")));
    assert_eq!(update_note(&a), Some(update::Note::Available(release("99.0.0"))), "asked: shown even if seen before");
}

#[test]
fn update_checks_daily_and_never_when_off() {
    let mut a = test_app(1.0);
    let t0 = Instant::now();
    a.cfg.update_check = false;
    assert!(!a.update_tick(t0), "off: no network call");
    a.cfg.update_check = true;
    assert!(a.update_tick(t0), "first check");
    assert!(!a.update_tick(t0 + Duration::from_secs(60)), "still running");
    a.on_update(false, None);
    assert!(!a.update_tick(t0 + Duration::from_secs(3600)), "not before 24 hours");
    assert!(a.update_tick(t0 + update::INTERVAL + Duration::from_secs(1)), "again after 24 hours");
    assert_eq!(update::fetches(), vec![false, false], "two automatic checks, no manual one");
}

#[test]
fn update_toast_download_opens_the_release() {
    let mut a = test_app(1.0);
    a.on_update(false, Some(release("99.0.0")));
    if let Some(t) = a.toast.as_mut() {
        t.t0 = Instant::now() - Duration::from_millis(1500);
    }
    let (bx, by, bw, bh) = update::button_rect(toast::TOAST_UPDATE.0, toast::TOAST_UPDATE.1);
    let _ = sys::opened();
    assert!(a.toast_click(bx + bw / 2.0, by + bh / 2.0));
    assert_eq!(sys::opened(), vec![release("99.0.0").url]);
    assert!(a.toast.is_none());
}

#[test]
fn update_row_in_alerts() {
    let mut a = test_app(1.0);
    let i = a.settings_rows(2).iter().position(|r| r.label == "Update notice").expect("Update notice row in Alerts");
    open_settings_tab(&mut a, 2);
    assert!(settingsview::row_cell_visible(i, 0).is_none(), "the 7th row starts below the grid");
    assert!(a.settings_rows_wheel(true));
    let (cx, cy) = settingsview::row_cell_visible(i, a.sui.rows_scroll).expect("visible after scrolling");
    assert!(a.cfg.update_check, "on by default");
    a.settings_click_rows(2, cx + settingsview::CELL_W - 30.0, cy + 17.0);
    assert!(!a.cfg.update_check);
}

#[test]
fn snap_update_toasts() {
    let _lang = i18n::restore_on_drop();
    for (lang, prefix) in [(i18n::EN, "update"), (i18n::ID, "id-update")] {
        i18n::set(lang);
        shot(&format!("{prefix}-toast-available"), 1.0, |a| show_toast(a, ToastKind::Update(update::Note::Available(release("0.9.0")))));
        shot(&format!("{prefix}-toast-up-to-date"), 1.0, |a| show_toast(a, ToastKind::Update(update::Note::UpToDate)));
        shot(&format!("{prefix}-toast-failed"), 1.0, |a| show_toast(a, ToastKind::Update(update::Note::Failed)));
        shot(&format!("{prefix}-settings-alerts"), 1.0, |a| open_settings_tab(a, 2));
    }
    shot("update-toast-available", 1.5, |a| show_toast(a, ToastKind::Update(update::Note::Available(release("0.9.0")))));
}

/// General has 9 rows: the first six show, the wheel brings the rest into view, and rows keep working there.
#[test]
fn rows_tab_scrolls_to_the_last_rows() {
    let mut a = test_app(1.0);
    open_settings_tab(&mut a, 1);
    assert_eq!(a.settings_rows(1).len(), 9);
    assert!(a.rows_scrollable());
    assert!(settingsview::row_cell_visible(8, 0).is_none(), "the 9th row starts below the grid");
    assert!(a.settings_rows_wheel(true));
    assert!(a.settings_rows_wheel(true));
    assert_eq!(a.sui.rows_scroll, 2, "9 rows = 5 rows of cells, 3 show");
    assert!(!a.settings_rows_wheel(true), "nothing left to scroll");
    assert!(settingsview::row_cell_visible(0, a.sui.rows_scroll).is_none(), "the first row scrolled out");
    assert!(settingsview::row_cell_visible(8, a.sui.rows_scroll).is_some(), "the last row is in view");
    let (x, y) = settingsview::row_cell_visible(6, a.sui.rows_scroll).expect("Minimal notch is visible");
    let was = a.minimal;
    a.settings_click_rows(1, x + settingsview::CELL_W - 30.0, y + 17.0);
    assert_eq!(a.minimal, !was, "its toggle works after scrolling");
    shot("settings-general-scrolled", 1.0, |a| {
        open_settings_tab(a, 1);
        a.settings_rows_wheel(true);
        a.settings_rows_wheel(true);
    });
    assert!(a.settings_rows_wheel(false));
    assert!(a.settings_rows_wheel(false));
    assert_eq!(a.sui.rows_scroll, 0);
    // changing tab starts at the top again
    a.settings_rows_wheel(true);
    a.settings_set_tab(2);
    assert_eq!(a.sui.rows_scroll, 0);
}

// ---------------------------------------------------------------- audio output switcher

fn three_outputs() {
    output::set_fake(
        &[
            ("spk", "Speakers (Realtek(R) Audio)", false),
            ("hp", "Headphones (WH-1000XM4 Stereo)", true),
            ("mon", "LG ULTRAGEAR (NVIDIA High Definition Audio)", false),
        ],
        Some("spk"),
    );
}

/// The Music slide with three playback devices, Speakers being the default.
fn music_with_outputs(a: &mut App) {
    three_outputs();
    a.out_refresh();
    open_slide(a, SlideId::Music);
}

fn default_output() -> Option<String> {
    output::with_fake(|f| f.cur.clone())
}

/// A click on the output button opens the list; a click on a row makes that device the default for every role.
#[test]
fn output_button_opens_the_list_and_picks_a_device() {
    let mut a = test_app(1.0);
    music_with_outputs(&mut a);
    assert!(!a.out_list_shown());
    assert!(a.out_click(output::BTN.0, output::BTN.1));
    assert!(a.out_list_shown());
    let (rx, ry) = output::row_pos(1);
    assert!(a.out_click(rx + 60.0, ry + 12.0));
    assert_eq!(default_output().as_deref(), Some("hp"));
    assert_eq!(output::with_fake(|f| f.calls.len()), 3, "Console, Multimedia and Communications");
    assert!(!a.out_list_shown(), "the list closes after a pick");
    assert_eq!(a.outs.cur.as_deref(), Some("hp"));
    assert!(a.out_t.elapsed() < Duration::from_secs(1), "the overlay shows the new device");
}

/// The list closes again on the button, on a click outside it (which does nothing else) and on a slide change.
#[test]
fn output_list_closes_on_button_outside_click_and_slide_change() {
    let mut a = test_app(1.0);
    music_with_outputs(&mut a);
    a.out_click(output::BTN.0, output::BTN.1);
    assert!(a.out_click(output::BTN.0, output::BTN.1));
    assert!(!a.out_list_shown(), "the button closes it again");
    a.out_click(output::BTN.0, output::BTN.1);
    assert!(a.out_click(RX - 10.0, 60.0), "a click outside only closes the list");
    assert!(!a.out_list_shown());
    assert_eq!(output::with_fake(|f| f.calls.len()), 0, "nothing was switched");
    a.out_click(output::BTN.0, output::BTN.1);
    a.step(1);
    assert!(!a.out_open, "another slide closes it");
    a.step(-1);
    assert!(!a.out_list_shown());
    a.out_click(output::BTN.0, output::BTN.1);
    a.hover = false;
    assert!(!a.out_list_shown(), "never drawn while the notch is closed");
    a.hover = true;
    a.lyric_mode = true;
    assert!(!a.out_list_shown(), "nor in the lyrics view");
    assert!(!a.out_click(output::BTN.0, output::BTN.1), "the lyrics view has no output button");
}

/// The wheel on the button moves to the next (down) or previous (up) device, wrapping around.
#[test]
fn output_wheel_on_the_button_cycles_devices() {
    let mut a = test_app(1.0);
    music_with_outputs(&mut a);
    let (bx, by) = output::BTN;
    for want in ["hp", "mon", "spk"] {
        assert!(a.out_wheel(bx, by, -120));
        assert_eq!(default_output().as_deref(), Some(want));
        assert_eq!(a.outs.cur.as_deref(), Some(want));
    }
    assert!(a.out_wheel(bx, by, 120));
    assert_eq!(default_output().as_deref(), Some("mon"), "wheel up goes back");
    assert!(!a.out_wheel(MCX, MCY, -120), "elsewhere on the controls the wheel still sets the volume");
}

/// Plugging and unplugging devices updates the list; when the last one goes the list closes.
#[test]
fn output_list_follows_devices_added_and_removed() {
    let mut a = test_app(1.0);
    music_with_outputs(&mut a);
    a.out_click(output::BTN.0, output::BTN.1);
    // speakers unplugged: Windows moves the default to the headphones
    output::with_fake(|f| {
        f.list.retain(|d| d.id != "spk");
        f.cur = Some("hp".into());
    });
    a.out_refresh();
    assert_eq!(a.outs.list.len(), 2);
    assert_eq!(a.outs.current().map(|d| d.id.as_str()), Some("hp"));
    assert!(a.out_list_shown());
    output::with_fake(|f| f.list.push(output::Device { id: "usb".into(), name: "Speakers (USB Audio)".into(), phones: false }));
    a.out_refresh();
    assert_eq!(a.outs.list.len(), 3, "a new device shows up");
    output::with_fake(|f| {
        f.list.clear();
        f.cur = None;
    });
    a.out_refresh();
    assert!(!a.out_list_shown(), "no devices left: the list closes");
    assert!(a.outs.current().is_none());
}

/// If Windows refuses the switch, the sound settings open; nothing else happens.
#[test]
fn output_switch_failure_opens_sound_settings() {
    let mut a = test_app(1.0);
    music_with_outputs(&mut a);
    output::with_fake(|f| f.refuse = true);
    let _ = sys::opened();
    a.out_pick("hp");
    assert_eq!(sys::opened(), vec!["ms-settings:sound".to_string()]);
    assert_eq!(a.outs.cur.as_deref(), Some("spk"), "still the old device");
    assert!(a.out_t.elapsed() > Duration::from_secs(2), "no overlay");
}

/// Picking the current device, or the wheel with only one device, switches nothing; the wheel still shows the name.
#[test]
fn output_single_device_changes_nothing() {
    let mut a = test_app(1.0);
    output::set_fake(&[("spk", "Speakers (Realtek(R) Audio)", false)], Some("spk"));
    a.out_refresh();
    open_slide(&mut a, SlideId::Music);
    assert!(a.out_wheel(output::BTN.0, output::BTN.1, -120), "taken, so the volume does not move");
    assert_eq!(output::with_fake(|f| f.calls.len()), 0);
    assert!(a.out_t.elapsed() < Duration::from_secs(1), "shows which device plays");
    a.out_pick("spk");
    assert_eq!(output::with_fake(|f| f.calls.len()), 0, "already the default");
}

/// Without any playback device the button opens the Windows sound settings.
#[test]
fn output_without_devices_opens_sound_settings() {
    let mut a = test_app(1.0);
    output::set_fake(&[], None);
    a.out_refresh();
    open_slide(&mut a, SlideId::Music);
    let _ = sys::opened();
    assert!(a.out_click(output::BTN.0, output::BTN.1));
    assert!(!a.out_list_shown());
    assert_eq!(sys::opened(), vec!["ms-settings:sound".to_string()]);
}

/// More devices than rows: the wheel over the list scrolls it, and rows map to the scrolled devices.
#[test]
fn output_list_scrolls_with_many_devices() {
    let mut a = test_app(1.0);
    let names: Vec<(String, String)> = (0..6).map(|i| (format!("d{i}"), format!("Speakers {i} (USB Audio)"))).collect();
    let list: Vec<(&str, &str, bool)> = names.iter().map(|(id, n)| (id.as_str(), n.as_str(), false)).collect();
    output::set_fake(&list, Some("d0"));
    a.out_refresh();
    open_slide(&mut a, SlideId::Music);
    a.out_click(output::BTN.0, output::BTN.1);
    let (px, py) = (output::PANEL.0 + 40.0, output::PANEL.1 + 40.0);
    assert!(a.out_wheel(px, py, -120));
    assert_eq!(a.out_scroll, 1);
    for _ in 0..5 {
        a.out_wheel(px, py, -120);
    }
    assert_eq!(a.out_scroll, 6 - output::MAX_ROWS, "stops at the last row");
    assert_eq!(output::with_fake(|f| f.calls.len()), 0, "scrolling the list switches nothing");
    let (rx, ry) = output::row_pos(0);
    a.out_click(rx + 60.0, ry + 12.0);
    assert_eq!(default_output().as_deref(), Some("d2"));
}

#[test]
fn snap_output() {
    shot("music-output-speaker", 1.0, music_with_outputs);
    shot("music-output-headphones", 1.0, |a| {
        music_with_outputs(a);
        output::with_fake(|f| f.cur = Some("hp".into()));
        a.out_refresh();
    });
    for s in SCALES {
        shot("music-output-list", s, |a| {
            music_with_outputs(a);
            a.out_click(output::BTN.0, output::BTN.1);
        });
    }
    shot("music-output-switched", 1.0, |a| {
        music_with_outputs(a);
        a.out_pick("hp");
        a.out_t = Instant::now() - Duration::from_millis(300);
    });
    // the volume overlay is as wide as the output overlay, so it covers the output button too
    shot("music-volume-overlay", 1.0, |a| {
        music_with_outputs(a);
        a.vol_state = (64, false);
        a.vol_t = Instant::now() - Duration::from_millis(300);
    });
    shot("music-output-many", 1.0, |a| {
        output::set_fake(
            &[
                ("a", "Speakers (Realtek(R) Audio)", false),
                ("b", "Headset Earphone (Jabra Evolve2 65 with a long product name)", true),
                ("c", "LG ULTRAGEAR (NVIDIA High Definition Audio)", false),
                ("d", "Headphones (AirPods Pro)", true),
                ("e", "Digital Output (S/PDIF)", false),
            ],
            Some("d"),
        );
        a.out_refresh();
        open_slide(a, SlideId::Music);
        a.out_click(output::BTN.0, output::BTN.1);
        a.out_wheel(output::PANEL.0 + 40.0, output::PANEL.1 + 40.0, -120);
    });
}
