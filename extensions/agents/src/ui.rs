//! What the slide shows and how it answers the notch. No I/O here: every input is a message from the notch, a hook
//! event or a clock tick, and every output is an `Out` that `main.rs` carries out.

use crate::cfg::Config;
use crate::claude_setup::Link;
use crate::event::Msg;
use crate::sessions::{Session, Sessions, Show, State};
use crate::toasts::{toast_for, Queue};
use crate::text::{ago_text, clip, duration_text, fill, Lang};
use serde_json::{json, Value};

/// While the slide is open, the page is sent again this often so its times stay right.
const REFRESH_SECS: u64 = 30;

pub enum Out {
    Send(Value),
    Save(Config),
    SaveSessions,
    /// Connect (`true`) or remove our hooks, then call `Ui::linked` with the result.
    Connect(bool),
    /// Read Claude Code's settings again, then call `Ui::linked`.
    Recheck,
}

#[derive(Clone, Debug, PartialEq)]
enum View {
    Home,
    Detail(String),
    Settings,
}

pub struct Ui {
    pub cfg: Config,
    pub link: Link,
    pub l: Lang,
    pub sessions: Sessions,
    /// Unix seconds and the foreground window; set by `main.rs` before each call.
    pub now: u64,
    pub fg: isize,
    pub visible: bool,
    rev: u64,
    view: View,
    next_page: u64,
    toasts: Queue,
    /// The "something new" dot is on.
    badge: bool,
    status: String,
    /// Another copy of the extension already receives the hooks.
    pub other_copy: bool,
    /// The sessions changed since the last page; the next tick sends it if the slide is open.
    page_dirty: bool,
}

impl Ui {
    pub fn new(cfg: Config, link: Link, id_lang: bool) -> Ui {
        Ui { cfg, link, l: Lang(id_lang), sessions: Sessions::default(), now: 0, fg: 0, visible: false, rev: 0, view: View::Home, next_page: 0, toasts: Queue::default(), badge: false, status: String::new(), other_copy: false, page_dirty: false }
    }

    pub fn start(&mut self) -> Vec<Out> {
        let mut out = Vec::new();
        self.page_to(&mut out);
        self.after(&mut out);
        out
    }

    /// A message from the notch (`quit` is handled by `main.rs`).
    pub fn on_msg(&mut self, m: &Value) -> Vec<Out> {
        let mut out = Vec::new();
        let id = m["id"].as_str().unwrap_or("").to_string();
        match m["t"].as_str().unwrap_or("") {
            "visible" => {
                self.visible = m["on"] == true;
                if self.visible {
                    // the notch clears the dot when the slide is opened
                    self.badge = false;
                } else {
                    self.view = View::Home;
                }
                self.page_to(&mut out);
            }
            "row" => match self.view {
                View::Home => {
                    if let Some(state) = self.sessions.get(&id).map(|s| s.state) {
                        if state == State::NeedsYou {
                            self.sessions.clear_needs(&id);
                            out.push(Out::SaveSessions);
                        }
                        self.view = View::Detail(id);
                    }
                    self.page_to(&mut out);
                }
                View::Settings => {
                    let toggled = match id.as_str() {
                        "privacy" => {
                            self.cfg.privacy = !self.cfg.privacy;
                            true
                        }
                        "toast_needs" => {
                            self.cfg.toast_needs = !self.cfg.toast_needs;
                            true
                        }
                        "toast_done" => {
                            self.cfg.toast_done = !self.cfg.toast_done;
                            true
                        }
                        "claude" => {
                            out.push(match self.link {
                                Link::Invalid => Out::Recheck,
                                Link::On => Out::Connect(false),
                                _ => Out::Connect(true),
                            });
                            false
                        }
                        _ => false,
                    };
                    if toggled {
                        out.push(Out::Save(self.cfg.clone()));
                        self.page_to(&mut out);
                    }
                }
                View::Detail(_) => {}
            },
            "back" => {
                self.view = View::Home;
                self.page_to(&mut out);
            }
            "action" => match id.as_str() {
                "settings" => {
                    self.view = View::Settings;
                    self.page_to(&mut out);
                }
                "setup" => out.push(Out::Connect(true)),
                "retry" => out.push(Out::Recheck),
                "dismiss" => {
                    if let View::Detail(sid) = &self.view {
                        let sid = sid.clone();
                        self.sessions.dismiss(&sid);
                        out.push(Out::SaveSessions);
                    }
                    self.view = View::Home;
                    self.page_to(&mut out);
                }
                _ => {}
            },
            "lang" => {
                self.l = Lang(m["lang"] == "id");
                self.page_to(&mut out);
            }
            _ => {}
        }
        self.after(&mut out);
        out
    }

