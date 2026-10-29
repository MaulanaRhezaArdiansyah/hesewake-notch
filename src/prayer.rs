//! Prayer times: computed locally (no internet) with the Kemenag RI parameters for the city picked in Settings
//! (the same city as the weather). A 15-minute countdown in the small notch, and a toast when a prayer time starts.

use super::*;
use std::sync::OnceLock;
use windows::Win32::System::Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION};

/// The five prayers, in order from dawn.
pub(crate) const NAMES: [&str; 5] = ["Subuh", "Dzuhur", "Ashar", "Maghrib", "Isya"];
/// The countdown appears this many minutes before a prayer time.
pub(crate) const WINDOW_MIN: u16 = 15;

/// Prayer times on `y-m-d` at (lat, lon), in minutes since local midnight (`tz` = hours from UTC).
pub(crate) fn times(y: i32, m: u32, d: u32, lat: f64, lon: f64, tz: f64) -> [u16; 5] {
    let jd = julian(y, m, d) - lon / (15.0 * 24.0);
    // Initial guess (hours), then two passes using the sun's position at that time (the PrayTimes method).
    let mut h = [5.0, 12.0, 13.0, 18.0, 18.0];
    for _ in 0..2 {
        let at = |i: usize| jd + h[i] / 24.0;
        h = [
            angle_time(FAJR_DEG, at(0), lat, true),
            midday(at(1)),
            asr_time(at(2), lat),
            angle_time(SUNSET_DEG, at(3), lat, false),
            angle_time(ISHA_DEG, at(4), lat, false),
        ];
    }
    let mut out = [0u16; 5];
    for i in 0..5 {
        let local_h = h[i] + tz - lon / 15.0;
        let m = (local_h * 60.0 + IHTIYAT_MIN[i]).ceil();
        out[i] = m.rem_euclid(24.0 * 60.0) as u16;
    }
    out
}

// Kemenag RI parameters. The per-prayer ihtiyat (safety margin) is tuned to match the official Bimas Islam schedule (see tests).
const FAJR_DEG: f64 = 20.0;
const ISHA_DEG: f64 = 18.0;
/// Sunset: the upper limb touches the horizon, plus refraction.
const SUNSET_DEG: f64 = 0.833;
const IHTIYAT_MIN: [f64; 5] = [2.0, 3.0, 2.0, 3.0, 2.0];

fn julian(y: i32, m: u32, d: u32) -> f64 {
    let (mut y, mut m) = (y as f64, m as f64);
    if m <= 2.0 {
        y -= 1.0;
        m += 12.0;
    }
    let a = (y / 100.0).floor();
    let b = 2.0 - a + (a / 4.0).floor();
    (365.25 * (y + 4716.0)).floor() + (30.6001 * (m + 1.0)).floor() + d as f64 + b - 1524.5
}

/// Sun declination (degrees) and equation of time (hours) on Julian day `jd`.
fn sun(jd: f64) -> (f64, f64) {
    let dd = jd - 2451545.0;
    let g = (357.529 + 0.98560028 * dd).rem_euclid(360.0).to_radians();
    let q = (280.459 + 0.98564736 * dd).rem_euclid(360.0);
    let l = (q + 1.915 * g.sin() + 0.020 * (2.0 * g).sin()).rem_euclid(360.0).to_radians();
    let e = (23.439 - 0.00000036 * dd).to_radians();
    let ra = (e.cos() * l.sin()).atan2(l.cos()).to_degrees() / 15.0;
    let eqt = q / 15.0 - ra.rem_euclid(24.0);
    ((e.sin() * l.sin()).asin().to_degrees(), eqt)
}

/// Solar noon (hours, mean solar time at longitude 0).
fn midday(jd: f64) -> f64 {
    (12.0 - sun(jd).1).rem_euclid(24.0)
}

/// Time when the sun is `deg` degrees below the horizon, before (`ccw`) or after noon.
fn angle_time(deg: f64, jd: f64, lat: f64, ccw: bool) -> f64 {
    let decl = sun(jd).0.to_radians();
    let lat = lat.to_radians();
    let c = (-deg.to_radians().sin() - decl.sin() * lat.sin()) / (decl.cos() * lat.cos());
    let t = c.clamp(-1.0, 1.0).acos().to_degrees() / 15.0;
    midday(jd) + if ccw { -t } else { t }
}

/// Ashar (Shafi'i): shadow length = object length + the shadow at noon.
fn asr_time(jd: f64, lat: f64) -> f64 {
    let decl = sun(jd).0;
    let deg = -(1.0 / (1.0 + (lat - decl).abs().to_radians().tan())).atan().to_degrees();
    angle_time(deg, jd, lat, false)
}

