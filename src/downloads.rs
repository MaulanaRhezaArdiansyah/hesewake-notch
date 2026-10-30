//! Download progress (issue #24): running browser downloads in the user's Downloads folder.
//! Windows cannot tell us another app's progress, but browsers write to a temporary file next to the final one
//! (Chrome / Edge `name.crdownload`, Firefox `name.part`) and rename it when they are done. A background thread
//! waits on folder changes (`FindFirstChangeNotificationW`, no polling while nothing downloads), and while a
//! download runs it reads the temp file size once a second for the speed. The final size is unknown, so the notch
//! shows size and speed, not a percentage.

use super::*;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::Mutex;
use windows::Win32::Foundation::{CloseHandle, BOOL, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::Storage::FileSystem::{
    FindCloseChangeNotification, FindFirstChangeNotificationW, FindNextChangeNotification, FILE_NOTIFY_CHANGE_FILE_NAME,
    FILE_NOTIFY_CHANGE_LAST_WRITE, FILE_NOTIFY_CHANGE_SIZE,
};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::Threading::{CreateEventW, SetEvent, WaitForMultipleObjects, INFINITE};
use windows::Win32::UI::Shell::{SHGetKnownFolderPath, FOLDERID_Downloads, KF_FLAG_DEFAULT};

/// Message to the window: the running downloads changed, or one finished (`take_update`).
pub(crate) const WM_DOWNLOADS: u32 = WM_APP + 16;

/// Temporary files of Chrome / Edge and Firefox.
const TEMP_SUFFIXES: [&str; 2] = [".crdownload", ".part"];
/// Speed over the last few seconds, so it does not jump around.
const SPEED_WINDOW: f64 = 3.0;
/// A download that has not grown for this long is paused or left over: hidden until it grows again.
pub(crate) const STALL_SECS: f64 = 60.0;

/// The real name of a browser temp file: "report.pdf.crdownload" -> "report.pdf". None for other files.
pub(crate) fn final_name(temp: &str) -> Option<&str> {
    let lower = temp.to_ascii_lowercase(); // same byte length, so the cut below stays on a char boundary
    TEMP_SUFFIXES.iter().find(|s| lower.ends_with(*s)).map(|s| &temp[..temp.len() - s.len()]).filter(|n| !n.is_empty())
}

/// "512 B", "23.4 MB", "112 MB", "1.5 GB" (1024-based, like Explorer and the browsers).
pub(crate) fn fmt_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let (mut v, mut i) = (bytes as f64 / 1024.0, 0);
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if v >= 100.0 {
        format!("{v:.0} {}", UNITS[i])
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

/// "4.1 MB/s".
pub(crate) fn fmt_speed(bps: u64) -> String {
    format!("{}/s", fmt_size(bps))
}

/// One temp file found in the folder: name, size, and seconds since it was last written.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Seen {
    pub(crate) name: String,
    pub(crate) size: u64,
    pub(crate) age: f64,
}

/// A running download as the notch shows it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DlView {
    /// Final file name ("report.pdf").
    pub(crate) name: String,
    /// Full path of the temp file (for "Show in folder").
    pub(crate) path: String,
    pub(crate) size: u64,
    /// Bytes per second.
    pub(crate) speed: u64,
}

#[derive(Clone, Debug)]
struct Item {
    temp: String,
    name: String,
    size: u64,
    samples: Vec<(f64, u64)>,
    /// When the size last changed (seconds, same clock as `now`).
    grew: f64,
}

impl Item {
    fn speed(&self) -> u64 {
        match (self.samples.first(), self.samples.last()) {
            (Some(&(t0, s0)), Some(&(t1, s1))) if t1 - t0 > 0.0 => (s1.saturating_sub(s0) as f64 / (t1 - t0)) as u64,
            _ => 0,
        }
    }
}

fn join(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{}\\{name}", dir.trim_end_matches('\\'))
    }
}

/// Follows the temp files from scan to scan.
#[derive(Default)]
pub(crate) struct Tracker {
    items: Vec<Item>,
}

