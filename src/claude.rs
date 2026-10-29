//! Claude Code sessions in the notch: which sessions are working, waiting for the person, or done. The messages come
//! from `claude_hook` (one per hook call); this module keeps the state and decides what to show.

// Wired up step by step (toasts and chip come next).
#![allow(dead_code)]

use super::*;
use crate::claude_hook::{EventKind, Msg};

/// A session that sends nothing for this long while it seems busy has most likely crashed or was closed.
pub(crate) const STALE: Duration = Duration::from_secs(15 * 60);
pub(crate) const MAX_SESSIONS: usize = 16;

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum State {
    /// Started, nothing running yet.
    Idle,
    Working,
    /// A permission prompt or a question is waiting.
    NeedsYou,
    /// The person went to the window to answer; quiet until the session's next event.
    Answered,
    Done,
    Error,
}

pub(crate) struct Session {
    pub(crate) id: String,
    pub(crate) project: String,
    pub(crate) state: State,
    pub(crate) turn_start: Option<Instant>,
    pub(crate) last: Instant,
    pub(crate) text: String,
    pub(crate) hwnd: isize,
}

/// Something the notch should pop up.
#[derive(Clone, PartialEq, Debug)]
pub(crate) enum Show {
    NeedsYou { session: String, project: String, text: String },
    Done { session: String, project: String, text: String, secs: Option<u64> },
    Error { session: String, project: String },
}

#[derive(Default)]
pub(crate) struct Sessions {
    pub(crate) list: Vec<Session>,
}

impl Sessions {
    /// Apply one hook message. `fg` is the foreground window now (0 = unknown).
    pub(crate) fn apply(&mut self, m: &Msg, now: Instant, fg: isize) -> Option<Show> {
        if m.kind == EventKind::SessionEnd {
            self.list.retain(|s| s.id != m.session);
            return None;
        }
        let i = match self.list.iter().position(|s| s.id == m.session) {
            Some(i) => i,
            // A subagent alone doesn't create a session.
            None if m.subagent => return None,
            None => {
                if self.list.len() >= MAX_SESSIONS {
                    if let Some(old) = self.list.iter().enumerate().min_by_key(|(_, s)| s.last).map(|(i, _)| i) {
                        self.list.remove(old);
                    }
                }
                self.list.push(Session { id: m.session.clone(), project: m.project.clone(), state: State::Idle, turn_start: None, last: now, text: String::new(), hwnd: 0 });
                self.list.len() - 1
            }
        };
        let s = &mut self.list[i];
        s.last = now;
        if m.hwnd != 0 {
            s.hwnd = m.hwnd;
        }
        if !m.project.is_empty() {
            s.project = m.project.clone();
        }
        // Subagents and background agents keep the session alive but never start or end a turn.
        if m.subagent {
            return None;
        }
        match m.kind {
            EventKind::SessionStart | EventKind::SessionEnd => None,
            EventKind::Prompt => {
                s.state = State::Working;
                s.turn_start = Some(now);
                None
            }
            EventKind::Tool => {
                match s.state {
                    // A tool after the turn ended: a new turn started without a prompt (e.g. a background task).
                    State::Done | State::Error => s.turn_start = Some(now),
                    _ => {}
                }
                s.state = State::Working;
                None
            }
            EventKind::NeedsYou => {
                s.state = State::NeedsYou;
                s.text = m.text.clone();
                Some(Show::NeedsYou { session: s.id.clone(), project: s.project.clone(), text: s.text.clone() })
            }
            EventKind::Stop | EventKind::StopFailure => {
                let secs = s.turn_start.take().map(|t| now.saturating_duration_since(t).as_secs());
                let failed = m.kind == EventKind::StopFailure;
                s.state = if failed { State::Error } else { State::Done };
                s.text = m.text.clone();
                let (id, project, text, hwnd) = (s.id.clone(), s.project.clone(), s.text.clone(), s.hwnd);
                // The person is already looking at this session's window.
                if fg != 0 && fg == hwnd && self.shares_window(hwnd) == 1 {
                    return None;
                }
                Some(if failed { Show::Error { session: id, project } } else { Show::Done { session: id, project, text, secs } })
            }
        }
    }

