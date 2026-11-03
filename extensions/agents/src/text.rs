//! Texts and small formatting helpers. Every text the user sees goes through `Lang::t`, with its Indonesian in
//! `indonesian()`; the English text is the key.

use std::fmt::Display;

/// The notch's language: Indonesian when true.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Lang(pub bool);

impl Lang {
    pub fn t(self, s: &'static str) -> &'static str {
        match (self.0, indonesian(s)) {
            (true, Some(id)) => id,
            (true, None) if cfg!(test) => panic!("no Indonesian for {s:?}"),
            _ => s,
        }
    }
}

/// The Indonesian text for an English one.
fn indonesian(s: &str) -> Option<&'static str> {
    Some(match s {
        "just now" => "baru saja",
        "{}m ago" => "{} mnt lalu",
        "{}h ago" => "{} jam lalu",
        "{}d ago" => "{} hari lalu",
        "Coding |Agents" => "Agen |Koding",
        "Settings" => "Pengaturan",
        "Privacy mode" => "Mode privasi",
        "Claude Code's settings.json could not be read. Nothing was changed." => "settings.json milik Claude Code tidak bisa dibaca. Tidak ada yang diubah.",
        "Connect Claude Code to see its sessions here" => "Sambungkan Claude Code untuk melihat sesinya di sini",
        "Connect" => "Sambungkan",
        "Needs you" => "Perlu kamu",
        "Working" => "Bekerja",
        "Recent" => "Terbaru",
        "No agent sessions yet. They show up here when Claude Code runs." => "Belum ada sesi agen. Sesi muncul di sini saat Claude Code berjalan.",
        "{} · Needs you · {}" => "{} · Perlu kamu · {}",
        "{} · Working {}" => "{} · Bekerja {}",
        "{} · Done {}" => "{} · Selesai {}",
        "{} · Error {}" => "{} · Galat {}",
        "{} · Idle {}" => "{} · Diam {}",
        "Done" => "Selesai",
        "Error" => "Galat",
        "Idle" => "Diam",
        "Agent" => "Agen",
        "State" => "Status",
        "Time" => "Waktu",
        "Branch" => "Cabang",
        "Tool" => "Alat",
        "Folder" => "Folder",
        "Copy path" => "Salin path",
        "Dismiss" => "Singkirkan",
        "Not connected" => "Belum tersambung",
        "Remove" => "Hapus",
        "Connected" => "Tersambung",
        "Fix" => "Perbaiki",
        "Moved, press to fix" => "Pindah, tekan untuk memperbaiki",
        "Replace old hooks" => "Ganti hook lama",
        "Old Hesewake hooks" => "Hook Hesewake lama",
        "Check again" => "Periksa lagi",
        "settings.json unreadable" => "settings.json tak terbaca",
        "On" => "Nyala",
        "Off" => "Mati",
        "Claude Code" => "Claude Code",
        "Options" => "Opsi",
        "Toast when a session needs you" => "Toast saat sesi perlu kamu",
        "Toast when a session finishes" => "Toast saat sesi selesai",
        "{} needs you" => "{} perlu kamu",
        "{} · Permission" => "{} · Izin",
        "Done · {}" => "Selesai · {}",
        "Stopped with an error" => "Berhenti karena galat",
        "{} sessions need you" => "{} sesi perlu kamu",
        "{} sessions finished" => "{} sesi selesai",
        "Claude connected · {} sessions · {} need you" => "Claude tersambung · {} sesi · {} perlu kamu",
        "Claude connected · {} sessions" => "Claude tersambung · {} sesi",
        "Claude settings unreadable" => "Pengaturan Claude tak terbaca",
        "Claude not connected" => "Claude belum tersambung",
        "Another copy is running" => "Salinan lain sedang berjalan",
        _ => return None,
    })
}

/// `s` with each `{}` replaced by the next of `args`.
pub fn fill(s: &str, args: &[&dyn Display]) -> String {
    let mut out = String::new();
    for (i, part) in s.split("{}").enumerate() {
        if i > 0 {
            if let Some(a) = args.get(i - 1) {
                out.push_str(&a.to_string());
            }
        }
        out.push_str(part);
    }
    out
}

