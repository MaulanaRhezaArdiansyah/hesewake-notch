//! Update check: asks GitHub for the latest release (at startup and every 24 hours, or from the menu) and tells the
//! person once per version when a newer one is out. Nothing is downloaded or installed.

use serde_json::Value;

/// The latest published release (drafts and pre-releases are never returned by this endpoint).
pub(crate) const LATEST_URL: &str = "https://api.github.com/repos/dirgaastya/hesewake-notch/releases/latest";
/// Only release pages of this repository are opened.
const RELEASES_PREFIX: &str = "https://github.com/dirgaastya/hesewake-notch/releases/";
/// Largest API answer read.
pub(crate) const MAX_BYTES: usize = 512 * 1024;

/// A release newer than the running version.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Release {
    /// Without the leading `v`, e.g. `0.9.0`.
    pub(crate) version: String,
    /// The release page.
    pub(crate) url: String,
}

/// `v1.2.3`, `1.2.3`, `1.2.3-alpha.4` → (1, 2, 3, pre-release?). None if it isn't a version.
pub(crate) fn parse_version(s: &str) -> Option<(u64, u64, u64, bool)> {
    let s = s.strip_prefix(['v', 'V']).unwrap_or(s);
    let s = s.split('+').next().unwrap_or("");
    let (core, pre) = match s.split_once('-') {
        Some((c, p)) => (c, !p.is_empty()),
        None => (s, false),
    };
    let mut parts = core.split('.');
    let mut num = || parts.next().filter(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())).and_then(|p| p.parse::<u64>().ok());
    let (a, b, c) = (num()?, num()?, num()?);
    if parts.next().is_some() {
        return None;
    }
    Some((a, b, c, pre))
}

/// Is `latest` newer than `current`? A pre-release `latest` never is.
pub(crate) fn is_newer(latest: &str, current: &str) -> bool {
    let (Some(l), Some(c)) = (parse_version(latest), parse_version(current)) else { return false };
    if l.3 {
        return false;
    }
    let (lv, cv) = ((l.0, l.1, l.2), (c.0, c.1, c.2));
    lv > cv || (lv == cv && c.3)
}

/// The release in the API's answer, if it is a proper, published release of this repository.
pub(crate) fn parse_release(json: &str) -> Option<Release> {
    let v: Value = serde_json::from_str(json).ok()?;
    let flag = |k: &str| v.get(k).and_then(Value::as_bool).unwrap_or(false);
    if flag("draft") || flag("prerelease") {
        return None;
    }
    let tag = v.get("tag_name").and_then(Value::as_str)?;
    let url = v.get("html_url").and_then(Value::as_str).filter(|u| u.starts_with(RELEASES_PREFIX))?;
    let (_, _, _, pre) = parse_version(tag)?;
    if pre {
        return None;
    }
    Some(Release { version: tag.trim_start_matches(['v', 'V']).to_string(), url: url.to_string() })
}

// ---------------------------------------------------------------- in the app

use super::*;
use std::sync::Mutex;

pub(crate) const WM_UPDATE: u32 = WM_APP + 12;
/// Automatic checks run at most this often.
pub(crate) const INTERVAL: Duration = Duration::from_secs(24 * 3600);
const CURRENT: &str = env!("CARGO_PKG_VERSION");

/// What an update toast says.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Note {
    Available(Release),
    UpToDate,
    Failed,
}

/// Result of the last check: (asked from the menu, the latest release or None when the check failed).
static SLOT: Mutex<Option<(bool, Option<Release>)>> = Mutex::new(None);