    /// Housekeeping, about once a second. `alive` says whether a window still exists. True if anything changed.
    pub(crate) fn tick(&mut self, now: Instant, fg: isize, alive: impl Fn(isize) -> bool) -> bool {
        let before = self.list.len();
        self.list.retain(|s| {
            let silent = now.saturating_duration_since(s.last) > STALE && matches!(s.state, State::Working | State::Answered);
            !silent && (s.hwnd == 0 || alive(s.hwnd))
        });
        let mut changed = self.list.len() != before;
        if fg != 0 && self.shares_window(fg) == 1 {
            for s in self.list.iter_mut().filter(|s| s.hwnd == fg && s.state == State::NeedsYou) {
                s.state = State::Answered;
                changed = true;
            }
        }
        changed
    }

    /// The person dealt with the prompt (clicked the toast).
    pub(crate) fn clear_needs(&mut self, session: &str) {
        for s in self.list.iter_mut().filter(|s| s.id == session && s.state == State::NeedsYou) {
            s.state = State::Answered;
        }
    }

    pub(crate) fn working(&self) -> usize {
        self.list.iter().filter(|s| s.state == State::Working).count()
    }

    /// The session that has been waiting longest.
    pub(crate) fn needing(&self) -> Option<&Session> {
        self.list.iter().filter(|s| s.state == State::NeedsYou).min_by_key(|s| s.last)
    }

    pub(crate) fn needing_count(&self) -> usize {
        self.list.iter().filter(|s| s.state == State::NeedsYou).count()
    }

    pub(crate) fn window_of(&self, session: &str) -> isize {
        self.get(session).map_or(0, |s| s.hwnd)
    }

    pub(crate) fn get(&self, session: &str) -> Option<&Session> {
        self.list.iter().find(|s| s.id == session)
    }

    /// How many sessions run in window `hwnd` (tabs of one terminal share it).
    fn shares_window(&self, hwnd: isize) -> usize {
        self.list.iter().filter(|s| s.hwnd == hwnd).count()
    }
}

// ---------------------------------------------------------------- in the notch

const ORANGE: [u8; 3] = [255, 159, 10];
const GREEN: [u8; 3] = [60, 200, 110];
const RED: [u8; 3] = [255, 69, 58];

impl Show {
    pub(crate) fn session(&self) -> &str {
        match self {
            Show::NeedsYou { session, .. } | Show::Done { session, .. } | Show::Error { session, .. } => session,
        }
    }
}

/// "6m 12s", "1h 5m", "40s".
pub(crate) fn duration_text(secs: u64) -> String {
    if secs < 60 {
        fill(tr("{}s"), &[&secs])
    } else if secs < 3600 {
        fill(tr("{}m {}s"), &[&(secs / 60), &(secs % 60)])
    } else {
        fill(tr("{}h {}m"), &[&(secs / 3600), &(secs % 3600 / 60)])
    }
}

/// A terminal prompt `>_`, the neutral Claude Code icon, centred on (cx, cy), `s` wide.
pub(crate) fn icon_prompt(g: &mut Gfx, cx: f32, cy: f32, s: f32, p: &Paint<'static>) {
    let lw = (s * 0.14).max(1.4);
    g.line(cx - 0.45 * s, cy - 0.32 * s, cx - 0.08 * s, cy, lw, p);
    g.line(cx - 0.08 * s, cy, cx - 0.45 * s, cy + 0.32 * s, lw, p);
    g.rect(cx + 0.04 * s, cy + 0.32 * s - lw / 2.0, 0.42 * s, lw, p);
}

impl App {
    /// A message from a hook (`WM_COPYDATA`).
    pub(crate) fn on_claude(&mut self, m: &Msg) {
        let fg = unsafe { GetForegroundWindow() }.0 as isize;
        if let Some(show) = self.claude.apply(m, Instant::now(), fg) {
            self.push_toast(ToastKind::Claude(show));
        }
        self.last = Instant::now();
    }

    /// About once a second: drop sessions that went away, notice the person answering. True if anything changed.
    pub(crate) fn claude_tick(&mut self) -> bool {
        if self.claude.list.is_empty() {
            return false;
        }
        let fg = unsafe { GetForegroundWindow() }.0 as isize;
        self.claude.tick(Instant::now(), fg, |h| unsafe { IsWindow(HWND(h as *mut c_void)).as_bool() })
    }

    /// Bring the session's terminal to the front and count the prompt as answered.
    pub(crate) fn claude_focus(&mut self, session: &str) {
        let h = self.claude.window_of(session);
        if h != 0 {
            let hwnd = HWND(h as *mut c_void);
            unsafe {
                if IsIconic(hwnd).as_bool() {
                    let _ = ShowWindow(hwnd, SW_RESTORE);
                }
                let _ = SetForegroundWindow(hwnd);
            }
        }
        self.claude.clear_needs(session);
    }

