//! Claude Code sessions in the notch: which sessions are working, waiting for the person, or done. The messages come
//! from `claude_hook` (one per hook call); this module keeps the state and decides what to show.

// Wired up step by step (toasts and chip come next).
#![allow(dead_code)]

use crate::claude_hook::{EventKind, Msg};
use std::time::{Duration, Instant};

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
