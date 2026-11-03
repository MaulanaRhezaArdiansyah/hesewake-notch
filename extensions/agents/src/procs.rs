//! The process tree: which agent process and which terminal window a hook call belongs to, and whether they still exist.

/// How far up the process tree to look for the terminal window.
const MAX_DEPTH: usize = 12;

/// (pid, parent pid, exe name) of every running process.
pub fn parent_map() -> Vec<(u32, u32, String)> {
    todo!()
}

/// The process chain above `start` (parent first) as (pid, exe name), stopping before Explorer (the desktop).
pub fn ancestors(map: &[(u32, u32, String)], start: u32) -> Vec<(u32, String)> {
    let _ = (map, start, MAX_DEPTH);
    todo!()
}

/// The agent's process among the ancestors: the first one that isn't a shell.
pub fn agent_pid(chain: &[(u32, String)]) -> u32 {
    let _ = chain;
    todo!()
}

/// The terminal (or editor) window this hook runs in: the first ancestor process with a visible top-level window,
/// preferring a window whose title contains `project`. 0 if none.
pub fn terminal_window(chain: &[(u32, String)], project: &str) -> isize {
    let _ = (chain, project);
    todo!()
}

/// Is process `pid` still running? A process we may not open (e.g. elevated) counts as running.
pub fn process_running(pid: u32) -> bool {
    let _ = pid;
    todo!()
}

/// Does window `hwnd` still exist?
pub fn window_alive(hwnd: isize) -> bool {
    let _ = hwnd;
    todo!()
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