    pub(crate) fn draw_claude_toast(&self, g: &mut Gfx, (w, h): (f32, f32), al: f32, t: f32, show: &Show) {
        let (x0, cy) = (CX - w / 2.0, h / 2.0);
        let join = |project: &str, text: &str| if text.is_empty() { project.to_string() } else { format!("{project} \u{b7} {text}") };
        let (rgb, title, sub) = match show {
            Show::NeedsYou { project, text, .. } => (ORANGE, tr("Claude needs you").to_string(), join(project, text)),
            Show::Done { project, text, secs, .. } => {
                let title = match secs {
                    Some(s) => fill(tr("Done \u{b7} {}"), &[&duration_text(*s)]),
                    None => tr("Done").to_string(),
                };
                (GREEN, title, join(project, text))
            }
            Show::Error { project, .. } => (RED, tr("Stopped with an error").to_string(), project.clone()),
        };
        let sc = 0.6 + 0.4 * toast::pop(t).clamp(0.0, 1.2);
        let (icx, r) = (x0 + 34.0, 18.0 * sc);
        g.circle(icx, cy, r, &paint(rgb[0], rgb[1], rgb[2], 0.22 * al));
        icon_prompt(g, icx, cy, 16.0 * sc, &paint(rgb[0], rgb[1], rgb[2], al));

        let k = toast::ss(0.12, 0.4, t);
        let tx = x0 + 66.0 + (1.0 - k) * 6.0;
        let maxw = x0 + w - 16.0 - tx;
        let title = if g.width_ex(&title, 14.0, true) > maxw { ellipsize(g, &title, 14.0, maxw) } else { title };
        g.txtb(&title, tx, cy - 19.0, 14.0, [255, 255, 255], al * k, maxw + 2.0);
        let sub = if g.width(&sub, 12.0) > maxw { ellipsize(g, &sub, 12.0, maxw) } else { sub };
        g.txt(&sub, tx, cy + 2.0, 12.0, TEXT_DIM, al * k, maxw);
    }
}

// ---------------------------------------------------------------- chip in the collapsed notch

/// Frame interval while the working dots move.
pub(crate) const DOTS_MS: u32 = 100;
/// Chip: a little gap, the `>_` icon, three dots.
const CHIP_W: f32 = 34.0;
/// Chip while waiting: the icon only (no dots).
const CHIP_W_WAITING: f32 = 22.0;
/// Extra room for a session count ("2") when several sessions are in the same state.
const COUNT_W: f32 = 12.0;

impl App {
    /// What the chip shows: (waiting for the person, number of sessions in that state). None when hidden.
    fn claude_chip(&self) -> Option<(bool, usize)> {
        if self.expanded() || self.thin() || self.toast.is_some() || self.splash_t().is_some() {
            return None;
        }
        match (self.claude.needing_count(), self.claude.working()) {
            (0, 0) => None,
            (0, n) => Some((false, n)),
            (n, _) => Some((true, n)),
        }
    }

    /// Width the chip takes at the right of the collapsed notch.
    pub(crate) fn claude_chip_w(&self) -> f32 {
        self.claude_chip().map_or(0.0, |(needs, n)| if needs { CHIP_W_WAITING } else { CHIP_W } + if n > 1 { COUNT_W } else { 0.0 })
    }

    /// Extra notch width for the chip. The idle content is centred, so it grows on both sides; the playing layout is
    /// anchored at its edges, so it only needs the chip's own width.
    pub(crate) fn claude_extra(&self) -> f32 {
        self.claude_chip_w() * if self.media.playing { 1.0 } else { 2.0 }
    }

    /// The working dots are on screen, so frames are needed.
    pub(crate) fn claude_animating(&self) -> bool {
        !self.display_off && matches!(self.claude_chip(), Some((false, _)))
    }

