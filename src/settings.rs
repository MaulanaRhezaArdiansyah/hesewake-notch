//! Pengaturan aplikasi (`settings.txt`, baris `kunci=nilai`). Kompatibel dengan berkas lama yang hanya berisi
//! `minimal=` dan `notifications=`; kunci tak dikenal dibuang saat menyimpan.

use super::*;
use crate::slides::{self, SlideCfg};

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
            "minimal={}\nnotifications={}\nbusy_on_maximized={}\nlyric_lead_ms={}\nnotif_avatar={}\nslides={}\n",
            self.minimal as u8,
            self.notifications as u8,
            self.busy_on_maximized as u8,
            self.lyric_lead_ms,
            self.notif_avatar as u8,
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
        assert_eq!(s.slides.len(), 6);
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
    fn garbage_lines_are_ignored() {
        let s = Settings::parse("???\n=1\nminimal\nslides=\n");
        assert_eq!(s, Settings::default());
    }
}
