//! What the slide shows and how it answers the notch. No I/O here: every input is a message from the notch, an answer
//! from the network thread or a clock tick, and every output is an `Out` that `main.rs` carries out. That keeps the whole
//! behaviour testable without a notch or a server.

use crate::cfg::{parse_url, OdooCfg};
use crate::tickets::{self, Change, Cmd, Detail, Evt, Report, Task};
use serde_json::{json, Value};
use std::fmt::Display;

/// The list is old after this; opening the slide refreshes it.
const STALE_SECS: u64 = 60;
/// Without a working setup, look again this often (for example after Odoo was set up in the Office edition).
const SETUP_RECHECK_SECS: u64 = 300;
const SETUP_MSG: &str = "Odoo is not set up yet. Press Set up and enter your server, login and API key.";

pub enum Out {
    Send(Value),
    Work(Cmd),
    Save(OdooCfg),
    /// Read odoo.json and the key again, then call `Ui::reloaded`.
    Reload,
    /// Open the setup window (server, database, login, API key); what it saves comes back to `Ui::setup_saved`.
    OpenSetup,
}

enum Load<T> {
    Loading,
    Ready(T),
    Failed(String),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum View {
    List,
    Detail,
    Report,
    Settings,
}

pub struct Ui {
    /// Indonesian (the notch's language), else English.
    pub id_lang: bool,
    pub cfg: OdooCfg,
    /// An API key is in Credential Manager.
    pub key: bool,
    /// Unix seconds, local day ("YYYY-MM-DD") and the local zone against UTC; set by `main.rs` before each call.
    pub now: u64,
    pub today: String,
    pub tz_secs: i64,
    rev: u64,
    pub view: View,
    tab: String,
    pub tasks: Vec<Task>,
    loaded_at: Option<u64>,
    pub loading: bool,
    err: Option<String>,
    /// New or moved since the slide was last looked at: the "new" dot of their rows.
    pub new_ids: Vec<u32>,
    detail: Option<(u32, Load<Detail>)>,
    detail_from: View,
    pub report_date: String,
    report: Option<Load<Report>>,
    test: Option<Result<String, String>>,
    testing: bool,
    pub visible: bool,
    /// The next list is not news (the stages were changed): no toast for what it brings.
    quiet: bool,
    next_refresh: u64,
    status: String,
    foot: String,
}

/// "{}" in `s` replaced by `args`, in order.
fn fill(s: &str, args: &[&dyn Display]) -> String {
    let mut out = String::new();
    let mut parts = s.split("{}");
    if let Some(p) = parts.next() {
        out.push_str(p);
    }
    for (i, p) in parts.enumerate() {
        if let Some(a) = args.get(i) {
            out.push_str(&a.to_string());
        }
        out.push_str(p);
    }
    out
}

fn clip(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(n - 1).collect();
        t.push('\u{2026}');
        t
    }
}

/// The Indonesian text for an English one (the English text is the key).
fn indonesian(s: &str) -> Option<&'static str> {
    Some(match s {
        "Tic|kets" => "Ti|ket",
        "All" => "Semua",
        "Refresh" => "Segarkan",
        "Report" => "Laporan",
        "Settings" => "Pengaturan",
        "No tickets here" => "Tidak ada tiket",
        "Updating" => "Memperbarui",
        "Updated just now" => "Diperbarui barusan",
        "Updated {} ago" => "Diperbarui {} lalu",
        "Update failed: {}" => "Gagal memperbarui: {}",
        "{} min" => "{} mnt",
        "{} h" => "{} jam",
        "{} d" => "{} hari",
        SETUP_MSG => "Odoo belum diatur. Tekan Atur lalu isi server, login, dan API key.",
        "Set up" => "Atur",
        "Stage" => "Tahap",
        "Project" => "Proyek",
        "Due" => "Tenggat",
        "Created" => "Dibuat",
        "Attachments" => "Lampiran",
        "No description" => "Tanpa deskripsi",
        "Comment" => "Komentar",
        "Open" => "Buka",
        "Copy" => "Salin",
        "Re|port" => "Lap|oran",
        "Worked on \u{b7} {}" => "Dikerjakan \u{b7} {}",
        "Queue today \u{b7} {}" => "Antrian hari ini \u{b7} {}",
        "Nothing" => "Tidak ada",
        "Set|tings" => "Penga|turan",
        "Every {} min" => "Tiap {} mnt",
        "Test" => "Tes",
        "Alerts on" => "Notif nyala",
        "Alerts off" => "Notif mati",
        "Account" => "Akun",
        "Connection" => "Koneksi",
        "Stages" => "Tahap",
        "Server" => "Server",
        "Login" => "Login",
        "API key" => "API key",
        "Saved" => "Tersimpan",
        "Missing" => "Belum ada",
        "Testing" => "Menguji",
        "Connected \u{b7} {} stages" => "Tersambung \u{b7} {} tahap",
        "Shown" => "Tampil",
        "Hidden" => "Disembunyikan",
        "Press Test to load the stages" => "Tekan Tes untuk memuat tahap",
        "Click a stage to show or hide it" => "Klik tahap untuk menampilkan atau menyembunyikannya",
        "New \u{b7} {}" => "Baru \u{b7} {}",
        "{} tickets changed" => "{} tiket berubah",
        "Not set up" => "Belum diatur",
        "Connecting" => "Menyambung",
        "Error: {}" => "Galat: {}",
        "{} tickets \u{b7} {}" => "{} tiket \u{b7} {}",
        _ => return None,
    })
}

