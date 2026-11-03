//! Texts and small formatting helpers. Every text the user sees goes through `Lang::t`, with its Indonesian in
//! `indonesian()`; the English text is the key.

use std::fmt::Display;

/// The notch's language: Indonesian when true.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Lang(pub bool);

impl Lang {
    pub fn t(self, s: &'static str) -> &'static str {
        let _ = s;
        todo!()
    }
}

/// The Indonesian text for an English one.
fn indonesian(s: &str) -> Option<&'static str> {
    Some(match s {
        "just now" => "baru saja",
        "{}m ago" => "{} mnt lalu",
        "{}h ago" => "{} jam lalu",
        "{}d ago" => "{} hari lalu",
        _ => return None,
    })
}

/// `s` with each `{}` replaced by the next of `args`.
pub fn fill(s: &str, args: &[&dyn Display]) -> String {
    let _ = (s, args);
    todo!()
}

/// At most `max` characters of `s`, ending in `…` when cut.
pub fn clip(s: &str, max: usize) -> String {
    let _ = (s, max);
    todo!()
}

/// How long something took: `45s`, `4m 12s`, `1h 5m` (the same in both languages).
pub fn duration_text(secs: u64) -> String {
    let _ = secs;
    todo!()
}

/// How long ago: `just now`, `4m ago`, `2h ago`, `3d ago`.
pub fn ago_text(l: Lang, secs: u64) -> String {
    let _ = (l, secs);
    todo!()
}

/// The last part of a path: `C:\code\hesewake-notch\` → `hesewake-notch`.
pub fn folder_name(path: &str) -> &str {
    let _ = path;
    todo!()
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

    /// The source files whose `.t("…")` texts must have Indonesian.
    const SOURCES: [&str; 1] = [include_str!("text.rs")];

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
