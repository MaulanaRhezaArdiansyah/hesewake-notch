//! Claude Code hooks: what one hook call tells the notch. Claude Code runs `hesewake-notch.exe --claude-hook` with a
//! JSON object on stdin; this module turns it into a small `Msg` that is sent to the running notch.

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
    /// Claude has been idle for a while (`idle_prompt`): nothing runs, e.g. after an interrupt.
    Idle,
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
    /// The Claude Code process (0 = unknown); when it exits, the session is gone.
    #[serde(default)]
    pub(crate) pid: u32,
}

pub(crate) const VERSION: u8 = 1;
/// Longest message the notch accepts.
const MAX_MSG: usize = 16 * 1024;
/// Characters kept from a notification or Claude's last message.
const PREVIEW_CHARS: usize = 200;

/// Notification types that mean "Claude is waiting for you". `idle_prompt` is not one of them: it fires after every
/// finished turn nobody answered ("Done" covers that); it only tells that nothing runs any more (`EventKind::Idle`).
const NEEDS_YOU: [&str; 4] = ["permission_prompt", "elicitation_dialog", "elicitation_url_dialog", "agent_needs_input"];

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
        "Notification" if field("notification_type") == Some("idle_prompt") => (EventKind::Idle, String::new()),
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
        pid: 0,
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

// ---------------------------------------------------------------- the bridge process

use std::ffi::c_void;
use std::io::Read;
use windows::core::{w, PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, BOOL, HWND, LPARAM, WPARAM};
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use windows::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS};
use windows::Win32::System::Threading::{GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FindWindowExW, GetWindow, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible, SendMessageTimeoutW, GW_OWNER, SMTO_ABORTIFHUNG,
    WM_COPYDATA,
};

/// `dwData` of the `WM_COPYDATA` message that carries a `Msg`.
pub(crate) const COPY_ID: usize = 0x434C_4155;
/// Most stdin read from one hook call. Claude's last message can be long and must not cut the JSON short.
const MAX_STDIN: u64 = 16 * 1024 * 1024;
/// How far up the process tree to look for the terminal window.
const MAX_DEPTH: usize = 12;

/// `--claude-hook`: read the hook JSON from stdin and pass it to the running notch. Never fails, prints nothing.
pub(crate) fn run_hook() {
    let Some(mut m) = read_hook(std::io::stdin()) else { return };
    let notch = find_notch();
    if notch == 0 {
        return;
    }
    let chain = ancestors(&parent_map(), unsafe { GetCurrentProcessId() });
    m.pid = claude_pid(&chain);
    m.hwnd = terminal_window(&chain, &m.project);
    deliver(notch, &m);
}

/// The hook JSON from `r` (at most `MAX_STDIN` bytes) as a `Msg`.
pub(crate) fn read_hook(r: impl Read) -> Option<Msg> {
    let mut buf = Vec::new();
    r.take(MAX_STDIN).read_to_end(&mut buf).ok()?;
    parse_hook(&String::from_utf8_lossy(&buf))
}

/// The process chain above `start` (parent first) as (pid, exe name), stopping before Explorer (the desktop).
pub(crate) fn ancestors(map: &[(u32, u32, String)], start: u32) -> Vec<(u32, String)> {
    let mut chain = Vec::new();
    let mut pid = start;
    for _ in 0..MAX_DEPTH {
        let Some(&(_, parent, _)) = map.iter().find(|(p, ..)| *p == pid) else { break };
        let Some((_, _, name)) = map.iter().find(|(p, ..)| *p == parent) else { break };
        if parent == 0 || parent == pid || name.eq_ignore_ascii_case("explorer.exe") {
            break;
        }
        chain.push((parent, name.clone()));
        pid = parent;
    }
    chain
}

/// The Claude Code process among the ancestors: the first one that isn't a shell.
pub(crate) fn claude_pid(chain: &[(u32, String)]) -> u32 {
    const SHELLS: [&str; 6] = ["cmd.exe", "powershell.exe", "pwsh.exe", "bash.exe", "sh.exe", "conhost.exe"];
    chain.iter().find(|(_, n)| !SHELLS.iter().any(|s| n.eq_ignore_ascii_case(s))).map_or(0, |(p, _)| *p)
}

/// The running notch's window (0 if none). Only a window of this same exe counts, so another program that names
/// its window class the same way gets nothing.
pub(crate) fn find_notch() -> isize {
    let mut prev = HWND::default();
    for _ in 0..16 {
        let Ok(h) = (unsafe { FindWindowExW(HWND::default(), prev, w!("HesewakeNotchWindow"), PCWSTR::null()) }) else { return 0 };
        if h.0.is_null() {
            return 0;
        }
        if is_ours(h.0 as isize) {
            return h.0 as isize;
        }
        prev = h;
    }
    0
}

