//! Claude Code hooks: what one hook call tells the notch. Claude Code runs `hesewake-notch.exe --claude-hook` with a
//! JSON object on stdin; this module turns it into a small `Msg` that is sent to the running notch.

// Wired up step by step (bridge process, then the notch side).
#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// What happened in the session, reduced to what the notch shows.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub(crate) enum EventKind {
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
}

/// One hook call, as sent to the notch.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub(crate) struct Msg {
    /// Format version; a notch that doesn't know it ignores the message.
    pub(crate) v: u8,
    pub(crate) kind: EventKind,
    pub(crate) session: String,
    pub(crate) project: String,
    pub(crate) cwd: String,
    /// The notification text, or the first line of Claude's last message.
    pub(crate) text: String,
    /// The terminal window to come back to (0 = unknown).
    pub(crate) hwnd: isize,
    pub(crate) subagent: bool,
}

pub(crate) const VERSION: u8 = 1;
/// Longest message the notch accepts.
const MAX_MSG: usize = 16 * 1024;
/// Characters kept from a notification or Claude's last message.
const PREVIEW_CHARS: usize = 200;

/// Notification types that mean "Claude is waiting for you". `idle_prompt` is left out on purpose: it fires after
/// every finished turn nobody answered, and "Done" already covers that.
const NEEDS_YOU: [&str; 3] = ["permission_prompt", "elicitation_dialog", "agent_needs_input"];

/// The hook's stdin JSON as a `Msg`, or None when the notch has nothing to show for it.
pub(crate) fn parse_hook(json: &str) -> Option<Msg> {
    let v: Value = serde_json::from_str(json).ok()?;
    let field = |k: &str| v.get(k).and_then(Value::as_str);
    let session = field("session_id").filter(|s| !s.is_empty())?;
    let (kind, text) = match field("hook_event_name")? {
        "SessionStart" => (EventKind::SessionStart, String::new()),
        "SessionEnd" => (EventKind::SessionEnd, String::new()),
        "UserPromptSubmit" => (EventKind::Prompt, String::new()),
        "PreToolUse" | "PostToolUse" => (EventKind::Tool, String::new()),
        "Notification" if NEEDS_YOU.contains(&field("notification_type")?) => (EventKind::NeedsYou, preview(field("message").unwrap_or(""))),
        "Stop" => (EventKind::Stop, preview(field("last_assistant_message").unwrap_or(""))),
        "StopFailure" => (EventKind::StopFailure, String::new()),
        _ => return None,
    };
    let cwd = field("cwd").unwrap_or("");
    Some(Msg {
        v: VERSION,
        kind,
        session: session.to_string(),
        project: project_name(cwd),
        cwd: cwd.to_string(),
        text,
        hwnd: 0,
        subagent: v.get("agent_id").is_some_and(|a| !a.is_null()),
    })
}

pub(crate) fn encode(m: &Msg) -> Vec<u8> {
    serde_json::to_vec(m).unwrap_or_default()
}

pub(crate) fn decode(b: &[u8]) -> Option<Msg> {
    if b.len() > MAX_MSG {
        return None;
    }
    serde_json::from_slice::<Msg>(b).ok().filter(|m| m.v == VERSION)
}