/// The next prayer if it is `1..=WINDOW_MIN` minutes away: (index, minutes left).
pub(crate) fn countdown(now_min: u16, t: &[u16; 5]) -> Option<(usize, u16)> {
    t.iter().enumerate().find_map(|(i, &at)| {
        let left = at.checked_sub(now_min)?;
        (1..=WINDOW_MIN).contains(&left).then_some((i, left))
    })
}

/// The prayer that starts exactly at `now_min` (for the toast).
pub(crate) fn starting(now_min: u16, t: &[u16; 5]) -> Option<usize> {
    t.iter().position(|&at| at == now_min)
}

/// UTC offset in hours from the Windows bias (minutes, UTC = local + bias).
pub(crate) fn utc_offset_hours(bias: i32, daylight_bias: i32, daylight: bool) -> f64 {
    -((bias + if daylight { daylight_bias } else { 0 }) as f64) / 60.0
}

/// The current Windows time zone, so the schedule follows the laptop clock (WIB/WITA/WIT automatically).
pub(crate) fn local_utc_offset() -> f64 {
    let mut tz = TIME_ZONE_INFORMATION::default();
    // 2 = TIME_ZONE_ID_DAYLIGHT
    let state = unsafe { GetTimeZoneInformation(&mut tz) };
    utc_offset_hours(tz.Bias, tz.DaylightBias, state == 2)
}

/// Display width of the longest countdown text ("Maghrib 15m", 12 px).
pub(crate) const LABEL_W: f32 = 72.0;
/// Notch while music plays: room for the label right of the title/lyrics.
pub(crate) const PLAY_EXTRA: f32 = LABEL_W + 10.0;
pub(crate) const GOLD: [u8; 3] = [255, 204, 102];

/// `--demo-prayer=N`: force an "Ashar Nm" countdown (visual check).
fn demo() -> Option<u16> {
    static D: OnceLock<Option<u16>> = OnceLock::new();
    *D.get_or_init(|| std::env::args().find_map(|a| a.strip_prefix("--demo-prayer=").and_then(|v| v.parse().ok())))
}

fn now_min() -> u16 {
    let st = now_tm();
    st.wHour * 60 + st.wMinute
}

impl App {
    /// Today's schedule for the selected city; recomputed when the date or city changes.
    pub(crate) fn prayer_refresh(&mut self) {
        let st = now_tm();
        let key = ((st.wYear as u32 * 100 + st.wMonth as u32) * 100 + st.wDay as u32) * 100 + self.cfg.weather_city as u32;
        if self.prayer.map_or(true, |(k, _)| k != key) {
            let (_, lat, lon) = weather::CITIES[self.cfg.weather_city];
            let t = times(st.wYear as i32, st.wMonth as u32, st.wDay as u32, lat as f64, lon as f64, local_utc_offset());
            self.prayer = Some((key, t));
        }
    }

    /// Active countdown: (prayer index, minutes left).
    pub(crate) fn prayer_countdown(&self) -> Option<(usize, u16)> {
        if !self.cfg.prayer_on || self.thin() {
            return None;
        }
        if let Some(n) = demo() {
            return Some((2, n));
        }
        countdown(now_min(), &self.prayer?.1)
    }

    /// Extra idle-notch width while the countdown shows without a timer pill (layout in `prayer_slots`). When the
    /// timer pill shows, `timer_extra` already accounts for it (the slot left of the clock is widened there).
    pub(crate) fn prayer_idle_extra(&self) -> f32 {
        self.prayer_slots().map_or(0.0, |s| (s.raw - IDLE.0).max(0.0))
    }

    pub(crate) fn prayer_play_extra(&self) -> f32 {
        if self.countdown_on() { PLAY_EXTRA } else { 0.0 }
    }

    /// Called on every minute change: one toast when a prayer time starts. True if a toast was started.
    pub(crate) fn prayer_minute(&mut self) -> bool {
        if !self.cfg.prayer_on {
            return false;
        }
        self.prayer_refresh();
        let Some((key, t)) = self.prayer else { return false };
        let Some(i) = starting(now_min(), &t) else { return false };
        if self.prayer_fired == Some((key, i)) {
            return false;
        }
        self.prayer_fired = Some((key, i));
        self.push_toast(ToastKind::Prayer { idx: i as u8, at: t[i] });
        true
    }
}