/// Is `hwnd` a window of a process running this same exe?
pub(crate) fn is_ours(hwnd: isize) -> bool {
    if hwnd == 0 {
        return false;
    }
    let Ok(me) = std::env::current_exe() else { return false };
    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(HWND(hwnd as *mut c_void), Some(&mut pid));
        if pid == 0 {
            return false;
        }
        let Ok(proc) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return false };
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(proc, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
        let _ = CloseHandle(proc);
        ok && String::from_utf16_lossy(&buf[..len as usize]).eq_ignore_ascii_case(&me.to_string_lossy())
    }
}

/// Send `m` to the notch window `hwnd`. False when there is no window or it doesn't answer within 500 ms.
pub(crate) fn deliver(hwnd: isize, m: &Msg) -> bool {
    if hwnd == 0 {
        return false;
    }
    let bytes = encode(m);
    let cds = COPYDATASTRUCT { dwData: COPY_ID, cbData: bytes.len() as u32, lpData: bytes.as_ptr() as *mut c_void };
    let mut result = 0usize;
    let sent = unsafe {
        SendMessageTimeoutW(HWND(hwnd as *mut c_void), WM_COPYDATA, WPARAM(0), LPARAM(&cds as *const _ as isize), SMTO_ABORTIFHUNG, 500, Some(&mut result))
    };
    sent.0 != 0
}

/// The terminal (or editor) window this hook runs in: the first ancestor process with a visible top-level window,
/// preferring a window whose title contains `project`. 0 if none.
pub(crate) fn terminal_window(chain: &[(u32, String)], project: &str) -> isize {
    let want = project.to_lowercase();
    for &(pid, _) in chain {
        let wins = windows_of(pid);
        if wins.is_empty() {
            continue;
        }
        let best = wins.iter().find(|(_, title)| !want.is_empty() && title.to_lowercase().contains(&want)).or(wins.first());
        return best.map_or(0, |(h, _)| *h);
    }
    0
}

/// (pid, parent pid, exe name) of every running process.
pub(crate) fn parent_map() -> Vec<(u32, u32, String)> {
    let mut out = Vec::new();
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return out };
        let mut e = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut ok = Process32FirstW(snap, &mut e).is_ok();
        while ok {
            let n = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(e.szExeFile.len());
            out.push((e.th32ProcessID, e.th32ParentProcessID, String::from_utf16_lossy(&e.szExeFile[..n])));
            ok = Process32NextW(snap, &mut e).is_ok();
        }
        let _ = CloseHandle(snap);
    }
    out
}

struct Found {
    pid: u32,
    wins: Vec<(isize, String)>,
}

unsafe extern "system" fn collect(hwnd: HWND, lp: LPARAM) -> BOOL {
    let f = &mut *(lp.0 as *mut Found);
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    let owned = GetWindow(hwnd, GW_OWNER).is_ok_and(|o| !o.0.is_null());
    if pid == f.pid && IsWindowVisible(hwnd).as_bool() && !owned {
        let mut buf = [0u16; 256];
        let n = GetWindowTextW(hwnd, &mut buf).max(0) as usize;
        if n > 0 {
            f.wins.push((hwnd.0 as isize, String::from_utf16_lossy(&buf[..n])));
        }
    }
    BOOL(1)
}

