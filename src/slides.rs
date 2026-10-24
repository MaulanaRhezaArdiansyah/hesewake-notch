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
    /// Selalu ada dan selalu terakhir (tidak bisa disembunyikan) supaya pengguna tidak terkunci.
    Settings,
}

impl SlideId {
    /// Slide yang bisa diatur pengguna (urutan bawaan).
    pub(crate) const USER: [SlideId; 6] =
        [SlideId::Music, SlideId::Water, SlideId::Todo, SlideId::Ledge, SlideId::Scrap, SlideId::Notifications];

    pub(crate) fn key(self) -> &'static str {
        match self {
            SlideId::Music => "music",
            SlideId::Water => "water",
            SlideId::Todo => "todo",
            SlideId::Ledge => "ledge",
            SlideId::Scrap => "scrap",
            SlideId::Notifications => "notifs",
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
    SlideId::USER.iter().map(|&s| (s, true)).collect()
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
            out.push((id, true));
        }
    }
    out
}

pub(crate) fn format_slides(cfg: &[(SlideId, bool)]) -> String {
    cfg.iter().map(|(s, on)| if *on { s.key().to_string() } else { format!("!{}", s.key()) }).collect::<Vec<_>>().join(",")
}

/// Urutan slide yang benar-benar tampil: yang aktif, lalu `Settings` di akhir.
pub(crate) fn visible_order(cfg: &[(SlideId, bool)]) -> Vec<SlideId> {
    let mut v: Vec<SlideId> = cfg.iter().filter(|(_, on)| *on).map(|(s, _)| *s).collect();
    v.push(SlideId::Settings);
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
        assert_eq!(c.len(), 6);
        assert_eq!(c[3], (SlideId::Ledge, true));
    }

    #[test]
    fn ignores_unknown_and_duplicates() {
        let c = parse_slides("music,bogus,music,!music,settings");
        assert_eq!(c.iter().filter(|(s, _)| *s == SlideId::Music).count(), 1);
        assert!(c.iter().all(|(s, _)| *s != SlideId::Settings));
        assert_eq!(c.len(), 6);
    }

    #[test]
    fn roundtrips() {
        let c = parse_slides("scrap,!ledge,music,water,todo,notifs");
        assert_eq!(parse_slides(&format_slides(&c)), c);
        assert_eq!(format_slides(&c), "scrap,!ledge,music,water,todo,notifs");
    }

    #[test]
    fn visible_order_ends_with_settings() {
        let c = parse_slides("!music,water");
        let v = visible_order(&c);
        assert_eq!(v.last(), Some(&SlideId::Settings));
        assert!(!v.contains(&SlideId::Music));
        assert_eq!(v[0], SlideId::Water);
        // semua disembunyikan -> hanya Settings
        let all_off: SlideCfg = SlideId::USER.iter().map(|&s| (s, false)).collect();
        assert_eq!(visible_order(&all_off), vec![SlideId::Settings]);
    }
}
