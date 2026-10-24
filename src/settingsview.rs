//! Slide Settings: tiga tab (Slides, Display, Features). Toggle berpegas, urutan slide bisa digeser dengan tombol
//! naik/turun (dua sel bertukar tempat dengan animasi), pengaturan tersimpan langsung ke `settings.txt`.

use super::*;
use crate::settings::{LEAD_MAX, LEAD_MIN, LEAD_STEP};
use crate::slides::SlideId;

const TAB_Y: f32 = 8.0;
const TAB_H: f32 = 24.0;
const TABS: [(&str, f32); 3] = [("Slides", 62.0), ("Display", 66.0), ("Features", 70.0)];
const TAB_X0: f32 = LX + 112.0;
const GRID_Y0: f32 = 42.0;
const CELL_W: f32 = 178.0;
const CELL_H: f32 = 34.0;
const GRID_GAP_X: f32 = 8.0;
const GRID_GAP_Y: f32 = 4.0;
const GREY: [u8; 3] = [150, 150, 158];
const BLUE: [u8; 3] = [90, 170, 255];
const MAX_ROWS: usize = 16;

pub(crate) struct SettingsUi {
    pub(crate) tab: u8,
    tab_t0: Instant,
    tab_dir: f32,
    knob: [f32; MAX_ROWS],
    kvel: [f32; MAX_ROWS],
    entered: bool,
    enter_t: Instant,
    /// Dua indeks slide yang baru bertukar tempat (animasi geser).
    swap: Option<(usize, usize, Instant)>,
    /// Status autostart di-cache (registry dibaca saat tab Features dibuka, bukan tiap frame).
    autostart: bool,
}

impl SettingsUi {
    pub(crate) fn new() -> SettingsUi {
        let old = Instant::now() - Duration::from_secs(10);
        SettingsUi { tab: 0, tab_t0: old, tab_dir: 1.0, knob: [0.0; MAX_ROWS], kvel: [0.0; MAX_ROWS], entered: false, enter_t: old, swap: None, autostart: false }
    }
}

