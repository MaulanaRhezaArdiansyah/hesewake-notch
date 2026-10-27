//! Slide Timer (default tersembunyi): stopwatch dan hitung mundur dengan preset. Saat berjalan di notch kecil yang idle,
//! sisa/lama waktunya tampil kecil di samping jam; hitung mundur yang habis memunculkan toast "Time's up".

use super::*;

const BLUE: [u8; 3] = [90, 170, 255];
const GREEN: [u8; 3] = [60, 200, 110];
const GREY: [u8; 3] = TEXT_DIM;

/// Preset hitung mundur (detik): 1, 5, 10, 15, 25, 45 menit.
pub(crate) const PRESETS: [u32; 6] = [60, 300, 600, 900, 1500, 2700];
const MIN_PRESET: u32 = 60;
const MAX_PRESET: u32 = 3 * 3600;

// Tata letak notch idle saat timer berjalan (px). Isi disusun sebagai satu deret dengan jarak sama:
// [margin][cuaca][jam][pil][margin atau lonceng]; lebar notch mengikuti isi supaya kiri-kanan seimbang.
const PAD: f32 = 18.0;
const GAP: f32 = 12.0;
const CLOCK_W: f32 = 38.0;
const WEATHER_W: f32 = 24.0;
const PILL_W: f32 = 62.0;
const PILL_W_HOURS: f32 = 80.0;
/// Ruang di kanan pil bila ada lonceng (ikon 16 px + jarak).
const BELL_PAD: f32 = 36.0;
/// Fase mata: mata tetap di tengah, pil di kanannya pada jarak ini dari tengah.
const EYE_GAP: f32 = 24.0;

/// Posisi elemen idle saat timer berjalan, relatif ke tepi kiri notch (fase jam) atau ke tengah (fase mata).
pub(crate) struct IdleSlots {
    /// Lebar isi (tanpa slot titik mic/kamera): notch idle dilebarkan sampai ini bila lebih lebar dari `IDLE`.
    pub(crate) raw: f32,
    /// Fase jam: (pusat ikon cuaca bila tampil, pusat jam, awal pil), semuanya dari tepi kiri notch.
    clock: Option<(Option<f32>, f32, f32)>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum TMode {
    Stopwatch,
    Countdown,
}

/// Status timer. Satu mekanisme untuk kedua mode: `elapsed` = waktu terkumpul + waktu sejak mulai.
pub(crate) struct TimerState {
    pub(crate) mode: TMode,
    /// Durasi hitung mundur (detik).
    pub(crate) preset: u32,
    base: Duration,
    started: Option<Instant>,
    /// Detik yang terakhir ditampilkan; berubah = perlu redraw.
    shown: u64,
}

impl TimerState {
    pub(crate) fn new(mode: TMode, preset: u32) -> TimerState {
        TimerState { mode, preset: preset.clamp(MIN_PRESET, MAX_PRESET), base: Duration::ZERO, started: None, shown: u64::MAX }
    }

    /// Format berkas: `mode|preset_detik` (`s` = stopwatch, `c` = hitung mundur). Isi rusak -> bawaan (hitung mundur 5 menit).
    pub(crate) fn parse(s: &str) -> TimerState {
        let mut it = s.trim().split('|');
        let mode = if it.next() == Some("s") { TMode::Stopwatch } else { TMode::Countdown };
        let preset = it.next().and_then(|v| v.parse().ok()).unwrap_or(300);
        TimerState::new(mode, preset)
    }

    pub(crate) fn serialize(&self) -> String {
        format!("{}|{}\n", if self.mode == TMode::Stopwatch { "s" } else { "c" }, self.preset)
    }

    pub(crate) fn load() -> TimerState {
        TimerState::parse(&std::fs::read_to_string(data_path("timer.txt")).unwrap_or_default())
    }

    pub(crate) fn save(&self) {
        if cfg!(test) {
            return; // tes tidak boleh menimpa data asli di %APPDATA%
        }
        write_data("timer.txt", self.serialize());
    }

    pub(crate) fn running(&self) -> bool {
        self.started.is_some()
    }

    fn total(&self) -> Duration {
        Duration::from_secs(self.preset as u64)
    }

    /// Waktu yang sudah berjalan; pada hitung mundur dibatasi sampai durasinya.
    pub(crate) fn elapsed(&self, now: Instant) -> Duration {
        let e = self.base + self.started.map_or(Duration::ZERO, |s| now.saturating_duration_since(s));
        if self.mode == TMode::Countdown { e.min(self.total()) } else { e }
    }

