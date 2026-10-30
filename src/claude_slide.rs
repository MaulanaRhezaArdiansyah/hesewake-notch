//! The Claude slide (optional): the Claude Code sessions with Open, the plan usage, and New session, which opens
//! Windows Terminal running `claude` in a recent project folder (or one picked with the folder dialog).

use super::*;
use crate::claude::State;
use std::sync::Mutex;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Shell::{FileOpenDialog, IFileOpenDialog, FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, SIGDN_FILESYSPATH};

/// Message to the window: the folder dialog closed with a folder (`take_picked`).
pub(crate) const WM_CLAUDE_FOLDER: u32 = WM_APP + 18;

/// Recent project folders kept for New session.
pub(crate) const RECENT_MAX: usize = 8;
/// Recent folders shown in the New session list (the last row is "Other folder...").
pub(crate) const RECENT_SHOWN: usize = 3;
const RECENT_FILE: &str = "claude_recent.txt";

/// Put `dir` first in `list` (no duplicates, any case or trailing slash), at most `RECENT_MAX`. True if it changed.
pub(crate) fn remember(list: &mut Vec<String>, dir: &str) -> bool {
    let dir = dir.trim().trim_end_matches(['\\', '/']);
    if dir.is_empty() || dir.chars().any(char::is_control) {
        return false;
    }
    if list.first().is_some_and(|d| d.eq_ignore_ascii_case(dir)) {
        return false;
    }
    list.retain(|d| !d.eq_ignore_ascii_case(dir));
    list.insert(0, dir.to_string());
    list.truncate(RECENT_MAX);
    true
}

pub(crate) fn load_recent() -> Vec<String> {
    if cfg!(test) {
        return Vec::new();
    }
    let text = std::fs::read_to_string(data_path(RECENT_FILE)).unwrap_or_default();
    let mut list = Vec::new();
    for l in text.lines().rev() {
        remember(&mut list, l);
    }
    list
}

pub(crate) fn save_recent(list: &[String]) {
    if cfg!(test) {
        return;
    }
    write_data(RECENT_FILE, list.join("\n") + "\n");
}

/// The last folder name of a path, as the project name.
pub(crate) fn folder_name(dir: &str) -> &str {
    dir.trim_end_matches(['\\', '/']).rsplit(['\\', '/']).next().unwrap_or(dir)
}

/// `name` on `path_var` (the PATH variable) with one of `pathext`'s extensions. Only absolute folders are searched:
/// never `.` or a relative entry, so a project folder can't slip in its own `claude.cmd`.
pub(crate) fn find_on_path(path_var: &str, pathext: &str, name: &str) -> Option<std::path::PathBuf> {
    let exts: Vec<&str> = pathext.split(';').map(str::trim).filter(|e| e.starts_with('.')).collect();
    for dir in path_var.split(';').map(|d| d.trim().trim_matches('"')) {
        let dir = std::path::Path::new(dir);
        if dir.as_os_str().is_empty() || !dir.is_absolute() {
            continue;
        }
        for ext in &exts {
            let f = dir.join(format!("{name}{}", ext.to_ascii_lowercase()));
            if f.is_file() {
                return Some(f);
            }
        }
    }
    None
}

/// Windows Terminal treats `;` as a command separator in its arguments.
fn wt_escape(s: &str) -> String {
    s.replace(';', "\;")
}

/// The programs to try for a new session in `dir`, all by absolute path: Windows Terminal (from the app alias
/// folder), then the console. `claude` is the full path of Claude Code, or None to open just the terminal there.
/// (program, arguments, working folder)
pub(crate) fn launch_plan(dir: &str, claude: Option<&str>, system_root: &str, local_appdata: &str) -> Vec<(String, String, String)> {
    let mut plan = Vec::new();
    if !local_appdata.is_empty() {
        let wt = format!("{local_appdata}\Microsoft\WindowsApps\wt.exe");
        let cmd = claude.map_or(String::new(), |c| format!(" \"{}\"", wt_escape(c)));
        plan.push((wt, format!("-d \"{}\"{cmd}", wt_escape(dir)), dir.to_string()));
    }
    if !system_root.is_empty() {
        let args = claude.map_or("/k".to_string(), |c| format!("/k \"{c}\""));
        plan.push((format!("{system_root}\System32\cmd.exe"), args, dir.to_string()));
    }
    plan
}