/// At most `max` characters of `s`, ending in `…` when cut.
pub fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// How long something took: `45s`, `4m 12s`, `1h 5m` (the same in both languages).
pub fn duration_text(secs: u64) -> String {
    match secs {
        0..=59 => format!("{secs}s"),
        60..=3599 => format!("{}m {}s", secs / 60, secs % 60),
        _ => format!("{}h {}m", secs / 3600, secs % 3600 / 60),
    }
}

/// How long ago: `just now`, `4m ago`, `2h ago`, `3d ago`.
pub fn ago_text(l: Lang, secs: u64) -> String {
    match secs {
        0..=59 => l.t("just now").to_string(),
        60..=3599 => fill(l.t("{}m ago"), &[&(secs / 60)]),
        3600..=86_399 => fill(l.t("{}h ago"), &[&(secs / 3600)]),
        _ => fill(l.t("{}d ago"), &[&(secs / 86_400)]),
    }
}

/// The last part of a path: `C:\code\hesewake-notch\` → `hesewake-notch`.
pub fn folder_name(path: &str) -> &str {
    path.trim_end_matches(['\\', '/']).rsplit(['\\', '/']).next().unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_and_clip() {
        assert_eq!(fill("{} sessions · {}", &[&3, &"Claude"]), "3 sessions · Claude");
        assert_eq!(fill("none", &[&1]), "none");
        assert_eq!(clip("hesewake", 20), "hesewake");
        assert_eq!(clip("hesewake-notch", 8), "hesewak…");
        assert_eq!(clip("😀😀😀😀", 3).chars().count(), 3);
        assert_eq!(clip("", 3), "");
    }

    #[test]
    fn durations_read_naturally() {
        assert_eq!(duration_text(45), "45s");
        assert_eq!(duration_text(252), "4m 12s");
        assert_eq!(duration_text(3900), "1h 5m");
        let (en, id) = (Lang(false), Lang(true));
        assert_eq!(ago_text(en, 30), "just now");
        assert_eq!(ago_text(en, 240), "4m ago");
        assert_eq!(ago_text(en, 7200), "2h ago");
        assert_eq!(ago_text(en, 3 * 86_400), "3d ago");
        assert_eq!(ago_text(id, 30), "baru saja");
        assert_eq!(ago_text(id, 240), "4 mnt lalu");
    }

    #[test]
    fn folder_names() {
        assert_eq!(folder_name(r"C:\Users\a\Projects\hesewake-notch\"), "hesewake-notch");
        assert_eq!(folder_name("C:/x/y"), "y");
        assert_eq!(folder_name("C:\\"), "C:");
        assert_eq!(folder_name(""), "");
    }

    /// The source files whose translated texts must have Indonesian.
    const SOURCES: [&str; 3] = [include_str!("text.rs"), include_str!("ui.rs"), include_str!("toasts.rs")];

    #[test]
    fn every_text_has_indonesian() {
        let mut used = Vec::new();
        for src in SOURCES {
            for part in src.split(".t(\"").skip(1) {
                let Some(end) = part.find("\")") else { continue };
                used.push(part[..end].to_string());
            }
        }
        for key in &used {
            let id = indonesian(key).unwrap_or_else(|| panic!("no Indonesian for {key:?}"));
            assert_eq!(key.matches("{}").count(), id.matches("{}").count(), "placeholders differ for {key:?}");
            assert!(!id.is_empty());
        }
        // the table has nothing that no file uses
        let table = include_str!("text.rs");
        let body = &table[table.find("Some(match s {").unwrap()..table.find("_ => return None").unwrap()];
        for line in body.lines().map(str::trim).filter(|l| l.starts_with('"')) {
            let key = &line[1..line[1..].find('"').unwrap() + 1];
            assert!(used.iter().any(|u| u == key), "unused text {key:?}");
        }
        assert_eq!(Lang(false).t("just now"), "just now");
        assert_eq!(Lang(true).t("just now"), "baru saja");
    }
}
