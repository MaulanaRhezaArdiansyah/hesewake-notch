//! Animasi idle: siklus clock<->eye, splash awal, hotkey, mata berkedip + glow.

use super::*;

// Idle animation cycle: 10 min clock, then 15 min eye, loop.
const CLOCK_PHASE_SECS: f32 = 10.0 * 60.0;
const EYE_PHASE_SECS: f32 = 15.0 * 60.0;
pub(crate) const IDLE_CYCLE_SECS: f32 = CLOCK_PHASE_SECS + EYE_PHASE_SECS;
/// Hotkey that toggles clock <-> eye. A *bare* global \"H\" would swallow the letter in every app, so by
/// default it is Ctrl+Alt+H. Set to true for a bare global H (not recommended). A plain \"h\" keypress
/// is always handled when the Hesewake Notch window itself has keyboard focus.
pub(crate) const HOTKEY_PLAIN_H: bool = false;
pub(crate) const HOTKEY_ID: i32 = 1;
/// Eye + glow splash right after launch, then a cross-fade into the clock.
pub(crate) const SPLASH_SECS: f32 = 3.0;
pub(crate) const SPLASH_FADE: f32 = 0.35;

const EYE_FADE_IN: f32 = 0.45;
const EYE_FADE_OUT: f32 = 0.50;
/// Timer period while the eye rests (only the glow flows and the eye breathes): about 12 fps. Blinks and glances
/// run at 16 ms.
pub(crate) const EYE_REST_MS: u32 = 83;
/// A blink or glance due this soon switches to 16 ms frames first, so it starts as smoothly as before.
const EYE_SOON_SECS: f32 = 0.12;
/// With no keyboard or mouse input for this long the eye stops moving (no frames at all) until input comes back.
pub(crate) const EYE_STILL_AFTER_SECS: f32 = 300.0;

// Eye geometry. The two shapes below are the eyes of eye.svg (viewBox 1000x1000), baked in as cubic
// beziers so nothing has to be parsed at runtime. The white plate of the SVG is just the bounding box.
const EYE_SVG_H: f32 = 348.83; // true height of both eyes together (SVG units)
const EYE_PIVOT: (f32, f32) = (499.98, 500.35); // centre of both eyes (SVG units)
const EYE_H_PX: f32 = 19.0; // on-screen height of the open eyes (notch is 32 px tall)

// Blink timeline (seconds), GSAP-style: anticipate -> close -> hold -> elastic open.
const BLINK_ANTICIPATE: f32 = 0.07;
const BLINK_CLOSE: f32 = 0.085;
const BLINK_HOLD: f32 = 0.05;
const BLINK_OPEN: f32 = 0.65;
const EYE_ANTICIPATE_SY: f32 = 1.10; // eyes widen slightly before the blink
const EYE_CLOSED_SY: f32 = 0.06; // closed = thin line
const EYE_SQUASH: f32 = 0.14; // horizontal squash & stretch (volume feel)
const EYE_DROP_PX: f32 = 1.0; // eyes sink a little while closing
const EYE_LOOK_MAX: f32 = 2.2; // glance range in px
const EYE_LOOK_K: f32 = 140.0;
const EYE_LOOK_C: f32 = 24.0;

// Glow: running palette of blue shades along X, fading to transparent upwards.
const GLOW_ALPHA: f32 = 0.85;
const GLOW_REACH: f32 = 0.95; // fraction of the notch height the glow climbs
const GLOW_FLOW: f32 = 0.10; // palette cycles per second
const EYE_PAL: [[f32; 3]; 5] = [
    [90.0, 150.0, 255.0],  // bright blue
    [40.0, 110.0, 235.0],  // royal blue
    [130.0, 190.0, 255.0], // light sky blue
    [20.0, 70.0, 190.0],   // deep blue
    [70.0, 140.0, 255.0],  // blue
];

// ---------------------------------------------------------------- idle animation
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum IdlePhase {
    Clock,
    Eye,
}

// GSAP-like easings.
fn ease_out_quad(u: f32) -> f32 {
    1.0 - (1.0 - u) * (1.0 - u)
}

/// elastic.out(1, 0.42): overshoots ~25% then wobbles to rest -> the "bouncy" pop.
fn elastic_out(u: f32) -> f32 {
    if u <= 0.0 {
        0.0
    } else if u >= 1.0 {
        1.0
    } else {
        let p = 0.42;
        2f32.powf(-10.0 * u) * ((u - p / 4.0) * std::f32::consts::TAU / p).sin() + 1.0
    }
}