#[cfg(test)]
thread_local! {
    static LAUNCHED: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Tests: the folders a new session was started in (and forget them).
#[cfg(test)]
pub(crate) fn launched() -> Vec<String> {
    LAUNCHED.with(|l| std::mem::take(&mut *l.borrow_mut()))
}

/// Start `claude` in `dir` (an existing folder): Windows Terminal, or the console when it is missing.
fn launch(dir: &str) {
    if !std::path::Path::new(dir).is_dir() {
        return;
    }
    #[cfg(test)]
    LAUNCHED.with(|l| l.borrow_mut().push(dir.to_string()));
    #[cfg(not(test))]
    {
        let env = |k: &str| std::env::var(k).unwrap_or_default();
        let pathext = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into());
        let claude = find_on_path(&env("PATH"), &pathext, "claude").map(|p| p.to_string_lossy().into_owned());
        for (exe, args, cwd) in launch_plan(dir, claude.as_deref(), &env("SystemRoot"), &env("LOCALAPPDATA")) {
            if std::path::Path::new(&exe).is_file() && shell_open(&exe, &args, &cwd) {
                return;
            }
        }
    }
}

/// `ShellExecuteW` "open" of an absolute path; false when Windows could not start it.
#[cfg(not(test))]
fn shell_open(exe: &str, args: &str, dir: &str) -> bool {
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let r = unsafe { ShellExecuteW(None, w!("open"), &HSTRING::from(exe), &HSTRING::from(args), &HSTRING::from(dir), SW_SHOWNORMAL) };
    r.0 as isize > 32
}

static PICKED: Mutex<Option<String>> = Mutex::new(None);

/// The folder dialog, on its own single-threaded COM thread; a picked folder comes back with `WM_CLAUDE_FOLDER`.
fn pick_folder(hwnd: isize) {
    if cfg!(test) {
        return;
    }
    std::thread::spawn(move || {
        let dir = unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let r = (|| -> windows::core::Result<String> {
                let dlg: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;
                dlg.SetOptions(dlg.GetOptions()? | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM)?;
                dlg.SetTitle(&HSTRING::from(tr("New session")))?;
                dlg.Show(HWND(hwnd as *mut c_void))?;
                let p = dlg.GetResult()?.GetDisplayName(SIGDN_FILESYSPATH)?;
                let s = p.to_string().unwrap_or_default();
                CoTaskMemFree(Some(p.0 as *const c_void));
                Ok(s)
            })();
            CoUninitialize();
            r.ok().filter(|s| !s.is_empty())
        };
        if let Some(dir) = dir {
            if let Ok(mut p) = PICKED.lock() {
                *p = Some(dir);
            }
            unsafe {
                let _ = PostMessageW(HWND(hwnd as *mut c_void), WM_CLAUDE_FOLDER, WPARAM(0), LPARAM(0));
            }
        }
    });
}

pub(crate) fn take_picked() -> Option<String> {
    PICKED.lock().ok()?.take()
}

// ---------------------------------------------------------------- layout

/// New session button at the top right.
pub(crate) const NEW_BTN: (f32, f32, f32, f32) = (RX - 120.0, 8.0, 120.0, 26.0);
/// Sessions on the left, usage on the right.
pub(crate) const LIST: (f32, f32, f32) = (LX, 42.0, 270.0);
pub(crate) const ROW_H: f32 = 32.0;
pub(crate) const ROWS: usize = 3;
const USAGE_X: f32 = LX + 288.0;
const USAGE_W: f32 = RX - USAGE_X;
const OPEN_W: f32 = 50.0;

/// Top-left of session (or New session list) row `i`.
pub(crate) fn row_pos(i: usize) -> (f32, f32) {
    (LIST.0, LIST.1 + i as f32 * ROW_H)
}

/// The Open button of row `i`.
pub(crate) fn open_rect(i: usize) -> (f32, f32, f32, f32) {
    let (x, y) = row_pos(i);
    (x + LIST.2 - OPEN_W - 6.0, y + 5.0, OPEN_W, ROW_H - 10.0)
}

fn state_color(s: State, acc: [u8; 3]) -> [u8; 3] {
    match s {
        State::Working => acc,
        State::NeedsYou => [255, 159, 10],
        State::Done => [60, 200, 110],
        State::Error => [255, 69, 58],
        State::Idle | State::Answered => [140, 144, 156],
    }
}

fn fit(g: &mut Gfx, s: &str, size: f32, bold: bool, maxw: f32) -> String {
    if g.width_ex(s, size, bold) <= maxw {
        return s.to_string();
    }
    match g.text.as_mut() {
        Some(t) => t.ellipsize(s, size, bold, maxw),
        None => s.to_string(),
    }
}