    /// A hook event from an agent.
    pub fn on_event(&mut self, m: &Msg) -> Vec<Out> {
        let mut out = Vec::new();
        if let Some(show) = self.sessions.apply(m, self.now, self.fg) {
            if let Some(t) = toast_for(&show, &self.cfg, self.l) {
                self.toasts.push(t);
            }
            if matches!(show, Show::NeedsYou { .. }) && !self.visible && !self.badge {
                self.badge = true;
                out.push(Out::Send(json!({"t": "badge", "on": true})));
            }
        }
        out.push(Out::SaveSessions);
        // The notch drops anything over 10 messages a second, and one tool call is two events: the page and the
        // status wait for the tick.
        self.page_dirty = true;
        self.badge_off(&mut out);
        out
    }

    /// Claude Code's settings were changed or read again.
    pub fn linked(&mut self, link: Link) -> Vec<Out> {
        let mut out = Vec::new();
        self.link = link;
        self.page_to(&mut out);
        self.after(&mut out);
        out
    }

    /// About once a second.
    pub fn tick(&mut self, now_ms: u64, alive: impl Fn(isize) -> bool, running: impl Fn(u32) -> bool) -> Vec<Out> {
        let mut out = Vec::new();
        self.now = now_ms / 1000;
        if self.sessions.tick(self.now, self.fg, alive, running) {
            out.push(Out::SaveSessions);
            self.page_dirty = true;
        }
        // a hidden slide gets its page when it opens
        if self.visible && (self.page_dirty || self.now >= self.next_page) {
            self.page_to(&mut out);
        }
        if let Some(t) = self.toasts.next(now_ms, self.l) {
            out.push(Out::Send(t));
        }
        self.after(&mut out);
        out
    }

    /// After every input: the dot goes off once nothing needs the person, and the status follows the sessions.
    fn after(&mut self, out: &mut Vec<Out>) {
        self.badge_off(out);
        let s = self.status_text();
        if s != self.status {
            out.push(Out::Send(json!({"t": "status", "text": s})));
            self.status = s;
        }
    }

    /// The dot goes off once nothing needs the person.
    fn badge_off(&mut self, out: &mut Vec<Out>) {
        if self.badge && self.sessions.needing_count() == 0 {
            self.badge = false;
            out.push(Out::Send(json!({"t": "badge", "on": false})));
        }
    }

    /// The line next to the extension in Settings > Extensions.
    fn status_text(&self) -> String {
        let l = self.l;
        let (n, k) = (self.sessions.list().len(), self.sessions.needing_count());
        let s = match self.link {
            _ if self.other_copy => l.t("Another copy is running").to_string(),
            Link::On if k > 0 => fill(l.t("Claude connected · {} sessions · {} need you"), &[&n, &k]),
            Link::On => fill(l.t("Claude connected · {} sessions"), &[&n]),
            Link::Invalid => l.t("Claude settings unreadable").to_string(),
            Link::Off | Link::Old | Link::Moved => l.t("Claude not connected").to_string(),
        };
        clip(&s, 60)
    }

    fn page_to(&mut self, out: &mut Vec<Out>) {
        self.page_dirty = false;
        let p = self.page();
        out.push(Out::Send(p));
    }

    /// The current page (`slide` or `detail`), with a new `rev`.
    pub fn page(&mut self) -> Value {
        self.rev += 1;
        self.next_page = self.now + REFRESH_SECS;
        let mut p = match self.view.clone() {
            View::Home => self.home(),
            View::Settings => self.settings(),
            View::Detail(id) => match self.sessions.get(&id).map(|s| self.detail(s)) {
                Some(p) => p,
                // the session ended while its page was open
                None => {
                    self.view = View::Home;
                    self.home()
                }
            },
        };
        p["rev"] = json!(self.rev);
        p
    }