/// Procedural eye: keyframed blink timeline (anticipate, close, hold, elastic open), squash & stretch,
/// random double blinks, breathing and small glances driven by a critically damped spring.
pub(crate) struct EyeAnim {
    pub(crate) age: f32,
    pub(crate) wait: f32,
    pub(crate) blinking: bool,
    pub(crate) bt: f32,
    pub(crate) hold: f32,
    pub(crate) double_next: bool,
    pub(crate) sy: f32,
    pub(crate) sx: f32,
    pub(crate) dy: f32,
    pub(crate) look: f32,
    pub(crate) look_v: f32,
    pub(crate) look_to: f32,
    pub(crate) look_wait: f32,
    pub(crate) boost: f32,
    pub(crate) rng: u64,
}

impl EyeAnim {
    /// Starts with closed eyes that pop open with an elastic bounce.
    pub(crate) fn new() -> Self {
        let mut e = Self {
            age: 0.0,
            wait: 0.0,
            blinking: true,
            bt: BLINK_ANTICIPATE + BLINK_CLOSE,
            hold: 0.32,
            double_next: false,
            sy: EYE_CLOSED_SY,
            sx: 1.0,
            dy: 0.0,
            look: 0.0,
            look_v: 0.0,
            look_to: 0.0,
            look_wait: 2.0,
            boost: 0.0,
            rng: 0x9E37_79B9_7F4A_7C15,
        };
        e.set_pose(EYE_CLOSED_SY);
        e
    }

    fn rand(&mut self) -> f32 {
        self.rng = self.rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.rng >> 33) as f32) / ((1u64 << 31) as f32)
    }

    pub(crate) fn total(&self) -> f32 {
        BLINK_ANTICIPATE + BLINK_CLOSE + self.hold + BLINK_OPEN
    }

    fn sy_at(&self, bt: f32) -> f32 {
        let (a, c) = (BLINK_ANTICIPATE, BLINK_CLOSE);
        if bt < a {
            1.0 + (EYE_ANTICIPATE_SY - 1.0) * ease_out_quad(bt / a)
        } else if bt < a + c {
            let u = (bt - a) / c;
            EYE_ANTICIPATE_SY + (EYE_CLOSED_SY - EYE_ANTICIPATE_SY) * u * u * u
        } else if bt < a + c + self.hold {
            EYE_CLOSED_SY
        } else {
            let u = ((bt - a - c - self.hold) / BLINK_OPEN).min(1.0);
            EYE_CLOSED_SY + (1.0 - EYE_CLOSED_SY) * elastic_out(u)
        }
    }

    fn set_pose(&mut self, sy: f32) {
        self.sy = sy;
        self.sx = (1.0 + EYE_SQUASH * (1.0 - sy)).clamp(0.90, 1.15);
        self.dy = ((1.0 - sy) * EYE_DROP_PX).clamp(-1.5, 1.2);
        self.boost = ((sy - 1.0).max(0.0) * 1.5).min(0.5);
    }

    /// True while something is moving (needs the fast 16 ms timer).
    pub(crate) fn active(&self) -> bool {
        self.blinking || self.age < EYE_FADE_IN + 0.1 || (self.look - self.look_to).abs() > 0.02 || self.look_v.abs() > 0.05
    }

    /// A blink or a glance starts within `EYE_SOON_SECS`.
    fn soon(&self) -> bool {
        !self.blinking && (self.wait < EYE_SOON_SECS || self.look_wait < EYE_SOON_SECS)
    }

    /// Timer period for the eye phase: 16 ms while (or just before) the eye moves, `EYE_REST_MS` while it rests,
    /// and 0 (no frames) once nobody has used the keyboard or mouse for `EYE_STILL_AFTER_SECS`. A blink that is
    /// already running finishes first, so the eye never stops half closed.
    pub(crate) fn frame_ms(&self, input_idle_secs: f32) -> u32 {
        if self.active() {
            16
        } else if input_idle_secs >= EYE_STILL_AFTER_SECS {
            0
        } else if self.soon() {
            16
        } else {
            EYE_REST_MS
        }
    }

    fn fade_in(&self) -> f32 {
        let u = (self.age / EYE_FADE_IN).clamp(0.0, 1.0);
        u * u * (3.0 - 2.0 * u)
    }

    pub(crate) fn update(&mut self, dt: f32) {
        if dt <= 0.0 {
            return;
        }
        self.age += dt;
        let n = ((dt / 0.008).ceil() as i32).max(1);
        let sdt = dt / n as f32;
        for _ in 0..n {
            self.look_v += (EYE_LOOK_K * (self.look_to - self.look) - EYE_LOOK_C * self.look_v) * sdt;
            self.look += self.look_v * sdt;
        }
        if self.blinking {
            self.bt += dt;
            if self.bt >= self.total() {
                self.blinking = false;
                self.bt = 0.0;
                self.set_pose(1.0);
                if self.double_next {
                    self.double_next = false;
                    self.wait = 0.09 + self.rand() * 0.07;
                } else {
                    self.wait = 2.4 + self.rand() * 3.8;
                    if self.rand() < 0.16 {
                        self.double_next = true;
                    }
                }
            } else {
                let sy = self.sy_at(self.bt);
                self.set_pose(sy);
            }
        } else {
            self.wait -= dt;
            self.set_pose(1.0 + 0.012 * (self.age * 2.1).sin());
            self.look_wait -= dt;
            if self.look_wait <= 0.0 {
                self.look_to = if self.rand() < 0.65 { (self.rand() * 2.0 - 1.0) * EYE_LOOK_MAX } else { 0.0 };
                self.look_wait = 1.8 + self.rand() * 3.2;
            }
            if self.wait <= 0.0 {
                self.blinking = true;
                self.bt = 0.0;
                self.hold = BLINK_HOLD;
            }
        }
    }
}