/// Visible, titled, unowned top-level windows of process `pid`.
fn windows_of(pid: u32) -> Vec<(isize, String)> {
    let mut f = Found { pid, wins: Vec::new() };
    unsafe {
        let _ = EnumWindows(Some(collect), LPARAM(&mut f as *mut Found as isize));
    }
    f.wins
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

    /// Claude's final reply can be long (and JSON-escaped); the Stop event must still arrive.
    #[test]
    fn long_final_message_still_reaches_the_notch() {
        let reply = "Selesai, ini ringkasannya: é日本🙂 ".repeat(40_000);
        let json = format!(r#"{{"session_id":"s1","cwd":"C:\\p\\app","hook_event_name":"Stop","last_assistant_message":"{reply}"}}"#);
        assert!(json.len() > 1_000_000);
        let m = read_hook(json.as_bytes()).expect("Stop with a 1 MB reply");
        assert_eq!(m.kind, EventKind::Stop);
        assert!(m.text.chars().count() <= 200 && m.text.starts_with("Selesai"));
    }

    #[test]
    fn the_walk_skips_shells_and_stops_at_explorer() {
        let map = |rows: &[(u32, u32, &str)]| rows.iter().map(|&(p, pp, n)| (p, pp, n.to_string())).collect::<Vec<_>>();
        let tree = map(&[(10, 9, "hesewake-notch.exe"), (9, 8, "claude.exe"), (8, 7, "pwsh.exe"), (7, 6, "WindowsTerminal.exe"), (6, 5, "explorer.exe"), (5, 0, "userinit.exe")]);
        let chain = ancestors(&tree, 10);
        let names: Vec<&str> = chain.iter().map(|(_, n)| n.as_str()).collect();
        assert_eq!(names, ["claude.exe", "pwsh.exe", "WindowsTerminal.exe"], "Explorer and above are left out");
        assert_eq!(claude_pid(&chain), 9);
        let via_shell = map(&[(10, 9, "hesewake-notch.exe"), (9, 8, "cmd.exe"), (8, 7, "node.exe"), (7, 0, "Code.exe")]);
        assert_eq!(claude_pid(&ancestors(&via_shell, 10)), 8, "a shell between Claude and the hook is skipped");
        let looped = map(&[(10, 9, "a.exe"), (9, 10, "b.exe")]);
        assert!(ancestors(&looped, 10).len() <= 12, "a pid loop ends");
        assert!(ancestors(&[], 10).is_empty());
        assert_eq!(claude_pid(&[]), 0);
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

    // ---- the bridge process

    use std::cell::RefCell;
    use windows::core::w;
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::DataExchange::COPYDATASTRUCT;
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{CreateWindowExW, DefWindowProcW, DestroyWindow, RegisterClassW, HWND_MESSAGE, WINDOW_EX_STYLE, WINDOW_STYLE, WM_COPYDATA, WNDCLASSW};

    thread_local! {
        static GOT: RefCell<Option<Msg>> = const { RefCell::new(None) };
    }

    unsafe extern "system" fn catcher(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        if msg == WM_COPYDATA {
            let cds = &*(lp.0 as *const COPYDATASTRUCT);
            if cds.dwData == COPY_ID {
                let bytes = std::slice::from_raw_parts(cds.lpData as *const u8, cds.cbData as usize);
                GOT.with(|g| *g.borrow_mut() = decode(bytes));
                return LRESULT(1);
            }
        }
        DefWindowProcW(hwnd, msg, wp, lp)
    }

    /// A message-only window standing in for the notch.
    fn test_window() -> HWND {
        unsafe {
            let hinst = GetModuleHandleW(None).unwrap();
            let class = w!("HesewakeClaudeHookTest");
            let wc = WNDCLASSW { lpfnWndProc: Some(catcher), hInstance: hinst.into(), lpszClassName: class, ..Default::default() };
            RegisterClassW(&wc);
            CreateWindowExW(WINDOW_EX_STYLE(0), class, w!(""), WINDOW_STYLE(0), 0, 0, 0, 0, HWND_MESSAGE, None, hinst, None).unwrap()
        }
    }

    #[test]
    fn deliver_reaches_a_window() {
        let hwnd = test_window();
        let mut m = parse_hook(&hook("Stop", r#","last_assistant_message":"All done""#)).unwrap();
        m.hwnd = 42;
        assert!(deliver(hwnd.0 as isize, &m));
        assert_eq!(GOT.with(|g| g.borrow_mut().take()), Some(m));
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
    }

    #[test]
    fn deliver_without_notch_is_quick() {
        let m = parse_hook(&hook("Stop", "")).unwrap();
        let t = std::time::Instant::now();
        assert!(!deliver(0, &m));
        assert!(t.elapsed() < std::time::Duration::from_secs(1));
    }

    /// Only a window owned by this same exe gets the hook's data, not any window that copies the class name.
    #[test]
    fn only_our_own_exe_is_the_notch() {
        let hwnd = test_window();
        assert!(is_ours(hwnd.0 as isize), "a window of this process");
        let desktop = unsafe { windows::Win32::UI::WindowsAndMessaging::GetDesktopWindow() };
        assert!(!is_ours(desktop.0 as isize), "a window of another process");
        assert!(!is_ours(0));
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
    }

    #[test]
    fn own_ancestors_are_walked() {
        // The test runner may or may not sit in a terminal window; this must just return.
        let chain = ancestors(&parent_map(), unsafe { GetCurrentProcessId() });
        let _ = terminal_window(&chain, "hesewake-notch");
        let _ = find_notch();
    }
}
