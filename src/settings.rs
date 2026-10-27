//! Pengaturan aplikasi (`settings.txt`, baris `kunci=nilai`). Kompatibel dengan berkas lama yang hanya berisi
//! `minimal=` dan `notifications=`; kunci tak dikenal dibuang saat menyimpan.

use super::*;
use crate::slides::{self, SlideCfg};

/// Pilihan "Idle display" di Settings.
pub(crate) const IDLE_DISPLAY_LABELS: [&str; 3] = ["Cycle", "Clock", "Eyes"];

#[derive(Clone, PartialEq, Debug)]
pub(crate) struct Settings {
    /// Mode "Minimal notch": garis tipis saat tidak ada musik.
    pub(crate) minimal: bool,
    /// Tampilkan notifikasi Windows di notch.
    pub(crate) notifications: bool,
    /// Jendela maximized (bukan hanya layar penuh) membuat notch menyusut jadi garis tipis.
    pub(crate) busy_on_maximized: bool,
    /// Baris lirik ditampilkan sekian ms lebih awal dari timestamp (negatif = lebih lambat).
    pub(crate) lyric_lead_ms: i32,
    /// Foto profil kontak (mis. WhatsApp) pada notifikasi bila tersedia.
    pub(crate) notif_avatar: bool,
    /// Toast saat Caps/Num/Scroll Lock ditekan.
    pub(crate) lock_toast: bool,
    /// Titik + toast saat aplikasi memakai mikrofon/kamera.
    pub(crate) privacy_dot: bool,
    /// Ctrl+Alt+1..8 open the Nth enabled slide (hotkeys.rs).
    pub(crate) slide_hotkeys: bool,
    /// HUD saat volume / kecerahan berubah dari luar (tombol keyboard, mixer).
    pub(crate) hud_volume: bool,
    pub(crate) hud_brightness: bool,
    /// Cuaca di kiri jam (suhu dan ikon kondisi bergantian).
    pub(crate) weather_on: bool,
    /// Monitor tempat notch tampil: 0 = utama, 1 = ikuti kursor, n >= 2 = monitor ke-(n-1).
    pub(crate) monitor: usize,
    /// Indeks di `weather::CITIES`.
    pub(crate) weather_city: usize,
    /// Indeks di `scale::SIZES`: 0 = Auto (ikut scaling Windows), selebihnya ukuran tetap.
    pub(crate) notch_size: usize,
    /// Tampilan notch idle: 0 = bergantian jam/mata (10/15 menit), 1 = jam saja, 2 = mata saja.
    pub(crate) idle_display: usize,
    pub(crate) slides: SlideCfg,
}

pub(crate) const LEAD_MIN: i32 = -500;
pub(crate) const LEAD_MAX: i32 = 1000;
pub(crate) const LEAD_STEP: i32 = 50;

impl Default for Settings {
    fn default() -> Self {
        Settings {
            minimal: false,
            notifications: true,
            busy_on_maximized: false,
            lyric_lead_ms: 150,
            notif_avatar: true,
            lock_toast: true,
            privacy_dot: true,
            slide_hotkeys: true,
            hud_volume: true,
            hud_brightness: true,
            weather_on: true,
            monitor: 0,
            weather_city: 0,
            notch_size: 0,
            idle_display: 0,
            slides: slides::default_cfg(),
        }
    }
}

impl Settings {
    pub(crate) fn parse(text: &str) -> Settings {
        let mut s = Settings::default();
        let mut slides_val: Option<&str> = None;
        for line in text.lines() {
            let Some((k, v)) = line.trim().split_once('=') else { continue };
            let (k, v) = (k.trim(), v.trim());
            let flag = v == "1";
            match k {
                "minimal" => s.minimal = flag,
                "notifications" => s.notifications = flag,
                "busy_on_maximized" => s.busy_on_maximized = flag,
                "notif_avatar" => s.notif_avatar = flag,
                "lock_toast" => s.lock_toast = flag,
                "privacy_dot" => s.privacy_dot = flag,
                "slide_hotkeys" => s.slide_hotkeys = flag,
                "hud_volume" => s.hud_volume = flag,
                "hud_brightness" => s.hud_brightness = flag,
                "weather" => s.weather_on = flag,
                "monitor" => {
                    if let Ok(n) = v.parse::<usize>() {
                        s.monitor = n.min(32);
                    }
                }
                "weather_city" => {
                    if let Ok(n) = v.parse::<usize>() {
                        s.weather_city = n.min(weather::CITIES.len() - 1);
                    }
                }
                "idle_display" => {
                    if let Ok(n) = v.parse::<usize>() {
                        s.idle_display = n.min(IDLE_DISPLAY_LABELS.len() - 1);
                    }
                }
                "notch_size" => {
                    if let Ok(n) = v.parse::<usize>() {
                        s.notch_size = n.min(scale::SIZES.len() - 1);
                    }
                }
                "lyric_lead_ms" => {
                    if let Ok(n) = v.parse::<i32>() {
                        s.lyric_lead_ms = n.clamp(LEAD_MIN, LEAD_MAX);
                    }
                }
                "slides" => slides_val = Some(v),
                _ => {}
            }
        }
        if let Some(v) = slides_val {
            s.slides = slides::parse_slides(v);
        }
        s
    }