const EYE_L_START: (f32, f32) = (464.64, 478.34);
const EYE_L: [[f32; 6]; 10] = [
    [464.83, 528.40, 455.22, 571.76, 430.28, 610.90],
    [413.71, 636.90, 392.13, 657.52, 362.70, 668.39],
    [327.41, 681.39, 295.70, 674.21, 267.95, 649.49],
    [245.04, 629.05, 231.95, 602.39, 222.73, 573.65],
    [213.58, 545.58, 209.29, 516.16, 210.05, 486.65],
    [211.17, 447.32, 220.99, 410.65, 245.43, 379.05],
    [266.06, 352.36, 292.50, 333.79, 326.01, 327.77],
    [364.61, 320.85, 396.94, 333.64, 422.62, 363.03],
    [444.73, 388.33, 456.09, 418.56, 461.57, 451.22],
    [463.30, 461.52, 463.94, 472.00, 464.64, 478.34],
];

const EYE_R_START: (f32, f32) = (790.00, 503.06);
const EYE_R: [[f32; 6]; 6] = [
    [790.09, 547.33, 763.48, 585.25, 722.53, 599.15],
    [688.15, 610.82, 652.67, 596.64, 636.89, 563.92],
    [619.75, 528.41, 620.49, 492.83, 641.06, 458.92],
    [655.45, 435.22, 676.88, 420.53, 704.89, 417.07],
    [734.47, 413.41, 762.80, 428.86, 777.82, 456.07],
    [785.83, 470.43, 790.03, 486.61, 790.00, 503.06],
];

fn eye_path(start: (f32, f32), segs: &[[f32; 6]]) -> Path {
    let mut pb = PathBuilder::new();
    pb.move_to(start.0, start.1);
    for c in segs {
        pb.cubic_to(c[0], c[1], c[2], c[3], c[4], c[5]);
    }
    pb.close();
    pb.finish().unwrap_or_else(dummy_path)
}

pub(crate) fn build_eye_paths() -> Vec<Path> {
    vec![eye_path(EYE_L_START, &EYE_L), eye_path(EYE_R_START, &EYE_R)]
}

/// Kolom maksimum kanvas (lebar notch pada skala terbesar).
const GLOW_COLS: usize = W as usize * scale::MAX as usize + 2;

