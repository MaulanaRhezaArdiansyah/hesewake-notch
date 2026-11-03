//! Toasts: what a `Show` from the sessions becomes, and a queue that keeps to the notch's one toast per 5 seconds,
//! sends needs-you first and merges a burst into one toast.

use crate::cfg::Config;
use crate::sessions::Show;
use crate::text::{clip, duration_text, fill, Lang};
use serde_json::{json, Value};

/// Toasts waiting at most; the oldest go first.
const MAX_WAITING: usize = 64;

/// The notch shows at most one toast per extension every 5 s; the margin covers the two processes' timing.
pub const GAP_MS: u64 = 5200;

#[derive(Clone, Debug, PartialEq)]
pub struct Toast {
    pub title: String,
    pub sub: String,
    /// A session needs the person: sent before the others.
    pub needs: bool,
    /// The project, for a merged toast's sub.
    pub project: String,
}

/// The toast for `show`, or none when its toggle is off.
pub fn toast_for(show: &Show, cfg: &Config, l: Lang) -> Option<Toast> {
    let (title, sub, needs, project) = match show {
        Show::NeedsYou { project, agent, tool, text, .. } => {
            if !cfg.toast_needs {
                return None;
            }
            let agent = agent.label();
            let sub = if cfg.privacy || text.is_empty() {
                fill(l.t("{} · Permission"), &[&agent])
            } else if tool.is_empty() {
                format!("{agent} · {text}")
            } else {
                format!("{agent} · {tool}: {text}")
            };
            (fill(l.t("{} needs you"), &[project]), sub, true, project)
        }
        Show::Done { project, agent, secs, .. } => {
            if !cfg.toast_done {
                return None;
            }
            let sub = match secs {
                Some(s) => format!("{} · {}", agent.label(), duration_text(*s)),
                None => agent.label().to_string(),
            };
            (fill(l.t("Done · {}"), &[project]), sub, false, project)
        }
        Show::Error { project, agent, .. } => {
            if !cfg.toast_done {
                return None;
            }
            (l.t("Stopped with an error").to_string(), format!("{project} · {}", agent.label()), false, project)
        }
    };
    Some(Toast { title: clip(&title, 80), sub: clip(&sub, 60), needs, project: project.clone() })
}

#[derive(Default)]
pub struct Queue {
    waiting: Vec<Toast>,
    last_ms: Option<u64>,
}

impl Queue {
    pub fn push(&mut self, t: Toast) {
        self.waiting.push(t);
        if self.waiting.len() > MAX_WAITING {
            self.waiting.remove(0);
        }
    }

