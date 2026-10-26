//! Registri slide: identitas slide, urutan tampil, dan penyimpanannya di `settings.txt`.

/// Identitas slide. Slide baru ditambahkan di sini (dan di `USER`) agar bisa diatur dari Settings.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum SlideId {
    Music,
    Water,
    Todo,
    Ledge,
    Scrap,
    Notifications,
    /// CPU, memori, jaringan, baterai. Default tersembunyi.
    System,
    /// Stopwatch dan hitung mundur. Default tersembunyi.
    Timer,
    /// Hanya muncul (di akhir) selama dibuka lewat item menu "Settings"; tidak ada di carousel saat tertutup.
    Settings,
}

impl SlideId {
    /// Slide yang bisa diatur pengguna (urutan bawaan).
    pub(crate) const USER: [SlideId; 8] =
        [SlideId::Music, SlideId::Water, SlideId::Todo, SlideId::Ledge, SlideId::Scrap, SlideId::Notifications, SlideId::System, SlideId::Timer];

    /// Aktif secara bawaan? (slide opsional dimulai tersembunyi)
    pub(crate) fn default_on(self) -> bool {
        !matches!(self, SlideId::System | SlideId::Timer)
    }

    pub(crate) fn key(self) -> &'static str {
        match self {
            SlideId::Music => "music",
            SlideId::Water => "water",
            SlideId::Todo => "todo",
            SlideId::Ledge => "ledge",
            SlideId::Scrap => "scrap",
            SlideId::Notifications => "notifs",
            SlideId::System => "system",
            SlideId::Timer => "timer",
            SlideId::Settings => "settings",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            SlideId::Music => "Music",
            SlideId::Water => "Water",
            SlideId::Todo => "To-do",
            SlideId::Ledge => "Ledge",
            SlideId::Scrap => "Scrap",
            SlideId::Notifications => "Notifications",
            SlideId::System => "System",
            SlideId::Timer => "Timer",
            SlideId::Settings => "Settings",
        }
    }

    /// Nama ringkas untuk sel sempit di Settings.
    pub(crate) fn short_label(self) -> &'static str {
        match self {
            SlideId::Notifications => "Notifs",
            other => other.label(),
        }
    }

    pub(crate) fn from_key(k: &str) -> Option<SlideId> {
        SlideId::USER.iter().copied().find(|s| s.key() == k)
    }
}

/// Konfigurasi slide: urutan tampil + apakah aktif. Hanya slide `USER`; `Settings` ditambahkan otomatis.
pub(crate) type SlideCfg = Vec<(SlideId, bool)>;

pub(crate) fn default_cfg() -> SlideCfg {
    SlideId::USER.iter().map(|&s| (s, s.default_on())).collect()
}

/// `music,water,!todo,...` -> konfigurasi. Awalan `!` = tersembunyi. Id tak dikenal dan duplikat diabaikan; slide yang
/// belum disebut ditambahkan di akhir (aktif), supaya versi baru dengan slide baru tetap menampilkannya.
pub(crate) fn parse_slides(s: &str) -> SlideCfg {
    let mut out: SlideCfg = Vec::new();
    for tok in s.split(',') {
        let tok = tok.trim();
        let (on, key) = match tok.strip_prefix('!') {
            Some(k) => (false, k),
            None => (true, tok),
        };
        if let Some(id) = SlideId::from_key(key) {
            if !out.iter().any(|(s, _)| *s == id) {
                out.push((id, on));
            }
        }
    }
    for id in SlideId::USER {
        if !out.iter().any(|(s, _)| *s == id) {
            out.push((id, id.default_on()));
        }
    }
    out
}

pub(crate) fn format_slides(cfg: &[(SlideId, bool)]) -> String {
    cfg.iter().map(|(s, on)| if *on { s.key().to_string() } else { format!("!{}", s.key()) }).collect::<Vec<_>>().join(",")
}

/// Urutan slide yang benar-benar tampil: yang aktif; `Settings` hanya ada (di akhir) selama dibuka lewat menu.
/// Tanpa slide aktif dan Settings tertutup, slide pertama tetap ditampilkan agar carousel tidak kosong.
pub(crate) fn visible_order(cfg: &[(SlideId, bool)], settings_open: bool) -> Vec<SlideId> {
    let mut v: Vec<SlideId> = cfg.iter().filter(|(_, on)| *on).map(|(s, _)| *s).collect();
    if v.is_empty() && !settings_open {
        v.push(cfg.first().map_or(SlideId::Music, |(s, _)| *s));
    }
    if settings_open {
        v.push(SlideId::Settings);
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_order_and_hidden() {
        let c = parse_slides("water,!music,todo");
        assert_eq!(&c[..3], &[(SlideId::Water, true), (SlideId::Music, false), (SlideId::Todo, true)]);
        // slide yang tidak disebut ditambahkan di akhir
        assert_eq!(c.len(), SlideId::USER.len());
        assert_eq!(c[3], (SlideId::Ledge, true));
        // slide opsional yang baru muncul di versi baru tetap tersembunyi
        assert_eq!(&c[c.len() - 2..], &[(SlideId::System, false), (SlideId::Timer, false)]);
    }

    #[test]
    fn ignores_unknown_and_duplicates() {
        let c = parse_slides("music,bogus,music,!music,settings");
        assert_eq!(c.iter().filter(|(s, _)| *s == SlideId::Music).count(), 1);
        assert!(c.iter().all(|(s, _)| *s != SlideId::Settings));
        assert_eq!(c.len(), SlideId::USER.len());
    }

    #[test]
    fn roundtrips() {
        let c = parse_slides("scrap,!ledge,music,water,todo,notifs");
        assert_eq!(parse_slides(&format_slides(&c)), c);
        assert_eq!(format_slides(&c), "scrap,!ledge,music,water,todo,notifs,!system,!timer");
    }

    #[test]
    fn settings_only_when_open() {
        let c = parse_slides("!music,water");
        let closed = visible_order(&c, false);
        assert!(!closed.contains(&SlideId::Settings));
        assert!(!closed.contains(&SlideId::Music));
        assert_eq!(closed[0], SlideId::Water);
        let open = visible_order(&c, true);
        assert_eq!(open.last(), Some(&SlideId::Settings));
        assert_eq!(&open[..open.len() - 1], &closed[..]);
        // semua disembunyikan: tertutup -> satu slide cadangan; terbuka -> hanya Settings
        let all_off: SlideCfg = SlideId::USER.iter().map(|&s| (s, false)).collect();
        assert_eq!(visible_order(&all_off, false), vec![SlideId::Music]);
        assert_eq!(visible_order(&all_off, true), vec![SlideId::Settings]);
    }
}