impl Tracker {
    /// New scan at `now` (seconds). `done(name)` is the size of a finished file with that name, if it exists.
    /// Returns the names of downloads that finished since the last scan.
    pub(crate) fn update(&mut self, now: f64, seen: &[Seen], done: &dyn Fn(&str) -> Option<u64>) -> Vec<String> {
        let mut finished = Vec::new();
        self.items.retain(|it| {
            if seen.iter().any(|s| s.name == it.temp) {
                return true;
            }
            // The temp file is gone: renamed to the real name (done), or deleted (cancelled, nothing to say).
            // Firefox leaves an empty placeholder with the real name until it cancels, so an empty file only
            // counts when the download itself was empty.
            if done(&it.name).is_some_and(|size| size > 0 || it.size == 0) {
                finished.push(it.name.clone());
            }
            false
        });
        for s in seen {
            let Some(name) = final_name(&s.name) else { continue };
            match self.items.iter_mut().find(|it| it.temp == s.name) {
                Some(it) => {
                    if s.size < it.size {
                        it.samples.clear(); // started over
                    }
                    if s.size != it.size {
                        it.grew = now;
                    }
                    it.size = s.size;
                    it.samples.push((now, s.size));
                    it.samples.retain(|&(t, _)| now - t <= SPEED_WINDOW + 1e-6);
                }
                None => self.items.push(Item {
                    temp: s.name.clone(),
                    name: name.to_string(),
                    size: s.size,
                    samples: vec![(now, s.size)],
                    // a temp file not written for a long time is left over (or paused) until it grows
                    grew: now - s.age,
                }),
            }
        }
        finished
    }

    /// The downloads to show: grown within the last `STALL_SECS`. `dir` is the folder, for the paths.
    pub(crate) fn view(&self, now: f64, dir: &str) -> Vec<DlView> {
        self.items
            .iter()
            .filter(|it| now - it.grew <= STALL_SECS)
            .map(|it| DlView { name: it.name.clone(), path: join(dir, &it.temp), size: it.size, speed: it.speed() })
            .collect()
    }

    /// Whether to re-read sizes every second (something is showing).
    pub(crate) fn polling(&self, now: f64) -> bool {
        self.items.iter().any(|it| now - it.grew <= STALL_SECS)
    }
}

// ---------------------------------------------------------------- watcher thread

static TARGET: AtomicIsize = AtomicIsize::new(0);
/// Stop event of the running watcher (0 = none).
static STOP: Mutex<isize> = Mutex::new(0);
/// Newest list and the finished paths not yet taken by the window.
static PENDING: Mutex<Option<(Vec<DlView>, Vec<String>)>> = Mutex::new(None);

/// The user's Downloads folder (it can be moved, so it is asked for, not guessed).
fn downloads_dir() -> Option<String> {
    unsafe {
        let p = SHGetKnownFolderPath(&FOLDERID_Downloads, KF_FLAG_DEFAULT, None).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as *const c_void));
        s
    }
}

/// Turn the watcher on or off. Nothing runs while it is off.
pub(crate) fn set_enabled(hwnd: isize, on: bool) {
    if cfg!(test) {
        return; // unit tests drive `watch` on their own folder
    }
    TARGET.store(hwnd, Ordering::Relaxed);
    let Ok(mut stop) = STOP.lock() else { return };
    if *stop != 0 {
        unsafe {
            let _ = SetEvent(HANDLE(*stop as *mut c_void));
            let _ = CloseHandle(HANDLE(*stop as *mut c_void));
        }
        *stop = 0;
    }
    if !on {
        return;
    }
    let Some(dir) = downloads_dir() else { return };
    let Ok(ev) = (unsafe { CreateEventW(None, BOOL::from(true), BOOL::from(false), None) }) else { return };
    *stop = ev.0 as isize;
    let ev = ev.0 as isize;
    std::thread::spawn(move || watch(&dir, ev, &publish));
}