/// One short line from a notification or a Markdown message: the first line with any letters or digits, without
/// Markdown markers, whitespace collapsed, at most `PREVIEW_CHARS` characters.
pub(crate) fn preview(s: &str) -> String {
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

/// The project's name: the last part of the session's folder.
pub(crate) fn project_name(cwd: &str) -> String {
    let t = cwd.trim_end_matches(['\\', '/']);
    let name = t.rsplit(['\\', '/']).next().unwrap_or("");
    if name.is_empty() { "Claude Code".to_string() } else { name.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hook(event: &str, extra: &str) -> String {
        format!(r#"{{"session_id":"s1","cwd":"C:\\p\\app","hook_event_name":"{event}"{extra}}}"#)
    }

    #[test]
    fn maps_each_event() {
        let kind = |e: &str, extra: &str| parse_hook(&hook(e, extra)).map(|m| m.kind);
        assert_eq!(kind("SessionStart", r#","source":"startup""#), Some(EventKind::SessionStart));
        assert_eq!(kind("SessionEnd", r#","reason":"other""#), Some(EventKind::SessionEnd));
        assert_eq!(kind("UserPromptSubmit", ""), Some(EventKind::Prompt));
        assert_eq!(kind("PreToolUse", r#","tool_name":"Bash""#), Some(EventKind::Tool));
        assert_eq!(kind("PostToolUse", r#","tool_name":"Bash""#), Some(EventKind::Tool));
        assert_eq!(kind("Stop", ""), Some(EventKind::Stop));
        assert_eq!(kind("StopFailure", ""), Some(EventKind::StopFailure));
        for t in ["permission_prompt", "elicitation_dialog", "agent_needs_input"] {
            let extra = format!(r#","notification_type":"{t}","message":"Claude needs your permission""#);
            let m = parse_hook(&hook("Notification", &extra)).expect(t);
            assert_eq!(m.kind, EventKind::NeedsYou);
            assert_eq!(m.text, "Claude needs your permission");
        }
        assert_eq!(kind("Notification", r#","notification_type":"idle_prompt","message":"x""#), None);
        assert_eq!(kind("Notification", r#","notification_type":"auth_success""#), None);
        assert_eq!(kind("PreCompact", ""), None);
        let m = parse_hook(&hook("PreToolUse", r#","agent_id":"a1""#)).unwrap();
        assert!(m.subagent);
        assert_eq!((m.session.as_str(), m.project.as_str(), m.cwd.as_str(), m.hwnd, m.v), ("s1", "app", "C:\\p\\app", 0, VERSION));
    }

    #[test]
    fn stop_carries_a_preview() {
        let m = parse_hook(&hook("Stop", r###","last_assistant_message":"## Done\n\n**PR #27** is up.""###)).unwrap();
        assert_eq!(m.text, "Done");
        assert_eq!(preview("\n\n  - `fix` merged"), "fix merged");
        assert_eq!(preview("> **Bold**   and   _soft_\nsecond"), "Bold and soft");
        assert_eq!(preview(""), "");
    }

    #[test]
    fn preview_cuts_on_char_boundaries() {
        let p = preview(&"é".repeat(300));
        assert_eq!(p.chars().count(), 200);
        let p = preview(&"日本🙂".repeat(400));
        assert!(p.chars().count() <= 200);
        let big = "x".repeat(1 << 20);
        assert!(preview(&big).chars().count() <= 200);
    }

    #[test]
    fn project_from_cwd() {
        assert_eq!(project_name("C:\\Users\\a\\Projects\\rust\\hesewake-notch\\"), "hesewake-notch");
        assert_eq!(project_name("/home/a/app"), "app");
        assert_eq!(project_name(""), "Claude Code");
        assert_eq!(project_name("C:\\"), "C:");
    }

    #[test]
    fn bad_input_is_ignored() {
        for bad in ["", "null", "[]", "{", "5", r#"{"hook_event_name":5}"#, r#"{"hook_event_name":"Stop"}"#, r#"{"hook_event_name":"Stop","session_id":7}"#] {
            assert_eq!(parse_hook(bad), None, "{bad}");
        }
    }

    #[test]
    fn round_trip() {
        let mut m = parse_hook(&hook("Stop", r#","last_assistant_message":"Done""#)).unwrap();
        m.hwnd = 0x1234;
        assert_eq!(decode(&encode(&m)), Some(m.clone()));
        assert_eq!(decode(&vec![b' '; 17 * 1024]), None);
        let mut newer = m;
        newer.v = 2;
        assert_eq!(decode(&encode(&newer)), None);
        assert_eq!(decode(b"garbage"), None);
    }
}