    fn home(&self) -> Value {
        let l = self.l;
        let mut p = json!({"t": "slide", "title": l.t("Coding |Agents"), "actions": [{"id": "settings", "label": l.t("Settings")}]});
        if self.cfg.privacy {
            p["foot"] = json!(l.t("Privacy mode"));
        }
        match self.link {
            Link::On => {}
            Link::Invalid => {
                p["state"] = json!("error");
                p["message"] = json!(l.t("Claude Code's settings.json could not be read. Nothing was changed."));
                return p;
            }
            Link::Off | Link::Old | Link::Moved => {
                p["state"] = json!("setup");
                p["message"] = json!(l.t("Connect Claude Code to see its sessions here"));
                p["setup_label"] = json!(l.t("Connect"));
                return p;
            }
        }
        let list = self.sessions.list();
        let pick = |states: &[State]| -> Vec<&Session> { list.iter().filter(|s| states.contains(&s.state)).collect() };
        let mut needs = pick(&[State::NeedsYou]);
        // the longest waiting first; the others newest first
        needs.sort_by_key(|s| s.since);
        let mut working = pick(&[State::Working, State::Answered]);
        working.sort_by_key(|s| std::cmp::Reverse(s.turn_started.unwrap_or(s.since)));
        let mut recent = pick(&[State::Done, State::Error, State::Idle]);
        recent.sort_by_key(|s| std::cmp::Reverse(s.since));
        let sections: Vec<Value> = [(l.t("Needs you"), needs), (l.t("Working"), working), (l.t("Recent"), recent)]
            .into_iter()
            .filter(|(_, rows)| !rows.is_empty())
            .map(|(title, rows)| json!({"title": title, "rows": rows.iter().map(|s| self.row(s)).collect::<Vec<_>>()}))
            .collect();
        if sections.is_empty() {
            p["state"] = json!("empty");
            p["message"] = json!(l.t("No agent sessions yet. They show up here when Claude Code runs."));
        } else {
            p["sections"] = json!(sections);
        }
        p
    }

    fn row(&self, s: &Session) -> Value {
        let l = self.l;
        let agent = s.agent.label();
        let text = match &s.branch {
            Some(b) => format!("{} · {b}", s.project),
            None => s.project.clone(),
        };
        let in_state = span(self.now.saturating_sub(s.since));
        let ago = ago_text(l, self.now.saturating_sub(s.since));
        let (sub, tone) = match s.state {
            State::NeedsYou => (fill(l.t("{} · Needs you · {}"), &[&agent, &in_state]), Some("warn")),
            State::Working | State::Answered => {
                let t = span(self.now.saturating_sub(s.turn_started.unwrap_or(s.since)));
                let mut sub = fill(l.t("{} · Working {}"), &[&agent, &t]);
                if !s.tool.is_empty() {
                    sub = format!("{sub} · {}", s.tool);
                }
                (sub, None)
            }
            State::Done => (fill(l.t("{} · Done {}"), &[&agent, &ago]), Some("good")),
            State::Error => (fill(l.t("{} · Error {}"), &[&agent, &ago]), Some("warn")),
            State::Idle => (fill(l.t("{} · Idle {}"), &[&agent, &in_state]), Some("muted")),
        };
        let mut r = json!({"id": s.id, "text": clip(&text, 120), "sub": clip(&sub, 40)});
        if let Some(t) = tone {
            r["tone"] = json!(t);
        }
        if s.state == State::NeedsYou {
            r["mark"] = json!(true);
        }
        r
    }

    fn detail(&self, s: &Session) -> Value {
        let l = self.l;
        let state = match s.state {
            State::NeedsYou => l.t("Needs you"),
            State::Working | State::Answered => l.t("Working"),
            State::Done => l.t("Done"),
            State::Error => l.t("Error"),
            State::Idle => l.t("Idle"),
        };
        let time = match s.state {
            State::Done | State::Error | State::Idle => ago_text(l, self.now.saturating_sub(s.since)),
            _ => duration_text(self.now.saturating_sub(s.turn_started.unwrap_or(s.since))),
        };
        let mut meta = vec![json!({"k": l.t("Agent"), "v": s.agent.label()}), json!({"k": l.t("State"), "v": state}), json!({"k": l.t("Time"), "v": time})];
        if let Some(b) = &s.branch {
            meta.push(json!({"k": l.t("Branch"), "v": clip(b, 80)}));
        }
        if !s.tool.is_empty() {
            meta.push(json!({"k": l.t("Tool"), "v": clip(&s.tool, 80)}));
        }
        let show_path = !self.cfg.privacy && !s.cwd.is_empty();
        if show_path {
            meta.push(json!({"k": l.t("Folder"), "v": clip(&s.cwd, 80)}));
        }
        let mut buttons = Vec::new();
        if show_path {
            buttons.push(json!({"id": "copy", "label": l.t("Copy path"), "copy": clip(&s.cwd, 8000)}));
        }
        buttons.push(json!({"id": "dismiss", "label": l.t("Dismiss")}));
        let mut p = json!({"t": "detail", "for": s.id, "title": clip(&s.project, 120), "meta": meta, "buttons": buttons});
        if !self.cfg.privacy && !s.text.is_empty() {
            p["body"] = json!(clip(&s.text, 4000));
        }
        p
    }

