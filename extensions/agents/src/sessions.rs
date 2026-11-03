//! The agent sessions the extension knows about and what each one is doing. Pure: the clock, the foreground window
//! and the "does this window / process still exist" checks are passed in.

use crate::event::{Agent, EventKind, Msg};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// A working session silent this long is gone (its terminal was killed without a SessionEnd).
pub const STALE_SECS: u64 = 15 * 60;
pub const MAX_SESSIONS: usize = 16;
/// Done, Error and Idle sessions stay in Recent this long.
pub const RECENT_KEEP_SECS: u64 = 2 * 3600;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// Started, nothing running.
    Idle,
    Working,
    /// A permission prompt or a question is waiting.
    NeedsYou,
    /// The person went to the window to answer; quiet until the session's next event.
    Answered,
    Done,
    Error,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Session {
    pub agent: Agent,
    pub id: String,
    pub project: String,
    pub cwd: String,
    pub branch: Option<String>,
    pub state: State,
    /// When the session entered its state (unix seconds).
    pub since: u64,
    /// The session's last event (unix seconds).
    pub last: u64,
    /// When the current (or last) turn started.
    pub turn_started: Option<u64>,
    /// The last tool that ran.
    pub tool: String,
    /// The waiting question, or the first line of the last reply.
    pub text: String,
    pub hwnd: isize,
    pub pid: u32,
}

/// Something worth a toast.
#[derive(Clone, Debug, PartialEq)]
pub enum Show {
    NeedsYou { session: String, project: String, agent: Agent, tool: String, text: String },
    Done { session: String, project: String, agent: Agent, secs: Option<u64> },
    Error { session: String, project: String, agent: Agent },
}

#[derive(Serialize, Deserialize, Default, Debug, PartialEq)]
pub struct Sessions {
    list: Vec<Session>,
}

impl Sessions {
    /// Apply one hook message. `fg` is the foreground window now (0 = unknown).
    pub fn apply(&mut self, m: &Msg, now: u64, fg: isize) -> Option<Show> {
        let _ = (m, now, fg, EventKind::Idle);
        todo!()
    }

    /// Housekeeping, about once a second. True if anything changed.
    pub fn tick(&mut self, now: u64, fg: isize, alive: impl Fn(isize) -> bool, running: impl Fn(u32) -> bool) -> bool {
        let _ = (now, fg, alive(0), running(0));
        todo!()
    }

    /// The person dealt with the prompt (clicked its row or toast).
    pub fn clear_needs(&mut self, id: &str) {
        let _ = id;
        todo!()
    }

    /// The person removed the row.
    pub fn dismiss(&mut self, id: &str) {
        let _ = id;
        todo!()
    }

    pub fn get(&self, id: &str) -> Option<&Session> {
        self.list.iter().find(|s| s.id == id)
    }

    pub fn list(&self) -> &[Session] {
        &self.list
    }

    pub fn needing_count(&self) -> usize {
        self.list.iter().filter(|s| s.state == State::NeedsYou).count()
    }

    /// The sessions saved by `save`; an unreadable file gives none.
    pub fn load(path: &Path) -> Sessions {
        let _ = path;
        todo!()
    }

    pub fn save(&self, path: &Path) {
        let _ = path;
        todo!()
    }
}