    pub(crate) fn remaining(&self, now: Instant) -> Duration {
        self.total().saturating_sub(self.elapsed(now))
    }

    /// Hitung mundur sudah habis (mode stopwatch tidak pernah habis).
    pub(crate) fn finished(&self, now: Instant) -> bool {
        self.mode == TMode::Countdown && self.elapsed(now) >= self.total()
    }

    /// Detik yang ditampilkan: sisa (dibulatkan ke atas) untuk hitung mundur, lama berjalan untuk stopwatch.
    pub(crate) fn display_secs(&self, now: Instant) -> u64 {
        match self.mode {
            TMode::Countdown => {
                let r = self.remaining(now);
                r.as_secs() + (r.subsec_nanos() > 0) as u64
            }
            TMode::Stopwatch => self.elapsed(now).as_secs(),
        }
    }

    pub(crate) fn start(&mut self, now: Instant) {
        if self.finished(now) {
            self.reset();
        }
        if self.started.is_none() {
            self.started = Some(now);
        }
    }

    pub(crate) fn pause(&mut self, now: Instant) {
        if let Some(s) = self.started.take() {
            self.base += now.saturating_duration_since(s);
        }
    }

    pub(crate) fn reset(&mut self) {
        self.base = Duration::ZERO;
        self.started = None;
        self.shown = u64::MAX;
    }

    pub(crate) fn set_mode(&mut self, m: TMode) {
        if self.mode != m {
            self.mode = m;
            self.reset();
        }
    }

    pub(crate) fn set_preset(&mut self, secs: u32) {
        self.preset = secs.clamp(MIN_PRESET, MAX_PRESET);
        self.reset();
    }

    /// Geser durasi `delta` detik (kelipatan menit), dibatasi 1 menit sampai 3 jam.
    pub(crate) fn adjust(&mut self, delta: i32) {
        self.set_preset((self.preset as i64 + delta as i64).clamp(MIN_PRESET as i64, MAX_PRESET as i64) as u32);
    }

    /// Hitung mundur habis: hentikan dan kunci di nol.
    fn finish(&mut self) {
        self.base = self.total();
        self.started = None;
    }
}

/// "04:59" atau "1:02:03" untuk 3600 detik ke atas.
pub(crate) fn fmt_clock(secs: u64) -> String {
    let (h, m, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m:02}:{s:02}") }
}

/// "5 min", "1 h", "1 h 30 min".
pub(crate) fn fmt_preset(secs: u32) -> String {
    let (h, m) = (secs / 3600, (secs % 3600) / 60);
    match (h, m) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    }
}

// ---------------------------------------------------------------- tata letak slide (koordinat kanvas)
#[derive(Clone, Copy, PartialEq, Debug)]
enum THit {
    Mode(TMode),
    Toggle,
    Reset,
    Preset(usize),
    Minus,
    Plus,
}

const MODE_Y: f32 = 6.0;
const MODE_W: f32 = 84.0;
const TOGGLE: (f32, f32, f32, f32) = (LX, 116.0, 96.0, 32.0);
const RESET: (f32, f32, f32, f32) = (LX + 104.0, 116.0, 84.0, 32.0);
const PX: f32 = LX + 216.0;
const PRESET_W: f32 = 58.0;
const PRESET_H: f32 = 26.0;
const ADJ_W: f32 = 90.0;

fn t_hits(mode: TMode) -> Vec<(THit, (f32, f32, f32, f32))> {
    let mut v = vec![
        (THit::Mode(TMode::Stopwatch), (RX - 2.0 * MODE_W - 6.0, MODE_Y, MODE_W, 22.0)),
        (THit::Mode(TMode::Countdown), (RX - MODE_W, MODE_Y, MODE_W, 22.0)),
        (THit::Toggle, TOGGLE),
        (THit::Reset, RESET),
    ];
    if mode == TMode::Countdown {
        for i in 0..PRESETS.len() {
            let (c, r) = ((i % 3) as f32, (i / 3) as f32);
            v.push((THit::Preset(i), (PX + c * (PRESET_W + 7.0), 40.0 + r * (PRESET_H + 6.0), PRESET_W, PRESET_H)));
        }
        v.push((THit::Minus, (PX, 106.0, ADJ_W, PRESET_H)));
        v.push((THit::Plus, (PX + ADJ_W + 8.0, 106.0, ADJ_W, PRESET_H)));
    }
    v
}