fn ss(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn cell_pos(i: usize) -> (f32, f32) {
    let (col, row) = (i / 3, i % 3);
    (LX + col as f32 * (CELL_W + GRID_GAP_X), GRID_Y0 + row as f32 * (CELL_H + GRID_GAP_Y))
}

/// Satu baris pengaturan pada tab Display/Features.
pub(crate) enum RowKind {
    Toggle(bool),
    /// Teks nilai + tombol - dan +.
    Stepper(String),
}

pub(crate) struct Row {
    pub(crate) label: &'static str,
    pub(crate) sub: &'static str,
    pub(crate) kind: RowKind,
}

impl App {
    fn settings_visible(&self) -> bool {
        self.expanded() && self.is_slide(SlideId::Settings) && self.h > 100.0
    }

    /// Daftar baris tab Display (1) atau Features (2).
    pub(crate) fn settings_rows(&self, tab: u8) -> Vec<Row> {
        match tab {
            1 => vec![
                Row { label: "Minimal notch", sub: "Thin line while idle", kind: RowKind::Toggle(self.minimal) },
                Row { label: "Thin on maximize", sub: "Off: only on fullscreen", kind: RowKind::Toggle(self.cfg.busy_on_maximized) },
                Row { label: "Lyric timing", sub: "Show earlier by", kind: RowKind::Stepper(format!("{:+} ms", self.cfg.lyric_lead_ms)) },
                Row { label: "Contact photos", sub: "In notifications", kind: RowKind::Toggle(self.cfg.notif_avatar) },
            ],
            2 => vec![
                Row { label: "Notifications", sub: "Windows toasts + bell", kind: RowKind::Toggle(self.notif_on) },
                Row { label: "Autostart", sub: "Start with Windows", kind: RowKind::Toggle(self.sui.autostart) },
                Row { label: "Lock keys", sub: "Caps / Num / Scroll", kind: RowKind::Toggle(self.cfg.lock_toast) },
                Row { label: "Mic / camera dot", sub: "When an app uses them", kind: RowKind::Toggle(self.cfg.privacy_dot) },
            ],
            _ => Vec::new(),
        }
    }

    fn settings_row_target(&self, i: usize) -> f32 {
        match self.sui.tab {
            0 => {
                if self.cfg.slides.get(i).map_or(false, |(_, on)| *on) { 1.0 } else { 0.0 }
            }
            t => match self.settings_rows(t).get(i).map(|r| &r.kind) {
                Some(RowKind::Toggle(true)) => 1.0,
                _ => 0.0,
            },
        }
    }

    pub(crate) fn save_settings(&mut self) {
        self.cfg.minimal = self.minimal;
        self.cfg.notifications = self.notif_on;
        self.cfg.save();
    }

    /// Terapkan perubahan konfigurasi slide: bangun ulang urutan dan tetap berada di slide Settings.
    pub(crate) fn apply_slide_cfg(&mut self) {
        let cur = self.cur_id();
        self.order = slides::visible_order(&self.cfg.slides);
        let idx = self.order.iter().position(|&s| s == cur).unwrap_or(self.order.len() - 1);
        self.slide = idx as i32;
        self.sp = idx as f32;
        self.sv = 0.0;
    }

    fn settings_set_tab(&mut self, t: u8) {
        if self.sui.tab != t {
            self.sui.tab_dir = if t > self.sui.tab { 1.0 } else { -1.0 };
            self.sui.tab = t;
            self.sui.tab_t0 = Instant::now();
            if t == 2 {
                self.sui.autostart = sys::autostart_enabled();
            }
        }
    }

    /// Satu langkah animasi (dipanggil tiap frame dari `tick`).
    pub(crate) fn settings_ui_step(&mut self, dt: f32) {
        if !self.settings_visible() {
            self.sui.entered = false;
            self.sui.swap = None;
            // snap knob ke keadaan sebenarnya supaya animasi masuk berikutnya mulai dari nilai yang benar
            for i in 0..MAX_ROWS {
                self.sui.knob[i] = self.settings_row_target(i);
                self.sui.kvel[i] = 0.0;
            }
            return;
        }
        if !self.sui.entered {
            self.sui.entered = true;
            self.sui.enter_t = Instant::now();
        }
        let n = ((dt / 0.008).ceil() as i32).max(1);
        let sdt = dt / n as f32;
        for i in 0..MAX_ROWS {
            let target = self.settings_row_target(i);
            let (mut x, mut v) = (self.sui.knob[i], self.sui.kvel[i]);
            for _ in 0..n {
                v += (380.0 * (target - x) - 30.0 * v) * sdt;
                x += v * sdt;
            }
            self.sui.knob[i] = x;
            self.sui.kvel[i] = v;
        }
        if self.sui.swap.map_or(false, |(_, _, t)| t.elapsed().as_secs_f32() > 0.28) {
            self.sui.swap = None;
        }
    }

    pub(crate) fn settings_animating(&self) -> bool {
        if !self.settings_visible() {
            return false;
        }
        (0..MAX_ROWS).any(|i| (self.sui.knob[i] - self.settings_row_target(i)).abs() > 0.01 || self.sui.kvel[i].abs() > 0.05)
            || self.sui.swap.is_some()
            || self.sui.tab_t0.elapsed().as_secs_f32() < 0.3
            || self.sui.enter_t.elapsed().as_secs_f32() < 0.5
    }

    // ------------------------------------------------------------ klik
    pub(crate) fn settings_click(&mut self, x: f32, y: f32) {
        // tab
        let mut tx = TAB_X0;
        for (i, (_, w)) in TABS.iter().enumerate() {
            if in_rect(x, y, tx, TAB_Y, *w, TAB_H) {
                self.settings_set_tab(i as u8);
                return;
            }
            tx += w + 6.0;
        }
        match self.sui.tab {
            0 => self.settings_click_slides(x, y),
            t => self.settings_click_rows(t, x, y),
        }
    }

    fn settings_click_slides(&mut self, x: f32, y: f32) {
        for i in 0..self.cfg.slides.len() {
            let (cx, cy) = cell_pos(i);
            if !in_rect(x, y, cx, cy, CELL_W, CELL_H) {
                continue;
            }
            // tombol naik / turun / toggle (kanan ke kiri di dalam sel)
            let (tgx, upx, dnx) = (cx + CELL_W - 46.0, cx + CELL_W - 92.0, cx + CELL_W - 68.0);
            if in_rect(x, y, tgx, cy, 44.0, CELL_H) {
                self.cfg.slides[i].1 = !self.cfg.slides[i].1;
                self.slides_changed();
            } else if in_rect(x, y, upx, cy, 22.0, CELL_H) && i > 0 {
                self.cfg.slides.swap(i, i - 1);
                self.sui.swap = Some((i - 1, i, Instant::now()));
                self.slides_changed();
            } else if in_rect(x, y, dnx, cy, 22.0, CELL_H) && i + 1 < self.cfg.slides.len() {
                self.cfg.slides.swap(i, i + 1);
                self.sui.swap = Some((i, i + 1, Instant::now()));
                self.slides_changed();
            }
            return;
        }
    }

    fn slides_changed(&mut self) {
        self.apply_slide_cfg();
        self.save_settings();
    }

    fn settings_click_rows(&mut self, tab: u8, x: f32, y: f32) {
        let n = self.settings_rows(tab).len();
        for i in 0..n {
            let (cx, cy) = cell_pos(i);
            if !in_rect(x, y, cx, cy, CELL_W, CELL_H) {
                continue;
            }
            // bagian kanan sel: toggle (44 px) atau stepper (- di kiri, + di kanan)
            let right = x > cx + CELL_W - 92.0;
            let plus = x > cx + CELL_W - 46.0;
            match (tab, i) {
                (1, 0) => self.minimal = !self.minimal,
                (1, 1) => self.cfg.busy_on_maximized = !self.cfg.busy_on_maximized,
                (1, 2) => {
                    if right {
                        let d = if plus { LEAD_STEP } else { -LEAD_STEP };
                        self.cfg.lyric_lead_ms = (self.cfg.lyric_lead_ms + d).clamp(LEAD_MIN, LEAD_MAX);
                    }
                }
                (1, 3) => {
                    self.cfg.notif_avatar = !self.cfg.notif_avatar;
                    notif::set_avatar(self.cfg.notif_avatar);
                }
                (2, 0) => {
                    self.notif_on = !self.notif_on;
                    notif::set_enabled(self.notif_on);
                    if self.notif_on {
                        if !self.notif_started {
                            self.notif_started = true;
                            notif::start(self.hwnd, self.notif_slot.clone());
                        }
                    } else {
                        self.notifs.clear();
                    }
                }
                (2, 1) => {
                    sys::set_autostart(!self.sui.autostart);
                    self.sui.autostart = sys::autostart_enabled();
                }
                (2, 2) => {
                    self.cfg.lock_toast = !self.cfg.lock_toast;
                    keys::set_enabled(self.hwnd, self.cfg.lock_toast);
                }
                (2, 3) => {
                    self.cfg.privacy_dot = !self.cfg.privacy_dot;
                    privacy::set_enabled(self.cfg.privacy_dot);
                    self.priv_t = Instant::now();
                }
                _ => {}
            }
            self.save_settings();
            return;
        }
    }

    // ------------------------------------------------------------ gambar
    pub(crate) fn slide_settings(&self, g: &mut Gfx, ox: f32, al: f32) {
        g.txtb("Settings", LX + ox, 10.0, 15.0, [255, 255, 255], al, 100.0);
        let mut tx = TAB_X0 + ox;
        for (i, (label, w)) in TABS.iter().enumerate() {
            g.chip(tx, TAB_Y, *w, TAB_H, label, i as u8 == self.sui.tab, al);
            tx += w + 6.0;
        }

        // konten tab: geser 16 px + fade saat ganti tab; baris masuk bertahap saat slide dibuka
        let k = ss(0.0, 0.26, self.sui.tab_t0.elapsed().as_secs_f32());
        let (vx, va) = (ox + (1.0 - k) * 16.0 * self.sui.tab_dir, al * k);
        let el = self.sui.enter_t.elapsed().as_secs_f32();
        match self.sui.tab {
            0 => self.draw_slides_tab(g, vx, va, el),
            t => self.draw_rows_tab(g, t, vx, va, el),
        }
    }

    fn draw_cell_bg(g: &mut Gfx, x: f32, y: f32, a: f32) {
        g.fill(&rrect(x, y, CELL_W, CELL_H, 11.0), &paint(255, 255, 255, 0.07 * a));
    }

    fn draw_slides_tab(&self, g: &mut Gfx, ox: f32, al: f32, el: f32) {
        let n = self.cfg.slides.len();
        for i in 0..n {
            let (id, on) = self.cfg.slides[i];
            let (mut x, mut y) = cell_pos(i);
            // animasi tukar tempat: sel yang baru pindah meluncur dari posisi lamanya
            if let Some((a, b, t)) = self.sui.swap {
                let e = ss(0.0, 0.26, t.elapsed().as_secs_f32());
                let from = if i == a { Some(cell_pos(b)) } else if i == b { Some(cell_pos(a)) } else { None };
                if let Some((fx, fy)) = from {
                    x = fx + (x - fx) * e;
                    y = fy + (y - fy) * e;
                }
            }
            let enter = ss(0.0, 0.3, el - i as f32 * 0.04);
            let a = al * enter;
            let (x, y) = (x + ox, y + (1.0 - enter) * 8.0);
            Self::draw_cell_bg(g, x, y, a);
            let lab = if on { [255, 255, 255] } else { [120, 120, 128] };
            g.txtb(id.short_label(), x + 12.0, y + ty(CELL_H, 12.0), 12.0, lab, a, CELL_W - 108.0);
            let dim = |ok: bool| paint(220, 220, 228, if ok { 0.9 } else { 0.25 } * a);
            // chevron naik / turun
            let (ux, dx, cy) = (x + CELL_W - 81.0, x + CELL_W - 57.0, y + CELL_H / 2.0);
            let (up, dn) = (dim(i > 0), dim(i + 1 < n));
            g.line(ux - 4.0, cy + 2.0, ux, cy - 2.0, 1.6, &up);
            g.line(ux, cy - 2.0, ux + 4.0, cy + 2.0, 1.6, &up);
            g.line(dx - 4.0, cy - 2.0, dx, cy + 2.0, 1.6, &dn);
            g.line(dx, cy + 2.0, dx + 4.0, cy - 2.0, 1.6, &dn);
            g.toggle(x + CELL_W - 44.0, y + (CELL_H - 20.0) / 2.0, 34.0, 20.0, self.sui.knob[i], 0.0, a);
        }
    }

    fn draw_rows_tab(&self, g: &mut Gfx, tab: u8, ox: f32, al: f32, el: f32) {
        for (i, r) in self.settings_rows(tab).iter().enumerate() {
            let (x, y) = cell_pos(i);
            let enter = ss(0.0, 0.3, el - i as f32 * 0.04);
            let a = al * enter;
            let (x, y) = (x + ox, y + (1.0 - enter) * 8.0);
            Self::draw_cell_bg(g, x, y, a);
            g.txtb(r.label, x + 12.0, y + 4.0, 12.0, [255, 255, 255], a, CELL_W - 62.0);
            g.txt(r.sub, x + 12.0, y + 19.0, 10.0, GREY, a, CELL_W - 62.0);
            match &r.kind {
                RowKind::Toggle(_) => g.toggle(x + CELL_W - 44.0, y + (CELL_H - 20.0) / 2.0, 34.0, 20.0, self.sui.knob[i], 0.0, a),
                RowKind::Stepper(v) => {
                    let cy = y + CELL_H / 2.0;
                    let (mx, px) = (x + CELL_W - 82.0, x + CELL_W - 18.0);
                    g.fill(&rrect(mx - 9.0, cy - 9.0, 18.0, 18.0, 9.0), &paint(255, 255, 255, 0.12 * a));
                    g.fill(&rrect(px - 9.0, cy - 9.0, 18.0, 18.0, 9.0), &paint(255, 255, 255, 0.12 * a));
                    let w = paint(235, 235, 240, a);
                    g.line(mx - 3.5, cy, mx + 3.5, cy, 1.6, &w);
                    g.line(px - 3.5, cy, px + 3.5, cy, 1.6, &w);
                    g.line(px, cy - 3.5, px, cy + 3.5, 1.6, &w);
                    g.txt_c(v, (mx + px) / 2.0, cy - 6.0, 11.0, [BLUE[0], BLUE[1], BLUE[2]], a, 50.0, true);
                }
            }
        }
    }
}