pub(crate) fn hhmm(min: u16) -> String {
    format!("{:02}:{:02}", min / 60, min % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Official Kemenag schedule (api.myquran.com, Bimas Islam data): city, zone (hours), date, Subuh..Isya.
    const KEMENAG: &str = "\
Jakarta 7 2026-01-01 04:19 12:00 15:26 18:14 19:29
Jakarta 7 2026-06-21 04:40 11:58 15:19 17:51 19:05
Jakarta 7 2026-10-26 04:11 11:40 14:52 17:49 18:59
Surabaya 7 2026-01-01 03:53 11:36 15:02 17:52 19:08
Surabaya 7 2026-06-21 04:19 11:34 14:54 17:25 18:39
Surabaya 7 2026-10-26 03:46 11:17 14:27 17:26 18:37
Medan 7 2026-01-01 05:10 12:32 15:55 18:30 19:44
Medan 7 2026-06-21 04:55 12:31 15:56 18:40 19:55
Medan 7 2026-10-26 04:53 12:13 15:31 18:13 19:22
Makassar 8 2026-01-01 04:31 12:09 15:35 18:21 19:36
Makassar 8 2026-06-21 04:48 12:08 15:29 18:02 19:16
Makassar 8 2026-10-26 04:22 11:50 15:03 17:57 19:08
Jayapura 9 2026-01-01 04:11 11:44 15:10 17:52 19:06
Jayapura 9 2026-06-21 04:18 11:42 15:06 17:41 18:56
Jayapura 9 2026-10-26 03:59 11:25 14:40 17:30 18:40
Karawang 7 2026-01-01 04:17 11:58 15:24 18:12 19:27
Karawang 7 2026-06-21 04:39 11:56 15:17 17:49 19:03
Karawang 7 2026-10-26 04:09 11:38 14:50 17:47 18:58
Banda_Aceh 7 2026-01-01 05:27 12:46 16:07 18:40 19:54
Banda_Aceh 7 2026-06-21 05:04 12:44 16:10 18:57 20:13
Banda_Aceh 7 2026-10-26 05:08 12:26 15:45 18:24 19:34
Kupang 8 2026-01-01 04:03 11:53 15:18 18:14 19:30
Kupang 8 2026-06-21 04:40 11:51 15:08 17:37 18:51
Kupang 8 2026-10-26 03:59 11:33 14:40 17:45 18:57";

    fn city(name: &str) -> (f64, f64) {
        let name = name.replace('_', " ");
        let (_, lat, lon) = crate::weather::CITIES.iter().find(|c| c.0 == name).expect("city is in the weather list");
        (*lat as f64, *lon as f64)
    }

    fn mins(s: &str) -> u16 {
        let (h, m) = s.split_once(':').unwrap();
        h.parse::<u16>().unwrap() * 60 + m.parse::<u16>().unwrap()
    }

    #[test]
    fn matches_kemenag_within_one_minute() {
        for line in KEMENAG.lines() {
            let p: Vec<&str> = line.split_whitespace().collect();
            let (lat, lon) = city(p[0]);
            let tz: f64 = p[1].parse().unwrap();
            let ymd: Vec<u32> = p[2].split('-').map(|x| x.parse().unwrap()).collect();
            let got = times(ymd[0] as i32, ymd[1], ymd[2], lat, lon, tz);
            for i in 0..5 {
                let want = mins(p[3 + i]);
                let diff = got[i] as i32 - want as i32;
                assert!(diff.abs() <= 1, "{} {} {}: got {} expected {}", p[0], p[2], NAMES[i], hhmm(got[i]), p[3 + i]);
            }
        }
    }

    const T: [u16; 5] = [251, 700, 892, 1069, 1139]; // 04:11 11:40 14:52 17:49 18:59

    #[test]
    fn countdown_only_in_the_last_15_minutes() {
        assert_eq!(countdown(892 - 16, &T), None);
        assert_eq!(countdown(892 - 15, &T), Some((2, 15)));
        assert_eq!(countdown(892 - 1, &T), Some((2, 1)));
        assert_eq!(countdown(892, &T), None, "at the exact time: toast, not countdown");
        assert_eq!(countdown(251 - 5, &T), Some((0, 5)));
        assert_eq!(countdown(23 * 60, &T), None);
    }

    #[test]
    fn starting_matches_exact_minute() {
        assert_eq!(starting(1069, &T), Some(3));
        assert_eq!(starting(1070, &T), None);
        assert_eq!(starting(0, &T), None);
    }

    #[test]
    fn utc_offset_from_windows_bias() {
        // Windows bias = UTC - local (minutes). WIB = -420, WITA = -480, WIT = -540.
        assert_eq!(utc_offset_hours(-420, 0, false), 7.0);
        assert_eq!(utc_offset_hours(-480, 0, false), 8.0);
        assert_eq!(utc_offset_hours(-540, 0, false), 9.0);
        // zones with daylight saving: the extra bias is included
        assert_eq!(utc_offset_hours(-60, -60, true), 2.0);
        assert_eq!(utc_offset_hours(-60, -60, false), 1.0);
    }

    #[test]
    fn formats_clock() {
        assert_eq!(hhmm(251), "04:11");
        assert_eq!(hhmm(1139), "18:59");
    }
}
