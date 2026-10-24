//! Loop animasi: ukuran target notch, timer, polling hover/busy/media.

use super::*;

// ---------------------------------------------------------------- animation
pub(crate) fn target(a: &App) -> (f32, f32) {
    if a.expanded() {
        (EXP_W, EXP_H)
    } else if a.toast.is_some() {
        a.toast_size()
    } else if a.thin() {
        THIN
    } else if a.media.playing && a.lyric_w > 0.0 {
        // Lebar menyesuaikan baris lirik (ruang teks = lebar - 88 px untuk ikon dan equalizer, + lonceng).
        ((a.lyric_w + 88.0 + 16.0 + a.bell_extra()).clamp(PLAYING.0 + a.bell_extra(), EXP_W), PLAYING.1)
    } else if a.media.playing {
        (PLAYING.0 + a.bell_extra(), PLAYING.1)
    } else {
        IDLE
    }
}

fn settled(a: &App) -> bool {
    let (tw, th) = target(a);
    (a.w - tw).abs() < 0.3
        && (a.h - th).abs() < 0.3
        && a.vw.abs() < 2.0
        && a.vh.abs() < 2.0
        && (a.sp - a.slide as f32).abs() < 0.003
        && a.sv.abs() < 0.05
}

/// Timer: 16 ms saat bergerak / animasi droplet / mid-blink, 40 ms saat mata istirahat (glow mengalir),
/// 33 ms untuk equalizer, dan mati kalau tidak ada yang bergerak.
pub(crate) unsafe fn schedule(a: &mut App) {
    let pulsing = a.drop_pulse.map_or(false, |t| t.elapsed().as_secs_f32() < DROP_ANIM_SECS);
    let want = if a.display_off {
        0
    } else if pulsing {
        16
    } else if !settled(a) || a.lyric_fading() || a.art_fading() || a.toast.is_some() || a.water_animating() || a.lyric_scroll_animating() || a.bell_animating() || a.notif_ui_animating() || a.settings_animating() || a.privacy_animating() || a.vol_overlay_active() {
        16
    } else if a.expanded() && a.is_slide(SlideId::Music) && a.tint.animating() {
        16 // artwork colour cross-fade on the Music slide
    } else if a.media.playing && !a.expanded() && !a.busy {
        33
    } else if a.splash_t().is_some() && !a.media.playing {
        if a.eye.active() {
            16
        } else {
            EYE_REST_MS
        }
    } else if a.idle() && a.idle_phase() == IdlePhase::Eye {
        // 60 fps saat berkedip / glance, santai saat istirahat.
        if a.eye.active() {
            16
        } else {
            EYE_REST_MS
        }
    } else if a.idle() && a.idle_phase() == IdlePhase::Clock && a.idle_secs() < CLOCK_FADE_IN + 0.1 {
        33 // clock fade-in right after the eye phase
    } else {
        0
    };
    if want != a.interval {
        let hwnd = HWND(a.hwnd as *mut c_void);
        if want == 0 {
            // Tidak trim working set di sini: tiap animasi selesai, halaman yang dibuang harus
            // di-page-fault lagi saat animasi berikutnya (patah-patah). Trim jarang di poll().
            if pace::available() {
                pace::cancel();
            } else {
                let _ = KillTimer(hwnd, 1);
            }
        } else if pace::available() {
            pace::set(want);
        } else {
            SetTimer(hwnd, 1, want, None);
        }
        a.interval = want;
    }
}