impl Ui {
    pub fn new(cfg: OdooCfg, key: bool, id_lang: bool) -> Ui {
        Ui {
            id_lang,
            cfg,
            key,
            now: 0,
            today: String::new(),
            tz_secs: 0,
            rev: 0,
            view: View::List,
            tab: "all".into(),
            tasks: Vec::new(),
            loaded_at: None,
            loading: false,
            err: None,
            new_ids: Vec::new(),
            detail: None,
            detail_from: View::List,
            report_date: String::new(),
            report: None,
            test: None,
            testing: false,
            visible: false,
            quiet: false,
            next_refresh: 0,
            status: String::new(),
            foot: String::new(),
        }
    }

    fn t(&self, s: &'static str) -> &'static str {
        if self.id_lang {
            indonesian(s).unwrap_or(s)
        } else {
            s
        }
    }

    pub fn ready(&self) -> bool {
        self.cfg.configured() && self.key
    }

    fn next_rev(&mut self) -> u64 {
        self.rev += 1;
        self.rev
    }

    // ------------------------------------------------------------ inputs
    pub fn start(&mut self) -> Vec<Out> {
        let mut out = Vec::new();
        if self.ready() {
            self.refresh(&mut out);
        } else {
            self.next_refresh = self.now + SETUP_RECHECK_SECS;
        }
        self.page_to(&mut out);
        self.status_to(&mut out);
        out
    }

    /// odoo.json and the key, read again.
    pub fn reloaded(&mut self, cfg: OdooCfg, key: bool) -> Vec<Out> {
        let mut out = Vec::new();
        self.cfg = cfg;
        self.key = key;
        self.err = None;
        if self.ready() {
            self.refresh(&mut out);
        } else {
            self.next_refresh = self.now + SETUP_RECHECK_SECS;
        }
        self.page_to(&mut out);
        self.status_to(&mut out);
        out
    }

    /// The setup window connected and handed over its settings (the key is already in Credential Manager).
    pub fn setup_saved(&mut self, cfg: OdooCfg) -> Vec<Out> {
        let mut out = Vec::new();
        self.cfg = cfg;
        self.key = true;
        self.err = None;
        self.testing = false;
        self.tasks.clear();
        self.loaded_at = None;
        self.new_ids.clear();
        out.push(Out::Save(self.cfg.clone()));
        let n = self.cfg.active_stages().count();
        self.test = Some(Ok(fill(self.t("Connected \u{b7} {} stages"), &[&n])));
        // what a new account already has is not news
        self.quiet = true;
        self.refresh_now(&mut out);
        self.page_to(&mut out);
        self.status_to(&mut out);
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
                    out.push(Out::Send(json!({"t": "badge", "on": false})));
                    let stale = self.loaded_at.map_or(true, |t| self.now.saturating_sub(t) > STALE_SECS);
                    if self.ready() && stale && !self.loading {
                        self.refresh(&mut out);
                    }
                } else {
                    // the "new" dots are for one look; the next look starts at the list
                    self.new_ids.clear();
                    self.view = View::List;
                    self.detail = None;
                }
                self.page_to(&mut out);
            }
            "tab" => {
                self.tab = id;
                self.view = View::List;
                self.page_to(&mut out);
            }
            "row" => match self.view {
                View::List | View::Report => {
                    if let Ok(n) = id.parse::<u32>() {
                        self.detail_from = self.view;
                        self.view = View::Detail;
                        self.open_detail(n, &mut out);
                        self.page_to(&mut out);
                    }
                }
                View::Settings if matches!(id.as_str(), "-server" | "-login" | "-key") => out.push(Out::OpenSetup),
                View::Settings => {
                    let sid = id.strip_prefix('s').and_then(|s| s.parse::<u32>().ok());
                    if let Some(st) = sid.and_then(|sid| self.cfg.stages.iter_mut().find(|s| s.id == sid)) {
                        st.on = !st.on;
                        out.push(Out::Save(self.cfg.clone()));
                        // tickets of a stage switched on are not news
                        self.quiet = true;
                        self.refresh_now(&mut out);
                        self.page_to(&mut out);
                    }
                }
                View::Detail => {}
            },
            "back" => {
                if self.view == View::Detail {
                    self.view = self.detail_from;
                    self.detail = None;
                } else {
                    self.view = View::List;
                }
                self.page_to(&mut out);
            }
            "step" => {
                let dir = m["dir"].as_i64().unwrap_or(0).signum();
                match self.view {
                    View::Report if dir != 0 => {
                        if let Some(d) = tickets::shift_date(&self.report_date, dir).filter(|d| d.as_str() <= self.today.as_str()) {
                            self.report_date = d;
                            self.open_report(&mut out);
                        }
                        self.page_to(&mut out);
                    }
                    View::Settings if dir != 0 => {
                        self.cfg.step_interval(dir > 0);
                        self.next_refresh = self.now + self.cfg.interval_secs();
                        out.push(Out::Save(self.cfg.clone()));
                        self.page_to(&mut out);
                    }
                    _ => {}
                }
            }
            "action" => match id.as_str() {
                "refresh" => {
                    self.refresh_now(&mut out);
                    self.page_to(&mut out);
                }
                "retry" => {
                    match (self.view, self.detail.as_ref().map(|d| d.0)) {
                        (View::Detail, Some(n)) => self.open_detail(n, &mut out),
                        (View::Report, _) => self.open_report(&mut out),
                        _ => self.refresh_now(&mut out),
                    }
                    self.page_to(&mut out);
                }
                "report" => {
                    self.view = View::Report;
                    // the daily report is about yesterday, as in the Office edition
                    self.report_date = tickets::shift_date(&self.today, -1).unwrap_or_else(|| self.today.clone());
                    self.open_report(&mut out);
                    self.page_to(&mut out);
                }
                "settings" => {
                    self.view = View::Settings;
                    self.page_to(&mut out);
                }
                "setup" | "account" => out.push(Out::OpenSetup),
                "test" => {
                    self.testing = true;
                    self.test = None;
                    out.push(Out::Work(Cmd::Test(self.cfg.clone())));
                    self.page_to(&mut out);
                }
                "alerts" => {
                    self.cfg.alerts = !self.cfg.alerts;
                    out.push(Out::Save(self.cfg.clone()));
                    self.page_to(&mut out);
                }
                _ => {}
            },
            // Settings > Extensions > Settings: the account; the stages and the rest are on the slide's Settings page
            "config" => out.push(Out::OpenSetup),
            "lang" => {
                self.id_lang = m["lang"] == "id";
                self.page_to(&mut out);
            }
            _ => {}
        }
        self.status_to(&mut out);
        out
    }

    /// An answer from the network thread.
    pub fn on_evt(&mut self, e: Evt) -> Vec<Out> {
        let mut out = Vec::new();
        match e {
            Evt::List { tasks, changes, first } => {
                self.tasks = tasks;
                self.loaded_at = Some(self.now);
                self.loading = false;
                self.err = None;
                self.next_refresh = self.now + self.cfg.interval_secs();
                if !first && !changes.is_empty() && !self.quiet {
                    self.notify(&changes, &mut out);
                }
                self.quiet = false;
                if self.view == View::List {
                    self.page_to(&mut out);
                }
            }
            Evt::ListErr(e) => {
                self.loading = false;
                self.err = Some(e);
                self.next_refresh = self.now + self.cfg.interval_secs();
                if self.view == View::List {
                    self.page_to(&mut out);
                }
            }
            Evt::Detail(id, r) => {
                if self.detail.as_ref().is_some_and(|d| d.0 == id) {
                    self.detail = Some((id, r.map_or_else(Load::Failed, Load::Ready)));
                    if self.view == View::Detail {
                        self.page_to(&mut out);
                    }
                }
            }
            Evt::Report(date, r) => {
                if date == self.report_date {
                    self.report = Some(r.map_or_else(Load::Failed, Load::Ready));
                    if self.view == View::Report {
                        self.page_to(&mut out);
                    }
                }
            }
            Evt::Test(r) => {
                self.testing = false;
                match r {
                    Ok(ok) => {
                        tickets::apply_test(&mut self.cfg, &ok);
                        out.push(Out::Save(self.cfg.clone()));
                        let n = self.cfg.active_stages().count();
                        self.test = Some(Ok(fill(self.t("Connected \u{b7} {} stages"), &[&n])));
                        self.quiet = true;
                        self.refresh_now(&mut out);
                    }
                    Err(e) => self.test = Some(Err(e)),
                }
                if self.view == View::Settings {
                    self.page_to(&mut out);
                }
            }
        }
        self.status_to(&mut out);
        out
    }

    /// Once a second: the monitor's refresh, and the "Updated ... ago" line while it is looked at.
    pub fn tick(&mut self) -> Vec<Out> {
        let mut out = Vec::new();
        if self.now < self.next_refresh {
            if self.visible && self.view == View::List && self.foot_text() != self.foot {
                self.page_to(&mut out);
            }
            return out;
        }
        if !self.ready() {
            self.next_refresh = self.now + SETUP_RECHECK_SECS;
            out.push(Out::Reload);
        } else if !self.loading {
            self.refresh(&mut out);
            if self.view == View::List {
                self.page_to(&mut out);
            }
        }
        out
    }

    // ------------------------------------------------------------ work
    fn refresh(&mut self, out: &mut Vec<Out>) {
        if !self.loading {
            self.refresh_now(out);
        }
    }

    /// Ask for the list even if one is on its way (the settings changed since it was asked for).
    fn refresh_now(&mut self, out: &mut Vec<Out>) {
        if !self.ready() {
            out.push(Out::Reload);
            return;
        }
        self.loading = true;
        self.next_refresh = self.now + self.cfg.interval_secs();
        out.push(Out::Work(Cmd::Refresh(self.cfg.clone())));
    }

    fn open_detail(&mut self, id: u32, out: &mut Vec<Out>) {
        self.detail = Some((id, Load::Loading));
        out.push(Out::Work(Cmd::Detail(self.cfg.clone(), id)));
    }

    fn open_report(&mut self, out: &mut Vec<Out>) {
        self.report = Some(Load::Loading);
        out.push(Out::Work(Cmd::Report(self.cfg.clone(), self.report_date.clone())));
    }

    fn notify(&mut self, changes: &[Change], out: &mut Vec<Out>) {
        for c in changes {
            if !self.new_ids.contains(&c.task.id) {
                self.new_ids.push(c.task.id);
            }
        }
        if !self.visible {
            out.push(Out::Send(json!({"t": "badge", "on": true})));
        }
        if !self.cfg.alerts {
            return;
        }
        let first = &changes[0];
        let name = format!("#{} {}", first.task.id, first.task.name);
        let sub = match &first.from {
            None => fill(self.t("New \u{b7} {}"), &[&first.task.stage]),
            Some(from) => format!("{} \u{2192} {}", from, first.task.stage),
        };
        let (title, sub) = if changes.len() == 1 { (name, sub) } else { (fill(self.t("{} tickets changed"), &[&changes.len()]), name) };
        out.push(Out::Send(json!({"t": "toast", "title": clip(&title, 80), "sub": clip(&sub, 60), "count": changes.len()})));
    }

    // ------------------------------------------------------------ pages
    fn page_to(&mut self, out: &mut Vec<Out>) {
        let p = self.page();
        out.push(Out::Send(p));
    }

    pub fn page(&mut self) -> Value {
        match self.view {
            View::List => self.home(),
            View::Detail => self.detail_page(),
            View::Report => self.report_page(),
            View::Settings => self.settings_page(),
        }
    }

    /// Stages that have tickets now, in Odoo's order, with how many.
    fn stage_tabs(&self) -> Vec<(u32, String, usize)> {
        self.cfg
            .active_stages()
            .filter_map(|s| {
                let n = self.tasks.iter().filter(|t| t.stage_id == s.id).count();
                (n > 0).then(|| (s.id, s.name.clone(), n))
            })
            .collect()
    }

    fn ago(&self, secs: u64) -> String {
        match secs {
            s if s < 3600 => fill(self.t("{} min"), &[&(s / 60).max(1)]),
            s if s < 86_400 => fill(self.t("{} h"), &[&(s / 3600)]),
            s => fill(self.t("{} d"), &[&(s / 86_400)]),
        }
    }

    fn foot_text(&self) -> String {
        if self.loading {
            return self.t("Updating").to_string();
        }
        if let Some(e) = self.err.as_ref().filter(|_| !self.tasks.is_empty()) {
            return clip(&fill(self.t("Update failed: {}"), &[e]), 120);
        }
        match self.loaded_at {
            Some(at) if self.now.saturating_sub(at) < 60 => self.t("Updated just now").to_string(),
            Some(at) => fill(self.t("Updated {} ago"), &[&self.ago(self.now.saturating_sub(at))]),
            None => String::new(),
        }
    }

    fn row(t: &Task, mark: bool) -> Value {
        json!({"id": t.id.to_string(), "text": format!("#{} {}", t.id, t.name), "sub": t.stage, "mark": mark})
    }

    fn home(&mut self) -> Value {
        let rev = self.next_rev();
        let title = self.t("Tic|kets");
        if !self.ready() {
            return json!({"t": "slide", "rev": rev, "title": title, "state": "setup", "message": self.t(SETUP_MSG), "setup_label": self.t("Set up")});
        }
        if self.loaded_at.is_none() {
            return match (&self.err, self.loading) {
                (Some(e), false) => json!({"t": "slide", "rev": rev, "title": title, "state": "error", "message": clip(e, 120)}),
                _ => json!({"t": "slide", "rev": rev, "title": title, "state": "loading"}),
            };
        }
        let stages = self.stage_tabs();
        // at most 8 chips: All and the first seven stages
        let stages = &stages[..stages.len().min(7)];
        let sel = self.tab.parse::<u32>().ok().filter(|id| stages.iter().any(|s| s.0 == *id));
        if sel.is_none() {
            self.tab = "all".into();
        }
        let mut tabs = vec![json!({"id": "all", "label": self.t("All"), "count": self.tasks.len()})];
        tabs.extend(stages.iter().map(|(id, name, n)| json!({"id": id.to_string(), "label": clip(name, 12), "count": n})));
        let rows: Vec<Value> = self
            .tasks
            .iter()
            .filter(|t| sel.map_or(true, |s| t.stage_id == s))
            .take(100)
            .map(|t| Ui::row(t, self.new_ids.contains(&t.id)))
            .collect();
        let actions = json!([
            {"id": "refresh", "label": self.t("Refresh")},
            {"id": "report", "label": self.t("Report")},
            {"id": "settings", "label": self.t("Settings")}
        ]);
        self.foot = self.foot_text();
        let mut p = json!({"t": "slide", "rev": rev, "title": title, "tabs": tabs, "tab": self.tab, "actions": actions, "rows": rows, "foot": self.foot});
        if self.tasks.is_empty() {
            p["state"] = json!("empty");
            p["message"] = json!(self.t("No tickets here"));
        }
        p
    }

    fn detail_page(&mut self) -> Value {
        let rev = self.next_rev();
        let Some((id, st)) = &self.detail else {
            return json!({"t": "detail", "rev": rev, "for": "", "state": "loading", "title": ""});
        };
        match st {
            Load::Loading => json!({"t": "detail", "rev": rev, "for": id.to_string(), "state": "loading", "title": format!("#{id}")}),
            Load::Failed(e) => json!({"t": "detail", "rev": rev, "for": id.to_string(), "state": "error", "title": format!("#{id}"), "message": clip(e, 120)}),
            Load::Ready(d) => {
                let title = format!("#{} {}", d.task.id, d.task.name);
                let mut meta = vec![json!({"k": self.t("Stage"), "v": d.task.stage})];
                if !d.project.is_empty() {
                    meta.push(json!({"k": self.t("Project"), "v": d.project}));
                }
                if !d.deadline.is_empty() {
                    meta.push(json!({"k": self.t("Due"), "v": d.deadline}));
                }
                if !d.task.created.is_empty() {
                    let day: String = tickets::local_minute(&d.task.created, self.tz_secs).chars().take(10).collect();
                    meta.push(json!({"k": self.t("Created"), "v": day}));
                }
                if d.attachments > 0 {
                    meta.push(json!({"k": self.t("Attachments"), "v": d.attachments.to_string()}));
                }
                let body = if d.description.is_empty() { self.t("No description").to_string() } else { d.description.chars().take(4000).collect() };
                let notes: Vec<Value> = d
                    .comments
                    .iter()
                    .map(|c| {
                        let who = if c.author.is_empty() { self.t("Comment").to_string() } else { c.author.clone() };
                        json!({"who": clip(&who, 24), "when": tickets::local_minute(&c.date, self.tz_secs), "text": clip(&c.text, 400)})
                    })
                    .collect();
                let mut buttons = Vec::new();
                if let Some(link) = ticket_link(&self.cfg, d.task.id) {
                    buttons.push(json!({"id": "open", "label": self.t("Open"), "link": link}));
                }
                buttons.push(json!({"id": "copy", "label": self.t("Copy"), "copy": title}));
                json!({"t": "detail", "rev": rev, "for": id.to_string(), "title": clip(&title, 120), "meta": meta, "body": body, "notes": notes, "buttons": buttons})
            }
        }
    }

    fn report_page(&mut self) -> Value {
        let rev = self.next_rev();
        let can_next = tickets::shift_date(&self.report_date, 1).is_some_and(|d| d.as_str() <= self.today.as_str());
        let mut p = json!({
            "t": "slide", "rev": rev, "title": self.t("Re|port"), "back": true,
            "stepper": {"label": self.report_date, "prev": true, "next": can_next}
        });
        match &self.report {
            None | Some(Load::Loading) => p["state"] = json!("loading"),
            Some(Load::Failed(e)) => {
                p["state"] = json!("error");
                p["message"] = json!(clip(e, 120));
            }
            Some(Load::Ready(r)) => {
                let rows = |list: &[Task], none: &str| -> Vec<Value> {
                    if list.is_empty() {
                        vec![json!({"id": "-", "text": none, "tone": "muted"})]
                    } else {
                        list.iter().take(48).map(|t| Ui::row(t, false)).collect()
                    }
                };
                let none = self.t("Nothing");
                p["actions"] = json!([{"id": "copy", "label": self.t("Copy"), "copy": tickets::report_text(&self.report_date, r)}]);
                p["sections"] = json!([
                    {"title": fill(self.t("Worked on \u{b7} {}"), &[&r.worked.len()]), "rows": rows(&r.worked[..], none)},
                    {"title": fill(self.t("Queue today \u{b7} {}"), &[&r.queue.len()]), "rows": rows(&r.queue[..], none)}
                ]);
            }
        }
        p
    }

    fn settings_page(&mut self) -> Value {
        let rev = self.next_rev();
        let host = parse_url(&self.cfg.url).map(|e| e.host).unwrap_or_else(|_| "\u{2014}".into());
        let login = if self.cfg.login.is_empty() { "\u{2014}".to_string() } else { self.cfg.login.clone() };
        let mut conn = vec![
            json!({"id": "-server", "text": host, "sub": self.t("Server")}),
            json!({"id": "-login", "text": login, "sub": self.t("Login")}),
            json!({"id": "-key", "text": self.t("API key"), "sub": if self.key { self.t("Saved") } else { self.t("Missing") }, "tone": if self.key { "good" } else { "warn" }}),
        ];
        match (&self.test, self.testing) {
            (_, true) => conn.push(json!({"id": "-test", "text": self.t("Testing"), "tone": "muted"})),
            (Some(Ok(s)), _) => conn.push(json!({"id": "-test", "text": s, "sub": self.t("Test"), "tone": "good"})),
            (Some(Err(e)), _) => conn.push(json!({"id": "-test", "text": clip(e, 120), "sub": self.t("Test"), "tone": "warn"})),
            _ => {}
        }
        let mut stages: Vec<Value> = self
            .cfg
            .stages
            .iter()
            .take(90)
            .map(|s| json!({"id": format!("s{}", s.id), "text": s.name, "sub": if s.on { self.t("Shown") } else { self.t("Hidden") }, "tone": if s.on { "good" } else { "muted" }}))
            .collect();
        if stages.is_empty() {
            stages.push(json!({"id": "-none", "text": self.t("Press Test to load the stages"), "tone": "muted"}));
        }
        json!({
            "t": "slide", "rev": rev, "title": self.t("Set|tings"), "back": true,
            "stepper": {"label": fill(self.t("Every {} min"), &[&self.cfg.interval_min]), "prev": true, "next": true},
            "actions": [
                {"id": "account", "label": self.t("Account")},
                {"id": "test", "label": self.t("Test")},
                {"id": "alerts", "label": if self.cfg.alerts { self.t("Alerts on") } else { self.t("Alerts off") }}
            ],
            "sections": [{"title": self.t("Connection"), "rows": conn}, {"title": self.t("Stages"), "rows": stages}],
            "foot": self.t("Click a stage to show or hide it")
        })
    }

    // ------------------------------------------------------------ status in Settings > Extensions
    fn status_text(&self) -> String {
        let s = if !self.ready() {
            self.t("Not set up").to_string()
        } else if let Some(e) = &self.err {
            fill(self.t("Error: {}"), &[e])
        } else if self.loaded_at.is_none() {
            self.t("Connecting").to_string()
        } else {
            fill(self.t("{} tickets \u{b7} {}"), &[&self.tasks.len(), &self.cfg.login])
        };
        clip(&s, 60)
    }

    fn status_to(&mut self, out: &mut Vec<Out>) {
        let s = self.status_text();
        if s != self.status {
            out.push(Out::Send(json!({"t": "status", "text": s})));
            self.status = s;
        }
    }
}

