//! What an agent's hook tells the extension, the same for every agent: one small `Msg` per event, sent from the hook
//! process to the running extension (`relay.rs`).

use serde::{Deserialize, Serialize};

/// The coding agent a session belongs to.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Agent {
    Claude,
}

impl Agent {
    /// The short name shown on rows and toasts (a product name: not translated).
    pub fn label(self) -> &'static str {
        match self {
            Agent::Claude => "Claude",
        }
    }
}

/// What happened in the session, reduced to what the notch shows.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    SessionStart,
    SessionEnd,
    /// The person sent a prompt.
    Prompt,
    /// A tool is about to run or just ran.
    Tool,
    /// A permission prompt or a question is waiting for the person.
    NeedsYou,
    /// The turn finished.
    Stop,
    /// The turn ended on an API error.
    StopFailure,
    /// Nothing runs any more (e.g. after an interrupt, which sends no Stop).
    Idle,
}

/// One hook call, as sent to the extension.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Msg {
    /// Format version; an extension that doesn't know it ignores the message.
    pub v: u8,
    pub agent: Agent,
    pub kind: EventKind,
    pub session: String,
    pub project: String,
    pub cwd: String,
    /// The notification text, or the first line of the agent's last message.
    pub text: String,
    /// The tool that is running (`Bash`, `Edit`, ...), empty when none.
    #[serde(default)]
    pub tool: String,
    /// The terminal window to come back to (0 = unknown).
    pub hwnd: isize,
    pub subagent: bool,
    /// The agent's process (0 = unknown); when it exits, the session is gone.
    #[serde(default)]
    pub pid: u32,
    /// When the hook started, in Unix milliseconds (0 = unknown). Hooks run as separate processes, so their
    /// messages can arrive out of order.
    #[serde(default)]
    pub at_ms: u64,
}

pub const VERSION: u8 = 1;
/// Longest message the extension accepts.
pub const MAX_MSG: usize = 16 * 1024;
/// Characters kept from a notification or the agent's last message.
const PREVIEW_CHARS: usize = 200;

pub fn encode(m: &Msg) -> Vec<u8> {
    serde_json::to_vec(m).unwrap_or_default()
}

pub fn decode(b: &[u8]) -> Option<Msg> {
    if b.len() > MAX_MSG {
        return None;
    }
    serde_json::from_slice::<Msg>(b).ok().filter(|m| m.v == VERSION)
}

/// One short line from a notification or a Markdown message: the first line with any letters or digits, without
/// Markdown markers, whitespace collapsed, at most `PREVIEW_CHARS` characters.
pub fn preview(s: &str) -> String {
    s.lines().map(clean_line).find(|l| l.chars().any(char::is_alphanumeric)).unwrap_or_default()
}

fn clean_line(line: &str) -> String {
    let mut l = line.trim_start();
    l = l.trim_start_matches('#').trim_start_matches('>').trim_start();
    for bullet in ["- ", "* ", "+ "] {
        if let Some(rest) = l.strip_prefix(bullet) {
            l = rest;
        }
    }
    let chars: Vec<char> = l.chars().take(PREVIEW_CHARS * 4).collect();
    let mut out = String::new();
    let mut n = 0;
    let mut space = false;
    for (i, &c) in chars.iter().enumerate() {
        // `_` is emphasis only at a word edge; inside a word (snake_case) it stays.
        let edge = |j: Option<usize>| j.and_then(|j| chars.get(j)).map_or(true, |c| !c.is_alphanumeric());
        if c == '*' || c == '`' || (c == '_' && (edge(i.checked_sub(1)) || edge(Some(i + 1)))) {
            continue;
        }
        if c.is_whitespace() {
            space = !out.is_empty();
            continue;
        }
        if n >= PREVIEW_CHARS {
            break;
        }
        if space {
            if n + 1 >= PREVIEW_CHARS {
                break;
            }
            out.push(' ');
            n += 1;
            space = false;
        }
        out.push(c);
        n += 1;
    }
    out
}

/// The project's name: the last part of the session's folder (empty when there is none).
pub fn project_name(cwd: &str) -> String {
    crate::text::folder_name(cwd).to_string()
}

#[cfg(test)]
pub(crate) fn sample() -> Msg {
    Msg {
        at_ms: 0,
        v: VERSION,
        agent: Agent::Claude,
        kind: EventKind::Stop,
        session: "s1".into(),
        project: "app".into(),
        cwd: "C:\\p\\app".into(),
        text: "Done".into(),
        tool: "Bash".into(),
        hwnd: 0x1234,
        subagent: false,
        pid: 42,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previews_are_one_clean_line() {
        assert_eq!(preview("## Done\n\n**PR #27** is up."), "Done");
        assert_eq!(preview("\n\n  - `fix` merged"), "fix merged");
        assert_eq!(preview("> **Bold**   and   _soft_\nsecond"), "Bold and soft");
        assert_eq!(preview("keep snake_case words"), "keep snake_case words");
        assert_eq!(preview(""), "");
    }

    #[test]
    fn preview_cuts_on_char_boundaries() {
        assert_eq!(preview(&"é".repeat(300)).chars().count(), 200);
        assert!(preview(&"日本🙂".repeat(400)).chars().count() <= 200);
        assert!(preview(&"x".repeat(1 << 20)).chars().count() <= 200);
    }

    #[test]
    fn project_from_cwd() {
        assert_eq!(project_name("C:\\Users\\a\\Projects\\rust\\hesewake-notch\\"), "hesewake-notch");
        assert_eq!(project_name("/home/a/app"), "app");
        assert_eq!(project_name(""), "");
        assert_eq!(project_name("C:\\"), "C:");
    }

    #[test]
    fn round_trip() {
        let m = sample();
        assert_eq!(decode(&encode(&m)), Some(m.clone()));
        let json = String::from_utf8(encode(&m)).unwrap();
        assert!(json.contains(r#""agent":"claude""#), "{json}");
    }

    #[test]
    fn decode_rejects_other_versions_and_big_frames() {
        let mut newer = sample();
        newer.v = 2;
        assert_eq!(decode(&encode(&newer)), None);
        assert_eq!(decode(&vec![b' '; 17 * 1024]), None);
        assert_eq!(decode(b"garbage"), None);
        assert_eq!(decode(b""), None);
        // an older hook without `tool` and `pid` still decodes
        let old = r#"{"v":1,"agent":"claude","kind":"Prompt","session":"s","project":"p","cwd":"","text":"","hwnd":0,"subagent":false}"#;
        assert_eq!(decode(old.as_bytes()).map(|m| (m.tool, m.pid)), Some((String::new(), 0)));
    }
}
