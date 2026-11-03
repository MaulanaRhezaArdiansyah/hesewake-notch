//! The extension's own settings (`config.json` in its data folder).

use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Config {
    /// No message text and no paths anywhere.
    pub privacy: bool,
    pub toast_needs: bool,
    pub toast_done: bool,
}

impl Default for Config {
    fn default() -> Config {
        Config { privacy: false, toast_needs: true, toast_done: true }
    }
}

impl Config {
    /// The settings in `text`; anything unreadable gives the defaults.
    pub fn parse(text: &str) -> Config {
        serde_json::from_str(text.trim_start_matches('\u{feff}')).unwrap_or_default()
    }

    pub fn load() -> Config {
        let Some(path) = data_dir().map(|d| d.join("config.json")) else { return Config::default() };
        let Ok(f) = std::fs::File::open(path) else { return Config::default() };
        let mut text = String::new();
        if f.take(64 * 1024).read_to_string(&mut text).is_err() {
            return Config::default();
        }
        Config::parse(&text)
    }

    /// Write `config.json` (never from tests).
    pub fn save(&self) {
        if cfg!(test) {
            return;
        }
        let Some(dir) = data_dir() else { return };
        let Ok(json) = serde_json::to_string_pretty(self) else { return };
        if std::fs::create_dir_all(&dir).is_err() {
            return;
        }
        let tmp = dir.join("config.json.tmp");
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(&tmp, dir.join("config.json"));
        }
    }
}

/// `%APPDATA%\hesewake-notch-agents`.
pub fn data_dir() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA").filter(|d| !d.is_empty())?;
    Some(PathBuf::from(appdata).join("hesewake-notch-agents"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults_and_round_trip() {
        let d = Config::default();
        assert_eq!((d.privacy, d.toast_needs, d.toast_done), (false, true, true));
        assert_eq!(Config::parse("{}"), d);
        assert_eq!(Config::parse("not json"), d);
        assert_eq!(Config::parse("[1]"), d);
        assert_eq!(Config::parse(r#"{"privacy":true,"other":1}"#), Config { privacy: true, ..d.clone() }, "unknown keys are ignored");
        let c = Config { privacy: true, toast_needs: false, toast_done: true };
        assert_eq!(Config::parse(&serde_json::to_string(&c).unwrap()), c);
        assert!(data_dir().is_some_and(|p| p.ends_with("hesewake-notch-agents")));
    }
}