/// Running gradient behind the eye: a palette (shades of blue) flows along X while the
/// alpha fades to transparent towards the top. Written straight into the pixmap, limited to the notch
/// rectangle (about 150x32 px), no allocations, no shaders -> negligible CPU.
fn eye_glow(g: &mut Gfx, w: f32, h: f32, t: f32, strength: f32) {
    let strength = strength.clamp(0.0, 1.0);
    if strength <= 0.01 {
        return;
    }
    // Loop ini bekerja per piksel fisik; rumus warnanya tetap memakai koordinat logis (dibagi skala).
    let k = g.s;
    let pw = g.pm.width() as usize;
    let ph = g.pm.height() as usize;
    let x0 = ((CX - w / 2.0) * k).floor().max(0.0) as usize;
    let x1 = (((CX + w / 2.0) * k).ceil().max(0.0) as usize).min(pw).min(GLOW_COLS);
    let y1 = ((h * k).ceil().max(0.0) as usize).min(ph);
    if x1 <= x0 || y1 == 0 {
        return;
    }
    // Per-column colour and horizontal intensity (computed once, reused for every row).
    let mut cols = [[0.0f32; 4]; GLOW_COLS];
    let np = EYE_PAL.len();
    for x in x0..x1 {
        let xf = (x as f32 + 0.5) / k;
        let pos = (xf - CX) * 0.0042 - t * GLOW_FLOW;
        let f = pos.rem_euclid(1.0) * np as f32;
        let i = (f.floor() as usize) % np;
        let fr = f - f.floor();
        let sm = fr * fr * (3.0 - 2.0 * fr);
        let (c0, c1) = (EYE_PAL[i], EYE_PAL[(i + 1) % np]);
        let hl = 0.78 + 0.22 * (std::f32::consts::TAU * (xf * 0.0065 - t * 0.35)).sin();
        cols[x] = [
            c0[0] + (c1[0] - c0[0]) * sm,
            c0[1] + (c1[1] - c0[1]) * sm,
            c0[2] + (c1[2] - c0[2]) * sm,
            hl,
        ];
    }
    let data = g.pm.data_mut();
    for y in 0..y1 {
        let d = (h - (y as f32 + 0.5) / k).max(0.0) / (h * GLOW_REACH);
        if d >= 1.0 {
            continue;
        }
        let v = 1.0 - d;
        let ra = strength * GLOW_ALPHA * v.powf(1.7);
        if ra <= 0.004 {
            continue;
        }
        let row = y * pw;
        for x in x0..x1 {
            let i = (row + x) * 4;
            let da = data[i + 3] as f32 * (1.0 / 255.0);
            if da <= 0.0 {
                continue;
            }
            let c = &cols[x];
            let a = ra * c[3];
            let inv = 1.0 - a;
            let k = a * da;
            data[i] = (data[i] as f32 * inv + c[0] * k) as u8;
            data[i + 1] = (data[i + 1] as f32 * inv + c[1] * k) as u8;
            data[i + 2] = (data[i + 2] as f32 * inv + c[2] * k) as u8;
        }
    }
}

/// Ends the startup splash once it is over: the idle cycle starts fresh at the clock phase (already
/// faded in, so the cross-fade stays smooth) and from now on only counts while idle.
/// Returns true when the splash just ended.
pub(crate) fn splash_tick(a: &mut App) -> bool {
    let Some(t0) = a.splash else { return false; };
    if t0.elapsed().as_secs_f32() < SPLASH_SECS {
        return false;
    }
    a.splash = None;
    // `--demo-eye`: langsung ke fase mata (uji tata letak notch idle tanpa menunggu 10 menit)
    let demo_eye = std::env::args().any(|x| x == "--demo-eye");
    a.idle_base = if demo_eye { CLOCK_PHASE_SECS + 1.0 } else { CLOCK_FADE_IN };
    a.idle_mark = if a.idle() { Some(Instant::now()) } else { None };
    a.cur_phase = if demo_eye { IdlePhase::Eye } else { IdlePhase::Clock };
    a.was_idle = a.idle();
    true
}

/// Manual toggle: jump the idle cycle to the start of the eye phase (if showing the clock) or to the
/// start of the clock phase (if showing the eye). The normal 10 min / 15 min loop continues from there.
pub(crate) unsafe fn toggle_idle_phase(a: &mut App) {
    if a.cfg.idle_display != 0 {
        return; // tampilan dikunci di Settings (Idle display)
    }
    a.splash = None;
    let to_eye = a.idle_phase() == IdlePhase::Clock;
    a.idle_base = if to_eye { CLOCK_PHASE_SECS } else { 0.0 };
    if a.idle_mark.is_some() {
        a.idle_mark = Some(Instant::now());
    }
    a.cur_phase = a.idle_phase();
    if to_eye {
        a.eye = EyeAnim::new();
    }
    a.last = Instant::now();
    present(a);
    schedule(a);
}

impl App {
    /// Seconds spent idle inside the current clock+eye cycle. Wall-clock based (not tied to timer ticks),
    /// frozen while the notch is not idle.
    pub(crate) fn idle_secs(&self) -> f32 {
        self.idle_base + self.idle_mark.map_or(0.0, |m| m.elapsed().as_secs_f32())
    }

    /// Seconds since launch while the startup splash is still running.
    pub(crate) fn splash_t(&self) -> Option<f32> {
        self.splash.map(|t| t.elapsed().as_secs_f32()).filter(|&t| t < SPLASH_SECS)
    }