/// Susunan murni: fase mata / jam, lebar slot cuaca (None = tidak tampil), lonceng tampil, pil lebar (jam >= 1 j).
/// Slot cuaca melebar saat ikut menampilkan hitung mundur sholat.
fn idle_slots(eye: bool, weather: Option<f32>, bell: bool, wide: bool) -> IdleSlots {
    let pw = if wide { PILL_W_HOURS } else { PILL_W };
    let right = if bell { BELL_PAD } else { PAD };
    if eye {
        return IdleSlots { raw: 2.0 * (EYE_GAP + pw + right), clock: None };
    }
    let mut x = PAD;
    let weather_c = weather.map(|ww| {
        let c = x + ww / 2.0;
        x += ww + GAP;
        c
    });
    let clock_c = x + CLOCK_W / 2.0;
    x += CLOCK_W + GAP;
    IdleSlots { raw: x + pw + right, clock: Some((weather_c, clock_c, x)) }
}

impl App {
    fn timer_visible(&self) -> bool {
        self.expanded() && self.is_slide(SlideId::Timer) && self.h > 100.0
    }

    /// Slide terbuka dan timer berjalan: butuh frame terus-menerus (angka/ring bergerak).
    pub(crate) fn timer_animating(&self) -> bool {
        self.timer_visible() && self.tm.running()
    }

    /// Pil timer tampil di notch idle: timer berjalan dan notch kecil dalam keadaan diam.
    fn timer_chip_active(&self) -> bool {
        self.tm.running() && !self.expanded() && self.toast.is_none() && !self.thin() && !self.media.playing && self.splash_t().is_none()
    }

    /// Susunan notch idle saat timer berjalan; None bila pil tidak tampil.
    pub(crate) fn timer_slots(&self) -> Option<IdleSlots> {
        if !self.timer_chip_active() {
            return None;
        }
        let wide = self.tm.display_secs(Instant::now()) >= 3600;
        let weather = self.weather_shown().then(|| if self.prayer_countdown().is_some() { prayer::LABEL_W } else { WEATHER_W });
        Some(idle_slots(self.idle_phase() == IdlePhase::Eye, weather, self.bell_pos().is_some(), wide))
    }

    /// Tambahan lebar notch idle saat timer berjalan (di atas `IDLE`; slot titik mic/kamera ditambah terpisah).
    pub(crate) fn timer_extra(&self) -> f32 {
        self.timer_slots().map_or(0.0, |s| (s.raw - IDLE.0).max(0.0))
    }

    /// Pusat jam di notch idle (bergeser ke kiri saat pil timer tampil di sebelah kanannya).
    pub(crate) fn idle_clock_cx(&self, w: f32) -> f32 {
        match self.timer_slots().and_then(|s| s.clock) {
            Some((_, c, _)) => CX - w / 2.0 + c,
            None => CX,
        }
    }

    /// Pusat ikon/suhu cuaca (dan hitung mundur sholat) di notch idle.
    pub(crate) fn idle_weather_cx(&self, w: f32) -> f32 {
        match self.timer_slots().and_then(|s| s.clock).and_then(|(wc, _, _)| wc) {
            Some(c) => CX - w / 2.0 + c,
            // Tanpa timer, hitung mundur sholat melebarkan notch: slot di tengah antara tepi kiri dan jam.
            None if self.prayer_idle_extra() > 0.0 => (CX - w / 2.0 + 12.0 + CX - 24.0) / 2.0,
            None => CX - w / 2.0 + 32.0,
        }
    }

    /// Dipanggil tiap poll: selesaikan hitung mundur (toast) dan minta redraw saat detik yang tampil berubah.
    pub(crate) fn timer_poll(&mut self) -> bool {
        let now = Instant::now();
        if self.tm.running() && self.tm.finished(now) {
            let label = format!("{} timer", fmt_preset(self.tm.preset));
            self.tm.finish();
            self.push_toast(toast::ToastKind::Timer { label });
            return true;
        }
        let s = self.tm.display_secs(now);
        if s != self.tm.shown {
            self.tm.shown = s;
            return self.tm.running();
        }
        false
    }

