//! Fitur Water: pelacak minum, pengingat, slide Water (angka besar, toggle reminder, grafik per jam, Settings).

use super::*;
use icons::{hgrad, vgrad};

pub(crate) const SNOOZE: Duration = Duration::from_secs(10 * 60);

// ---------------------------------------------------------------- layout (koordinat kanvas, slide di LX..RX)
const BLUE: [u8; 3] = [80, 170, 255];
const GREEN: [u8; 3] = [60, 200, 110];
const GREY: [u8; 3] = [150, 150, 158];

const TOGGLE: (f32, f32, f32, f32) = (LX + 152.0, 42.0, 46.0, 26.0);
const DRINK: (f32, f32, f32, f32) = (LX, 116.0, 84.0, 32.0);
const SIDE: (f32, f32, f32, f32) = (LX + 92.0, 116.0, 106.0, 32.0); // Settings, atau Snooze saat reminder aktif
const BACK: (f32, f32, f32, f32) = (RX - 70.0, 8.0, 70.0, 26.0);
const DIV_X: f32 = LX + 210.0;
const PLOT_L: f32 = LX + 224.0;
const PLOT_R: f32 = RX - 36.0;
const PLOT_T: f32 = 54.0;
const PLOT_B: f32 = 126.0;
const N_BARS: usize = 8;

const GOALS: [u32; 4] = [1500, 2000, 2500, 3000];
const REMINDS: [u32; 4] = [0, 30, 60, 90];
const GLASSES: [u32; 4] = [200, 250, 330, 500];

// ---------------------------------------------------------------- data
pub(crate) struct Water {
    pub(crate) day: String,
    pub(crate) ml: u32,
    pub(crate) goal: u32,
    pub(crate) remind_min: u32,
    pub(crate) glass: u32,
    pub(crate) hours: [u32; 24],
    /// Interval terakhir yang aktif, dipakai saat toggle reminder dinyalakan lagi.
    pub(crate) last_remind: u32,
    pub(crate) next: Instant,
}

impl Water {
    /// Format: `day|ml|goal|remind[|glass[|h0,h1,...,h23]]` (field setelah `remind` opsional).
    pub(crate) fn parse(s: &str, today: &str) -> Water {
        let mut w = Water {
            day: today.to_string(),
            ml: 0,
            goal: 2000,
            remind_min: 60,
            glass: 250,
            hours: [0; 24],
            last_remind: 60,
            next: Instant::now(),
        };
        let p: Vec<&str> = s.trim().split('|').collect();
        if p.len() >= 4 {
            w.goal = p[2].parse().unwrap_or(2000);
            w.remind_min = p[3].parse().unwrap_or(60);
            if let Some(g) = p.get(4).and_then(|g| g.parse::<u32>().ok()).filter(|g| (50..=2000).contains(g)) {
                w.glass = g;
            }
            if p[0] == today {
                w.ml = p[1].parse().unwrap_or(0);
                if let Some(h) = p.get(5) {
                    for (i, v) in h.split(',').take(24).enumerate() {
                        w.hours[i] = v.parse().unwrap_or(0);
                    }
                }
            }
        }
        if w.remind_min > 0 {
            w.last_remind = w.remind_min;
        }
        w
    }

    pub(crate) fn serialize(&self) -> String {
        let h: Vec<String> = self.hours.iter().map(|v| v.to_string()).collect();
        format!("{}|{}|{}|{}|{}|{}\n", self.day, self.ml, self.goal, self.remind_min, self.glass, h.join(","))
    }