    /// The `toast` message to send now, if one is waiting and the last was at least 5 s ago.
    pub fn next(&mut self, now_ms: u64, l: Lang) -> Option<Value> {
        // a clock set back (now before the last toast) lets the next one go
        if self.waiting.is_empty() || self.last_ms.is_some_and(|t| now_ms >= t && now_ms < t.saturating_add(GAP_MS)) {
            return None;
        }
        // needs-you first; everything waiting of that kind goes out as one toast
        let needs = self.waiting.iter().any(|t| t.needs);
        let (group, rest): (Vec<Toast>, Vec<Toast>) = std::mem::take(&mut self.waiting).into_iter().partition(|t| t.needs == needs);
        self.waiting = rest;
        self.last_ms = Some(now_ms);
        let n = group.len();
        let (title, sub) = if n == 1 {
            (group[0].title.clone(), group[0].sub.clone())
        } else {
            let title = if needs { fill(l.t("{} sessions need you"), &[&n]) } else { fill(l.t("{} sessions finished"), &[&n]) };
            (title, group.iter().map(|t| t.project.as_str()).collect::<Vec<_>>().join(", "))
        };
        Some(json!({"t": "toast", "title": clip(&title, 80), "sub": clip(&sub, 60), "count": n}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Agent;

    fn done(project: &str, secs: Option<u64>) -> Show {
        Show::Done { session: project.into(), project: project.into(), title: String::new(), agent: Agent::Claude, secs }
    }

    fn needs(project: &str, tool: &str, text: &str) -> Show {
        Show::NeedsYou { session: project.into(), project: project.into(), title: String::new(), agent: Agent::Claude, tool: tool.into(), text: text.into() }
    }

    fn toast(show: &Show) -> Toast {
        toast_for(show, &Config::default(), Lang(false)).unwrap()
    }

    #[test]
    fn toast_texts() {
        let t = toast(&needs("hesewake-notch", "Bash", "Claude needs your permission to use Bash"));
        assert_eq!((t.title.as_str(), t.sub.as_str(), t.needs), ("hesewake-notch needs you", "Claude · Bash: Claude needs your permission to use Bash", true));
        assert_eq!(toast(&needs("app", "", "Pick one")).sub, "Claude · Pick one");
        assert_eq!(toast(&needs("app", "Bash", "")).sub, "Claude · Permission");
        let d = toast(&done("hesewake-notch", Some(252)));
        assert_eq!((d.title.as_str(), d.sub.as_str(), d.needs), ("Done · hesewake-notch", "Claude · 4m 12s", false));
        assert_eq!(toast(&done("app", None)).sub, "Claude");
        let e = toast(&Show::Error { session: "s".into(), project: "app".into(), title: String::new(), agent: Agent::Claude });
        assert_eq!((e.title.as_str(), e.sub.as_str()), ("Stopped with an error", "app · Claude"));
        let id = toast_for(&done("app", Some(30)), &Config::default(), Lang(true)).unwrap();
        assert_eq!(id.title, "Selesai · app");
        let long = toast(&needs(&"p".repeat(200), &"t".repeat(200), &"x".repeat(5000)));
        assert!(long.title.chars().count() <= 80 && long.sub.chars().count() <= 60);
    }

    #[test]
    fn one_toast_per_five_seconds() {
        let mut q = Queue::default();
        let l = Lang(false);
        assert!(q.next(0, l).is_none(), "nothing waiting");
        q.push(toast(&done("a", None)));
        let first = q.next(0, l).unwrap();
        assert_eq!((first["t"].as_str(), first["title"].as_str(), first["count"].as_u64()), (Some("toast"), Some("Done · a"), Some(1)));
        q.push(toast(&done("b", None)));
        assert!(q.next(5199, l).is_none(), "a margin over the notch's 5 s");
        assert_eq!(q.next(5200, l).unwrap()["title"], "Done · b");
        assert!(q.next(20_000, l).is_none(), "sent once");
    }

    #[test]
    fn a_burst_becomes_one_toast() {
        let mut q = Queue::default();
        let l = Lang(false);
        for p in ["a", "b", "c"] {
            q.push(toast(&done(p, Some(60))));
        }
        q.push(toast(&needs("n", "Bash", "")));
        assert_eq!(q.next(0, l).unwrap()["title"], "n needs you", "needs-you first");
        let merged = q.next(5200, l).unwrap();
        assert_eq!((merged["title"].as_str(), merged["sub"].as_str(), merged["count"].as_u64()), (Some("3 sessions finished"), Some("a, b, c"), Some(3)));
        assert!(q.next(10_000, l).is_none());
        for p in ["x", "y"] {
            q.push(toast(&needs(p, "Bash", "")));
        }
        let m = q.next(10_400, Lang(true)).unwrap();
        assert_eq!((m["title"].as_str(), m["sub"].as_str()), (Some("2 sesi perlu kamu"), Some("x, y")));
        for i in 0..40 {
            q.push(toast(&done(&format!("project-{i}"), None)));
        }
        let many = q.next(15_600, l).unwrap();
        assert!(many["sub"].as_str().unwrap().chars().count() <= 60 && many["title"].as_str().unwrap().chars().count() <= 80);
        assert_eq!(many["count"], 40);
    }

    #[test]
    fn a_clock_set_back_does_not_stop_toasts() {
        let mut q = Queue::default();
        q.push(toast(&done("a", None)));
        assert!(q.next(1_000_000, Lang(false)).is_some());
        q.push(toast(&done("b", None)));
        assert!(q.next(10_000, Lang(false)).is_some(), "the clock went back");
    }

    #[test]
    fn a_renamed_session_shows_its_name() {
        let l = Lang(false);
        let named = Show::Done { session: "s".into(), project: "app".into(), title: "Payslip fix".into(), agent: Agent::Claude, secs: Some(59) };
        assert_eq!(toast_for(&named, &Config::default(), l).unwrap().title, "Done · Payslip fix");
        let private = Config { privacy: true, ..Config::default() };
        assert_eq!(toast_for(&named, &private, l).unwrap().title, "Done · app", "privacy mode shows the folder name");
        let needs = Show::NeedsYou { session: "s".into(), project: "app".into(), title: "Payslip fix".into(), agent: Agent::Claude, tool: String::new(), text: String::new() };
        assert_eq!(toast_for(&needs, &Config::default(), l).unwrap().title, "Payslip fix needs you");
        assert_eq!(toast_for(&needs, &Config::default(), l).unwrap().project, "Payslip fix", "a merged toast lists the name");
        let err = Show::Error { session: "s".into(), project: "app".into(), title: "Payslip fix".into(), agent: Agent::Claude };
        assert_eq!(toast_for(&err, &Config::default(), l).unwrap().sub, "Payslip fix · Claude");
    }

    #[test]
    fn toggles_and_privacy_shape_toasts() {
        let mut cfg = Config { toast_done: false, ..Config::default() };
        assert!(toast_for(&done("a", Some(5)), &cfg, Lang(false)).is_none());
        assert!(toast_for(&Show::Error { session: "s".into(), project: "a".into(), title: String::new(), agent: Agent::Claude }, &cfg, Lang(false)).is_none(), "an error is a finish too");
        assert!(toast_for(&needs("a", "Bash", "x"), &cfg, Lang(false)).is_some());
        cfg.toast_needs = false;
        assert!(toast_for(&needs("a", "Bash", "x"), &cfg, Lang(false)).is_none());
        let private = Config { privacy: true, ..Config::default() };
        let t = toast_for(&needs("a", "Bash", "rm -rf secrets"), &private, Lang(false)).unwrap();
        assert_eq!(t.sub, "Claude · Permission");
        assert_eq!(toast_for(&needs("a", "Bash", "x"), &private, Lang(true)).unwrap().sub, "Claude · Izin");
    }
}