/// The git branch of the folder `cwd` (or of the repository it is in), from `.git/HEAD`; a detached head gives its
/// short hash.
pub fn branch_of(cwd: &str) -> Option<String> {
    let _ = cwd;
    todo!()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::event::VERSION;
    use std::path::PathBuf;

    pub(crate) fn msg(kind: EventKind, session: &str, hwnd: isize) -> Msg {
        Msg {
            v: VERSION,
            agent: Agent::Claude,
            kind,
            session: session.into(),
            project: "app".into(),
            cwd: "C:\\nowhere\\app".into(),
            text: String::new(),
            tool: String::new(),
            hwnd,
            subagent: false,
            pid: 0,
        }
    }

    fn state(s: &Sessions, id: &str) -> Option<State> {
        s.get(id).map(|x| x.state)
    }

    fn working(s: &Sessions) -> usize {
        s.list().iter().filter(|x| x.state == State::Working).count()
    }

    /// A fresh, empty folder under the temp dir, removed when dropped.
    pub(crate) struct TempDir(pub PathBuf);
    impl TempDir {
        pub(crate) fn new(name: &str) -> TempDir {
            let p = std::env::temp_dir().join(format!("hesewake-agents-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).unwrap();
            TempDir(p)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    const T0: u64 = 1_793_000_000;

    #[test]
    fn prompt_then_stop_shows_done_with_duration() {
        let mut s = Sessions::default();
        assert_eq!(s.apply(&msg(EventKind::SessionStart, "a", 7), T0, 0), None);
        assert_eq!(s.apply(&msg(EventKind::Prompt, "a", 7), T0, 0), None);
        assert_eq!(state(&s, "a"), Some(State::Working));
        assert_eq!(working(&s), 1);
        let mut stop = msg(EventKind::Stop, "a", 7);
        stop.text = "PR is up".into();
        let shown = s.apply(&stop, T0 + 372, 0);
        assert_eq!(shown, Some(Show::Done { session: "a".into(), project: "app".into(), agent: Agent::Claude, secs: Some(372) }));
        assert_eq!(state(&s, "a"), Some(State::Done));
        assert_eq!(s.get("a").unwrap().text, "PR is up");
        assert_eq!(s.get("a").unwrap().since, T0 + 372);
    }

    #[test]
    fn stop_without_prompt_has_no_duration() {
        let mut s = Sessions::default();
        let shown = s.apply(&msg(EventKind::Stop, "a", 7), T0, 0);
        assert!(matches!(shown, Some(Show::Done { secs: None, .. })), "{shown:?}");
    }

    #[test]
    fn permission_prompt_needs_you_until_answered() {
        let mut s = Sessions::default();
        s.apply(&msg(EventKind::Prompt, "a", 7), T0, 0);
        let mut tool = msg(EventKind::Tool, "a", 7);
        tool.tool = "Bash".into();
        s.apply(&tool, T0, 0);
        let mut n = msg(EventKind::NeedsYou, "a", 7);
        n.text = "Claude needs your permission".into();
        assert_eq!(
            s.apply(&n, T0, 0),
            Some(Show::NeedsYou { session: "a".into(), project: "app".into(), agent: Agent::Claude, tool: "Bash".into(), text: "Claude needs your permission".into() })
        );
        assert_eq!(s.needing_count(), 1);
        assert!(!s.tick(T0, 99, |_| true, |_| true), "another window in front changes nothing");
        assert!(s.tick(T0, 7, |_| true, |_| true));
        assert_eq!(state(&s, "a"), Some(State::Answered));
        assert_eq!(s.needing_count(), 0);
        s.apply(&msg(EventKind::Tool, "a", 7), T0, 7);
        assert_eq!(state(&s, "a"), Some(State::Working));
    }

    #[test]
    fn shared_window_does_not_clear_needs_you() {
        let mut s = Sessions::default();
        s.apply(&msg(EventKind::Prompt, "a", 7), T0, 0);
        s.apply(&msg(EventKind::Prompt, "b", 7), T0, 0);
        s.apply(&msg(EventKind::NeedsYou, "a", 7), T0, 0);
        s.tick(T0, 7, |_| true, |_| true);
        assert_eq!(state(&s, "a"), Some(State::NeedsYou), "window 7 has two sessions: can't tell which tab is in front");
        s.clear_needs("a");
        assert_eq!(state(&s, "a"), Some(State::Answered));
    }

    #[test]
    fn done_is_quiet_when_its_window_is_in_front() {
        let mut s = Sessions::default();
        s.apply(&msg(EventKind::Prompt, "a", 7), T0, 0);
        assert_eq!(s.apply(&msg(EventKind::Stop, "a", 7), T0, 7), None);
        assert_eq!(state(&s, "a"), Some(State::Done));
        s.apply(&msg(EventKind::Prompt, "a", 7), T0, 0);
        assert!(matches!(s.apply(&msg(EventKind::StopFailure, "a", 7), T0, 0), Some(Show::Error { .. })));
        assert_eq!(state(&s, "a"), Some(State::Error));
    }

    #[test]
    fn subagents_never_change_state() {
        let mut s = Sessions::default();
        s.apply(&msg(EventKind::Prompt, "a", 7), T0, 0);
        s.apply(&msg(EventKind::Stop, "a", 7), T0, 0);
        let mut sub = msg(EventKind::Tool, "a", 7);
        sub.subagent = true;
        assert_eq!(s.apply(&sub, T0, 0), None);
        assert_eq!(state(&s, "a"), Some(State::Done));
        let mut sub_stop = msg(EventKind::Stop, "z", 7);
        sub_stop.subagent = true;
        assert_eq!(s.apply(&sub_stop, T0, 0), None);
        assert!(s.get("z").is_none(), "a subagent alone doesn't create a session");
    }

    #[test]
    fn tool_after_stop_starts_a_new_turn() {
        let mut s = Sessions::default();
        s.apply(&msg(EventKind::Prompt, "a", 7), T0, 0);
        s.apply(&msg(EventKind::Stop, "a", 7), T0, 0);
        s.apply(&msg(EventKind::Tool, "a", 7), T0 + 10, 0);
        assert_eq!(state(&s, "a"), Some(State::Working));
        let shown = s.apply(&msg(EventKind::Stop, "a", 7), T0 + 40, 0);
        assert!(matches!(shown, Some(Show::Done { secs: Some(30), .. })), "{shown:?}");
    }

    #[test]
    fn the_current_tool_is_remembered() {
        let mut s = Sessions::default();
        s.apply(&msg(EventKind::Prompt, "a", 7), T0, 0);
        let mut t = msg(EventKind::Tool, "a", 7);
        t.tool = "Edit".into();
        s.apply(&t, T0, 0);
        assert_eq!(s.get("a").unwrap().tool, "Edit");
        s.apply(&msg(EventKind::Tool, "a", 7), T0, 0);
        assert_eq!(s.get("a").unwrap().tool, "Edit", "a tool event without a name keeps the last one");
        s.apply(&msg(EventKind::Stop, "a", 7), T0, 0);
        assert_eq!(s.get("a").unwrap().tool, "Edit", "kept for the summary");
        s.apply(&msg(EventKind::Prompt, "a", 7), T0, 0);
        assert_eq!(s.get("a").unwrap().tool, "", "a new prompt starts without a tool");
    }

    #[test]
    fn stale_and_closed_windows_are_dropped() {
        let mut s = Sessions::default();
        s.apply(&msg(EventKind::Prompt, "busy", 7), T0, 0);
        s.apply(&msg(EventKind::NeedsYou, "waiting", 8), T0, 0);
        s.apply(&msg(EventKind::Stop, "done", 9), T0, 0);
        assert!(!s.tick(T0 + 60, 0, |_| true, |_| true));
        assert!(s.tick(T0 + STALE_SECS + 60, 0, |_| true, |_| true));
        assert!(s.get("busy").is_none(), "working but silent for 16 min");
        assert!(s.get("waiting").is_some(), "a prompt can wait as long as it likes");
        assert!(s.get("done").is_some());
        assert!(s.tick(T0, 0, |h| h != 8, |_| true));
        assert!(s.get("waiting").is_none(), "its window was closed");
    }

    #[test]
    fn done_sessions_leave_recent_after_two_hours() {
        let mut s = Sessions::default();
        s.apply(&msg(EventKind::Stop, "done", 9), T0, 0);
        s.apply(&msg(EventKind::StopFailure, "failed", 9), T0, 0);
        s.apply(&msg(EventKind::NeedsYou, "waiting", 8), T0, 0);
        assert!(!s.tick(T0 + RECENT_KEEP_SECS, 0, |_| true, |_| true));
        assert!(s.tick(T0 + RECENT_KEEP_SECS + 1, 0, |_| true, |_| true));
        assert!(s.get("done").is_none() && s.get("failed").is_none());
        assert!(s.get("waiting").is_some(), "only finished sessions age out");
    }

    #[test]
    fn subagent_needs_you_is_shown() {
        let mut s = Sessions::default();
        s.apply(&msg(EventKind::Prompt, "a", 7), T0, 0);
        let mut n = msg(EventKind::NeedsYou, "a", 7);
        n.subagent = true;
        assert!(matches!(s.apply(&n, T0, 0), Some(Show::NeedsYou { .. })));
        assert_eq!(state(&s, "a"), Some(State::NeedsYou));
        let mut fresh = msg(EventKind::NeedsYou, "new", 7);
        fresh.subagent = true;
        assert!(s.apply(&fresh, T0, 0).is_some(), "even for a session not seen before");
    }

    /// Esc / Ctrl+C sends no Stop; the agent's idle notice ends Working.
    #[test]
    fn idle_after_interrupt_stops_working() {
        let mut s = Sessions::default();
        s.apply(&msg(EventKind::Prompt, "a", 7), T0, 0);
        assert_eq!(s.apply(&msg(EventKind::Idle, "a", 7), T0, 0), None, "no toast");
        assert_eq!(state(&s, "a"), Some(State::Idle));
        assert_eq!(working(&s), 0);
        s.apply(&msg(EventKind::Stop, "b", 7), T0, 0);
        s.apply(&msg(EventKind::Idle, "b", 7), T0, 0);
        assert_eq!(state(&s, "b"), Some(State::Done), "a finished turn stays done");
        s.apply(&msg(EventKind::NeedsYou, "c", 7), T0, 0);
        s.apply(&msg(EventKind::Idle, "c", 7), T0, 0);
        assert_eq!(state(&s, "c"), Some(State::NeedsYou), "a waiting prompt keeps waiting");
    }

    #[test]
    fn a_closed_agent_process_drops_the_session() {
        let mut s = Sessions::default();
        let mut m = msg(EventKind::NeedsYou, "a", 7);
        m.pid = 4242;
        s.apply(&m, T0, 0);
        assert!(!s.tick(T0, 0, |_| true, |_| true));
        assert!(s.tick(T0, 0, |_| true, |p| p != 4242));
        assert!(s.get("a").is_none());
    }

    #[test]
    fn session_end_and_dismiss_remove() {
        let mut s = Sessions::default();
        s.apply(&msg(EventKind::Prompt, "a", 7), T0, 0);
        assert_eq!(s.apply(&msg(EventKind::SessionEnd, "a", 7), T0, 0), None);
        assert!(s.list().is_empty());
        s.apply(&msg(EventKind::SessionEnd, "never-seen", 7), T0, 0);
        assert!(s.list().is_empty());
        s.apply(&msg(EventKind::Stop, "b", 7), T0, 0);
        s.dismiss("b");
        assert!(s.list().is_empty());
        s.dismiss("never-seen");
    }

    #[test]
    fn at_most_16_sessions() {
        let mut s = Sessions::default();
        for i in 0..20u64 {
            s.apply(&msg(EventKind::Prompt, &format!("s{i}"), 7), T0 + i, 0);
        }
        assert_eq!(s.list().len(), MAX_SESSIONS);
        assert!(s.get("s0").is_none() && s.get("s19").is_some(), "the oldest go first");
    }

    #[test]
    fn snapshot_drops_dead_sessions() {
        let dir = TempDir::new("snapshot");
        let file = dir.0.join("sessions.json");
        let mut s = Sessions::default();
        for (id, pid) in [("alive", 7), ("dead", 8)] {
            let mut m = msg(EventKind::Prompt, id, 0);
            m.pid = pid;
            s.apply(&m, T0, 0);
        }
        s.save(&file);
        let mut back = Sessions::load(&file);
        assert_eq!(back, s, "saved and loaded as it was");
        assert!(back.tick(T0 + 1, 0, |_| true, |p| p == 7));
        assert_eq!(back.list().iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["alive"]);
        std::fs::write(&file, "not json").unwrap();
        assert!(Sessions::load(&file).list().is_empty());
        assert!(Sessions::load(&dir.0.join("missing.json")).list().is_empty());
    }

    #[test]
    fn branch_from_head_files() {
        let dir = TempDir::new("branch");
        let repo = dir.0.join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::create_dir_all(repo.join("src").join("deep")).unwrap();
        let head = repo.join(".git").join("HEAD");
        std::fs::write(&head, "ref: refs/heads/feat/x\n").unwrap();
        assert_eq!(branch_of(&repo.to_string_lossy()), Some("feat/x".into()));
        assert_eq!(branch_of(&repo.join("src").join("deep").to_string_lossy()), Some("feat/x".into()), "found from a subfolder");
        std::fs::write(&head, "0123456789abcdef0123456789abcdef01234567\n").unwrap();
        assert_eq!(branch_of(&repo.to_string_lossy()), Some("0123456".into()), "detached");
        std::fs::write(&head, "garbage").unwrap();
        assert_eq!(branch_of(&repo.to_string_lossy()), None);
        // a worktree: `.git` is a file pointing at the real git dir
        let wt = dir.0.join("wt");
        let gitdir = dir.0.join("gitdir");
        std::fs::create_dir_all(&wt).unwrap();
        std::fs::create_dir_all(&gitdir).unwrap();
        std::fs::write(gitdir.join("HEAD"), "ref: refs/heads/ext/agents\n").unwrap();
        std::fs::write(wt.join(".git"), format!("gitdir: {}\n", gitdir.to_string_lossy())).unwrap();
        assert_eq!(branch_of(&wt.to_string_lossy()), Some("ext/agents".into()));
        assert_eq!(branch_of(""), None);
    }

    #[test]
    fn the_branch_is_read_when_a_session_starts() {
        let dir = TempDir::new("branch-start");
        std::fs::create_dir_all(dir.0.join(".git")).unwrap();
        std::fs::write(dir.0.join(".git").join("HEAD"), "ref: refs/heads/main\n").unwrap();
        let mut s = Sessions::default();
        let mut m = msg(EventKind::Prompt, "a", 7);
        m.cwd = dir.0.to_string_lossy().into_owned();
        s.apply(&m, T0, 0);
        assert_eq!(s.get("a").unwrap().branch.as_deref(), Some("main"));
        std::fs::write(dir.0.join(".git").join("HEAD"), "ref: refs/heads/fix/y\n").unwrap();
        s.apply(&m, T0, 0);
        assert_eq!(s.get("a").unwrap().branch.as_deref(), Some("fix/y"), "read again on each prompt");
    }
}