impl App {
    /// "Working · 3m", "Needs you", "Done", "Error", "Idle".
    pub(crate) fn claude_state_text(s: &claude::Session, now: Instant) -> String {
        match s.state {
            State::Working => {
                let secs = s.turn_start.map_or(0, |t| now.saturating_duration_since(t).as_secs());
                fill(tr("Working \u{b7} {}"), &[&claude::duration_text(secs)])
            }
            State::NeedsYou => tr("Needs you").to_string(),
            State::Done => tr("Done").to_string(),
            State::Error => tr("Error").to_string(),
            State::Idle | State::Answered => tr("Idle").to_string(),
        }
    }

    /// Start a new session in `dir` and keep it among the recent folders.
    pub(crate) fn claude_launch(&mut self, dir: &str) {
        self.claude_new = false;
        if !std::path::Path::new(dir).is_dir() {
            // a folder that is gone leaves the recent list
            self.claude_recent.retain(|d| !d.eq_ignore_ascii_case(dir));
            save_recent(&self.claude_recent);
            return;
        }
        launch(dir);
        if remember(&mut self.claude_recent, dir) {
            save_recent(&self.claude_recent);
        }
    }

    /// Recent folders shown in the New session list.
    fn claude_recent_shown(&self) -> usize {
        self.claude_recent.len().min(RECENT_SHOWN)
    }

    /// Click on the Claude slide. True if it did something.
    pub(crate) fn claude_click(&mut self, x: f32, y: f32) -> bool {
        let on = |r: (f32, f32, f32, f32)| in_rect(x, y, r.0, r.1, r.2, r.3);
        if self.claude_new {
            // the New session list: a recent folder, Other folder..., or anywhere else to close it
            let n = self.claude_recent_shown();
            for i in 0..=n {
                let (rx, ry) = row_pos(i);
                if in_rect(x, y, rx, ry, LIST.2, ROW_H) {
                    if i < n {
                        let dir = self.claude_recent[i].clone();
                        self.claude_launch(&dir);
                    } else {
                        self.claude_new = false;
                        pick_folder(self.hwnd);
                    }
                    return true;
                }
            }
            self.claude_new = false;
            return true;
        }
        if on(NEW_BTN) {
            self.claude_new = true;
            return true;
        }
        let visible = self.claude.list.len().saturating_sub(self.claude_scroll).min(ROWS);
        for vi in 0..visible {
            let (rx, ry) = row_pos(vi);
            if in_rect(x, y, rx, ry, LIST.2, ROW_H) {
                // a click on the row or its Open button brings the session's window to the front
                let id = self.claude.list[self.claude_scroll + vi].id.clone();
                self.claude_focus(&id);
                return true;
            }
        }
        false
    }

    /// Wheel over the session list. True if it scrolled.
    pub(crate) fn claude_wheel(&mut self, down: bool) -> bool {
        let max = self.claude.list.len().saturating_sub(ROWS);
        let old = self.claude_scroll;
        self.claude_scroll = if down { (old + 1).min(max) } else { old.saturating_sub(1) };
        self.claude_scroll != old
    }