    pub(crate) fn draw_claude_chip(&self, g: &mut Gfx, w: f32, h: f32, ca: f32) {
        let Some((needs, n)) = self.claude_chip() else { return };
        if ca <= 0.05 {
            return;
        }
        let right = CX + w / 2.0 - 10.0 - self.priv_extra();
        let left = right - self.claude_chip_w();
        let cy = h / 2.0;
        let (icx, rgb, a) = (left + 12.0, if needs { ORANGE } else { [235, 235, 240] }, if needs { ca } else { 0.85 * ca });
        icon_prompt(g, icx, cy, 13.0, &paint(rgb[0], rgb[1], rgb[2], a));
        let mut x = icx + 10.0;
        if !needs {
            // Three dots filling up one by one.
            let step = ((self.t * 1000.0 / DOTS_MS as f32) as usize) % 4;
            for i in 0..3 {
                let on = if i < step { 0.95 } else { 0.28 };
                g.circle(x + i as f32 * 5.0, cy + 3.0, 1.4, &paint(255, 255, 255, on * ca));
            }
            x += 15.0;
        }
        if n > 1 {
            g.txt(&n.to_string(), x + 1.0, ty(h, 10.0), 10.0, rgb, ca, COUNT_W);
        }
    }

    /// The thin busy line: an orange mark while a session waits for the person.
    pub(crate) fn draw_claude_thin(&self, g: &mut Gfx, w: f32, h: f32) {
        if self.claude.needing_count() == 0 || h > 12.0 {
            return;
        }
        let x = CX + w / 2.0 - 24.0;
        g.fill(&rrect(x, 0.0, 16.0, h.max(2.0), 1.5), &paint(ORANGE[0], ORANGE[1], ORANGE[2], 1.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude_hook::VERSION;

    fn msg(kind: EventKind, session: &str, hwnd: isize) -> Msg {
        Msg { v: VERSION, kind, session: session.into(), project: "app".into(), cwd: "C:\\p\\app".into(), text: String::new(), hwnd, subagent: false }
    }

    fn state(s: &Sessions, id: &str) -> Option<State> {
        s.get(id).map(|x| x.state)
    }

    #[test]
    fn prompt_then_stop_shows_done_with_duration() {
        let mut s = Sessions::default();
        let t0 = Instant::now();
        assert_eq!(s.apply(&msg(EventKind::SessionStart, "a", 7), t0, 0), None);
        assert_eq!(s.apply(&msg(EventKind::Prompt, "a", 7), t0, 0), None);
        assert_eq!(state(&s, "a"), Some(State::Working));
        assert_eq!(s.working(), 1);
        let mut stop = msg(EventKind::Stop, "a", 7);
        stop.text = "PR is up".into();
        let shown = s.apply(&stop, t0 + Duration::from_secs(372), 0);
        assert_eq!(shown, Some(Show::Done { session: "a".into(), project: "app".into(), text: "PR is up".into(), secs: Some(372) }));
        assert_eq!(state(&s, "a"), Some(State::Done));
        assert_eq!(s.working(), 0);
    }

    #[test]
    fn stop_without_prompt_has_no_duration() {
        let mut s = Sessions::default();
        let shown = s.apply(&msg(EventKind::Stop, "a", 7), Instant::now(), 0);
        assert!(matches!(shown, Some(Show::Done { secs: None, .. })), "{shown:?}");
    }

    #[test]
    fn permission_prompt_needs_you_until_answered() {
        let mut s = Sessions::default();
        let t0 = Instant::now();
        s.apply(&msg(EventKind::Prompt, "a", 7), t0, 0);
        let mut n = msg(EventKind::NeedsYou, "a", 7);
        n.text = "Claude needs your permission".into();
        assert_eq!(s.apply(&n, t0, 0), Some(Show::NeedsYou { session: "a".into(), project: "app".into(), text: "Claude needs your permission".into() }));
        assert_eq!(s.needing().map(|x| x.id.as_str()), Some("a"));
        assert!(!s.tick(t0, 99, |_| true), "another window in front changes nothing");
        assert!(s.tick(t0, 7, |_| true));
        assert_eq!(state(&s, "a"), Some(State::Answered));
        assert!(s.needing().is_none());
        s.apply(&msg(EventKind::Tool, "a", 7), t0, 7);
        assert_eq!(state(&s, "a"), Some(State::Working));
    }

    #[test]
    fn shared_window_does_not_clear_needs_you() {
        let mut s = Sessions::default();
        let t0 = Instant::now();
        s.apply(&msg(EventKind::Prompt, "a", 7), t0, 0);
        s.apply(&msg(EventKind::Prompt, "b", 7), t0, 0);
        s.apply(&msg(EventKind::NeedsYou, "a", 7), t0, 0);
        s.tick(t0, 7, |_| true);
        assert_eq!(state(&s, "a"), Some(State::NeedsYou), "window 7 has two sessions: can't tell which tab is in front");
        s.clear_needs("a");
        assert_eq!(state(&s, "a"), Some(State::Answered));
    }

    #[test]
    fn done_is_quiet_when_its_window_is_in_front() {
        let mut s = Sessions::default();
        let t0 = Instant::now();
        s.apply(&msg(EventKind::Prompt, "a", 7), t0, 0);
        assert_eq!(s.apply(&msg(EventKind::Stop, "a", 7), t0, 7), None);
        assert_eq!(state(&s, "a"), Some(State::Done));
        s.apply(&msg(EventKind::Prompt, "a", 7), t0, 0);
        assert!(matches!(s.apply(&msg(EventKind::StopFailure, "a", 7), t0, 0), Some(Show::Error { .. })));
        assert_eq!(state(&s, "a"), Some(State::Error));
    }

    #[test]
    fn subagents_never_change_state() {
        let mut s = Sessions::default();
        let t0 = Instant::now();
        s.apply(&msg(EventKind::Prompt, "a", 7), t0, 0);
        s.apply(&msg(EventKind::Stop, "a", 7), t0, 0);
        let mut sub = msg(EventKind::Tool, "a", 7);
        sub.subagent = true;
        assert_eq!(s.apply(&sub, t0, 0), None);
        assert_eq!(state(&s, "a"), Some(State::Done));
        let mut sub_stop = msg(EventKind::Stop, "z", 7);
        sub_stop.subagent = true;
        assert_eq!(s.apply(&sub_stop, t0, 0), None);
        assert!(s.get("z").is_none(), "a subagent alone doesn't create a session");
    }

    #[test]
    fn tool_after_stop_starts_a_new_turn() {
        let mut s = Sessions::default();
        let t0 = Instant::now();
        s.apply(&msg(EventKind::Prompt, "a", 7), t0, 0);
        s.apply(&msg(EventKind::Stop, "a", 7), t0, 0);
        s.apply(&msg(EventKind::Tool, "a", 7), t0 + Duration::from_secs(10), 0);
        assert_eq!(state(&s, "a"), Some(State::Working));
        let shown = s.apply(&msg(EventKind::Stop, "a", 7), t0 + Duration::from_secs(40), 0);
        assert!(matches!(shown, Some(Show::Done { secs: Some(30), .. })), "{shown:?}");
    }

    #[test]
    fn stale_and_closed_windows_are_dropped() {
        let mut s = Sessions::default();
        let t0 = Instant::now();
        s.apply(&msg(EventKind::Prompt, "busy", 7), t0, 0);
        s.apply(&msg(EventKind::NeedsYou, "waiting", 8), t0, 0);
        s.apply(&msg(EventKind::Stop, "done", 9), t0, 0);
        assert!(!s.tick(t0 + Duration::from_secs(60), 0, |_| true));
        assert!(s.tick(t0 + STALE + Duration::from_secs(60), 0, |_| true));
        assert!(s.get("busy").is_none(), "working but silent for 16 min");
        assert!(s.get("waiting").is_some(), "a prompt can wait as long as it likes");
        assert!(s.get("done").is_some());
        assert!(s.tick(t0, 0, |h| h != 8));
        assert!(s.get("waiting").is_none(), "its window was closed");
    }

    #[test]
    fn session_end_removes() {
        let mut s = Sessions::default();
        let t0 = Instant::now();
        s.apply(&msg(EventKind::Prompt, "a", 7), t0, 0);
        assert_eq!(s.apply(&msg(EventKind::SessionEnd, "a", 7), t0, 0), None);
        assert!(s.list.is_empty());
        s.apply(&msg(EventKind::SessionEnd, "never-seen", 7), t0, 0);
        assert!(s.list.is_empty());
    }

    #[test]
    fn at_most_16_sessions() {
        let mut s = Sessions::default();
        let t0 = Instant::now();
        for i in 0..20u64 {
            s.apply(&msg(EventKind::Prompt, &format!("s{i}"), 7), t0 + Duration::from_secs(i), 0);
        }
        assert_eq!(s.list.len(), MAX_SESSIONS);
        assert!(s.get("s0").is_none() && s.get("s19").is_some(), "the oldest go first");
        assert_eq!(s.window_of("s19"), 7);
        assert_eq!(s.window_of("nope"), 0);
    }
}