    pub(crate) fn load() -> Water {
        let s = std::fs::read_to_string(data_path("water.txt")).unwrap_or_default();
        let mut w = Water::parse(&s, &today());
        w.reset_timer();
        w
    }
    pub(crate) fn save(&self) {
        if cfg!(test) {
            return; // tes tidak boleh menimpa data asli di %APPDATA%
        }
        write_data("water.txt", self.serialize());
    }
    pub(crate) fn reset_timer(&mut self) {
        self.next = Instant::now() + Duration::from_secs(self.remind_min.max(1) as u64 * 60);
    }
    /// Tunda pengingat berikutnya.
    pub(crate) fn snooze(&mut self) {
        self.next = Instant::now() + SNOOZE;
    }
    pub(crate) fn roll(&mut self) -> bool {
        let d = today();
        if d != self.day {
            self.day = d;
            self.ml = 0;
            self.hours = [0; 24];
            self.save();
            return true;
        }
        false
    }
    /// Tambah/kurangi `d` ml; dicatat pada bucket jam `hour`.
    pub(crate) fn add(&mut self, d: i32, hour: usize) {
        self.ml = (self.ml as i32 + d).max(0) as u32;
        let h = hour.min(23);
        self.hours[h] = (self.hours[h] as i32 + d).max(0) as u32;
        self.reset_timer();
        self.save();
    }
    pub(crate) fn remind_on(&self) -> bool {
        self.remind_min > 0
    }
    pub(crate) fn toggle_remind(&mut self) {
        if self.remind_min > 0 {
            self.last_remind = self.remind_min;
            self.remind_min = 0;
        } else {
            self.remind_min = self.last_remind.max(30);
            self.reset_timer();
        }
        self.save();
    }
    /// Delapan jam yang ditampilkan di grafik: dimulai `start`, jam sekarang di dekat ujung kanan.
    pub(crate) fn window_start(now_hour: usize) -> usize {
        (now_hour as i32 - 5).clamp(0, (24 - N_BARS) as i32) as usize
    }
    /// Batas atas sumbu grafik (kelipatan 200 ml, minimal 400).
    pub(crate) fn axis_max(&self, start: usize) -> u32 {
        let m = self.hours[start..start + N_BARS].iter().copied().max().unwrap_or(0).max(400);
        (m + 199) / 200 * 200
    }
}

pub(crate) fn check_reminder(a: &mut App) -> bool {
    if a.water.remind_min == 0 {
        return false;
    }
    let now = Instant::now();
    if now < a.water.next {
        return false;
    }
    a.water.reset_timer();
    let st = now_tm();
    if st.wHour < 8 || st.wHour >= 22 || a.water.ml >= a.water.goal {
        return false;
    }
    a.show(1); // slide Water (index 1)
    a.water_view(0);
    a.reminding = true;
    a.peek_until = Some(now + Duration::from_secs(10));
    true
}

// ---------------------------------------------------------------- animasi
/// State animasi slide Water (pegas ala carousel): angka/bar/ring menghitung naik, batang tumbuh bertahap,
/// knob toggle bergeser, tombol menyusut saat ditekan, riak dari ikon tetes saat Drink.
pub(crate) struct WaterUi {
    entered: bool,
    enter_t: Instant,
    shown: f32,
    vel: f32,
    knob: f32,
    kvel: f32,
    bars: [f32; N_BARS],
    bvel: [f32; N_BARS],
    press: Option<(Instant, u8)>,
    ripple: Option<Instant>,
    bell: Option<Instant>,
    view: u8,
    view_t0: Instant,
    view_dir: f32,
}

impl WaterUi {
    pub(crate) fn new() -> WaterUi {
        let old = Instant::now() - Duration::from_secs(10);
        WaterUi {
            entered: false,
            enter_t: old,
            shown: 0.0,
            vel: 0.0,
            knob: 0.0,
            kvel: 0.0,
            bars: [0.0; N_BARS],
            bvel: [0.0; N_BARS],
            press: None,
            ripple: None,
            bell: None,
            view: 0,
            view_t0: old,
            view_dir: 1.0,
        }
    }
}