    pub(crate) fn slide_claude(&self, g: &mut Gfx, ox: f32, al: f32) {
        let acc = g.acc.main;
        g.txtb("Claude Code", LX + ox, 12.0, 15.0, [255, 255, 255], al, 200.0);
        let (bx, by, bw, bh) = NEW_BTN;
        g.pill(bx + ox, by, bw, bh, g.acc.deep, if self.claude_new { 1.0 } else { 0.85 } * al, 1.0);
        let label = format!("+  {}", tr("New session"));
        let label = fit(g, &label, 12.0, true, bw - 16.0);
        g.txt_c(&label, bx + ox + bw / 2.0, by + ty(bh, 12.0), 12.0, [255, 255, 255], al, bw, true);

        self.draw_claude_usage(g, ox, al);
        if self.claude_new {
            self.draw_claude_new_list(g, ox, al);
            return;
        }
        let list = &self.claude.list;
        if list.is_empty() {
            let (x, y) = row_pos(0);
            g.txtb(tr("No Claude Code sessions"), x + ox, y + 18.0, 13.0, TEXT_DIM, al, LIST.2);
            let hint = if self.claude_link == claude_setup::Link::On { tr("Start one with New session") } else { tr("Connect in Settings > Alerts") };
            g.txt(hint, x + ox, y + 38.0, 11.0, TEXT_FAINT, al, LIST.2);
            return;
        }
        let now = Instant::now();
        for (vi, s) in list.iter().skip(self.claude_scroll).take(ROWS).enumerate() {
            let (x, y) = row_pos(vi);
            let (x, cy) = (x + ox, y + ROW_H / 2.0);
            g.fill(&rrect(x, y + 2.0, LIST.2, ROW_H - 4.0, 9.0), &paint(255, 255, 255, 0.05 * al));
            let c = state_color(s.state, acc);
            g.circle(x + 13.0, cy, 4.0, &paint(c[0], c[1], c[2], al));
            let state = Self::claude_state_text(s, now);
            let has_open = s.hwnd != 0;
            let room = LIST.2 - 26.0 - if has_open { OPEN_W + 14.0 } else { 8.0 };
            let project = fit(g, &s.project, 13.0, true, room);
            g.txtb(&project, x + 26.0, cy - 13.0, 13.0, [255, 255, 255], al, room + 2.0);
            let state = fit(g, &state, 11.0, false, room);
            g.txt(&state, x + 26.0, cy + 1.0, 11.0, c, al, room + 2.0);
            if has_open {
                let (ox2, oy, ow, oh) = open_rect(vi);
                g.pill(ox2 + ox, oy, ow, oh, [58, 60, 72], 0.9 * al, 1.0);
                g.txt_c(tr("Open"), ox2 + ox + ow / 2.0, oy + ty(oh, 11.0), 11.0, [255, 255, 255], al, ow, true);
            }
        }
        if list.len() > ROWS {
            // thin scrollbar at the right of the list
            let (tx, ty0, th) = (LIST.0 + LIST.2 + 4.0 + ox, LIST.1 + 4.0, ROWS as f32 * ROW_H - 8.0);
            let kh = th * ROWS as f32 / list.len() as f32;
            let ky = ty0 + (th - kh) * self.claude_scroll as f32 / (list.len() - ROWS) as f32;
            g.fill(&rrect(tx, ky, 3.0, kh, 1.5), &paint(255, 255, 255, 0.3 * al));
        }
    }

    /// The New session list over the sessions: recent folders, then Other folder...
    fn draw_claude_new_list(&self, g: &mut Gfx, ox: f32, al: f32) {
        let n = self.claude_recent_shown();
        let (x0, y0) = row_pos(0);
        g.fill(&rrect(x0 + ox, y0, LIST.2, (n + 1) as f32 * ROW_H, 11.0), &paint(22, 24, 32, al));
        for i in 0..=n {
            let (x, y) = row_pos(i);
            let (x, cy) = (x + ox, y + ROW_H / 2.0);
            if i < n {
                let dir = &self.claude_recent[i];
                let name = fit(g, folder_name(dir), 13.0, true, LIST.2 - 24.0);
                g.txtb(&name, x + 12.0, cy - 13.0, 13.0, [255, 255, 255], al, LIST.2 - 20.0);
                let path = fit(g, dir, 10.0, false, LIST.2 - 24.0);
                g.txt(&path, x + 12.0, cy + 1.5, 10.0, TEXT_FAINT, al, LIST.2 - 20.0);
            } else {
                g.txtb(tr("Other folder..."), x + 12.0, cy - 7.5, 12.0, g.acc.bright, al, LIST.2 - 20.0);
            }
            if i > 0 {
                g.rect(x + 10.0, y, LIST.2 - 20.0, 1.0, &paint(255, 255, 255, 0.08 * al));
            }
        }
    }