/// The ticket in Odoo's web client, if the server address is a valid https one.
pub fn ticket_link(cfg: &OdooCfg, id: u32) -> Option<String> {
    parse_url(&cfg.url).ok()?;
    let link = format!("{}/web#id={}&model=project.task&view_type=form", cfg.url.trim().trim_end_matches('/'), id);
    (link.chars().count() <= 500).then_some(link)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cfg::StageCfg;

    fn task(id: u32, stage_id: u32, stage: &str) -> Task {
        Task { id, name: format!("t{id}"), stage_id, stage: stage.into(), created: "2026-10-24 03:12:00".into(), users: vec![5] }
    }

    fn cfg() -> OdooCfg {
        OdooCfg {
            url: "https://odoo.example.com/".into(),
            db: "db".into(),
            login: "me@example.com".into(),
            uid: 5,
            stages: vec![StageCfg { id: 19, name: "TODO".into(), on: true }, StageCfg { id: 20, name: "IN PROGRESS".into(), on: true }, StageCfg { id: 26, name: "RELEASED".into(), on: false }],
            ..Default::default()
        }
    }

    fn ui() -> Ui {
        let mut u = Ui::new(cfg(), true, false);
        u.now = 1_000_000;
        u.today = "2026-11-02".into();
        u
    }

    fn sent(out: &[Out]) -> Vec<&Value> {
        out.iter().filter_map(|o| if let Out::Send(v) = o { Some(v) } else { None }).collect()
    }

    fn pages(out: &[Out]) -> Vec<&Value> {
        sent(out).into_iter().filter(|v| v["t"] == "slide" || v["t"] == "detail").collect()
    }

    fn works(out: &[Out]) -> Vec<&Cmd> {
        out.iter().filter_map(|o| if let Out::Work(c) = o { Some(c) } else { None }).collect()
    }

    fn loaded(u: &mut Ui, tasks: Vec<Task>) -> Vec<Out> {
        u.on_evt(Evt::List { tasks, changes: vec![], first: true })
    }

    #[test]
    fn fill_and_clip() {
        assert_eq!(fill("{} tickets \u{b7} {}", &[&3, &"me"]), "3 tickets \u{b7} me");
        assert_eq!(fill("none", &[&1]), "none");
        assert_eq!(clip("abcdef", 4), "abc\u{2026}");
        assert_eq!(clip("abc", 4), "abc");
    }

    #[test]
    fn every_text_has_indonesian() {
        let src = include_str!("ui.rs");
        // every text this file passes to t() has a translation
        for part in src.split("self.t(\"").skip(1) {
            let key = &part[..part.find("\")").unwrap()];
            let key = key.replace("\\u{b7}", "\u{b7}");
            assert!(indonesian(&key).is_some(), "no Indonesian for {key:?}");
        }
        assert!(indonesian(SETUP_MSG).is_some());
    }

    #[test]
    fn starts_by_loading_and_shows_the_list() {
        let mut u = ui();
        let out = u.start();
        assert!(matches!(works(&out)[..], [Cmd::Refresh(_)]));
        assert_eq!(pages(&out)[0]["state"], "loading");
        let out = loaded(&mut u, vec![task(3, 19, "TODO"), task(2, 20, "IN PROGRESS"), task(1, 20, "IN PROGRESS")]);
        let p = pages(&out)[0];
        assert_eq!(p["rows"].as_array().unwrap().len(), 3);
        assert_eq!(p["rows"][0], json!({"id": "3", "text": "#3 t3", "sub": "TODO", "mark": false}));
        let tabs: Vec<String> = p["tabs"].as_array().unwrap().iter().map(|t| format!("{} {}", t["label"].as_str().unwrap(), t["count"])).collect();
        assert_eq!(tabs, vec!["All 3", "TODO 1", "IN PROGRESS 2"]);
        assert_eq!(p["foot"], "Updated just now");
        assert!(sent(&out).iter().any(|v| v["t"] == "status" && v["text"] == "3 tickets \u{b7} me@example.com"));
    }

    #[test]
    fn a_tab_filters_and_a_row_opens_its_page() {
        let mut u = ui();
        u.start();
        loaded(&mut u, vec![task(3, 19, "TODO"), task(2, 20, "IN PROGRESS")]);
        let out = u.on_msg(&json!({"t": "tab", "id": "20"}));
        assert_eq!(pages(&out)[0]["rows"].as_array().unwrap().len(), 1);
        assert_eq!(pages(&out)[0]["tab"], "20");
        let out = u.on_msg(&json!({"t": "row", "id": "2"}));
        assert!(matches!(works(&out)[..], [Cmd::Detail(_, 2)]));
        assert_eq!(pages(&out)[0]["state"], "loading");
        let d = Detail { task: task(2, 20, "IN PROGRESS"), project: "P".into(), deadline: String::new(), description: String::new(), comments: vec![], attachments: 2 };
        let out = u.on_evt(Evt::Detail(2, Ok(d)));
        let p = pages(&out)[0];
        assert_eq!(p["t"], "detail");
        assert_eq!(p["body"], "No description");
        assert_eq!(p["buttons"][0]["link"], "https://odoo.example.com/web#id=2&model=project.task&view_type=form");
        assert_eq!(p["buttons"][1]["copy"], "#2 t2");
        assert!(p["meta"].as_array().unwrap().iter().any(|m| m["k"] == "Attachments" && m["v"] == "2"));
        let out = u.on_msg(&json!({"t": "back"}));
        assert_eq!(pages(&out)[0]["tab"], "20", "back to the same filter");
        // a stage without tickets any more: back to All
        let out = loaded(&mut u, vec![task(3, 19, "TODO")]);
        assert_eq!(pages(&out)[0]["tab"], "all");
    }

    #[test]
    fn report_steps_over_days_but_not_past_today() {
        let mut u = ui();
        u.start();
        loaded(&mut u, vec![]);
        let out = u.on_msg(&json!({"t": "action", "id": "report"}));
        assert_eq!(u.report_date, "2026-11-01");
        assert!(matches!(works(&out)[..], [Cmd::Report(_, d)] if d == "2026-11-01"));
        assert_eq!(pages(&out)[0]["stepper"], json!({"label": "2026-11-01", "prev": true, "next": true}));
        u.on_msg(&json!({"t": "step", "dir": 1}));
        assert_eq!(u.report_date, "2026-11-02");
        let out = u.on_msg(&json!({"t": "step", "dir": 1}));
        assert_eq!(u.report_date, "2026-11-02", "not into tomorrow");
        assert_eq!(pages(&out)[0]["stepper"]["next"], false);
        // an answer for another day is old
        let out = u.on_evt(Evt::Report("2026-11-01".into(), Ok(Report::default())));
        assert!(pages(&out).is_empty());
        let r = Report { worked: vec![task(7, 20, "IN PROGRESS")], queue: vec![] };
        let out = u.on_evt(Evt::Report("2026-11-02".into(), Ok(r)));
        let p = pages(&out)[0];
        assert_eq!(p["sections"][0]["title"], "Worked on \u{b7} 1");
        assert_eq!(p["sections"][1]["rows"][0]["text"], "Nothing");
        assert!(p["actions"][0]["copy"].as_str().unwrap().starts_with("Worked on 2026-11-02\n- #7 t7"));
        // a row of the report opens the ticket, and Back comes back to the report
        u.on_msg(&json!({"t": "row", "id": "7"}));
        assert_eq!(u.view, View::Detail);
        u.on_msg(&json!({"t": "row", "id": "-"}));
        u.on_msg(&json!({"t": "back"}));
        assert_eq!(u.view, View::Report);
        u.on_msg(&json!({"t": "back"}));
        assert_eq!(u.view, View::List);
    }

    #[test]
    fn the_monitor_toasts_new_and_moved_tickets() {
        let mut u = ui();
        u.start();
        loaded(&mut u, vec![task(1, 19, "TODO")]);
        // nothing is due yet
        u.now += 60;
        assert!(works(&u.tick()).is_empty());
        u.now += 15 * 60;
        let out = u.tick();
        assert!(matches!(works(&out)[..], [Cmd::Refresh(_)]));
        assert!(works(&u.tick()).is_empty(), "one at a time");
        let changes = vec![Change { task: task(1, 20, "IN PROGRESS"), from: Some("TODO".into()) }];
        let out = u.on_evt(Evt::List { tasks: vec![task(1, 20, "IN PROGRESS")], changes: changes.clone(), first: false });
        let toast = sent(&out).into_iter().find(|v| v["t"] == "toast").unwrap();
        assert_eq!(toast["title"], "#1 t1");
        assert_eq!(toast["sub"], "TODO \u{2192} IN PROGRESS");
        assert!(sent(&out).iter().any(|v| v["t"] == "badge" && v["on"] == true));
        assert_eq!(pages(&out)[0]["rows"][0]["mark"], true);
        // looking clears the badge; leaving clears the marks
        let out = u.on_msg(&json!({"t": "visible", "on": true}));
        assert!(sent(&out).iter().any(|v| v["t"] == "badge" && v["on"] == false));
        u.on_msg(&json!({"t": "visible", "on": false}));
        assert!(u.new_ids.is_empty());
        // two at once
        let two = vec![Change { task: task(8, 19, "TODO"), from: None }, Change { task: task(9, 19, "TODO"), from: None }];
        let out = u.on_evt(Evt::List { tasks: vec![], changes: two, first: false });
        let toast = sent(&out).into_iter().find(|v| v["t"] == "toast").unwrap();
        assert_eq!((toast["title"].as_str(), toast["count"].as_u64()), (Some("2 tickets changed"), Some(2)));
        // alerts off: marks and badge, no toast
        u.cfg.alerts = false;
        let out = u.on_evt(Evt::List { tasks: vec![], changes, first: false });
        assert!(!sent(&out).iter().any(|v| v["t"] == "toast"));
    }

    #[test]
    fn settings_toggle_stages_and_step_the_interval() {
        let mut u = ui();
        u.start();
        loaded(&mut u, vec![]);
        let out = u.on_msg(&json!({"t": "action", "id": "settings"}));
        let p = pages(&out)[0];
        assert_eq!(p["stepper"]["label"], "Every 15 min");
        assert_eq!(p["sections"][0]["rows"][0]["text"], "odoo.example.com");
        assert_eq!(p["sections"][1]["rows"][2], json!({"id": "s26", "text": "RELEASED", "sub": "Hidden", "tone": "muted"}));
        let out = u.on_msg(&json!({"t": "row", "id": "s26"}));
        assert!(u.cfg.stages[2].on);
        assert!(out.iter().any(|o| matches!(o, Out::Save(c) if c.stages[2].on)));
        assert!(matches!(works(&out)[..], [Cmd::Refresh(_)]));
        // the tickets of the stage just switched on are not news
        let out = u.on_evt(Evt::List { tasks: vec![task(4, 26, "RELEASED")], changes: vec![Change { task: task(4, 26, "RELEASED"), from: None }], first: false });
        assert!(!sent(&out).iter().any(|v| v["t"] == "toast"));
        let out = u.on_msg(&json!({"t": "step", "dir": 1}));
        assert_eq!(u.cfg.interval_min, 30);
        assert!(out.iter().any(|o| matches!(o, Out::Save(c) if c.interval_min == 30)));
        let out = u.on_msg(&json!({"t": "action", "id": "alerts"}));
        assert!(!u.cfg.alerts);
        assert_eq!(pages(&out)[0]["actions"][1]["label"], "Alerts off");
        u.on_msg(&json!({"t": "action", "id": "test"}));
        let ok = tickets::TestOk { uid: 5, partner_id: 9, stage_subtype: 3, stages: vec![(19, "TODO".into()), (30, "QA".into())] };
        let out = u.on_evt(Evt::Test(Ok(ok)));
        assert_eq!(u.cfg.partner_id, 9);
        assert_eq!(pages(&out)[0]["sections"][0]["rows"][3]["text"], "Connected \u{b7} 2 stages");
        assert!(matches!(works(&out)[..], [Cmd::Refresh(_)]));
    }

    #[test]
    fn the_setup_window_connects_quietly() {
        let mut u = Ui::new(OdooCfg::default(), false, false);
        u.now = 10;
        u.start();
        let out = u.setup_saved(cfg());
        assert!(u.ready());
        assert!(out.iter().any(|o| matches!(o, Out::Save(c) if c.configured())));
        assert!(matches!(works(&out)[..], [Cmd::Refresh(_)]));
        assert_eq!(pages(&out)[0]["state"], "loading");
        // the account rows of the slide's Settings page open the window too
        u.view = View::Settings;
        assert!(matches!(u.on_msg(&json!({"t": "row", "id": "-key"}))[..], [Out::OpenSetup]));
        assert!(matches!(u.on_msg(&json!({"t": "action", "id": "account"}))[..], [Out::OpenSetup]));
    }

    #[test]
    fn without_setup_it_asks_and_looks_again() {
        let mut u = Ui::new(OdooCfg::default(), false, true);
        u.now = 10;
        let out = u.start();
        assert!(works(&out).is_empty());
        let p = pages(&out)[0];
        assert_eq!(p["state"], "setup");
        assert_eq!(p["setup_label"], "Atur");
        assert!(sent(&out).iter().any(|v| v["t"] == "status" && v["text"] == "Belum diatur"));
        assert!(matches!(u.on_msg(&json!({"t": "action", "id": "setup"}))[..], [Out::OpenSetup]));
        assert!(matches!(u.on_msg(&json!({"t": "config"}))[..], [Out::OpenSetup]));
        u.now += SETUP_RECHECK_SECS;
        assert!(matches!(u.tick()[..], [Out::Reload]));
        let out = u.reloaded(cfg(), true);
        assert!(matches!(works(&out)[..], [Cmd::Refresh(_)]));
    }

    #[test]
    fn errors_show_with_retry_or_in_the_foot() {
        let mut u = ui();
        u.start();
        let out = u.on_evt(Evt::ListErr("Could not reach the server: timeout".into()));
        assert_eq!(pages(&out)[0]["state"], "error");
        assert!(matches!(works(&u.on_msg(&json!({"t": "action", "id": "retry"})))[..], [Cmd::Refresh(_)]));
        loaded(&mut u, vec![task(1, 19, "TODO")]);
        let out = u.on_evt(Evt::ListErr("timeout".into()));
        let p = pages(&out)[0];
        assert_eq!(p["rows"].as_array().unwrap().len(), 1, "the last list stays");
        assert_eq!(p["foot"], "Update failed: timeout");
    }

    #[test]
    fn revs_grow_and_language_follows_the_notch() {
        let mut u = ui();
        let a = u.page()["rev"].as_u64().unwrap();
        let out = u.on_msg(&json!({"t": "lang", "lang": "id"}));
        assert!(pages(&out)[0]["rev"].as_u64().unwrap() > a);
        assert_eq!(pages(&out)[0]["title"], "Ti|ket");
    }

    #[test]
    fn links_only_for_valid_servers() {
        let mut c = cfg();
        assert!(ticket_link(&c, 1).is_some());
        c.url = "http://odoo.example.com".into();
        assert!(ticket_link(&c, 1).is_none());
    }
}