    pub(crate) fn idle_phase(&self) -> IdlePhase {
        // Pengaturan "Idle display": jam saja / mata saja menimpa siklus bergantian.
        match self.cfg.idle_display {
            1 => return IdlePhase::Clock,
            2 => return IdlePhase::Eye,
            _ => {}
        }
        let cycle_t = self.idle_secs() % IDLE_CYCLE_SECS;
        if cycle_t < CLOCK_PHASE_SECS {
            IdlePhase::Clock
        } else {
            IdlePhase::Eye
        }
    }

    /// The blinking eye from eye.svg over the running glow. Everything is cheap: two pre-built paths
    /// drawn with a transform, and the glow is computed analytically per pixel inside the notch only.
    pub(crate) fn draw_eye(&self, g: &mut Gfx, w: f32, h: f32, ca: f32) {
        let e = &self.eye;
        let out = match self.splash_t() {
            Some(st) => ((SPLASH_SECS - st) / SPLASH_FADE).clamp(0.0, 1.0),
            None => {
                let remaining = IDLE_CYCLE_SECS - (self.idle_secs() % IDLE_CYCLE_SECS);
                (remaining / EYE_FADE_OUT).clamp(0.0, 1.0)
            }
        };
        let al = ca * e.fade_in() * out;
        if al <= 0.01 {
            return;
        }

        eye_glow(g, w, h, self.t, al * (1.0 + e.boost));

        let k = EYE_H_PX / EYE_SVG_H;
        let (sx, sy) = (k * e.sx, k * e.sy);
        let (cx, cy) = (CX + e.look, h / 2.0 + e.dy);
        let tr = g.ts.pre_concat(Transform::from_row(sx, 0.0, 0.0, sy, cx - EYE_PIVOT.0 * sx, cy - EYE_PIVOT.1 * sy));
        let white = paint(255, 255, 255, al);
        for p in &self.eye_paths {
            g.pm.fill_path(p, &white, FillRule::Winding, tr, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Open eyes in the middle of a rest: no blink or glance due for a while.
    fn resting() -> EyeAnim {
        let mut e = EyeAnim::new();
        for _ in 0..120 {
            e.update(1.0 / 60.0);
        }
        e.blinking = false;
        e.bt = 0.0;
        e.set_pose(1.0);
        (e.wait, e.look_wait) = (2.0, 2.0);
        (e.look, e.look_to, e.look_v) = (0.0, 0.0, 0.0);
        e
    }

    #[test]
    fn resting_eye_runs_at_about_12_fps() {
        let e = resting();
        assert!(!e.active());
        assert_eq!(e.frame_ms(0.0), EYE_REST_MS);
        assert!((70..=100).contains(&EYE_REST_MS), "10 to 14 fps");
    }

    #[test]
    fn blinks_and_glances_stay_at_60_fps() {
        let mut e = resting();
        e.wait = 0.1;
        assert_eq!(e.frame_ms(0.0), 16, "a blink is due: 60 fps before it starts");
        let mut e = resting();
        e.look_wait = 0.05;
        assert_eq!(e.frame_ms(0.0), 16, "a glance is due");
        let mut e = resting();
        e.wait = 0.0;
        e.update(1.0 / 60.0);
        assert!(e.blinking);
        assert_eq!(e.frame_ms(0.0), 16, "blinking");
    }

    /// Driven by its own timer from a rest, every blink frame is 16 ms, so the short anticipation (70 ms) is seen.
    #[test]
    fn blink_frames_are_short_even_coming_from_a_rest() {
        let mut e = resting();
        e.wait = 1.0;
        let (mut t, mut blink_frames, mut long) = (0.0, 0, 0);
        while t < 2.5 {
            let dt = e.frame_ms(0.0) as f32 / 1000.0;
            let was = e.blinking;
            e.update(dt);
            t += dt;
            if was {
                blink_frames += 1;
                if dt > 0.017 {
                    long += 1;
                }
            }
        }
        assert!(blink_frames > 20, "a blink happened ({blink_frames} frames)");
        assert_eq!(long, 0, "no blink frame at the rest rate");
    }

    #[test]
    fn eye_stops_when_nobody_is_there() {
        let e = resting();
        assert_eq!(e.frame_ms(EYE_STILL_AFTER_SECS + 1.0), 0, "away: no frames");
        assert_eq!(e.frame_ms(EYE_STILL_AFTER_SECS - 1.0), EYE_REST_MS);
        let mut e = resting();
        e.wait = 0.0;
        e.update(1.0 / 60.0);
        assert_eq!(e.frame_ms(EYE_STILL_AFTER_SECS + 1.0), 16, "a blink in progress finishes first");
    }
}
