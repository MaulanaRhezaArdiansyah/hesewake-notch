//! The process tree: which agent process and which terminal window a hook call belongs to, and whether they still exist.

use std::ffi::c_void;
use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, BOOL, E_ACCESSDENIED, HWND, LPARAM, WAIT_TIMEOUT};
use windows::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, WaitForSingleObject, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
};
use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, GetWindow, GetWindowTextW, GetWindowThreadProcessId, IsWindow, IsWindowVisible, GW_OWNER};

/// How far up the process tree to look for the terminal window.
const MAX_DEPTH: usize = 12;

/// (pid, parent pid, exe name) of every running process.
pub fn parent_map() -> Vec<(u32, u32, String)> {
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

/// The process chain above `start` (parent first) as (pid, exe name), stopping before Explorer (the desktop).
pub fn ancestors(map: &[(u32, u32, String)], start: u32) -> Vec<(u32, String)> {
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

/// The agent's process among the ancestors: the first one that isn't a shell.
pub fn agent_pid(chain: &[(u32, String)]) -> u32 {
    const SHELLS: [&str; 6] = ["cmd.exe", "powershell.exe", "pwsh.exe", "bash.exe", "sh.exe", "conhost.exe"];
    chain.iter().find(|(_, n)| !SHELLS.iter().any(|s| n.eq_ignore_ascii_case(s))).map_or(0, |(p, _)| *p)
}

/// The terminal (or editor) window this hook runs in: the first ancestor process with a visible top-level window,
/// preferring a window whose title contains `project`. 0 if none.
pub fn terminal_window(chain: &[(u32, String)], project: &str) -> isize {
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

/// Is process `pid` still running? A process we may not open (e.g. elevated) counts as running.
pub fn process_running(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    unsafe {
        match OpenProcess(PROCESS_SYNCHRONIZE, false, pid) {
            Ok(h) => {
                let alive = WaitForSingleObject(h, 0) == WAIT_TIMEOUT;
                let _ = CloseHandle(h);
                alive
            }
            Err(e) => e.code() == E_ACCESSDENIED,
        }
    }
}

/// Does window `hwnd` still exist?
pub fn window_alive(hwnd: isize) -> bool {
    hwnd != 0 && unsafe { IsWindow(HWND(hwnd as *mut c_void)) }.as_bool()
}

/// The full path of process `pid`'s exe, if it may be read.
pub fn image_path(pid: u32) -> Option<String> {
    unsafe {
        let proc = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(proc, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
        let _ = CloseHandle(proc);
        ok.then(|| String::from_utf16_lossy(&buf[..len as usize]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::System::Threading::GetCurrentProcessId;

    fn map(rows: &[(u32, u32, &str)]) -> Vec<(u32, u32, String)> {
        rows.iter().map(|&(p, pp, n)| (p, pp, n.to_string())).collect()
    }

    #[test]
    fn the_walk_skips_shells_and_stops_at_explorer() {
        let tree = map(&[(10, 9, "hesewake-agents.exe"), (9, 8, "claude.exe"), (8, 7, "pwsh.exe"), (7, 6, "WindowsTerminal.exe"), (6, 5, "explorer.exe"), (5, 0, "userinit.exe")]);
        let chain = ancestors(&tree, 10);
        let names: Vec<&str> = chain.iter().map(|(_, n)| n.as_str()).collect();
        assert_eq!(names, ["claude.exe", "pwsh.exe", "WindowsTerminal.exe"], "Explorer and above are left out");
        assert_eq!(agent_pid(&chain), 9);
        let via_shell = map(&[(10, 9, "hesewake-agents.exe"), (9, 8, "cmd.exe"), (8, 7, "node.exe"), (7, 0, "Code.exe")]);
        assert_eq!(agent_pid(&ancestors(&via_shell, 10)), 8, "a shell between the agent and the hook is skipped");
        let looped = map(&[(10, 9, "a.exe"), (9, 10, "b.exe")]);
        assert!(ancestors(&looped, 10).len() <= 12, "a pid loop ends");
        assert!(ancestors(&[], 10).is_empty());
        assert_eq!(agent_pid(&[]), 0);
    }

    #[test]
    fn own_ancestors_are_walked() {
        let me = unsafe { GetCurrentProcessId() };
        assert!(parent_map().iter().any(|(p, ..)| *p == me), "this process is in the map");
        // the test runner may or may not sit in a terminal window; this must just return
        let chain = ancestors(&parent_map(), me);
        let _ = terminal_window(&chain, "hesewake-notch");
    }

    #[test]
    fn running_processes_and_live_windows() {
        assert!(process_running(unsafe { GetCurrentProcessId() }));
        assert!(!process_running(0xFFFF_FFF0));
        let desktop = unsafe { windows::Win32::UI::WindowsAndMessaging::GetDesktopWindow() };
        assert!(window_alive(desktop.0 as isize));
        assert!(!window_alive(0));
    }
}
