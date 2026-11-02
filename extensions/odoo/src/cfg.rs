//! Settings in `%APPDATA%\hesewake-notch-odoo\odoo.json`. The first time, they are copied from the Office edition
//! (`%APPDATA%\hesewake-notch-office\odoo.json`), so an existing setup keeps working. No secrets here: the API key is in
//! Credential Manager (`cred.rs`).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StageCfg {
    pub id: u32,
    pub name: String,
    /// The stage is fetched and shown.
    pub on: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OdooCfg {
    pub url: String,
    pub db: String,
    pub login: String,
    /// Filled in by Test.
    pub uid: u32,
    pub partner_id: u32,
    /// Empty = every project.
    pub project_ids: Vec<u32>,
    pub interval_min: u32,
    /// Toasts for new tickets and tickets that changed stage.
    pub alerts: bool,
    pub stages: Vec<StageCfg>,
    /// Team stages: tickets of these users are shown there too.
    pub team_user_ids: Vec<u32>,
    pub team_stage_ids: Vec<u32>,
    /// Automatic accounts (res.partner) whose activity counts as mine in the report.
    pub api_partner_ids: Vec<u32>,
    /// A piece of text in the comments of those accounts that counts as my activity.
    pub bot_comment: String,
    /// Tickets in these stages are not "worked on" in the report (a queue).
    pub report_exclude_ids: Vec<u32>,
    /// The stages of "Queue today"; empty = every stage that is on.
    pub queue_ids: Vec<u32>,
    /// mail.message.subtype id of "Stage Changed"; filled in by Test (0 = not used).
    pub stage_subtype: u32,
    /// Local time zone against UTC (minutes) for the day of the report; 0 = UTC.
    pub tz_offset_min: i32,
}

impl Default for OdooCfg {
    fn default() -> OdooCfg {
        OdooCfg {
            url: String::new(),
            db: String::new(),
            login: String::new(),
            uid: 0,
            partner_id: 0,
            project_ids: Vec::new(),
            interval_min: 15,
            alerts: true,
            stages: Vec::new(),
            team_user_ids: Vec::new(),
            team_stage_ids: Vec::new(),
            api_partner_ids: Vec::new(),
            bot_comment: String::new(),
            report_exclude_ids: Vec::new(),
            queue_ids: Vec::new(),
            stage_subtype: 0,
            tz_offset_min: 0,
        }
    }
}

pub const INTERVALS: [u32; 5] = [1, 5, 15, 30, 60];

fn appdata(folder: &str) -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join(folder))
}

/// This extension's own folder (settings, the last list for the monitor).
pub fn data_dir() -> Option<PathBuf> {
    appdata("hesewake-notch-odoo")
}

/// A settings file; a UTF-8 BOM (Notepad, PowerShell 5.1) does not make it unreadable.
pub fn parse(s: &str) -> Option<OdooCfg> {
    serde_json::from_str(s.trim_start_matches('\u{feff}')).ok()
}