fn publish(list: Vec<DlView>, finished: Vec<String>) {
    if let Ok(mut p) = PENDING.lock() {
        let mut all = p.take().map(|(_, f)| f).unwrap_or_default();
        all.extend(finished);
        *p = Some((list, all));
    }
    unsafe {
        let _ = PostMessageW(HWND(TARGET.load(Ordering::Relaxed) as *mut c_void), WM_DOWNLOADS, WPARAM(0), LPARAM(0));
    }
}

/// The newest state from the watcher thread: running downloads and newly finished paths.
pub(crate) fn take_update() -> Option<(Vec<DlView>, Vec<String>)> {
    PENDING.lock().ok()?.take()
}

/// Browser temp files in `dir`, with their current size (read from the file, not the folder listing, which lags
/// behind while a file is open for writing).
fn scan(dir: &str) -> Vec<Seen> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    rd.flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            final_name(&name)?;
            let m = std::fs::metadata(e.path()).ok().filter(|m| m.is_file())?;
            let age = m.modified().ok().and_then(|t| t.elapsed().ok()).map_or(0.0, |d| d.as_secs_f64());
            Some(Seen { name, size: m.len(), age })
        })
        .collect()
}

/// Watch `dir` until the `stop` event is set. `sink` gets the running downloads whenever they change, and the full
/// paths of the ones that finished. Waits without a timeout while nothing downloads.
fn watch(dir: &str, stop: isize, sink: &dyn Fn(Vec<DlView>, Vec<String>)) {
    let filter = FILE_NOTIFY_CHANGE_FILE_NAME | FILE_NOTIFY_CHANGE_SIZE | FILE_NOTIFY_CHANGE_LAST_WRITE;
    let Ok(note) = (unsafe { FindFirstChangeNotificationW(&HSTRING::from(dir), BOOL::from(false), filter) }) else { return };
    let handles = [note, HANDLE(stop as *mut c_void)];
    let t0 = Instant::now();
    let mut tr = Tracker::default();
    let mut last: Vec<DlView> = Vec::new();
    loop {
        let now = t0.elapsed().as_secs_f64();
        let done = |name: &str| std::fs::metadata(join(dir, name)).ok().filter(|m| m.is_file()).map(|m| m.len());
        let finished: Vec<String> = tr.update(now, &scan(dir), &done).iter().map(|n| join(dir, n)).collect();
        let view = tr.view(now, dir);
        if view != last || !finished.is_empty() {
            sink(view.clone(), finished);
            last = view;
        }
        let timeout = if tr.polling(now) { 1000 } else { INFINITE };
        let r = unsafe { WaitForMultipleObjects(&handles, BOOL::from(false), timeout) };
        if r == WAIT_OBJECT_0 {
            unsafe {
                let _ = FindNextChangeNotification(note);
            }
            // a busy folder changes many times a second: look again a little later, once
            std::thread::sleep(Duration::from_millis(200));
        } else if r != WAIT_TIMEOUT {
            break; // stop event, or the folder went away
        }
    }
    unsafe {
        let _ = FindCloseChangeNotification(note);
    }
}

// ---------------------------------------------------------------- notch

/// The finished toast: a download icon, the file name, and Open / Show in folder.
pub(crate) const TOAST_DOWNLOAD: (f32, f32) = (440.0, 66.0);
const BTN_OPEN_W: f32 = 64.0;
const BTN_FOLDER_W: f32 = 120.0;

/// Open and Show-in-folder buttons of the finished toast.
pub(crate) fn toast_buttons() -> ((f32, f32, f32, f32), (f32, f32, f32, f32)) {
    let (w, h) = TOAST_DOWNLOAD;
    let (x0, cy) = (CX - w / 2.0, h / 2.0);
    let folder = (x0 + w - 16.0 - BTN_FOLDER_W, cy - 14.0, BTN_FOLDER_W, 28.0);
    let open = (folder.0 - 8.0 - BTN_OPEN_W, cy - 14.0, BTN_OPEN_W, 28.0);
    (open, folder)
}

/// Rows of the download list on the Ledge slide.
pub(crate) const LEDGE_ROWS: usize = 3;
pub(crate) const LEDGE_ROW_H: f32 = 30.0;