unsafe fn poll(a: &mut App) {
    a.tick_n = a.tick_n.wrapping_add(1);
    let mut changed = false;
    let mut redraw = false;
    if splash_tick(a) {
        redraw = true;
    }
    if a.lyric_update() {
        redraw = true;
    }
    if a.toast_update() {
        redraw = true;
    }
    if a.settings_open && settled(a) && a.settings_autoclose() {
        redraw = true;
    }
    a.notif_ui_step();
    if a.splash.is_none() {
        if let Some(k) = a.demo_toast.take() {
            if let toast::ToastKind::Privacy { cam, app } = &k {
                if *cam {
                    a.priv_cam = vec![app.clone()];
                } else {
                    a.priv_mic = vec![app.clone()];
                }
                a.priv_t = Instant::now();
            }
            if let toast::ToastKind::Notification { id } = &k {
                a.notifs.insert(0, notif::NotifItem {
                    id: *id,
                    app: "Messages".into(),
                    title: "Alice".into(),
                    body: "Are we still on for dinner tonight?".into(),
                    at_unix: notif::now_unix(),
                    unread: true,
                    icon: None,
                    avatar: None,
                    dismiss: None,
                });
                a.notif_access = notif::Access::Allowed;
                a.bell_t = Instant::now();
            }
            a.push_toast(k);
            redraw = true;
        }
    }
    if a.splash.is_none() {
        if let Some(n) = a.demo_slide.take() {
            if n == usize::MAX {
                a.open_settings();
            } else {
                a.show(n);
            }
            a.peek_until = Some(Instant::now() + Duration::from_secs(25));
            redraw = true;
        }
    }
    // Baterai rendah tidak selalu memicu pesan power; cek berkala (~30 dtk).
    if a.tick_n % 300 == 0 {
        a.power_check();
    }

    // Animasi droplet selesai -> lepas state
    if let Some(t) = a.drop_pulse {
        if t.elapsed().as_secs_f32() >= DROP_ANIM_SECS {
            a.drop_pulse = None;
        }
        redraw = true;
    }

    // Idle clock: start/stop the stopwatch, wrap it once per clock+eye cycle.
    let now = Instant::now();
    let idle_now = a.idle();
    if idle_now {
        // The idle stopwatch only runs while idle, and not during the startup splash.
        if a.idle_mark.is_none() && a.splash.is_none() {
            a.idle_mark = Some(now);
        }
    } else if let Some(m) = a.idle_mark.take() {
        a.idle_base += (now - m).as_secs_f32();
    }
    let secs = a.idle_secs();
    if secs >= IDLE_CYCLE_SECS {
        a.idle_base = secs % IDLE_CYCLE_SECS;
        if a.idle_mark.is_some() {
            a.idle_mark = Some(now);
        }
    }
    let cur_phase = a.idle_phase();
    let resumed = idle_now && !a.was_idle;
    a.was_idle = idle_now;
    if cur_phase != a.cur_phase || resumed {
        a.cur_phase = cur_phase;
        if cur_phase == IdlePhase::Eye {
            a.eye = EyeAnim::new(); // eyes pop open again
        }
        redraw = true;
    }

    if !a.menu_open {
        let mut p = POINT::default();
        if GetCursorPos(&mut p).is_ok() {
            let (zw, zh) = if a.hover || a.expanded() {
                (EXP_W + 20.0, EXP_H + 8.0)
            } else if a.thin() {
                (a.w.max(THIN.0) + 60.0, 10.0)
            } else {
                (a.w.max(IDLE.0) + 12.0, a.h.max(IDLE.1) + 6.0)
            };
            let x = (p.x - a.wx) as f32;
            let y = p.y as f32;
            let inside = (x - CX).abs() <= zw / 2.0 && y >= 0.0 && y <= zh;
            if inside != a.hover {
                // Pointer masuk tepat di lonceng saat ada yang belum dibaca: buka langsung slide Notifications.
                let to_notif = inside
                    && a.unread_count() > 0
                    && a.bell_pos().map_or(false, |(bx, by)| (x - bx).abs() < 14.0 && (y - by).abs() < 14.0);
                a.hover = inside;
                changed = true;
                if to_notif {
                    a.show_id(SlideId::Notifications);
                }
            }
        }
    }
    if a.tick_n % 5 == 0 {
        let b = is_busy(HWND(a.hwnd as *mut c_void), a.cfg.busy_on_maximized);
        if b != a.busy {
            a.busy = b;
            changed = true;
        }
    }
    // Window TOPMOST bisa "tenggelam" (Start menu, taskbar, app fullscreen, explorer restart, dll).
    // Cek tiap ~1 detik; kalau flag topmost hilang, pasang lagi tanpa merebut fokus.
    if a.tick_n % 10 == 0 {
        let hwnd = HWND(a.hwnd as *mut c_void);
        let ex = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        if ex & WS_EX_TOPMOST.0 == 0 {
            let _ = SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }
    if a.tick_n % 50 == 0 && a.water.roll() {
        redraw = true;
    }
    // Trim working set ±tiap 60 detik. Tidak lagi menunggu timer animasi mati: saat musik main timer tidak pernah
    // mati, sehingga memori sisa decode (cover, ikon) tidak pernah dikembalikan.
    if a.tick_n % 600 == 0 {
        trim_memory();
    }
    if let Some(t) = a.peek_until {
        if Instant::now() >= t {
            a.peek_until = None;
            a.reminding = false;
            changed = true;
        }
    }
    if check_reminder(a) {
        changed = true;
    }

    let st = now_tm();
    let expanded = a.expanded();
    let cur = a.cur();
    // Slide 0 (Music + jam) dan slide 2 (To-do) menampilkan menit/tanggal.
    if expanded && (cur == 0 || cur == 2) && st.wMinute != a.last_min {
        redraw = true;
    }
    if !expanded && !a.thin() && !a.media.playing && cur_phase == IdlePhase::Clock && st.wMinute != a.last_min {
        redraw = true;
    }
    a.last_sec = st.wSecond;
    a.last_min = st.wMinute;

    if let Some(t) = a.shelf_flash {
        if t.elapsed() >= Duration::from_millis(1800) {
            a.shelf_flash = None;
            redraw = true;
        }
    }
    if a.editing {
        let on = (a.edit_start.elapsed().as_millis() / 500) % 2 == 0;
        if on != a.caret_on {
            a.caret_on = on;
            redraw = true;
        }
    }

    if changed || redraw {
        present(a);
    }
    if changed {
        a.last = Instant::now();
    }
    schedule(a);
}

pub(crate) unsafe fn tick(id: usize) {
    with_app(|a| {
        if id == 2 {
            poll(a);
            return;
        }
        splash_tick(a);
        a.lyric_update();
        a.toast_update();
        let now = Instant::now();
        let dt = (now - a.last).as_secs_f32().min(0.05);
        a.last = now;
        a.t += dt;
        a.water_ui_step(dt);
        a.lyric_scroll_step(dt);
        a.settings_ui_step(dt);
        a.notif_ui_step();

        // Only advance the eye once the notch is back to its small size, so the pop-open is actually seen.
        let eye_on = (a.splash_t().is_some() && !a.media.playing) || (a.idle() && a.idle_phase() == IdlePhase::Eye);
        if eye_on && a.h < IDLE.1 + 10.0 {
            a.eye.update(dt);
        }

        let (tw, th) = target(a);
        const K: f32 = 380.0;
        const C: f32 = 40.0;
        const SK: f32 = 320.0;
        const SC: f32 = 36.0;
        let n = ((dt / 0.008).ceil() as i32).max(1);
        let sdt = dt / n as f32;
        for _ in 0..n {
            a.vw += (K * (tw - a.w) - C * a.vw) * sdt;
            a.vh += (K * (th - a.h) - C * a.vh) * sdt;
            a.w += a.vw * sdt;
            a.h += a.vh * sdt;
            a.sv += (SK * (a.slide as f32 - a.sp) - SC * a.sv) * sdt;
            a.sp += a.sv * sdt;
        }
        a.w = a.w.max(20.0);
        a.h = a.h.max(2.0);
        if settled(a) {
            a.w = tw;
            a.h = th;
            a.vw = 0.0;
            a.vh = 0.0;
            a.sp = a.slide as f32;
            a.sv = 0.0;
            let n = a.n_slides() as i32;
            let k = a.slide.div_euclid(n) * n;
            a.slide -= k;
            a.sp -= k as f32;
            a.settings_autoclose();
        }
        present(a);
        schedule(a);
    });
}