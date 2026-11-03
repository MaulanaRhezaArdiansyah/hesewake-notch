//! Claude Code's hooks: Claude Code runs `hesewake-agents.exe --hook claude` with a JSON object on stdin; this module
//! turns it into an `event::Msg`.

use crate::event::{preview, project_name, Agent, EventKind, Msg, VERSION};
use crate::text::clip;
use serde_json::Value;
use std::io::Read;

/// Most stdin read from one hook call. Claude's last message can be long and must not cut the JSON short.
const MAX_STDIN: u64 = 16 * 1024 * 1024;
/// Characters kept of a tool's name.
const TOOL_CHARS: usize = 40;

/// Notification types that mean "Claude is waiting for you". `idle_prompt` is not one of them: it fires after every
/// finished turn nobody answered ("Done" covers that); it only tells that nothing runs any more (`EventKind::Idle`).
const NEEDS_YOU: [&str; 4] = ["permission_prompt", "elicitation_dialog", "elicitation_url_dialog", "agent_needs_input"];

/// The hook's stdin JSON as a `Msg`, or None when the notch has nothing to show for it.
pub fn parse_hook(json: &str) -> Option<Msg> {
    let v: Value = serde_json::from_str(json).ok()?;
    let field = |k: &str| v.get(k).and_then(Value::as_str);
    let session = field("session_id").filter(|s| !s.is_empty())?;
    let (kind, text) = match field("hook_event_name")? {
        "SessionStart" => (EventKind::SessionStart, String::new()),
        "SessionEnd" => (EventKind::SessionEnd, String::new()),
        "UserPromptSubmit" => (EventKind::Prompt, String::new()),
        "PreToolUse" | "PostToolUse" => (EventKind::Tool, String::new()),
        "Notification" if NEEDS_YOU.contains(&field("notification_type")?) => (EventKind::NeedsYou, preview(field("message").unwrap_or(""))),
        "Notification" if field("notification_type") == Some("idle_prompt") => (EventKind::Idle, String::new()),
        "Stop" => (EventKind::Stop, preview(field("last_assistant_message").unwrap_or(""))),
        "StopFailure" => (EventKind::StopFailure, String::new()),
        _ => return None,
    };
    let tool = if kind == EventKind::Tool { clip(field("tool_name").unwrap_or(""), TOOL_CHARS) } else { String::new() };
    let cwd = field("cwd").unwrap_or("");
    let project = project_name(cwd);
    Some(Msg {
        at_ms: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0),
        v: VERSION,
        agent: Agent::Claude,
        kind,
        session: session.to_string(),
        project: if project.is_empty() { "Claude Code".to_string() } else { project },
        cwd: cwd.to_string(),
        text,
        tool,
        hwnd: 0,
        subagent: v.get("agent_id").is_some_and(|a| !a.is_null()),
        pid: 0,
    })
}

/// The hook JSON from `r` (at most `MAX_STDIN` bytes) as a `Msg`.
pub fn read_hook(r: impl Read) -> Option<Msg> {
    let mut buf = Vec::new();
    r.take(MAX_STDIN).read_to_end(&mut buf).ok()?;
    parse_hook(&String::from_utf8_lossy(&buf))
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
        for t in ["permission_prompt", "elicitation_dialog", "elicitation_url_dialog", "agent_needs_input"] {
            let extra = format!(r#","notification_type":"{t}","message":"Claude needs your permission""#);
            let m = parse_hook(&hook("Notification", &extra)).expect(t);
            assert_eq!(m.kind, EventKind::NeedsYou);
            assert_eq!(m.text, "Claude needs your permission");
        }
        assert_eq!(kind("Notification", r#","notification_type":"idle_prompt","message":"x""#), Some(EventKind::Idle));
        assert_eq!(kind("Notification", r#","notification_type":"auth_success""#), None);
        assert_eq!(kind("PreCompact", ""), None);
        let m = parse_hook(&hook("PreToolUse", r#","agent_id":"a1""#)).unwrap();
        assert!(m.subagent);
        assert_eq!((m.session.as_str(), m.project.as_str(), m.cwd.as_str(), m.hwnd, m.v), ("s1", "app", "C:\\p\\app", 0, VERSION));
        assert_eq!(m.agent, Agent::Claude);
    }

    #[test]
    fn the_tool_name_is_kept() {
        let m = parse_hook(&hook("PreToolUse", r#","tool_name":"Bash""#)).unwrap();
        assert_eq!((m.kind, m.tool.as_str()), (EventKind::Tool, "Bash"));
        let long = format!(r#","tool_name":"{}""#, "mcp__server__tool_".repeat(10));
        assert!(parse_hook(&hook("PreToolUse", &long)).unwrap().tool.chars().count() <= 40);
        assert_eq!(parse_hook(&hook("Stop", "")).unwrap().tool, "", "only tool events carry a tool");
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
        assert!(parse_hook(&hook("Stop", "")).unwrap().at_ms.abs_diff(now) < 60_000, "the hook stamps when it ran");
    }

    #[test]
    fn stop_carries_a_preview() {
        let m = parse_hook(&hook("Stop", r###","last_assistant_message":"## Done\n\n**PR #27** is up.""###)).unwrap();
        assert_eq!(m.text, "Done");
    }

    #[test]
    fn a_session_without_a_folder_is_named_claude_code() {
        let m = parse_hook(r#"{"session_id":"s1","hook_event_name":"Stop"}"#).unwrap();
        assert_eq!(m.project, "Claude Code");
    }

    /// Claude's final reply can be long (and JSON-escaped); the Stop event must still arrive.
    #[test]
    fn long_final_message_still_reaches_the_extension() {
        let reply = "Selesai, ini ringkasannya: é日本🙂 ".repeat(40_000);
        let json = format!(r#"{{"session_id":"s1","cwd":"C:\\p\\app","hook_event_name":"Stop","last_assistant_message":"{reply}"}}"#);
        assert!(json.len() > 1_000_000);
        let m = read_hook(json.as_bytes()).expect("Stop with a 1 MB reply");
        assert_eq!(m.kind, EventKind::Stop);
        assert!(m.text.chars().count() <= 200 && m.text.starts_with("Selesai"));
    }

    #[test]
    fn huge_and_broken_stdin_never_panics() {
        assert!(read_hook(std::io::repeat(b'{').take(20 * 1024 * 1024)).is_none());
        assert!(read_hook(&b"\xff\xfe{\"hook_event_name\":"[..]).is_none());
        assert!(read_hook(&b""[..]).is_none());
    }

    #[test]
    fn bad_input_is_ignored() {
        for bad in ["", "null", "[]", "{", "5", r#"{"hook_event_name":5}"#, r#"{"hook_event_name":"Stop"}"#, r#"{"hook_event_name":"Stop","session_id":7}"#, r#"{"hook_event_name":"Stop","session_id":""}"#] {
            assert_eq!(parse_hook(bad), None, "{bad}");
        }
    }
}