/// Top-left of download row `i` on the Ledge slide.
pub(crate) fn ledge_row(i: usize) -> (f32, f32) {
    (LX, shelf::SH_Y + 4.0 + i as f32 * LEDGE_ROW_H)
}

/// Down arrow onto a line (the download icon).
fn icon_download(g: &mut Gfx, cx: f32, cy: f32, s: f32, p: &Paint) {
    let lw = (s * 0.12).max(1.4);
    g.line(cx, cy - s * 0.45, cx, cy + s * 0.15, lw, p);
    g.line(cx - s * 0.28, cy - s * 0.12, cx, cy + s * 0.16, lw, p);
    g.line(cx + s * 0.28, cy - s * 0.12, cx, cy + s * 0.16, lw, p);
    g.line(cx - s * 0.38, cy + s * 0.45, cx + s * 0.38, cy + s * 0.45, lw, p);
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
    /// New state from the watcher: the running downloads, and the full paths of the ones that just finished.
    pub(crate) fn dl_apply(&mut self, list: Vec<DlView>, finished: Vec<String>) {
        if !self.cfg.downloads {
            self.dls.clear();
            return; // the watcher may still send one last update after the setting went off
        }
        // a download not seen before: the next time the pointer enters, open Ledge
        if list.iter().any(|d| !self.dls.iter().any(|o| o.path == d.path)) {
            self.dl_seen = false;
        }
        self.dls = list;
        for path in finished {
            self.push_toast(ToastKind::Downloaded { path });
        }
    }

    /// Total speed of the running downloads (None when nothing downloads).
    pub(crate) fn dl_speed(&self) -> Option<u64> {
        (!self.dls.is_empty()).then(|| self.dls.iter().map(|d| d.speed).sum())
    }

    /// The pointer just entered the notch: the first time after a new download starts, open the Ledge slide
    /// (where the downloads are listed). True if it switched.
    pub(crate) fn dl_on_hover(&mut self) -> bool {
        if self.dls.is_empty() || self.dl_seen {
            return false;
        }
        self.dl_seen = true;
        self.show_id(SlideId::Ledge);
        self.cur_id() == SlideId::Ledge
    }

    /// Click on the finished toast. True if taken.
    pub(crate) fn dl_toast_click(&mut self, x: f32, y: f32) -> bool {
        let Some(ToastKind::Downloaded { path }) = self.toast.as_ref().map(|t| t.kind.clone()) else { return false };
        let (w, h) = TOAST_DOWNLOAD;
        if !in_rect(x, y, CX - w / 2.0, 0.0, w, h) {
            return false;
        }
        let (o, f) = toast_buttons();
        if in_rect(x, y, o.0, o.1, o.2, o.3) {
            sys::open_url(&path);
        } else if in_rect(x, y, f.0, f.1, f.2, f.3) {
            sys::show_in_folder(&path);
        }
        self.end_toast();
        true
    }

    /// Click on the Ledge slide while downloads are listed: shows that file in Explorer. True if taken.
    pub(crate) fn dl_ledge_click(&mut self, x: f32, y: f32) -> bool {
        for i in 0..self.dls.len().min(LEDGE_ROWS) {
            let (rx, ry) = ledge_row(i);
            if in_rect(x, y, rx, ry, CW, LEDGE_ROW_H) {
                sys::show_in_folder(&self.dls[i].path);
                return true;
            }
        }
        false
    }

    pub(crate) fn draw_download_toast(&self, g: &mut Gfx, (w, h): (f32, f32), al: f32, t: f32, path: &str) {
        let (x0, cy) = (CX - w / 2.0, h / 2.0);
        const GREEN: [u8; 3] = [60, 200, 110];
        let sc = 0.6 + 0.4 * toast::pop(t).clamp(0.0, 1.2);
        let (icx, r) = (x0 + 34.0, 18.0 * sc);
        g.circle(icx, cy, r, &paint(GREEN[0], GREEN[1], GREEN[2], 0.22 * al));
        icon_download(g, icx, cy, 16.0 * sc, &paint(GREEN[0], GREEN[1], GREEN[2], al));

        let k = toast::ss(0.12, 0.4, t);
        let (open, folder) = toast_buttons();
        let tx = x0 + 66.0 + (1.0 - k) * 6.0;
        let maxw = open.0 - 10.0 - tx;
        let name = path.rsplit(['\\', '/']).next().unwrap_or(path);
        let name = fit(g, name, 14.0, true, maxw);
        g.txtb(&name, tx, cy - 19.0, 14.0, [255, 255, 255], al * k, maxw + 2.0);
        g.txt(tr("Downloaded"), tx, cy + 2.0, 12.0, TEXT_DIM, al * k, maxw);

        let cur = self.cursor_canvas();
        for ((bx, by, bw, bh), label, col) in [(open, tr("Open"), g.acc.deep), (folder, tr("Show in folder"), [58, 60, 72])] {
            let over = cur.is_some_and(|(x, y)| in_rect(x, y, bx, by, bw, bh));
            g.pill(bx, by, bw, bh, col, if over { 1.0 } else { 0.85 } * al * k, 1.0);
            let label = fit(g, label, 13.0, true, bw - 14.0);
            g.txt_c(&label, bx + bw / 2.0, by + ty(bh, 13.0), 13.0, [255, 255, 255], al * k, bw, true);
        }
    }

    /// The running downloads inside the Ledge drop zone. True if drawn (the shelf items wait meanwhile).
    pub(crate) fn draw_ledge_downloads(&self, g: &mut Gfx, ox: f32, al: f32) -> bool {
        let n = self.dls.len();
        if n == 0 {
            return false;
        }
        let acc = g.acc.bright;
        // with more downloads than rows, the last row says how many more there are
        let shown = if n > LEDGE_ROWS { LEDGE_ROWS - 1 } else { n };
        for (i, d) in self.dls.iter().take(shown).enumerate() {
            let (x, y) = ledge_row(i);
            let (x, cy) = (x + ox, y + LEDGE_ROW_H / 2.0);
            g.fill(&rrect(x + 8.0, y + 2.0, CW - 16.0, LEDGE_ROW_H - 4.0, 9.0), &paint(255, 255, 255, 0.05 * al));
            icon_download(g, x + 24.0, cy, 13.0, &paint(acc[0], acc[1], acc[2], al));
            let right = format!("{} \u{b7} {}", fmt_size(d.size), fmt_speed(d.speed));
            let rw = g.width(&right, 12.0);
            g.txt(&right, x + CW - 18.0 - rw, cy - 7.0, 12.0, TEXT_DIM, al, rw + 2.0);
            let room = CW - 18.0 - rw - 12.0 - 40.0;
            let name = fit(g, &d.name, 13.0, true, room);
            g.txtb(&name, x + 40.0, cy - 8.5, 13.0, [255, 255, 255], al, room + 2.0);
        }
        if n > shown {
            let (_, y) = ledge_row(shown);
            let more = fill(tr("+{} more"), &[&(n - shown)]);
            g.txt_c(&more, CX + ox, y + LEDGE_ROW_H / 2.0 - 7.0, 12.0, TEXT_DIM, al, CW, false);
        }
        true
    }

    /// A thin accent line along the bottom of the small notch while a download runs.
    pub(crate) fn draw_download_strip(&self, g: &mut Gfx, w: f32, h: f32, ca: f32) {
        if self.dls.is_empty() || self.toast.is_some() || self.thin() {
            return;
        }
        let c = g.acc.main;
        g.fill(&rrect(CX - w / 2.0 + 16.0, h - 3.0, w - 32.0, 2.0, 1.0), &paint(c[0], c[1], c[2], 0.75 * ca));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MB: u64 = 1024 * 1024;

    fn seen(name: &str, size: u64) -> Seen {
        Seen { name: name.into(), size, age: 0.0 }
    }

    fn none(_: &str) -> Option<u64> {
        None
    }

    #[test]
    fn temp_files_of_chrome_edge_and_firefox() {
        assert_eq!(final_name("report.pdf.crdownload"), Some("report.pdf"));
        assert_eq!(final_name("setup.exe.part"), Some("setup.exe"));
        assert_eq!(final_name("Movie.MKV.CRDOWNLOAD"), Some("Movie.MKV"), "any case");
        assert_eq!(final_name("Unconfirmed 123456.crdownload"), Some("Unconfirmed 123456"));
        assert_eq!(final_name("report.pdf"), None);
        assert_eq!(final_name("notes.partial"), None);
        assert_eq!(final_name(".crdownload"), None, "no name left");
        assert_eq!(final_name(".part"), None);
    }

    #[test]
    fn sizes_and_speeds_read_like_explorer() {
        assert_eq!(fmt_size(0), "0 B");
        assert_eq!(fmt_size(512), "512 B");
        assert_eq!(fmt_size(1536), "1.5 KB");
        assert_eq!(fmt_size(23 * MB + 400 * 1024), "23.4 MB");
        assert_eq!(fmt_size(112 * MB), "112 MB");
        assert_eq!(fmt_size(3 * 1024 * MB / 2), "1.5 GB");
        assert_eq!(fmt_speed(4 * MB + 100 * 1024), "4.1 MB/s");
        assert_eq!(fmt_speed(800 * 1024), "800 KB/s");
        assert_eq!(fmt_speed(0), "0 B/s");
    }

    #[test]
    fn speed_comes_from_the_last_seconds() {
        let mut t = Tracker::default();
        t.update(0.0, &[seen("a.zip.crdownload", 0)], &none);
        t.update(1.0, &[seen("a.zip.crdownload", MB)], &none);
        t.update(2.0, &[seen("a.zip.crdownload", 3 * MB)], &none);
        let v = t.view(2.0, "C:\\Dl");
        assert_eq!(v.len(), 1);
        assert_eq!((v[0].name.as_str(), v[0].size), ("a.zip", 3 * MB));
        assert_eq!(v[0].speed, 3 * MB / 2, "3 MB in 2 s");
        assert_eq!(v[0].path, "C:\\Dl\\a.zip.crdownload");
        // older samples drop out of the window
        t.update(3.0, &[seen("a.zip.crdownload", 4 * MB)], &none);
        t.update(4.0, &[seen("a.zip.crdownload", 5 * MB)], &none);
        t.update(5.0, &[seen("a.zip.crdownload", 6 * MB)], &none);
        assert_eq!(t.view(5.0, "C:\\Dl")[0].speed, MB, "1 MB/s over the last 3 s");
    }

    #[test]
    fn rename_at_the_end_is_a_finished_download() {
        let mut t = Tracker::default();
        t.update(0.0, &[seen("report.pdf.crdownload", MB)], &none);
        let done = t.update(1.0, &[], &|n| (n == "report.pdf").then_some(2 * MB));
        assert_eq!(done, vec!["report.pdf".to_string()]);
        assert!(t.view(1.0, "").is_empty());
        assert!(!t.polling(1.0), "nothing left to poll");
    }

    #[test]
    fn cancelled_download_is_silent() {
        let mut t = Tracker::default();
        t.update(0.0, &[seen("big.iso.crdownload", 5 * MB)], &none);
        assert!(t.update(1.0, &[], &none).is_empty(), "temp file deleted, no final file: cancelled");
        assert!(t.view(1.0, "").is_empty());
    }

    #[test]
    fn firefox_placeholder_is_not_a_finished_download() {
        // Firefox creates an empty "name" next to "name.part" and deletes both on cancel
        let mut t = Tracker::default();
        t.update(0.0, &[seen("a.zip.part", 5 * MB)], &none);
        assert!(t.update(1.0, &[], &|n| (n == "a.zip").then_some(0)).is_empty(), "an empty file is the placeholder");
        let mut t = Tracker::default();
        t.update(0.0, &[seen("empty.txt.part", 0)], &none);
        assert_eq!(t.update(1.0, &[], &|n| (n == "empty.txt").then_some(0)), vec!["empty.txt".to_string()], "a real empty file");
    }

    #[test]
    fn chrome_naming_the_file_later_is_not_a_finished_download() {
        let mut t = Tracker::default();
        t.update(0.0, &[seen("Unconfirmed 1.crdownload", MB)], &none);
        let done = t.update(1.0, &[seen("report.pdf.crdownload", 2 * MB)], &none);
        assert!(done.is_empty());
        let v = t.view(1.0, "");
        assert_eq!(v.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(), ["report.pdf"]);
    }

    #[test]
    fn leftover_temp_file_shows_only_once_it_grows() {
        let mut t = Tracker::default();
        let old = Seen { name: "old.zip.crdownload".into(), size: MB, age: 3600.0 };
        t.update(100.0, &[old.clone()], &none);
        assert!(t.view(100.0, "").is_empty(), "left over from an earlier session");
        assert!(!t.polling(100.0), "and no polling for it");
        t.update(101.0, &[Seen { size: 2 * MB, ..old }], &none);
        assert_eq!(t.view(101.0, "").len(), 1, "resumed");
    }

    #[test]
    fn paused_download_hides_after_a_minute() {
        let mut t = Tracker::default();
        t.update(0.0, &[seen("a.zip.crdownload", MB)], &none);
        t.update(1.0, &[seen("a.zip.crdownload", 2 * MB)], &none);
        t.update(30.0, &[seen("a.zip.crdownload", 2 * MB)], &none);
        let v = t.view(30.0, "");
        assert_eq!(v.len(), 1, "still shown");
        assert_eq!(v[0].speed, 0, "no bytes in the last seconds");
        t.update(1.0 + STALL_SECS + 1.0, &[seen("a.zip.crdownload", 2 * MB)], &none);
        assert!(t.view(1.0 + STALL_SECS + 1.0, "").is_empty());
    }

    /// A real folder: a temp file appears, grows and is renamed, and the watcher reports the running download and
    /// then the finished file. It uses its own folder under %TEMP%, so it is safe to run anywhere.
    #[test]
    fn watcher_follows_a_real_download() {
        use std::io::Write;
        use std::sync::mpsc;
        let dir = std::env::temp_dir().join(format!("hesewake-dl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let d = dir.to_string_lossy().into_owned();
        let stop = unsafe { CreateEventW(None, BOOL::from(true), BOOL::from(false), None) }.unwrap();
        let (tx, rx) = mpsc::channel();
        let (dd, sraw) = (d.clone(), stop.0 as isize);
        let th = std::thread::spawn(move || {
            watch(&dd, sraw, &|l, f| {
                let _ = tx.send((l, f));
            })
        });
        let wait_for = |what: &str, ok: &dyn Fn(&Vec<DlView>, &Vec<String>) -> bool| {
            let end = Instant::now() + Duration::from_secs(10);
            while Instant::now() < end {
                if let Ok((l, f)) = rx.recv_timeout(Duration::from_millis(200)) {
                    if ok(&l, &f) {
                        return (l, f);
                    }
                }
            }
            panic!("the watcher never reported {what}");
        };
        std::thread::sleep(Duration::from_millis(300));
        let temp = dir.join("report.pdf.crdownload");
        let mut f = std::fs::File::create(&temp).unwrap();
        f.write_all(&vec![1u8; 64 * 1024]).unwrap();
        f.flush().unwrap();
        let (l, _) = wait_for("the new download", &|l, _| l.iter().any(|d| d.name == "report.pdf"));
        assert_eq!(l[0].path, format!("{d}\\report.pdf.crdownload"));
        f.write_all(&vec![1u8; 1024 * 1024]).unwrap();
        f.flush().unwrap();
        wait_for("the size growing", &|l, _| l.iter().any(|d| d.size > 64 * 1024));
        drop(f);
        std::fs::rename(&temp, dir.join("report.pdf")).unwrap();
        let (l, fin) = wait_for("the finished file", &|_, f| !f.is_empty());
        assert_eq!(fin, vec![format!("{d}\\report.pdf")]);
        assert!(l.is_empty(), "nothing running any more");
        unsafe {
            SetEvent(stop).unwrap();
        }
        th.join().unwrap();
        unsafe {
            let _ = CloseHandle(stop);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