/// Write `text` next to `path` first and then move it over, so a crash never leaves half a file.
pub fn write_file(path: &std::path::Path, text: &str) {
    if cfg!(test) {
        return;
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = path.with_extension("tmp");
    if std::fs::write(&tmp, text).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

impl OdooCfg {
    /// Ours, or else a copy of the Office edition's (saved as ours), or else empty.
    pub fn load() -> OdooCfg {
        let read = |p: Option<PathBuf>| p.and_then(|p| std::fs::read_to_string(p).ok()).and_then(|s| parse(&s));
        if let Some(c) = read(data_dir().map(|d| d.join("odoo.json"))) {
            return c;
        }
        match read(appdata("hesewake-notch-office").map(|d| d.join("odoo.json"))) {
            Some(c) => {
                c.save();
                c
            }
            None => OdooCfg::default(),
        }
    }

    pub fn save(&self) {
        if let (Some(dir), Ok(s)) = (data_dir(), serde_json::to_string_pretty(self)) {
            write_file(&dir.join("odoo.json"), &s);
        }
    }

    /// The settings for another server or login, from the setup window. The same server keeps its stage and project
    /// choices; what belongs to the person (the team, the automatic accounts) starts over, and Test fills in the user.
    pub fn for_account(&self, url: &str, db: &str, login: &str) -> OdooCfg {
        let same_server = self.same_server(url, db);
        if same_server && self.login.eq_ignore_ascii_case(login) {
            return OdooCfg { url: url.to_string(), db: db.to_string(), login: login.to_string(), ..self.clone() };
        }
        let mut c = OdooCfg {
            url: url.to_string(),
            db: db.to_string(),
            login: login.to_string(),
            interval_min: self.interval_min,
            alerts: self.alerts,
            tz_offset_min: self.tz_offset_min,
            ..OdooCfg::default()
        };
        if same_server {
            c.project_ids = self.project_ids.clone();
            c.stages = self.stages.clone();
            c.report_exclude_ids = self.report_exclude_ids.clone();
            c.queue_ids = self.queue_ids.clone();
            c.stage_subtype = self.stage_subtype;
        }
        c
    }

    /// The server and database these settings are for (the URL compared without case or a last `/`). Nothing set up is
    /// no server.
    pub fn same_server(&self, url: &str, db: &str) -> bool {
        let norm = |u: &str| u.trim().trim_end_matches('/').to_ascii_lowercase();
        !self.url.trim().is_empty() && norm(&self.url) == norm(url) && self.db == db
    }

    /// Complete enough to use (the key is checked on its own).
    pub fn configured(&self) -> bool {
        !self.url.is_empty() && !self.db.is_empty() && !self.login.is_empty() && self.uid > 0
    }

    pub fn interval_secs(&self) -> u64 {
        60 * self.interval_min.clamp(1, 240) as u64
    }

    /// The stages that are on, in Odoo's order.
    pub fn active_stages(&self) -> impl Iterator<Item = &StageCfg> {
        self.stages.iter().filter(|s| s.on)
    }

    pub fn stage_index(&self, id: u32) -> usize {
        self.stages.iter().position(|s| s.id == id).unwrap_or(usize::MAX)
    }

    /// One step of the refresh interval, going round.
    pub fn step_interval(&mut self, forward: bool) {
        let i = INTERVALS.iter().position(|&m| m >= self.interval_min).unwrap_or(2);
        let n = INTERVALS.len();
        self.interval_min = INTERVALS[if forward { (i + 1) % n } else { (i + n - 1) % n }];
    }
}

/// The server address: `https://host[:port][/prefix]`. Plain http only for loopback (local tests).
#[derive(Clone, Debug, PartialEq)]
pub struct Endpoint {
    pub https: bool,
    pub host: String,
    pub port: u16,
    /// Path prefix without the last slash, e.g. "" or "/odoo".
    pub prefix: String,
}

pub fn parse_url(s: &str) -> Result<Endpoint, String> {
    let s = s.trim();
    let (https, rest) = if let Some(r) = s.strip_prefix("https://") {
        (true, r)
    } else if let Some(r) = s.strip_prefix("http://") {
        (false, r)
    } else {
        return Err("The URL must start with https://".to_string());
    };
    let (hostport, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    if hostport.is_empty() || hostport.contains(['@', ' ', '?', '#']) {
        return Err("The URL is not valid".to_string());
    }
    let (host, port) = match hostport.rsplit_once(':') {
        Some((h, p)) => (h, p.parse::<u16>().map_err(|_| "The port is not valid".to_string())?),
        None => (hostport, if https { 443 } else { 80 }),
    };
    let loopback = matches!(host, "localhost" | "127.0.0.1" | "[::1]");
    if !https && !loopback {
        return Err("https is required (http only for localhost)".to_string());
    }
    Ok(Endpoint { https, host: host.to_string(), port, prefix: path.trim_end_matches('/').to_string() })
}

/// Stages whose name says "done" are off unless chosen.
pub fn done_like(name: &str) -> bool {
    let n = name.to_lowercase();
    ["done", "released", "closed", "cancel", "selesai", "archiv"].iter().any(|w| n.contains(w))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_urls() {
        let e = parse_url("https://ticket.example.com").unwrap();
        assert_eq!((e.https, e.host.as_str(), e.port, e.prefix.as_str()), (true, "ticket.example.com", 443, ""));
        let e = parse_url(" https://x.io:8443/odoo/ ").unwrap();
        assert_eq!((e.port, e.prefix.as_str()), (8443, "/odoo"));
        assert!(parse_url("http://ticket.example.com").is_err());
        assert_eq!(parse_url("http://127.0.0.1:8069").unwrap().port, 8069);
        assert!(parse_url("ticket.example.com").is_err());
        assert!(parse_url("https://").is_err());
        assert!(parse_url("https://user@host").is_err());
        assert!(parse_url("https://host:abc").is_err());
    }

    #[test]
    fn done_stages() {
        assert!(done_like("RELEASED") && done_like("DONE BUG / HOTFIX") && done_like("Closed"));
        assert!(!done_like("TODO") && !done_like("IN PROGRESS") && !done_like("REVIEW (QA)"));
    }

    #[test]
    fn reads_the_office_file_with_bom_and_defaults() {
        let c = OdooCfg { url: "https://a".into(), uid: 7, ..Default::default() };
        let s = serde_json::to_string(&c).unwrap();
        assert_eq!(parse(&s).unwrap(), c);
        let p = parse("\u{feff}{\"url\":\"https://b\",\"something_new\":1}").unwrap();
        assert_eq!((p.url.as_str(), p.interval_min, p.alerts, p.uid), ("https://b", 15, true, 0));
        assert!(!p.configured());
        assert!(parse("{ broken").is_none());
    }

    #[test]
    fn interval_steps_go_round() {
        let mut c = OdooCfg::default();
        c.step_interval(true);
        assert_eq!(c.interval_min, 30);
        c.step_interval(false);
        c.step_interval(false);
        assert_eq!(c.interval_min, 5);
        c.interval_min = 60;
        c.step_interval(true);
        assert_eq!(c.interval_min, 1);
        assert_eq!(c.interval_secs(), 60);
    }
    #[test]
    fn another_account_starts_over() {
        let mut c = OdooCfg { url: "https://a.example.com".into(), db: "a".into(), login: "me@a".into(), uid: 7, partner_id: 9, ..OdooCfg::default() };
        c.project_ids = vec![2];
        c.team_user_ids = vec![5];
        c.stages = vec![StageCfg { id: 1, name: "TODO".into(), on: true }];
        c.interval_min = 5;
        // the same account (the URL written a little differently) keeps everything
        let same = c.for_account("https://A.example.com/", "a", "ME@a");
        assert_eq!((same.uid, same.team_user_ids.clone(), same.url.as_str()), (7, vec![5], "https://A.example.com/"));
        // another person on the same server keeps the server's choices, not the person's
        let other = c.for_account("https://a.example.com", "a", "you@a");
        assert_eq!((other.uid, other.partner_id, other.project_ids.clone(), other.stages.len()), (0, 0, vec![2], 1));
        assert!(other.team_user_ids.is_empty());
        assert_eq!(other.interval_min, 5);
        // another server keeps only the choices that are not about it
        assert!(c.same_server("https://A.example.com/", "a") && !c.same_server("https://a.example.com", "b"));
        assert!(!OdooCfg::default().same_server("", ""));
        let new = c.for_account("https://b.example.com", "b", "me@b");
        assert!(new.project_ids.is_empty() && new.stages.is_empty() && new.uid == 0);
        assert_eq!(new.interval_min, 5);
    }
}