fn ss(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn spring(x: &mut f32, v: &mut f32, target: f32, k: f32, c: f32, dt: f32) {
    let n = ((dt / 0.008).ceil() as i32).max(1);
    let sdt = dt / n as f32;
    for _ in 0..n {
        *v += (k * (target - *x) - c * *v) * sdt;
        *x += *v * sdt;
    }
}

fn fmt_l(ml: f32) -> String {
    let s = format!("{:.2}", ml / 1000.0);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() { "0".to_string() } else { s.to_string() }
}

impl App {
    fn water_visible(&self) -> bool {
        self.expanded() && self.cur() == 1 && self.h > 100.0
    }

    pub(crate) fn water_view(&mut self, v: u8) {
        if self.wui.view != v {
            self.wui.view_dir = if v == 1 { 1.0 } else { -1.0 };
            self.wui.view = v;
            self.wui.view_t0 = Instant::now();
        }
    }

    fn water_bar_targets(&self) -> [f32; N_BARS] {
        let start = Water::window_start(now_tm().wHour as usize);
        let max = self.water.axis_max(start) as f32;
        let mut t = [0.0; N_BARS];
        for i in 0..N_BARS {
            t[i] = self.water.hours[start + i] as f32 / max;
        }
        t
    }

    /// Maju satu langkah animasi Water (dipanggil tiap frame dari `tick`).
    pub(crate) fn water_ui_step(&mut self, dt: f32) {
        if !self.water_visible() {
            // Slide tidak terlihat: reset supaya animasi masuk berjalan lagi saat dibuka.
            let u = &mut self.wui;
            u.entered = false;
            u.view = 0;
            u.press = None;
            u.shown = 0.0;
            u.vel = 0.0;
            u.knob = if self.water.remind_on() { 1.0 } else { 0.0 };
            u.kvel = 0.0;
            u.bars = [0.0; N_BARS];
            u.bvel = [0.0; N_BARS];
            return;
        }
        let targets = self.water_bar_targets();
        let (ml, on) = (self.water.ml as f32, self.water.remind_on());
        let u = &mut self.wui;
        if !u.entered {
            u.entered = true;
            u.enter_t = Instant::now();
            u.shown = 0.0;
            u.knob = if on { 1.0 } else { 0.0 };
        }
        spring(&mut u.shown, &mut u.vel, ml, 190.0, 24.0, dt);
        spring(&mut u.knob, &mut u.kvel, if on { 1.0 } else { 0.0 }, 380.0, 30.0, dt);
        let el = u.enter_t.elapsed().as_secs_f32();
        for i in 0..N_BARS {
            if el >= i as f32 * 0.03 {
                spring(&mut u.bars[i], &mut u.bvel[i], targets[i], 260.0, 22.0, dt);
            }
        }
        for slot in [&mut u.press] {
            if slot.map_or(false, |(t, _)| t.elapsed().as_secs_f32() > 0.35) {
                *slot = None;
            }
        }
        if u.ripple.map_or(false, |t| t.elapsed().as_secs_f32() > 0.8) {
            u.ripple = None;
        }
        if u.bell.map_or(false, |t| t.elapsed().as_secs_f32() > 0.7) {
            u.bell = None;
        }
    }

    /// True selama ada animasi Water yang masih bergerak (menahan timer 16 ms).
    pub(crate) fn water_animating(&self) -> bool {
        if !self.water_visible() {
            return false;
        }
        let u = &self.wui;
        let targets = self.water_bar_targets();
        let on = if self.water.remind_on() { 1.0 } else { 0.0 };
        (u.shown - self.water.ml as f32).abs() > 1.0
            || u.vel.abs() > 2.0
            || (u.knob - on).abs() > 0.01
            || u.kvel.abs() > 0.05
            || u.bars.iter().zip(targets.iter()).any(|(b, t)| (b - t).abs() > 0.004)
            || u.bvel.iter().any(|v| v.abs() > 0.02)
            || u.press.is_some()
            || u.ripple.is_some()
            || u.bell.is_some()
            || !u.entered
            || u.view_t0.elapsed().as_secs_f32() < 0.3
    }

    pub(crate) fn water_add(&mut self, d: i32) {
        self.water.add(d, now_tm().wHour as usize);
        self.reminding = false;
        self.peek_until = None;
    }

    pub(crate) fn water_snooze(&mut self) {
        self.water.snooze();
        self.reminding = false;
        self.peek_until = None;
    }

    /// Klik pada slide Water (koordinat kanvas).
    pub(crate) fn water_click(&mut self, x: f32, y: f32) {
        let hit = |r: (f32, f32, f32, f32)| in_rect(x, y, r.0, r.1, r.2, r.3);
        if self.wui.view == 1 {
            if hit(BACK) {
                self.water_view(0);
                return;
            }
            for (row, ry) in [(0usize, 38.0f32), (1, 70.0), (2, 102.0)] {
                for i in 0..4 {
                    let (cx, cw) = (LX + 84.0 + i as f32 * 62.0, 56.0);
                    if in_rect(x, y, cx, ry, cw, 24.0) {
                        match row {
                            0 => self.water.goal = GOALS[i],
                            1 => {
                                self.water.remind_min = REMINDS[i];
                                if REMINDS[i] > 0 {
                                    self.water.last_remind = REMINDS[i];
                                    self.water.reset_timer();
                                }
                            }
                            _ => self.water.glass = GLASSES[i],
                        }
                        self.water.save();
                        return;
                    }
                }
            }
            return;
        }
        if hit(TOGGLE) {
            self.water.toggle_remind();
            if self.water.remind_on() {
                self.wui.bell = Some(Instant::now());
            }
        } else if hit(DRINK) {
            self.wui.press = Some((Instant::now(), 0));
            self.wui.ripple = Some(Instant::now());
            let g = self.water.glass as i32;
            self.water_add(g);
        } else if hit(SIDE) {
            if self.reminding {
                self.water_snooze();
            } else {
                self.wui.press = Some((Instant::now(), 1));
                self.water_view(1);
            }
        }
    }

    // ------------------------------------------------------------ gambar
    pub(crate) fn slide_water(&self, g: &mut Gfx, ox: f32, al: f32) {
        // Transisi Main <-> Settings: konten masuk bergeser 16 px + fade.
        let k = ss(0.0, 0.24, self.wui.view_t0.elapsed().as_secs_f32());
        let vx = ox + (1.0 - k) * 16.0 * self.wui.view_dir;
        let va = al * k;
        self.water_header(g, ox, al);
        if self.wui.view == 1 {
            self.water_settings(g, vx, va);
        } else {
            self.water_main(g, vx, va, al);
        }
    }

    fn water_header(&self, g: &mut Gfx, ox: f32, al: f32) {
        let wt = &self.water;
        let y = 10.0;
        g.txtb("Water", LX + ox, y, 15.0, [255, 255, 255], al, 120.0);
        let w1 = g.width_ex("Water ", 15.0, true);
        g.txtb("Reminder", LX + ox + w1, y, 15.0, BLUE, al, 120.0);
        if self.wui.view == 1 {
            let k = ss(0.0, 0.24, self.wui.view_t0.elapsed().as_secs_f32());
            g.pill(BACK.0 + ox, BACK.1, BACK.2, BACK.3, [255, 255, 255], 0.12 * al * k, 1.0);
            g.txt_c("Back", BACK.0 + ox + BACK.2 / 2.0, BACK.1 + ty(BACK.3, 12.0), 12.0, [220, 225, 235], al * k, BACK.2, true);
            return;
        }
        // Ring progres kecil + tanggal + jam, atau "Time to drink" saat reminder aktif.
        let rx = LX + ox + w1 + g.width_ex("Reminder", 15.0, true) + 22.0;
        let frac = if wt.goal > 0 { (self.wui.shown / wt.goal as f32).clamp(0.0, 1.0) } else { 0.0 };
        let rc = if wt.ml >= wt.goal { GREEN } else { BLUE };
        g.ring(rx, y + 9.5, 8.0, frac, 2.6, &paint(255, 255, 255, 0.14 * al), &paint(rc[0], rc[1], rc[2], al));
        let tx = rx + 16.0;
        if self.reminding {
            g.txtb("Time to drink", tx, y + 1.0, 13.0, BLUE, al, 140.0);
        } else {
            let st = now_tm();
            let date = format!("{} {} {} {}", DAYS_S[(st.wDayOfWeek as usize) % 7], st.wDay, MONTHS[((st.wMonth as usize).max(1) - 1) % 12], st.wYear);
            g.txt(&date, tx, y + 1.0, 13.0, [225, 225, 230], al, 150.0);
            let dw = g.width(&date, 13.0);
            g.txtb(&format!("{:02}:{:02}", st.wHour, st.wMinute), tx + dw + 8.0, y + 1.0, 13.0, [255, 255, 255], al, 60.0);
        }
    }

    fn water_main(&self, g: &mut Gfx, ox: f32, al: f32, hal: f32) {
        let wt = &self.water;
        let shown = self.wui.shown.max(0.0);
        // Angka besar + target.
        let big = fmt_l(shown);
        g.txtb(&big, LX + ox, 36.0, 42.0, [255, 255, 255], al, 120.0);
        let bw = g.width_ex(&big, 42.0, true);
        g.txt(&format!("/ {} L", fmt_l(wt.goal as f32)), LX + ox + bw + 6.0, 56.0, 14.0, GREY, al, 80.0);

        // Toggle reminder (+ goyang bel saat dinyalakan).
        let swing = self.wui.bell.map_or(0.0, |t| {
            let e = t.elapsed().as_secs_f32();
            (e * 22.0).sin() * (1.0 - e / 0.7).max(0.0)
        });
        g.toggle(TOGGLE.0 + ox, TOGGLE.1, TOGGLE.2, TOGGLE.3, self.wui.knob, swing, al);

        // "Today · 100% · Goal reached"
        let pct = if wt.goal > 0 { (wt.ml * 100 / wt.goal).min(999) } else { 0 };
        let sy = 84.0;
        let part1 = "Today \u{b7} ";
        g.txt(part1, LX + ox, sy, 12.0, GREY, al, 120.0);
        let mut x = LX + ox + g.width(part1, 12.0);
        let pc = format!("{}%", pct);
        g.txtb(&pc, x, sy, 12.0, BLUE, al, 80.0);
        x += g.width_ex(&pc, 12.0, true);
        let rest = if wt.ml >= wt.goal {
            " \u{b7} Goal reached".to_string()
        } else {
            format!(" \u{b7} {} L to go", fmt_l((wt.goal - wt.ml) as f32))
        };
        g.txt(&rest, x, sy, 12.0, GREY, al, 140.0);

        // Progress bar gradasi.
        let (bx, by, bwid) = (LX + ox, 102.0, 150.0);
        g.fill(&rrect(bx, by, bwid, 7.0, 3.5), &paint(255, 255, 255, 0.12 * al));
        let frac = if wt.goal > 0 { (shown / wt.goal as f32).clamp(0.0, 1.0) } else { 0.0 };
        if frac > 0.005 {
            let fw = (bwid * frac).max(7.0);
            g.fill(&rrect(bx, by, fw, 7.0, 3.5), &hgrad(bx, bx + bwid, [120, 205, 255], BLUE, al));
        }

        // Tombol Drink (+ efek tekan + riak dari ikon tetes) dan Settings / Snooze.
        let press = |which: u8| -> f32 {
            match self.wui.press {
                Some((t, w)) if w == which => 1.0 - 0.05 * (1.0 - ss(0.0, 0.3, t.elapsed().as_secs_f32())) * (t.elapsed().as_secs_f32() * 14.0).cos().abs(),
                _ => 1.0,
            }
        };
        let (dx, dy, dw, dh) = (DRINK.0 + ox, DRINK.1, DRINK.2, DRINK.3);
        g.pill(dx, dy, dw, dh, [30, 90, 170], 0.85 * al, press(0));
        let (ix, iy) = (dx + 22.0, dy + dh / 2.0);
        if let Some(t) = self.wui.ripple {
            let e = t.elapsed().as_secs_f32() / 0.8;
            let r = 6.0 + 20.0 * e;
            let p = paint(BLUE[0], BLUE[1], BLUE[2], (1.0 - e).max(0.0) * 0.55 * al);
            g.ring(ix, iy, r, 0.0, 1.5, &p, &p);
        }
        g.icon_drop(ix, iy, 14.0, &paint(120, 200, 255, al));
        g.txtb("Drink", dx + 36.0, dy + ty(dh, 14.0), 14.0, [220, 238, 255], al, 60.0);

        let (sx, sy2, sw, sh) = (SIDE.0 + ox, SIDE.1, SIDE.2, SIDE.3);
        if self.reminding {
            g.pill(sx, sy2, sw, sh, [255, 255, 255], 0.14 * al, press(1));
            g.txt_c("Snooze 10 min", sx + sw / 2.0, sy2 + ty(sh, 13.0), 13.0, [225, 230, 240], al, sw, true);
        } else {
            g.pill(sx, sy2, sw, sh, [255, 255, 255], 0.10 * al, press(1));
            g.icon_sliders(sx + 20.0, sy2 + sh / 2.0, 14.0, &paint(225, 225, 232, al));
            g.txtb("Settings", sx + 36.0, sy2 + ty(sh, 13.0), 13.0, [225, 225, 232], al, 64.0);
        }

        // Garis pemisah + grafik per jam.
        g.rect(DIV_X + ox, 46.0, 1.0, 104.0, &paint(255, 255, 255, 0.12 * hal));
        self.water_chart(g, ox, hal);
    }

    fn water_chart(&self, g: &mut Gfx, ox: f32, al: f32) {
        let wt = &self.water;
        let now_h = now_tm().wHour as usize;
        let start = Water::window_start(now_h);
        let max = wt.axis_max(start) as f32;
        let (pl, pr) = (PLOT_L + ox, PLOT_R + ox);
        let gap = 6.0;
        let bw = (pr - pl - gap * (N_BARS as f32 - 1.0)) / N_BARS as f32;
        let ph = PLOT_B - PLOT_T;

        // Garis bantu putus-putus + label sumbu (0, tengah, atas).
        for (frac, label) in [(1.0f32, format!("{}", max as u32)), (0.5, format!("{}", (max / 2.0) as u32)), (0.0, "0 ml".to_string())] {
            let y = PLOT_B - ph * frac;
            if frac > 0.0 {
                let mut x = pl;
                while x < pr {
                    g.rect(x, y, 3.0, 1.0, &paint(255, 255, 255, 0.16 * al));
                    x += 6.0;
                }
            }
            g.txt(&label, pr + 8.0, y - 6.0, 10.0, GREY, al, 40.0);
        }
        // Batang: latar redup penuh + isi bergradasi yang tumbuh dari dasar.
        for i in 0..N_BARS {
            let x = pl + i as f32 * (bw + gap);
            g.fill(&rrect(x, PLOT_T, bw, ph, bw / 2.0), &paint(255, 255, 255, 0.06 * al));
            let f = self.wui.bars[i].clamp(0.0, 1.05);
            let target = wt.hours[start + i];
            if target > 0 || f > 0.003 {
                let hgt = (ph * f).max(if target > 0 { bw } else { 0.0 }).min(ph);
                if hgt > 0.5 {
                    let top = PLOT_B - hgt;
                    g.fill(&rrect(x, top, bw, hgt, bw / 2.0), &vgrad(top, PLOT_B, [110, 205, 255], [30, 110, 235], al));
                }
            }
            if i % 3 == 0 {
                g.txt_c(&format!("{:02}", start + i), x + bw / 2.0, PLOT_B + 6.0, 10.0, GREY, al, 24.0, false);
            }
        }
    }

    fn water_settings(&self, g: &mut Gfx, ox: f32, al: f32) {
        let wt = &self.water;
        let rows: [(&str, f32, [String; 4], usize); 3] = [
            ("Daily goal", 38.0, GOALS.map(|v| format!("{} L", fmt_l(v as f32))), GOALS.iter().position(|&v| v == wt.goal).unwrap_or(9)),
            ("Reminder", 70.0, REMINDS.map(|v| if v == 0 { "Off".to_string() } else { format!("{} min", v) }), REMINDS.iter().position(|&v| v == wt.remind_min).unwrap_or(9)),
            ("Glass", 102.0, GLASSES.map(|v| format!("{} ml", v)), GLASSES.iter().position(|&v| v == wt.glass).unwrap_or(9)),
        ];
        for (label, y, chips, sel) in rows {
            g.txt(label, LX + ox, y + ty(24.0, 12.0), 12.0, GREY, al, 80.0);
            for (i, c) in chips.iter().enumerate() {
                g.chip(LX + 84.0 + i as f32 * 62.0 + ox, y, 56.0, 24.0, c, i == sel, al);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_old_format() {
        let w = Water::parse("2026-10-24|750|2500|90\n", "2026-10-24");
        assert_eq!((w.ml, w.goal, w.remind_min, w.glass), (750, 2500, 90, 250));
        assert_eq!(w.hours.iter().sum::<u32>(), 0);
    }

    #[test]
    fn roundtrips_new_format() {
        let mut w = Water::parse("2026-10-24|0|2000|60", "2026-10-24");
        w.add(250, 9);
        w.add(500, 14);
        w.glass = 330;
        let r = Water::parse(&w.serialize(), "2026-10-24");
        assert_eq!((r.ml, r.glass, r.hours[9], r.hours[14]), (750, 330, 250, 500));
    }

    #[test]
    fn other_day_resets_ml_and_hours() {
        let w = Water::parse("2026-10-23|900|2000|60|250|0,0,0,0,0,0,0,0,0,900", "2026-10-24");
        assert_eq!((w.ml, w.hours[9]), (0, 0));
        assert_eq!(w.goal, 2000);
    }

    #[test]
    fn remove_never_goes_negative() {
        let mut w = Water::parse("2026-10-24|0|2000|60", "2026-10-24");
        w.add(250, 10);
        w.add(-250, 10);
        w.add(-250, 10);
        assert_eq!((w.ml, w.hours[10]), (0, 0));
    }

    #[test]
    fn chart_window_and_axis() {
        assert_eq!(Water::window_start(2), 0);
        assert_eq!(Water::window_start(14), 9);
        assert_eq!(Water::window_start(23), 16);
        let mut w = Water::parse("2026-10-24|0|2000|60", "2026-10-24");
        assert_eq!(w.axis_max(9), 400);
        w.hours[10] = 700;
        assert_eq!(w.axis_max(9), 800);
    }

    #[test]
    fn toggle_restores_last_interval() {
        let mut w = Water::parse("2026-10-24|0|2000|90", "2026-10-24");
        w.toggle_remind();
        assert_eq!(w.remind_min, 0);
        w.toggle_remind();
        assert_eq!(w.remind_min, 90);
    }
}
