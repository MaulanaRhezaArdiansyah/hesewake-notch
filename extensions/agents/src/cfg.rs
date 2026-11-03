//! The extension's own settings (`config.json` in its data folder).

use serde::{Deserialize, Serialize};
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
        let _ = text;
        todo!()
    }

    pub fn load() -> Config {
        todo!()
    }

    /// Write `config.json` (never from tests).
    pub fn save(&self) {
        todo!()
    }
}

/// `%APPDATA%\hesewake-notch-agents`.
pub fn data_dir() -> Option<PathBuf> {
    todo!()
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