    pub(crate) fn serialize(&self) -> String {
        format!(
            "minimal={}\nnotifications={}\nbusy_on_maximized={}\nlyric_lead_ms={}\nnotif_avatar={}\nlock_toast={}\nprivacy_dot={}\nslide_hotkeys={}\nhud_volume={}\nhud_brightness={}\nweather={}\nweather_city={}\nmonitor={}\nnotch_size={}\nidle_display={}\nslides={}\n",
            self.minimal as u8,
            self.notifications as u8,
            self.busy_on_maximized as u8,
            self.lyric_lead_ms,
            self.notif_avatar as u8,
            self.lock_toast as u8,
            self.privacy_dot as u8,
            self.slide_hotkeys as u8,
            self.hud_volume as u8,
            self.hud_brightness as u8,
            self.weather_on as u8,
            self.weather_city,
            self.monitor,
            self.notch_size,
            self.idle_display,
            slides::format_slides(&self.slides),
        )
    }

    pub(crate) fn load() -> Settings {
        std::fs::read_to_string(data_path("settings.txt")).map(|t| Settings::parse(&t)).unwrap_or_default()
    }

    pub(crate) fn save(&self) {
        if cfg!(test) {
            return; // tes tidak boleh menimpa pengaturan asli
        }
        write_data("settings.txt", self.serialize());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slides::SlideId;

    #[test]
    fn legacy_file_keeps_defaults_for_new_keys() {
        let s = Settings::parse("minimal=1\nnotifications=0\n");
        assert!(s.minimal);
        assert!(!s.notifications);
        assert!(!s.busy_on_maximized);
        assert_eq!(s.lyric_lead_ms, 150);
        assert_eq!(s.slides.len(), SlideId::USER.len());
    }

    #[test]
    fn roundtrip_and_clamp() {
        let mut s = Settings::default();
        s.minimal = true;
        s.busy_on_maximized = true;
        s.lyric_lead_ms = -200;
        s.slides = slides::parse_slides("scrap,!music,water,todo,ledge,notifs");
        assert_eq!(Settings::parse(&s.serialize()), s);
        assert_eq!(Settings::parse("lyric_lead_ms=99999").lyric_lead_ms, LEAD_MAX);
        assert_eq!(Settings::parse("lyric_lead_ms=-99999").lyric_lead_ms, LEAD_MIN);
        assert_eq!(Settings::parse("lyric_lead_ms=abc").lyric_lead_ms, 150);
        assert_eq!(s.slides[1], (SlideId::Music, false));
    }

    #[test]
    fn notch_size_roundtrip_and_default() {
        assert_eq!(Settings::parse("minimal=1
").notch_size, 0, "berkas lama = Auto");
        let mut s = Settings::default();
        s.notch_size = 3;
        assert_eq!(Settings::parse(&s.serialize()).notch_size, 3);
        assert_eq!(Settings::parse("notch_size=99").notch_size, scale::SIZES.len() - 1);
        assert_eq!(Settings::parse("notch_size=x").notch_size, 0);
    }

    #[test]
    fn slide_hotkeys_roundtrip_and_default() {
        assert!(Settings::parse("minimal=1\n").slide_hotkeys, "old file = on");
        let mut s = Settings::default();
        s.slide_hotkeys = false;
        assert!(!Settings::parse(&s.serialize()).slide_hotkeys);
    }

    #[test]
    fn idle_display_roundtrip_and_default() {
        assert_eq!(Settings::default().idle_display, 0);
        let mut s = Settings::default();
        s.idle_display = 2;
        assert_eq!(Settings::parse(&s.serialize()).idle_display, 2);
        assert_eq!(Settings::parse("idle_display=9").idle_display, IDLE_DISPLAY_LABELS.len() - 1);
        assert_eq!(Settings::parse("idle_display=x").idle_display, 0);
    }

    #[test]
    fn garbage_lines_are_ignored() {
        let s = Settings::parse("???\n=1\nminimal\nslides=\n");
        assert_eq!(s, Settings::default());
    }
}