    /// Usage at the right: 5h and 7d with percentage, bar and reset time, or why there are no numbers.
    fn draw_claude_usage(&self, g: &mut Gfx, ox: f32, al: f32) {
        let x = USAGE_X + ox;
        g.txt(tr("Usage"), x, LIST.1 + 2.0, 11.0, TEXT_FAINT, al, USAGE_W);
        let now = notif::now_unix() as i64;
        let off = calendar::local_offset();
        let wins = [("5h", self.claude_window(false, now)), ("7d", self.claude_window(true, now))];
        if wins.iter().all(|(_, w)| w.is_none()) {
            let why = if self.claude_status == claude_setup::StatusLink::On { tr("After Claude's first reply") } else { tr("Turn on in Settings") };
            let why = fit(g, why, 11.0, false, USAGE_W);
            g.txt(&why, x, LIST.1 + 22.0, 11.0, TEXT_DIM, al, USAGE_W + 2.0);
            return;
        }
        for (k, (name, w)) in wins.iter().enumerate() {
            let y = LIST.1 + 20.0 + k as f32 * 44.0;
            g.txtb(name, x, y, 12.0, [255, 255, 255], al, 30.0);
            let pct = w.map_or("\u{2013}".to_string(), |w| format!("{:.0}%", w.pct));
            let warn = w.is_some_and(|w| w.pct >= claude_usage::WARN_PCT);
            let col = if warn { [255, 159, 10] } else { [255, 255, 255] };
            let pw = g.width_ex(&pct, 12.0, true);
            g.txtb(&pct, x + USAGE_W - pw, y, 12.0, col, al, pw + 2.0);
            let by = y + 18.0;
            g.fill(&rrect(x, by, USAGE_W, 4.0, 2.0), &paint(255, 255, 255, 0.14 * al));
            if let Some(w) = w {
                let c = if warn { [255, 159, 10] } else { g.acc.main };
                let fw = (USAGE_W * (w.pct / 100.0).clamp(0.0, 1.0)).max(4.0);
                g.fill(&rrect(x, by, fw, 4.0, 2.0), &paint(c[0], c[1], c[2], al));
                let reset = claude_usage::reset_text(w.resets_at, now, off);
                let reset = fit(g, &reset, 10.0, false, USAGE_W);
                g.txt(&reset, x, by + 7.0, 10.0, TEXT_FAINT, al, USAGE_W + 2.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_folders_newest_first_without_duplicates() {
        let mut l = Vec::new();
        assert!(remember(&mut l, "C:\\code\\notch"));
        assert!(remember(&mut l, "C:\\code\\api\\"));
        assert!(!remember(&mut l, "c:\\code\\api"), "same folder, other case and slash");
        assert!(remember(&mut l, "C:\\code\\notch"));
        assert_eq!(l, ["C:\\code\\notch", "C:\\code\\api"]);
        assert!(!remember(&mut l, "  "));
        assert!(!remember(&mut l, "C:\\bad\nname"));
        for i in 0..20 {
            remember(&mut l, &format!("C:\\p{i}"));
        }
        assert_eq!(l.len(), RECENT_MAX);
        assert_eq!(l[0], "C:\\p19");
    }

    #[test]
    fn folder_names_and_launch_plan() {
        assert_eq!(folder_name("C:\code\hesewake-notch\\"), "hesewake-notch");
        assert_eq!(folder_name("D:/work/api"), "api");
        let claude = "C:\Users\me\.local\bin\claude.exe";
        let plan = launch_plan("C:\My Code\a;b", Some(claude), "C:\Windows", "C:\Users\me\AppData\Local");
        assert_eq!(plan[0].0, "C:\Users\me\AppData\Local\Microsoft\WindowsApps\wt.exe", "absolute path");
        assert_eq!(plan[0].1, format!("-d \"C:\My Code\a\;b\" \"{claude}\""), "quoted, `;` escaped for Windows Terminal, claude by full path");
        assert_eq!(plan[1], ("C:\Windows\System32\cmd.exe".to_string(), format!("/k \"{claude}\""), "C:\My Code\a;b".to_string()), "fallback: the console in the folder");
        let plan = launch_plan("C:\p", None, "C:\Windows", "");
        assert_eq!(plan, vec![("C:\Windows\System32\cmd.exe".to_string(), "/k".to_string(), "C:\p".to_string())], "no claude found: just the terminal");
    }

    /// `claude` is only looked up in absolute PATH folders: a `claude.cmd` in `.` or a relative folder is never used.
    #[test]
    fn claude_is_found_only_in_absolute_path_folders() {
        let base = std::env::temp_dir().join(format!("hesewake-claude-path-{}", std::process::id()));
        let bin = base.join("bin");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("claude.cmd"), "@echo off").unwrap();
        let path = format!(".;relative\bin;\"{}\"", bin.display());
        assert_eq!(find_on_path(&path, ".COM;.EXE;.BAT;.CMD", "claude"), Some(bin.join("claude.cmd")));
        assert_eq!(find_on_path(".;relative", ".EXE;.CMD", "claude"), None);
        assert_eq!(find_on_path(&bin.display().to_string(), ".EXE", "claude"), None, "only the listed extensions");
        let _ = std::fs::remove_dir_all(&base);
    }
}