    fn settings(&self) -> Value {
        let l = self.l;
        let (text, sub, tone) = match self.link {
            Link::Off => (l.t("Connect"), l.t("Not connected"), "muted"),
            Link::On => (l.t("Remove"), l.t("Connected"), "good"),
            Link::Moved => (l.t("Fix"), l.t("Moved, press to fix"), "warn"),
            Link::Old => (l.t("Replace old hooks"), l.t("Old Hesewake hooks"), "warn"),
            Link::Invalid => (l.t("Check again"), l.t("settings.json unreadable"), "warn"),
        };
        let toggle = |id: &str, text: &str, on: bool| {
            json!({"id": id, "text": text, "sub": if on { l.t("On") } else { l.t("Off") }, "tone": if on { "good" } else { "muted" }})
        };
        json!({
            "t": "slide", "title": l.t("Coding |Agents"), "back": true,
            "sections": [
                {"title": l.t("Claude Code"), "rows": [{"id": "claude", "text": text, "sub": sub, "tone": tone}]},
                {"title": l.t("Options"), "rows": [
                    toggle("privacy", l.t("Privacy mode"), self.cfg.privacy),
                    toggle("toast_needs", l.t("Toast when a session needs you"), self.cfg.toast_needs),
                    toggle("toast_done", l.t("Toast when a session finishes"), self.cfg.toast_done)
                ]}
            ]
        })
    }
}