#[cfg(test)]
thread_local! {
    static FETCHES: std::cell::RefCell<Vec<bool>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Ask GitHub on a background thread; the answer comes back as `WM_UPDATE`.
pub(crate) fn fetch(hwnd: isize, manual: bool) {
    #[cfg(test)]
    {
        let _ = hwnd;
        FETCHES.with(|f| f.borrow_mut().push(manual));
    }
    #[cfg(not(test))]
    std::thread::spawn(move || {
        let latest = weather::http_get(LATEST_URL, MAX_BYTES).and_then(|body| parse_release(&body));
        if let Ok(mut slot) = SLOT.lock() {
            *slot = Some((manual, latest));
        }
        unsafe {
            let _ = PostMessageW(HWND(hwnd as *mut c_void), WM_UPDATE, WPARAM(0), LPARAM(0));
        }
    });
}

pub(crate) fn take() -> Option<(bool, Option<Release>)> {
    SLOT.lock().ok()?.take()
}

/// The checks started in this test thread (true = from the menu), cleared on read.
#[cfg(test)]
pub(crate) fn fetches() -> Vec<bool> {
    FETCHES.with(|f| std::mem::take(&mut *f.borrow_mut()))
}

/// The Download button of the "available" toast.
pub(crate) fn button_rect(w: f32, h: f32) -> (f32, f32, f32, f32) {
    let (bw, bh) = (88.0, 28.0);
    (CX + w / 2.0 - 16.0 - bw, h / 2.0 - bh / 2.0, bw, bh)
}

impl App {
    /// Called once a minute: start an automatic check when one is due. True if a check started.
    pub(crate) fn update_tick(&mut self, now: Instant) -> bool {
        if !self.cfg.update_check || self.update_busy {
            return false;
        }
        if self.update_last.is_some_and(|t| now.saturating_duration_since(t) < INTERVAL) {
            return false;
        }
        self.update_last = Some(now);
        self.update_busy = true;
        fetch(self.hwnd, false);
        true
    }

    /// "Check for updates" from the menu: always answers with a toast.
    pub(crate) fn update_check_now(&mut self) {
        self.update_last = Some(Instant::now());
        self.update_busy = true;
        fetch(self.hwnd, true);
    }

    pub(crate) fn on_update(&mut self, manual: bool, latest: Option<Release>) {
        self.update_busy = false;
        match latest {
            Some(r) if is_newer(&r.version, CURRENT) => {
                self.update = Some(r.clone());
                if manual || self.cfg.update_seen != r.version {
                    self.cfg.update_seen = r.version.clone();
                    self.save_settings();
                    self.push_toast(ToastKind::Update(Note::Available(r)));
                }
            }
            Some(_) => {
                self.update = None;
                if manual {
                    self.push_toast(ToastKind::Update(Note::UpToDate));
                }
            }
            None => {
                if manual {
                    self.push_toast(ToastKind::Update(Note::Failed));
                }
            }
        }
        self.last = Instant::now();
    }

    pub(crate) fn draw_update_toast(&self, g: &mut Gfx, (w, h): (f32, f32), al: f32, t: f32, note: &Note) {
        let (x0, cy) = (CX - w / 2.0, h / 2.0);
        let (rgb, title, sub, button) = match note {
            Note::Available(r) => (g.acc.bright, fill(tr("Version {} is available"), &[&r.version]), tr("Release notes and installer").to_string(), true),
            Note::UpToDate => ([60, 200, 110], tr("You're up to date").to_string(), fill(tr("Version {}"), &[&CURRENT]), false),
            Note::Failed => (TEXT_DIM, tr("Couldn't check for updates").to_string(), tr("Check your internet connection").to_string(), false),
        };
        let sc = 0.6 + 0.4 * toast::pop(t).clamp(0.0, 1.2);
        let (icx, r) = (x0 + 34.0, 18.0 * sc);
        g.circle(icx, cy, r, &paint(rgb[0], rgb[1], rgb[2], 0.22 * al));
        icon_download(g, icx, cy, 15.0 * sc, &paint(rgb[0], rgb[1], rgb[2], al));

        let k = toast::ss(0.12, 0.4, t);
        let tx = x0 + 66.0 + (1.0 - k) * 6.0;
        let right = if button { button_rect(w, h).2 + 16.0 + 10.0 } else { 16.0 };
        let maxw = x0 + w - right - tx;
        let title = if g.width_ex(&title, 14.0, true) > maxw { ellipsize(g, &title, 14.0, maxw) } else { title };
        g.txtb(&title, tx, cy - 19.0, 14.0, [255, 255, 255], al * k, maxw + 2.0);
        let sub = if g.width(&sub, 12.0) > maxw { ellipsize(g, &sub, 12.0, maxw) } else { sub };
        g.txt(&sub, tx, cy + 2.0, 12.0, TEXT_DIM, al * k, maxw);
        if button {
            let (bx, by, bw, bh) = button_rect(w, h);
            let over = self.cursor_canvas().is_some_and(|(x, y)| in_rect(x, y, bx, by, bw, bh));
            g.pill(bx, by, bw, bh, g.acc.deep, if over { 1.0 } else { 0.85 } * al * k, 1.0);
            g.txt_c(tr("Download"), bx + bw / 2.0, by + ty(bh, 13.0), 13.0, [255, 255, 255], al * k, bw, true);
        }
    }
}

/// A download arrow over a tray line, centred on (cx, cy), `s` tall.
fn icon_download(g: &mut Gfx, cx: f32, cy: f32, s: f32, p: &Paint<'static>) {
    let lw = (s * 0.13).max(1.4);
    g.line(cx, cy - 0.5 * s, cx, cy + 0.18 * s, lw, p);
    g.line(cx - 0.3 * s, cy - 0.1 * s, cx, cy + 0.2 * s, lw, p);
    g.line(cx + 0.3 * s, cy - 0.1 * s, cx, cy + 0.2 * s, lw, p);
    g.rect(cx - 0.45 * s, cy + 0.42 * s - lw / 2.0, 0.9 * s, lw, p);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_like_semver() {
        assert_eq!(parse_version("v0.8.0"), Some((0, 8, 0, false)));
        assert_eq!(parse_version("1.2.3-alpha.4"), Some((1, 2, 3, true)));
        assert_eq!(parse_version("V2.0.1+build5"), Some((2, 0, 1, false)));
        for bad in ["", "v", "1.2", "1.2.x", "a.b.c", "1.2.3.4", "-1.2.3"] {
            assert_eq!(parse_version(bad), None, "{bad}");
        }
        assert!(is_newer("v0.8.1", "0.8.0"));
        assert!(is_newer("0.9.0", "0.8.9"));
        assert!(is_newer("1.0.0", "0.99.99"));
        assert!(!is_newer("v0.8.0", "0.8.0"), "same version");
        assert!(!is_newer("0.7.9", "0.8.0"), "older");
        assert!(!is_newer("0.9.0-beta.1", "0.8.0"), "pre-releases are ignored");
        assert!(is_newer("0.8.0", "0.8.0-alpha.1"), "the release after its own pre-release");
        assert!(!is_newer("garbage", "0.8.0"));
        assert!(!is_newer("0.9.0", "garbage"), "can't tell: say nothing");
        assert!(!is_newer("0.10.0", "0.10.0"));
        assert!(is_newer("0.10.0", "0.9.0"), "numbers, not text");
    }

    /// The real endpoint (GitHub Actions only, it needs the network): the latest release is found and readable.
    #[test]
    fn real_latest_release_is_read() {
        if std::env::var_os("CI").is_none() {
            return;
        }
        let body = crate::weather::http_get(LATEST_URL, MAX_BYTES).expect("GitHub answers");
        let r = parse_release(&body).expect("a published release");
        assert!(parse_version(&r.version).is_some(), "{r:?}");
    }

    #[test]
    fn releases_are_read_and_checked() {
        let json = r#"{"tag_name":"v0.9.0","html_url":"https://github.com/dirgaastya/hesewake-notch/releases/tag/v0.9.0","draft":false,"prerelease":false}"#;
        assert_eq!(parse_release(json), Some(Release { version: "0.9.0".into(), url: "https://github.com/dirgaastya/hesewake-notch/releases/tag/v0.9.0".into() }));
        let bad = [
            "",
            "{",
            r#"{"message":"Not Found","documentation_url":"https://docs.github.com"}"#,
            r#"{"tag_name":"v0.9.0","html_url":"https://evil.example/releases/tag/v0.9.0"}"#,
            r#"{"tag_name":"v0.9.0","html_url":"https://github.com/dirgaastya/hesewake-notch/releases.evil/x"}"#,
            r#"{"tag_name":"v0.9.0","html_url":"http://github.com/dirgaastya/hesewake-notch/releases/tag/v0.9.0"}"#,
            r#"{"tag_name":"nightly","html_url":"https://github.com/dirgaastya/hesewake-notch/releases/tag/nightly"}"#,
            r#"{"tag_name":"v0.9.0","html_url":"https://github.com/dirgaastya/hesewake-notch/releases/tag/v0.9.0","draft":true}"#,
            r#"{"tag_name":"v0.9.0","html_url":"https://github.com/dirgaastya/hesewake-notch/releases/tag/v0.9.0","prerelease":true}"#,
        ];
        for b in bad {
            assert_eq!(parse_release(b), None, "{b}");
        }
    }
}