    pub(crate) fn timer_click(&mut self, x: f32, y: f32) {
        let now = Instant::now();
        let Some((h, _)) = t_hits(self.tm.mode).into_iter().find(|(_, r)| in_rect(x, y, r.0, r.1, r.2, r.3)) else { return };
        let running = self.tm.running();
        match h {
            THit::Mode(m) => self.tm.set_mode(m),
            THit::Toggle => {
                if running {
                    self.tm.pause(now);
                } else {
                    self.tm.start(now);
                }
            }
            THit::Reset => self.tm.reset(),
            // durasi hanya bisa diubah saat tidak berjalan
            THit::Preset(i) if !running => self.tm.set_preset(PRESETS[i]),
            THit::Minus if !running => self.tm.adjust(-60),
            THit::Plus if !running => self.tm.adjust(60),
            _ => return,
        }
        self.tm.save();
    }

    pub(crate) fn slide_timer(&self, g: &mut Gfx, ox: f32, al: f32) {
        let tm = &self.tm;
        let now = Instant::now();
        let running = tm.running();
        let count = tm.mode == TMode::Countdown;
        g.txtb("Timer", LX + ox, 10.0, 15.0, [255, 255, 255], al, 100.0);
        for (h, r) in t_hits(tm.mode) {
            match h {
                THit::Mode(m) => g.chip(r.0 + ox, r.1, r.2, r.3, if m == TMode::Stopwatch { "Stopwatch" } else { "Countdown" }, m == tm.mode, al),
                THit::Toggle => {
                    let label = if running { "Pause" } else if tm.elapsed(now) > Duration::ZERO && !tm.finished(now) { "Resume" } else { "Start" };
                    g.pill(r.0 + ox, r.1, r.2, r.3, if running { [255, 255, 255] } else { [30, 90, 170] }, if running { 0.14 } else { 0.85 } * al, 1.0);
                    g.txt_c(label, r.0 + ox + r.2 / 2.0, r.1 + ty(r.3, 14.0), 14.0, [225, 238, 255], al, r.2, true);
                }
                THit::Reset => {
                    g.pill(r.0 + ox, r.1, r.2, r.3, [255, 255, 255], 0.10 * al, 1.0);
                    g.txt_c("Reset", r.0 + ox + r.2 / 2.0, r.1 + ty(r.3, 13.0), 13.0, [225, 228, 236], al, r.2, true);
                }
                THit::Preset(i) => {
                    let label = format!("{} m", PRESETS[i] / 60);
                    g.chip(r.0 + ox, r.1, r.2, r.3, &label, PRESETS[i] == tm.preset, if running { al * 0.45 } else { al });
                }
                THit::Minus | THit::Plus => {
                    let label = if h == THit::Minus { "\u{2212} 1 min" } else { "+ 1 min" };
                    g.pill(r.0 + ox, r.1, r.2, r.3, [255, 255, 255], 0.10 * al, 1.0);
                    g.txt_c(label, r.0 + ox + r.2 / 2.0, r.1 + ty(r.3, 12.0), 12.0, [225, 228, 236], if running { al * 0.45 } else { al }, r.2, true);
                }
            }
        }

        // Angka besar (+ persepuluh detik untuk stopwatch) dan keterangan.
        let secs = tm.display_secs(now);
        let done = count && tm.finished(now);
        let col = if done { GREEN } else { [255, 255, 255] };
        let big = fmt_clock(secs);
        g.txtb(&big, LX + ox, 36.0, 44.0, col, al, 215.0);
        if !count {
            let w = g.width_ex(&big, 44.0, true);
            let tenth = tm.elapsed(now).subsec_millis() / 100;
            g.txtb(&format!(".{tenth}"), LX + ox + w + 2.0, 54.0, 20.0, GREY, al, 40.0);
        }
        let sub = if count {
            if done { "Time's up".to_string() } else { format!("Countdown \u{b7} {}", fmt_preset(tm.preset)) }
        } else {
            "Stopwatch".to_string()
        };
        g.txt(&sub, LX + ox, 92.0, 12.0, if done { GREEN } else { GREY }, al, 200.0);

        // Stopwatch: ring yang menyapu sekali tiap menit di sisi kanan.
        if !count {
            let (cx, cy) = (PX + 94.0 + ox, 90.0);
            let frac = (tm.elapsed(now).as_secs_f32() % 60.0) / 60.0;
            g.ring(cx, cy, 36.0, frac, 5.0, &paint(255, 255, 255, 0.10 * al), &paint(BLUE[0], BLUE[1], BLUE[2], al));
            g.txt_c(&format!("{:02}", tm.elapsed(now).as_secs() % 60), cx, cy - 10.0, 20.0, [235, 238, 245], al, 60.0, true);
        }
    }