/// A short time span for a row: `45s`, `3m`, `2h`, `4d` (the same in both languages).
fn span(secs: u64) -> String {
    match secs {
        0..=59 => format!("{secs}s"),
        60..=3599 => format!("{}m", secs / 60),
        3600..=86_399 => format!("{}h", secs / 3600),
        _ => format!("{}d", secs / 86_400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{Agent, EventKind};
    use crate::sessions::tests::msg;
    use crate::sessions::{Session, State};
    use serde_json::json;

    const NOW: u64 = 1_793_000_000;
    const ALL_LINKS: [Link; 5] = [Link::Off, Link::On, Link::Moved, Link::Old, Link::Invalid];
    const ALL_STATES: [State; 6] = [State::Idle, State::Working, State::NeedsYou, State::Answered, State::Done, State::Error];

    fn sess(state: State, project: &str, branch: Option<&str>) -> Session {
        Session {
            needs_at: 0,
            agent: Agent::Claude,
            id: project.chars().take(36).collect(),
            project: project.into(),
            cwd: format!("C:\\code\\{project}"),
            branch: branch.map(Into::into),
            state,
            since: NOW - 120,
            last: NOW - 120,
            turn_started: Some(NOW - 180),
            tool: "Bash".into(),
            text: "Allow cargo test?".into(),
            hwnd: 7,
            pid: 0,
        }
    }

    fn ui_with(link: Link, list: Vec<Session>) -> Ui {
        let mut u = Ui::new(Config::default(), link, false);
        u.now = NOW;
        u.sessions = serde_json::from_value(json!({ "list": list })).unwrap();
        u
    }

    fn sent(out: &[Out]) -> Vec<&Value> {
        out.iter().filter_map(|o| if let Out::Send(v) = o { Some(v) } else { None }).collect()
    }

    fn pages(out: &[Out]) -> Vec<&Value> {
        sent(out).into_iter().filter(|v| v["t"] == "slide" || v["t"] == "detail").collect()
    }

    fn saves(out: &[Out]) -> Vec<&Config> {
        out.iter().filter_map(|o| if let Out::Save(c) = o { Some(c) } else { None }).collect()
    }

    fn connects(out: &[Out]) -> Vec<bool> {
        out.iter().filter_map(|o| if let Out::Connect(on) = o { Some(*on) } else { None }).collect()
    }

    fn saved_sessions(out: &[Out]) -> bool {
        out.iter().any(|o| matches!(o, Out::SaveSessions))
    }

    fn titles(p: &Value) -> Vec<&str> {
        p["sections"].as_array().into_iter().flatten().map(|s| s["title"].as_str().unwrap()).collect()
    }

    fn row<'a>(p: &'a Value, id: &str) -> &'a Value {
        p["sections"].as_array().into_iter().flatten().flat_map(|s| s["rows"].as_array().unwrap()).find(|r| r["id"] == id).expect(id)
    }

    fn meta_keys(p: &Value) -> Vec<&str> {
        p["meta"].as_array().into_iter().flatten().map(|m| m["k"].as_str().unwrap()).collect()
    }

    fn len(v: &Value) -> usize {
        v.as_str().map_or(0, |s| s.chars().count())
    }

    /// The protocol's limits (docs/extensions.md, section 6).
    fn check_limits(p: &Value) {
        let ctx = p.to_string();
        let slide = p["t"] == "slide";
        assert!(len(&p["title"]) <= if slide { 24 } else { 120 }, "title: {ctx}");
        for (k, n) in [("message", 120), ("foot", 120), ("setup_label", 16), ("for", 64), ("body", 4000)] {
            assert!(len(&p[k]) <= n, "{k}: {ctx}");
        }
        let actions = p["actions"].as_array().cloned().unwrap_or_default();
        assert!(actions.len() <= 3, "actions: {ctx}");
        let sections = p["sections"].as_array().cloned().unwrap_or_default();
        assert!(sections.len() <= 4, "sections: {ctx}");
        let mut rows = 0;
        for s in &sections {
            assert!(len(&s["title"]) <= 24, "section title: {ctx}");
            for r in s["rows"].as_array().unwrap() {
                rows += 1;
                assert!(len(&r["id"]) <= 64 && len(&r["text"]) <= 120 && len(&r["sub"]) <= 40, "row: {r}");
                assert!(r["tone"].is_null() || ["good", "warn", "muted"].contains(&r["tone"].as_str().unwrap()), "tone: {r}");
            }
        }
        assert!(rows <= 100);
        let meta = p["meta"].as_array().cloned().unwrap_or_default();
        assert!(meta.len() <= 6, "meta: {ctx}");
        for m in &meta {
            assert!(len(&m["k"]) <= 24 && len(&m["v"]) <= 80, "meta: {m}");
        }
        let buttons = p["buttons"].as_array().cloned().unwrap_or_default();
        assert!(buttons.len() <= 4, "buttons: {ctx}");
        for b in actions.iter().chain(&buttons) {
            assert!(len(&b["label"]) <= 16 && len(&b["id"]) <= 64 && len(&b["copy"]) <= 8000, "button: {b}");
        }
    }

    /// Home, the detail of every session, and Settings.
    fn all_pages(u: &mut Ui) -> Vec<Value> {
        let mut out = vec![u.page()];
        let ids: Vec<String> = u.sessions.list().iter().map(|s| s.id.clone()).collect();
        for id in ids {
            out.extend(pages(&u.on_msg(&json!({"t": "row", "id": id}))).into_iter().cloned());
            u.on_msg(&json!({"t": "back"}));
        }
        out.extend(pages(&u.on_msg(&json!({"t": "action", "id": "settings"}))).into_iter().cloned());
        u.on_msg(&json!({"t": "back"}));
        out
    }

    #[test]
    fn needs_you_comes_first_with_the_mark() {
        let mut u = ui_with(
            Link::On,
            vec![sess(State::Working, "app", Some("main")), sess(State::NeedsYou, "hesewake-notch", Some("main")), sess(State::Done, "site", None)],
        );
        let p = u.page();
        assert_eq!(p["t"], "slide");
        assert_eq!(p["title"], "Coding |Agents");
        assert_eq!(p["actions"], json!([{"id": "settings", "label": "Settings"}]));
        assert_eq!(titles(&p), ["Needs you", "Working", "Recent"]);
        let first = &p["sections"][0]["rows"][0];
        assert_eq!(first["id"], "hesewake-notch");
        assert_eq!(first["text"], "hesewake-notch · main");
        assert_eq!(first["sub"], "Claude · Needs you · 2m");
        assert_eq!((first["mark"].as_bool(), first["tone"].as_str()), (Some(true), Some("warn")));
        let working = row(&p, "app");
        assert_eq!(working["sub"], "Claude · Working 3m · Bash");
        assert!(working["tone"].is_null() && working["mark"] != true);
        let done = row(&p, "site");
        assert_eq!((done["text"].as_str(), done["sub"].as_str(), done["tone"].as_str()), (Some("site"), Some("Claude · Done 2m ago"), Some("good")));
        assert!(p.get("state").is_none_or(|s| s == "ready"));
        assert!(p.get("back").is_none(), "home has no Back");
    }

    #[test]
    fn rows_for_each_state() {
        let mut u = ui_with(Link::On, vec![sess(State::Error, "e", None), sess(State::Idle, "i", None), sess(State::Answered, "a", None)]);
        let p = u.page();
        assert_eq!(titles(&p), ["Working", "Recent"], "empty sections are left out");
        assert_eq!((row(&p, "e")["sub"].as_str(), row(&p, "e")["tone"].as_str()), (Some("Claude · Error 2m ago"), Some("warn")));
        assert_eq!((row(&p, "i")["sub"].as_str(), row(&p, "i")["tone"].as_str()), (Some("Claude · Idle 2m"), Some("muted")));
        assert_eq!(row(&p, "a")["sub"], "Claude · Working 3m · Bash", "answered goes on working");
    }

    #[test]
    fn not_connected_asks_to_connect() {
        for link in [Link::Off, Link::Old, Link::Moved] {
            let mut u = ui_with(link, vec![]);
            let p = u.page();
            assert_eq!((p["state"].as_str(), p["setup_label"].as_str()), (Some("setup"), Some("Connect")), "{link:?}");
            assert_eq!(p["message"], "Connect Claude Code to see its sessions here");
            assert_eq!(connects(&u.on_msg(&json!({"t": "action", "id": "setup"}))), [true]);
        }
        let mut u = ui_with(Link::Invalid, vec![]);
        let p = u.page();
        assert_eq!(p["state"], "error");
        assert_eq!(p["message"], "Claude Code's settings.json could not be read. Nothing was changed.");
        let out = u.on_msg(&json!({"t": "action", "id": "retry"}));
        assert!(out.iter().any(|o| matches!(o, Out::Recheck)), "Retry reads the file again and never connects");
        assert!(connects(&out).is_empty());
        let p = pages(&u.linked(Link::On))[0].clone();
        assert_eq!(p["state"], "empty");
        assert_eq!(p["message"], "No agent sessions yet. They show up here when Claude Code runs.");
    }

    #[test]
    fn privacy_hides_text_and_paths() {
        let mut u = ui_with(Link::On, vec![sess(State::Done, "app", Some("main"))]);
        let p = pages(&u.on_msg(&json!({"t": "row", "id": "app"})))[0].clone();
        assert_eq!((p["t"].as_str(), p["for"].as_str(), p["title"].as_str()), (Some("detail"), Some("app"), Some("app")));
        assert_eq!(meta_keys(&p), ["Agent", "State", "Time", "Branch", "Tool", "Folder"]);
        assert_eq!(p["meta"][5]["v"], "C:\\code\\app");
        assert_eq!(p["body"], "Allow cargo test?");
        assert_eq!(p["buttons"], json!([{"id": "copy", "label": "Copy path", "copy": "C:\\code\\app"}, {"id": "dismiss", "label": "Dismiss"}]));
        u.on_msg(&json!({"t": "back"}));
        u.cfg.privacy = true;
        let home = u.page();
        assert_eq!(home["foot"], "Privacy mode");
        assert_eq!(row(&home, "app")["text"], "app · main", "folder names stay");
        let p = pages(&u.on_msg(&json!({"t": "row", "id": "app"})))[0].clone();
        assert!(!meta_keys(&p).contains(&"Folder"));
        assert!(len(&p["body"]) == 0, "no reply text");
        assert_eq!(p["buttons"], json!([{"id": "dismiss", "label": "Dismiss"}]));
        assert!(!p.to_string().contains("code\\\\app"), "no path anywhere: {p}");
    }

    #[test]
    fn opening_a_waiting_session_answers_it_and_dismiss_removes_it() {
        let mut u = ui_with(Link::On, vec![sess(State::NeedsYou, "app", None)]);
        let out = u.on_msg(&json!({"t": "row", "id": "app"}));
        assert_eq!(u.sessions.get("app").unwrap().state, State::Answered);
        assert!(saved_sessions(&out));
        let out = u.on_msg(&json!({"t": "action", "id": "dismiss"}));
        assert!(u.sessions.get("app").is_none());
        assert!(saved_sessions(&out));
        assert_eq!(pages(&out)[0]["t"], "slide", "back home");
        assert!(pages(&u.on_msg(&json!({"t": "row", "id": "gone"}))).iter().all(|p| p["t"] == "slide"), "an unknown row stays home");
    }

    #[test]
    fn events_and_ticks_update_the_page() {
        let mut u = ui_with(Link::On, vec![]);
        u.on_msg(&json!({"t": "visible", "on": true}));
        let mut m = msg(EventKind::NeedsYou, "s1", 7);
        m.project = "app".into();
        let out = u.on_event(&m);
        assert!(saved_sessions(&out));
        assert!(pages(&out).is_empty(), "pages wait for the tick");
        let out = u.tick(NOW * 1000 + 500, |_| true, |_| true);
        assert_eq!(u.now, NOW);
        assert_eq!(titles(pages(&out)[0]), ["Needs you"]);
        let out = u.tick(NOW * 1000 + 1500, |h| h != 7, |_| true);
        assert!(saved_sessions(&out), "its window closed");
        assert_eq!(pages(&out)[0]["state"], "empty");
        assert!(pages(&u.tick(NOW * 1000 + 2500, |_| true, |_| true)).is_empty(), "nothing changed");
        // The notch sends `visible` only for the Settings panel, never for the slide: a change goes out on the next
        // tick whether anyone looks or not, and the times move on every 30 s.
        u.on_msg(&json!({"t": "visible", "on": false}));
        u.on_event(&m);
        assert_eq!(titles(pages(&u.tick(NOW * 1000 + 3500, |_| true, |_| true))[0]), ["Needs you"]);
        assert!(pages(&u.tick(NOW * 1000 + 4500, |_| true, |_| true)).is_empty(), "nothing changed");
        assert_eq!(pages(&u.tick((NOW + 40) * 1000, |_| true, |_| true)).len(), 1, "the times move on");
    }

    #[test]
    fn settings_rows_toggle_and_save() {
        let mut u = ui_with(Link::On, vec![]);
        let p = pages(&u.on_msg(&json!({"t": "action", "id": "settings"})))[0].clone();
        assert_eq!((p["t"].as_str(), p["back"].as_bool(), p["title"].as_str()), (Some("slide"), Some(true), Some("Coding |Agents")));
        assert_eq!(titles(&p), ["Claude Code", "Options"]);
        assert_eq!((row(&p, "claude")["text"].as_str(), row(&p, "claude")["sub"].as_str()), (Some("Remove"), Some("Connected")));
        assert_eq!(row(&p, "privacy")["sub"], "Off");
        let out = u.on_msg(&json!({"t": "row", "id": "privacy"}));
        assert_eq!(saves(&out).iter().map(|c| c.privacy).collect::<Vec<_>>(), [true]);
        assert_eq!((row(pages(&out)[0], "privacy")["sub"].as_str(), row(pages(&out)[0], "privacy")["tone"].as_str()), (Some("On"), Some("good")));
        for id in ["toast_needs", "toast_done"] {
            let out = u.on_msg(&json!({"t": "row", "id": id}));
            assert_eq!(row(pages(&out)[0], id)["sub"], "Off");
        }
        assert_eq!((u.cfg.toast_needs, u.cfg.toast_done), (false, false));
        assert_eq!(connects(&u.on_msg(&json!({"t": "row", "id": "claude"}))), [false]);
        let p = pages(&u.linked(Link::Off))[0].clone();
        assert_eq!(p["back"], true, "still on Settings");
        assert_eq!((row(&p, "claude")["text"].as_str(), row(&p, "claude")["sub"].as_str()), (Some("Connect"), Some("Not connected")));
        assert_eq!(connects(&u.on_msg(&json!({"t": "row", "id": "claude"}))), [true]);
        for (link, text, sub) in [(Link::Moved, "Fix", "Moved, press to fix"), (Link::Old, "Replace old hooks", "Old Hesewake hooks")] {
            let p = pages(&u.linked(link))[0].clone();
            assert_eq!((row(&p, "claude")["text"].as_str(), row(&p, "claude")["sub"].as_str()), (Some(text), Some(sub)));
        }
        let p = pages(&u.linked(Link::Invalid))[0].clone();
        assert_eq!(row(&p, "claude")["sub"], "settings.json unreadable");
        let out = u.on_msg(&json!({"t": "row", "id": "claude"}));
        assert!(connects(&out).is_empty() && out.iter().any(|o| matches!(o, Out::Recheck)));
        assert!(pages(&u.on_msg(&json!({"t": "back"})))[0].get("back").is_none(), "back home");
    }

    #[test]
    fn long_and_wide_names_are_clipped() {
        let mut s = sess(State::NeedsYou, &"x".repeat(300), Some("🌿".repeat(50).as_str()));
        s.tool = "T".repeat(300);
        s.text = "y".repeat(10_000);
        s.cwd = format!("C:\\{}", "z".repeat(9_000));
        for id_lang in [false, true] {
            let mut u = ui_with(Link::On, vec![s.clone(), sess(State::Working, &"w".repeat(300), Some("b".repeat(300).as_str()))]);
            u.l = Lang(id_lang);
            for p in all_pages(&mut u) {
                check_limits(&p);
            }
        }
    }

    #[test]
    fn every_page_fits_in_both_languages() {
        for id_lang in [false, true] {
            for privacy in [false, true] {
                for link in ALL_LINKS {
                    let list: Vec<Session> = ALL_STATES.iter().enumerate().map(|(i, st)| sess(*st, &format!("hesewake-notch-{i}"), Some("feature/coding-agents"))).collect();
                    let mut u = ui_with(link, list);
                    u.l = Lang(id_lang);
                    u.cfg.privacy = privacy;
                    let ps = all_pages(&mut u);
                    assert_eq!(ps.len(), 2 + ALL_STATES.len());
                    for p in ps {
                        check_limits(&p);
                    }
                    let mut empty = ui_with(link, vec![]);
                    empty.l = Lang(id_lang);
                    check_limits(&empty.page());
                }
            }
        }
    }

    #[test]
    fn revs_grow_and_language_follows_the_notch() {
        let mut u = ui_with(Link::On, vec![sess(State::Done, "app", None)]);
        let a = u.page()["rev"].as_u64().unwrap();
        let out = u.on_msg(&json!({"t": "lang", "lang": "id"}));
        let p = pages(&out)[0];
        assert!(p["rev"].as_u64().unwrap() > a);
        assert_eq!(p["title"], "Agen |Koding");
        assert_eq!(row(p, "app")["sub"], "Claude · Selesai 2 mnt lalu");
        let start = u.start();
        assert!(pages(&start)[0]["rev"].as_u64().unwrap() > p["rev"].as_u64().unwrap());
    }

    #[test]
    fn badge_toasts_and_status_follow_the_sessions() {
        let mut u = ui_with(Link::On, vec![]);
        let mut m = msg(EventKind::NeedsYou, "s1", 7);
        m.project = "app".into();
        let out = u.on_event(&m);
        assert!(sent(&out).contains(&&json!({"t": "badge", "on": true})));
        assert!(!sent(&out).iter().any(|v| v["t"] == "status"), "the status waits for the tick");
        let out = u.tick(NOW * 1000, |_| true, |_| true);
        assert!(sent(&out).iter().any(|v| v["t"] == "status" && v["text"] == "Claude connected · 1 sessions · 1 need you"));
        let toast = sent(&out).into_iter().find(|v| v["t"] == "toast").expect("the toast goes out on the next tick");
        assert_eq!(toast["title"], "app needs you");
        let out = u.on_msg(&json!({"t": "row", "id": "s1"}));
        assert!(sent(&out).contains(&&json!({"t": "badge", "on": false})));
        assert!(sent(&out).iter().any(|v| v["t"] == "status" && v["text"] == "Claude connected · 1 sessions"));
        // while the slide is open a new prompt sets no badge
        u.on_msg(&json!({"t": "visible", "on": true}));
        let out = u.on_event(&msg(EventKind::NeedsYou, "s2", 8));
        assert!(!sent(&out).iter().any(|v| v["t"] == "badge" && v["on"] == true));
        for (link, text) in [(Link::Off, "Claude not connected"), (Link::Invalid, "Claude settings unreadable")] {
            assert!(sent(&u.linked(link)).iter().any(|v| v["t"] == "status" && v["text"] == text), "{link:?}");
        }
        let mut quiet = ui_with(Link::On, vec![]);
        quiet.cfg.toast_needs = false;
        quiet.on_event(&m);
        assert!(!sent(&quiet.tick(NOW * 1000, |_| true, |_| true)).iter().any(|v| v["t"] == "toast"));
    }

    #[test]
    fn a_second_copy_says_so_in_the_status() {
        let mut u = ui_with(Link::On, vec![]);
        u.other_copy = true;
        assert!(sent(&u.start()).iter().any(|v| v["t"] == "status" && v["text"] == "Another copy is running"));
    }

    #[test]
    fn a_burst_of_events_stays_under_the_notch_rate_limit() {
        // the notch drops anything over 10 messages a second, toasts included
        for visible in [false, true] {
            let mut u = ui_with(Link::On, vec![]);
            u.start();
            u.on_msg(&json!({"t": "visible", "on": visible}));
            let mut frames = 0;
            for i in 0..10 {
                for kind in [EventKind::Prompt, EventKind::Tool, EventKind::Tool, EventKind::Stop] {
                    frames += sent(&u.on_event(&msg(kind, &format!("s{i}"), 7))).len();
                }
            }
            let out = u.tick(NOW * 1000, |_| true, |_| true);
            frames += sent(&out).len();
            assert!(frames <= 8, "{frames} frames in one second (visible: {visible})");
            let toast = sent(&out).into_iter().find(|v| v["t"] == "toast").expect("the merged toast");
            assert_eq!(toast["count"], 10);
            assert_eq!(pages(&out).len(), 1, "one page for the whole burst");
        }
    }

    #[test]
    fn hiding_the_slide_goes_home() {
        let mut u = ui_with(Link::On, vec![sess(State::Done, "app", None)]);
        u.on_msg(&json!({"t": "row", "id": "app"}));
        let out = u.on_msg(&json!({"t": "visible", "on": false}));
        assert!(!u.visible);
        assert_eq!(pages(&out)[0]["t"], "slide");
    }
}