    /// Pil waktu di kanan jam saat timer berjalan dan notch idle: ring progres + waktu, biru (tidak menyerupai mata).
    pub(crate) fn draw_timer_chip(&self, g: &mut Gfx, w: f32, h: f32, ca: f32) {
        if self.timer_extra() <= 0.0 || ca <= 0.05 {
            return;
        }
        let now = Instant::now();
        let text = fmt_clock(self.tm.display_secs(now));
        let frac = match self.tm.mode {
            TMode::Countdown => self.tm.remaining(now).as_secs_f32() / self.tm.preset.max(1) as f32,
            TMode::Stopwatch => (self.tm.elapsed(now).as_secs_f32() % 60.0) / 60.0,
        };
        let tw = g.width_ex(&text, 12.0, true);
        let x = match self.timer_slots().and_then(|s| s.clock) {
            Some((_, _, px)) => CX - w / 2.0 + px,
            None => CX + EYE_GAP,
        };
        let (pw, ph) = (24.0 + tw + 8.0, 20.0);
        g.fill(&rrect(x, h / 2.0 - ph / 2.0, pw, ph, ph / 2.0), &paint(40, 90, 170, 0.22 * ca));
        g.ring(x + 12.0, h / 2.0, 5.5, frac.clamp(0.0, 1.0), 2.0, &paint(255, 255, 255, 0.18 * ca), &paint(BLUE[0], BLUE[1], BLUE[2], ca));
        g.txtb(&text, x + 24.0, ty(h, 12.0), 12.0, BLUE, ca, tw + 4.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(base: Instant, ms: u64) -> Instant {
        base + Duration::from_millis(ms)
    }

    #[test]
    fn stopwatch_accumulates_across_pauses() {
        let t0 = Instant::now();
        let mut t = TimerState::new(TMode::Stopwatch, 300);
        t.start(at(t0, 0));
        assert!(t.running());
        t.pause(at(t0, 2500));
        assert_eq!(t.elapsed(at(t0, 9000)), Duration::from_millis(2500)); // berhenti saat dijeda
        t.start(at(t0, 10_000));
        assert_eq!(t.elapsed(at(t0, 11_000)), Duration::from_millis(3500));
        assert_eq!(t.display_secs(at(t0, 11_000)), 3);
        t.reset();
        assert!(!t.running() && t.elapsed(at(t0, 20_000)) == Duration::ZERO);
    }

    #[test]
    fn countdown_finishes_and_restarts() {
        let t0 = Instant::now();
        let mut t = TimerState::new(TMode::Countdown, 60);
        t.start(t0);
        assert_eq!(t.display_secs(at(t0, 0)), 60);
        assert_eq!(t.display_secs(at(t0, 1)), 60); // dibulatkan ke atas
        assert_eq!(t.display_secs(at(t0, 59_001)), 1);
        assert!(!t.finished(at(t0, 59_999)) && t.finished(at(t0, 60_000)));
        // elapsed tidak melewati durasi walau poll terlambat
        assert_eq!(t.remaining(at(t0, 90_000)), Duration::ZERO);
        t.finish();
        assert!(!t.running() && t.finished(at(t0, 100_000)) && t.display_secs(at(t0, 100_000)) == 0);
        // mulai lagi setelah habis: dari awal
        t.start(at(t0, 100_000));
        assert!(t.running() && !t.finished(at(t0, 100_000)) && t.display_secs(at(t0, 100_000)) == 60);
    }

    #[test]
    fn presets_clamp_and_reset_progress() {
        let t0 = Instant::now();
        let mut t = TimerState::new(TMode::Countdown, 300);
        t.start(t0);
        t.pause(at(t0, 5000));
        t.set_preset(600);
        assert_eq!((t.preset, t.elapsed(at(t0, 6000))), (600, Duration::ZERO));
        t.adjust(-60);
        assert_eq!(t.preset, 540);
        t.adjust(-100_000);
        assert_eq!(t.preset, MIN_PRESET);
        t.adjust(1_000_000);
        assert_eq!(t.preset, MAX_PRESET);
        t.set_preset(5);
        assert_eq!(t.preset, MIN_PRESET);
    }

    #[test]
    fn switching_mode_resets() {
        let t0 = Instant::now();
        let mut t = TimerState::new(TMode::Countdown, 300);
        t.start(t0);
        t.set_mode(TMode::Countdown); // mode sama: tidak mengganggu
        assert!(t.running());
        t.set_mode(TMode::Stopwatch);
        assert!(!t.running() && t.elapsed(at(t0, 1000)) == Duration::ZERO);
    }

    #[test]
    fn parse_serialize_and_garbage() {
        let t = TimerState::parse("s|900\n");
        assert_eq!((t.mode, t.preset), (TMode::Stopwatch, 900));
        let r = TimerState::parse(&t.serialize());
        assert_eq!((r.mode, r.preset), (TMode::Stopwatch, 900));
        let d = TimerState::parse("???");
        assert_eq!((d.mode, d.preset), (TMode::Countdown, 300));
        assert_eq!(TimerState::parse("c|99999999").preset, MAX_PRESET);
    }

    #[test]
    fn formats() {
        assert_eq!(fmt_clock(0), "00:00");
        assert_eq!(fmt_clock(299), "04:59");
        assert_eq!(fmt_clock(3723), "1:02:03");
        assert_eq!(fmt_preset(300), "5 min");
        assert_eq!(fmt_preset(3600), "1 h");
        assert_eq!(fmt_preset(5400), "1 h 30 min");
    }

    #[test]
    fn layout_fits_and_does_not_overlap() {
        for mode in [TMode::Stopwatch, TMode::Countdown] {
            let hits = t_hits(mode);
            for (i, (_, a)) in hits.iter().enumerate() {
                assert!(a.0 >= LX - 0.1 && a.0 + a.2 <= RX + 0.1 && a.1 >= 0.0 && a.1 + a.3 <= 152.0, "{a:?} di luar slide");
                for (_, b) in &hits[i + 1..] {
                    let apart = a.0 + a.2 <= b.0 || b.0 + b.2 <= a.0 || a.1 + a.3 <= b.1 || b.1 + b.3 <= a.1;
                    assert!(apart, "{a:?} menimpa {b:?}");
                }
            }
        }
    }

    #[test]
    fn idle_slots_are_evenly_spaced() {
        // cuaca + jam + pil: margin kiri = margin kanan, jarak antar elemen sama
        let s = idle_slots(false, Some(WEATHER_W), false, false);
        let (wc, cc, px) = s.clock.unwrap();
        let wc = wc.unwrap();
        assert!((wc - WEATHER_W / 2.0 - PAD).abs() < 0.01, "margin kiri");
        assert!(((cc - CLOCK_W / 2.0) - (wc + WEATHER_W / 2.0) - GAP).abs() < 0.01, "jarak cuaca-jam");
        assert!((px - (cc + CLOCK_W / 2.0) - GAP).abs() < 0.01, "jarak jam-pil");
        assert!((s.raw - (px + PILL_W) - PAD).abs() < 0.01, "margin kanan");
        // tanpa cuaca: jam menempel di margin kiri
        let n = idle_slots(false, None, false, false);
        let (w0, c0, _) = n.clock.unwrap();
        assert!(w0.is_none() && (c0 - CLOCK_W / 2.0 - PAD).abs() < 0.01);
        // lonceng menambah ruang kanan; pil jam lebih lebar
        assert!(idle_slots(false, Some(WEATHER_W), true, false).raw > s.raw);
        assert!(idle_slots(false, Some(WEATHER_W), false, true).raw > s.raw);
        // slot cuaca yang ikut menampilkan hitung mundur sholat: lebih lebar, jarak tetap sama
        let p = idle_slots(false, Some(72.0), false, false);
        let (pwc, pcc, ppx) = p.clock.unwrap();
        let pwc = pwc.unwrap();
        assert!((pwc - 36.0 - PAD).abs() < 0.01, "margin kiri slot lebar");
        assert!(((pcc - CLOCK_W / 2.0) - (pwc + 36.0) - GAP).abs() < 0.01, "jarak slot-jam");
        assert!((p.raw - (ppx + PILL_W) - PAD).abs() < 0.01, "margin kanan");
        assert!((p.raw - s.raw - (72.0 - WEATHER_W)).abs() < 0.01);
        // fase mata simetris terhadap tengah
        let e = idle_slots(true, None, false, false);
        assert!(e.clock.is_none() && (e.raw / 2.0 - (EYE_GAP + PILL_W + PAD)).abs() < 0.01);
    }
}
