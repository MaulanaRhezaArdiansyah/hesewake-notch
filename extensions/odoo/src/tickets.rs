//! Ticket data: my tickets per stage, a ticket's page, the daily report, the monitor's diff, and the network thread that
//! makes every call. Ported from the Office edition (`tickets.rs`); projects, stages and automatic accounts come from
//! `odoo.json`, nothing in the code is specific to one company.

use crate::cfg::{data_dir, done_like, write_file, OdooCfg, StageCfg};
use crate::odoo::Session;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::mpsc::{Receiver, Sender};

const TASK_FIELDS: [&str; 5] = ["id", "name", "stage_id", "create_date", "user_ids"];
const LIMIT: u64 = 200;

#[derive(Clone, Debug, PartialEq)]
pub struct Task {
    pub id: u32,
    pub name: String,
    pub stage_id: u32,
    pub stage: String,
    pub created: String,
    pub users: Vec<u32>,
}

/// `[19, "TODO"]` or `false` -> (id, name).
fn pair(v: &Value) -> (u32, String) {
    match v.as_array() {
        Some(a) if a.len() >= 2 => (a[0].as_u64().unwrap_or(0) as u32, a[1].as_str().unwrap_or("").to_string()),
        _ => (0, String::new()),
    }
}

pub fn parse_task(v: &Value) -> Option<Task> {
    let id = v.get("id")?.as_u64()? as u32;
    let (stage_id, stage) = pair(v.get("stage_id").unwrap_or(&Value::Null));
    Some(Task {
        id,
        name: v.get("name").and_then(Value::as_str).unwrap_or("").trim().to_string(),
        stage_id,
        stage,
        created: v.get("create_date").and_then(Value::as_str).unwrap_or("").to_string(),
        users: v.get("user_ids").and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_u64().map(|n| n as u32)).collect()).unwrap_or_default(),
    })
}

fn tasks_of(rows: Vec<Value>) -> Vec<Task> {
    rows.iter().filter_map(parse_task).collect()
}

fn project_domain(cfg: &OdooCfg, dom: &mut Vec<Value>) {
    if !cfg.project_ids.is_empty() {
        dom.push(json!(["project_id", "in", cfg.project_ids]));
    }
}

/// My tickets in every stage that is on, by stage and then newest first; a team stage also has the team's tickets.
pub fn fetch_active(s: &Session, cfg: &OdooCfg) -> Result<Vec<Task>, String> {
    let on: Vec<u32> = cfg.active_stages().map(|st| st.id).collect();
    if on.is_empty() {
        return Ok(Vec::new());
    }
    let team_ids: Vec<u32> = on.iter().copied().filter(|id| cfg.team_stage_ids.contains(id)).collect();
    let mine_ids: Vec<u32> = on.iter().copied().filter(|id| !team_ids.contains(id)).collect();
    let mut all: Vec<Task> = Vec::new();
    let mut run = |stage_ids: Vec<u32>, users: Vec<u32>| -> Result<(), String> {
        if stage_ids.is_empty() {
            return Ok(());
        }
        let mut dom: Vec<Value> = Vec::new();
        project_domain(cfg, &mut dom);
        dom.push(json!(["stage_id", "in", stage_ids]));
        dom.push(json!(["user_ids", "in", users]));
        let rows = s.search_read("project.task", Value::Array(dom), json!(TASK_FIELDS), json!({"order": "id desc", "limit": LIMIT}))?;
        all.extend(tasks_of(rows));
        Ok(())
    };
    run(mine_ids, vec![cfg.uid])?;
    let team_users = if cfg.team_user_ids.is_empty() { vec![cfg.uid] } else { cfg.team_user_ids.clone() };
    run(team_ids, team_users)?;
    sort_tasks(cfg, &mut all);
    Ok(all)
}

pub fn sort_tasks(cfg: &OdooCfg, all: &mut Vec<Task>) {
    all.sort_by(|a, b| cfg.stage_index(a.stage_id).cmp(&cfg.stage_index(b.stage_id)).then(b.id.cmp(&a.id)));
    all.dedup_by_key(|t| t.id);
}

// ---------------------------------------------------------------- a ticket's page
#[derive(Clone, Debug, PartialEq)]
pub struct Comment {
    pub author: String,
    pub date: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Detail {
    pub task: Task,
    pub project: String,
    pub deadline: String,
    pub description: String,
    /// The last comments (at most 3), oldest first.
    pub comments: Vec<Comment>,
    pub attachments: usize,
}

pub fn fetch_detail(s: &Session, id: u32) -> Result<Detail, String> {
    // active_test:false: a CLOSED ticket is usually active=false and would look "not found"
    let rows = s.search_read(
        "project.task",
        json!([["id", "=", id]]),
        json!(["id", "name", "description", "stage_id", "project_id", "date_deadline", "user_ids", "create_date"]),
        json!({"context": {"active_test": false}}),
    )?;
    let row = rows.first().ok_or_else(|| format!("Ticket #{id} was not found"))?;
    let task = parse_task(row).ok_or("The ticket could not be read")?;
    let msgs = s.search_read(
        "mail.message",
        json!([["model", "=", "project.task"], ["res_id", "=", id]]),
        json!(["id", "body", "author_id", "date", "message_type", "attachment_ids"]),
        json!({"order": "date asc", "limit": 100}),
    )?;
    Ok(build_detail(task, row, &msgs))
}

pub fn build_detail(task: Task, row: &Value, msgs: &[Value]) -> Detail {
    let mut comments: Vec<Comment> = msgs
        .iter()
        .filter(|m| m.get("message_type").and_then(Value::as_str) == Some("comment"))
        .filter_map(|m| {
            let text = html_to_text(m.get("body").and_then(Value::as_str).unwrap_or(""));
            (!text.is_empty()).then(|| Comment {
                author: pair(m.get("author_id").unwrap_or(&Value::Null)).1,
                date: m.get("date").and_then(Value::as_str).unwrap_or("").to_string(),
                text,
            })
        })
        .collect();
    let keep = comments.len().saturating_sub(3);
    comments.drain(..keep);
    let attachments = msgs.iter().filter_map(|m| m.get("attachment_ids").and_then(Value::as_array)).map(Vec::len).sum();
    let desc = row.get("description").and_then(Value::as_str).unwrap_or("");
    Detail {
        task,
        project: pair(row.get("project_id").unwrap_or(&Value::Null)).1,
        deadline: row.get("date_deadline").and_then(Value::as_str).unwrap_or("").to_string(),
        description: html_to_text(desc),
        comments,
        attachments,
    }
}

/// Odoo's HTML -> plain text: tags dropped, blocks become new lines, entities decoded, spaces tidied.
pub fn html_to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut chars = html.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '<' => {
                let mut tag = String::new();
                for t in chars.by_ref() {
                    if t == '>' {
                        break;
                    }
                    tag.push(t);
                }
                let name: String = tag.trim_start_matches('/').chars().take_while(|c| c.is_ascii_alphanumeric()).collect::<String>().to_lowercase();
                let closing = tag.starts_with('/');
                match name.as_str() {
                    "br" | "hr" => out.push('\n'),
                    // a block: one line break (no empty line between paragraphs)
                    "p" | "div" | "h1" | "h2" | "h3" | "h4" | "ul" | "ol" | "table" | "tr" | "blockquote" | "pre" => {
                        if !out.is_empty() && !out.ends_with('\n') {
                            out.push('\n');
                        }
                    }
                    "li" if !closing => {
                        if !out.is_empty() && !out.ends_with('\n') {
                            out.push('\n');
                        }
                        out.push_str("\u{2022} ");
                    }
                    "td" | "th" if closing => out.push(' '),
                    _ => {}
                }
            }
            '&' => {
                let mut ent = String::new();
                let mut ok = false;
                for _ in 0..10 {
                    match chars.peek().copied() {
                        Some(';') => {
                            chars.next();
                            ok = true;
                            break;
                        }
                        Some(ch) if ch.is_ascii_alphanumeric() || ch == '#' => {
                            ent.push(ch);
                            chars.next();
                        }
                        _ => break,
                    }
                }
                match (ok, decode_entity(&ent)) {
                    (true, Some(s)) => out.push_str(&s),
                    _ => {
                        out.push('&');
                        out.push_str(&ent);
                        if ok {
                            out.push(';');
                        }
                    }
                }
            }
            c => out.push(c),
        }
    }
    tidy(&out)
}

fn decode_entity(e: &str) -> Option<String> {
    Some(match e {
        "amp" => "&".into(),
        "lt" => "<".into(),
        "gt" => ">".into(),
        "quot" => "\"".into(),
        "apos" => "'".into(),
        "nbsp" => " ".into(),
        _ => {
            let n = if let Some(h) = e.strip_prefix("#x").or_else(|| e.strip_prefix("#X")) {
                u32::from_str_radix(h, 16).ok()?
            } else {
                e.strip_prefix('#')?.parse::<u32>().ok()?
            };
            char::from_u32(n)?.to_string()
        }
    })
}

/// Tidy the spaces of each line and drop runs of empty lines.
fn tidy(s: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    for l in s.replace('\r', "").split('\n') {
        let t = l.split_whitespace().collect::<Vec<_>>().join(" ");
        if t.is_empty() && lines.last().map_or(true, |p| p.is_empty()) {
            continue;
        }
        lines.push(t);
    }
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

// ---------------------------------------------------------------- dates
fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y } as i64;
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    ((y + if m <= 2 { 1 } else { 0 }) as i32, m, d)
}

/// A valid "YYYY-MM-DD" (leap years too) -> (year, month, day).
pub fn parse_date(s: &str) -> Option<(i32, u32, u32)> {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let (y, m, d) = (s[0..4].parse::<i32>().ok()?, s[5..7].parse::<u32>().ok()?, s[8..10].parse::<u32>().ok()?);
    let back = civil_from_days(days_from_civil(y, m, d));
    (back == (y, m, d) && (1..=12).contains(&m)).then_some((y, m, d))
}

pub fn shift_date(s: &str, delta: i64) -> Option<String> {
    let (y, m, d) = parse_date(s)?;
    let (y, m, d) = civil_from_days(days_from_civil(y, m, d) + delta);
    Some(format!("{y:04}-{m:02}-{d:02}"))
}

/// "2026-10-24 03:12:00" (UTC, as Odoo writes it) -> Unix seconds.
pub fn odoo_unix(s: &str) -> Option<i64> {
    let (d, t) = s.split_once(' ')?;
    let (y, m, dd) = parse_date(d)?;
    let mut it = t.split(':').map(|x| x.parse::<i64>().ok());
    let (h, mi, se) = (it.next()??, it.next()??, it.next().flatten().unwrap_or(0));
    Some(days_from_civil(y, m, dd) * 86_400 + h * 3600 + mi * 60 + se)
}

fn fmt_secs(secs: i64) -> String {
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}", rem / 3600, (rem % 3600) / 60, rem % 60)
}

/// An Odoo date in local time, to the minute: "2026-10-24 10:12". Anything else is shown as it came.
pub fn local_minute(s: &str, tz_secs: i64) -> String {
    match odoo_unix(s) {
        Some(t) => fmt_secs(t + tz_secs)[..16].to_string(),
        None => s.to_string(),
    }
}

/// The day `date` for an Odoo query (UTC). `tz_offset_min`: the local zone against UTC; 0 = UTC.
pub fn report_range(date: &str, tz_offset_min: i32) -> Option<(String, String)> {
    let (y, m, d) = parse_date(date)?;
    let start = days_from_civil(y, m, d) * 86_400 - tz_offset_min as i64 * 60;
    Some((fmt_secs(start), fmt_secs(start + 86_399)))
}

// ---------------------------------------------------------------- report
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Report {
    pub worked: Vec<Task>,
    pub queue: Vec<Task>,
}

pub fn fetch_report(s: &Session, cfg: &OdooCfg, date: &str) -> Result<Report, String> {
    let (start, end) = report_range(date, cfg.tz_offset_min).ok_or("Not a date (YYYY-MM-DD)")?;
    let msg_fields = json!(["id", "res_id", "date"]);
    let kw = json!({"order": "date asc", "limit": 500});
    let mut touched: Vec<u32> = Vec::new();
    let mut add = |rows: Vec<Value>| {
        for r in rows {
            if let Some(id) = r.get("res_id").and_then(Value::as_u64) {
                if !touched.contains(&(id as u32)) {
                    touched.push(id as u32);
                }
            }
        }
    };
    // 1) my comments and chatter activity
    add(s.search_read(
        "mail.message",
        json!([["model", "=", "project.task"], ["author_id", "=", cfg.partner_id], ["date", ">=", start], ["date", "<=", end]]),
        msg_fields.clone(),
        kw.clone(),
    )?);
    // 2) comments of automatic accounts that count as mine (for example "Push to branch ...")
    if !cfg.api_partner_ids.is_empty() && !cfg.bot_comment.is_empty() {
        add(s.search_read(
            "mail.message",
            json!([["model", "=", "project.task"], ["author_id", "in", cfg.api_partner_ids], ["body", "like", cfg.bot_comment], ["date", ">=", start], ["date", "<=", end]]),
            msg_fields.clone(),
            kw.clone(),
        )?);
    }
    // 3) stage changes by me or an automatic account. On purpose not project.task's write_date (it moves without real
    //    work) and not stage changes by colleagues.
    if cfg.stage_subtype > 0 {
        let mut authors = vec![cfg.partner_id];
        authors.extend(cfg.api_partner_ids.iter().copied());
        add(s.search_read(
            "mail.message",
            json!([["model", "=", "project.task"], ["subtype_id", "=", cfg.stage_subtype], ["author_id", "in", authors], ["date", ">=", start], ["date", "<=", end]]),
            msg_fields,
            kw,
        )?);
    }
    let mut worked = Vec::new();
    if !touched.is_empty() {
        let rows = s.search_read("project.task", json!([["id", "in", touched]]), json!(TASK_FIELDS), json!({"order": "id desc"}))?;
        // still mine, and not in a queue stage
        worked = tasks_of(rows).into_iter().filter(|t| !cfg.report_exclude_ids.contains(&t.stage_id) && t.users.contains(&cfg.uid)).collect();
    }
    let queue_ids: Vec<u32> = if cfg.queue_ids.is_empty() { cfg.active_stages().map(|st| st.id).collect() } else { cfg.queue_ids.clone() };
    let mut queue = Vec::new();
    if !queue_ids.is_empty() {
        let mut dom: Vec<Value> = Vec::new();
        project_domain(cfg, &mut dom);
        dom.push(json!(["user_ids", "in", [cfg.uid]]));
        dom.push(json!(["stage_id", "in", queue_ids]));
        queue = tasks_of(s.search_read("project.task", Value::Array(dom), json!(TASK_FIELDS), json!({"order": "id desc", "limit": 100}))?);
    }
    Ok(Report { worked, queue })
}

/// The report as text for the clipboard (the same as the Office edition's Copy).
pub fn report_text(date: &str, r: &Report) -> String {
    let mut s = format!("Worked on {date}\n");
    for t in &r.worked {
        s.push_str(&format!("- #{} {} ({})\n", t.id, t.name, t.stage));
    }
    s.push_str("\nQueue today\n");
    for t in &r.queue {
        s.push_str(&format!("- #{} {} ({})\n", t.id, t.name, t.stage));
    }
    s
}

// ---------------------------------------------------------------- monitor (the diff against the last list)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Snap {
    pub id: u32,
    pub stage_id: u32,
    pub stage: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    pub task: Task,
    /// The stage it was in; None = a new ticket in the list.
    pub from: Option<String>,
}

/// New tickets (not in the last list) and tickets that changed stage. Tickets that left the list are not news.
pub fn diff(prev: &[Snap], now: &[Task]) -> Vec<Change> {
    now.iter()
        .filter_map(|t| match prev.iter().find(|p| p.id == t.id) {
            None => Some(Change { task: t.clone(), from: None }),
            Some(p) if p.stage_id != t.stage_id => Some(Change { task: t.clone(), from: Some(p.stage.clone()) }),
            _ => None,
        })
        .collect()
}

pub fn snapshot_of(tasks: &[Task]) -> Vec<Snap> {
    tasks.iter().map(|t| Snap { id: t.id, stage_id: t.stage_id, stage: t.stage.clone() }).collect()
}

fn load_snapshot() -> Option<Vec<Snap>> {
    serde_json::from_str(&std::fs::read_to_string(data_dir()?.join("snapshot.json")).ok()?).ok()
}

fn save_snapshot(tasks: &[Task]) {
    if let (Some(dir), Ok(s)) = (data_dir(), serde_json::to_string(&snapshot_of(tasks))) {
        write_file(&dir.join("snapshot.json"), &s);
    }
}

// ---------------------------------------------------------------- Test
#[derive(Clone, Debug, PartialEq)]
pub struct TestOk {
    pub uid: u32,
    pub partner_id: u32,
    pub stage_subtype: u32,
    pub stages: Vec<(u32, String)>,
}

pub fn run_test(cfg: &OdooCfg, key: &str) -> Result<TestOk, String> {
    if cfg.url.is_empty() || cfg.db.is_empty() || cfg.login.is_empty() {
        return Err("No server, database or login".to_string());
    }
    let uid = crate::odoo::authenticate(cfg, key)?;
    let probe = OdooCfg { uid, ..cfg.clone() };
    let s = Session::new(&probe, key)?;
    let users = s.search_read("res.users", json!([["id", "=", uid]]), json!(["partner_id"]), json!({}))?;
    let partner_id = users.first().map(|u| pair(u.get("partner_id").unwrap_or(&Value::Null)).0).unwrap_or(0);
    let dom = if cfg.project_ids.is_empty() { json!([]) } else { json!([["project_ids", "in", cfg.project_ids]]) };
    let rows = s
        .search_read("project.task.type", dom, json!(["id", "name"]), json!({"order": "sequence, id", "limit": 100}))
        .or_else(|_| s.search_read("project.task.type", json!([]), json!(["id", "name"]), json!({"order": "sequence, id", "limit": 100})))?;
    let stages = rows.iter().filter_map(|r| Some((r.get("id")?.as_u64()? as u32, r.get("name")?.as_str()?.to_string()))).collect();
    // "Stage Changed" by its xmlid (its id differs per install); without it the report has no stage changes
    let stage_subtype = s
        .search_read("ir.model.data", json!([["module", "=", "project"], ["name", "=", "mt_task_stage"]]), json!(["res_id"]), json!({"limit": 1}))
        .ok()
        .and_then(|r| r.first().and_then(|x| x.get("res_id")?.as_u64()))
        .unwrap_or(0) as u32;
    Ok(TestOk { uid, partner_id, stage_subtype, stages })
}

/// Take a Test's answer: new stages are on (unless named "done"), known stages keep their choice.
pub fn apply_test(cfg: &mut OdooCfg, ok: &TestOk) {
    cfg.uid = ok.uid;
    cfg.partner_id = ok.partner_id;
    if ok.stage_subtype > 0 {
        cfg.stage_subtype = ok.stage_subtype;
    }
    cfg.stages = ok
        .stages
        .iter()
        .map(|(id, name)| StageCfg {
            id: *id,
            name: name.clone(),
            on: cfg.stages.iter().find(|s| s.id == *id).map_or(!done_like(name), |s| s.on),
        })
        .collect();
}

// ---------------------------------------------------------------- the network thread
pub enum Cmd {
    Refresh(OdooCfg),
    Detail(OdooCfg, u32),
    Report(OdooCfg, String),
    Test(OdooCfg),
}

pub enum Evt {
    /// `first`: there was no earlier list, so nothing counts as new.
    List { tasks: Vec<Task>, changes: Vec<Change>, first: bool },
    ListErr(String),
    Detail(u32, Result<Detail, String>),
    Report(String, Result<Report, String>),
    Test(Result<TestOk, String>),
}

fn session(cfg: &OdooCfg) -> Result<Session, String> {
    if !cfg.configured() {
        return Err("Odoo is not set up".to_string());
    }
    let key = crate::cred::get_key().ok_or("No API key in Credential Manager")?;
    Session::new(cfg, &key)
}

/// Runs until the other side of either channel is gone.
pub fn worker(rx: Receiver<Cmd>, tx: Sender<Evt>) {
    while let Ok(cmd) = rx.recv() {
        let evt = match cmd {
            Cmd::Refresh(cfg) => match session(&cfg).and_then(|s| fetch_active(&s, &cfg)) {
                Ok(tasks) => {
                    let prev = load_snapshot();
                    let first = prev.is_none();
                    let changes = prev.map(|p| diff(&p, &tasks)).unwrap_or_default();
                    save_snapshot(&tasks);
                    Evt::List { tasks, changes, first }
                }
                Err(e) => Evt::ListErr(e),
            },
            Cmd::Detail(cfg, id) => Evt::Detail(id, session(&cfg).and_then(|s| fetch_detail(&s, id))),
            Cmd::Report(cfg, date) => {
                let r = session(&cfg).and_then(|s| fetch_report(&s, &cfg, &date));
                Evt::Report(date, r)
            }
            Cmd::Test(cfg) => Evt::Test(match crate::cred::get_key() {
                Some(key) => run_test(&cfg, &key),
                None => Err("No API key in Credential Manager".to_string()),
            }),
        };
        if tx.send(evt).is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn task(id: u32, stage_id: u32, stage: &str) -> Task {
        Task { id, name: format!("t{id}"), stage_id, stage: stage.into(), created: String::new(), users: vec![1] }
    }

    #[test]
    fn parses_task_rows() {
        let v = json!({"id": 8193, "name": "  Fix the login ", "stage_id": [19, "TODO"], "create_date": "2026-10-24 03:12:00", "user_ids": [32, 40]});
        let t = parse_task(&v).unwrap();
        assert_eq!((t.id, t.name.as_str(), t.stage_id, t.stage.as_str()), (8193, "Fix the login", 19, "TODO"));
        assert_eq!(t.users, vec![32, 40]);
        // name=false (Odoo's empty) and no stage do not panic
        let t = parse_task(&json!({"id": 1, "name": false, "stage_id": false})).unwrap();
        assert_eq!((t.name.as_str(), t.stage_id), ("", 0));
        assert!(parse_task(&json!({"name": "x"})).is_none());
    }

    #[test]
    fn html_to_text_basics() {
        assert_eq!(html_to_text("<p>Hello <b>world</b></p><p>Line two</p>"), "Hello world\nLine two");
        assert_eq!(html_to_text("a<br>b<br/>c"), "a\nb\nc");
        assert_eq!(html_to_text("<ul><li>one</li><li>two &amp; three</li></ul>"), "\u{2022} one\n\u{2022} two & three");
        assert_eq!(html_to_text("1 &lt; 2 &gt; 0 &quot;ok&quot; &#39;x&#39; &#x41;&nbsp;B"), "1 < 2 > 0 \"ok\" 'x' A B");
        assert_eq!(html_to_text("&unknown; & no semicolon"), "&unknown; & no semicolon");
        assert_eq!(html_to_text("<p><br></p><p>x</p><p><br></p>"), "x");
        assert_eq!(html_to_text(""), "");
    }

    #[test]
    fn dates() {
        assert_eq!(parse_date("2026-02-28"), Some((2026, 2, 28)));
        assert!(parse_date("2026-02-30").is_none());
        assert!(parse_date("2026-13-01").is_none());
        assert!(parse_date("26-1-1").is_none());
        assert!(parse_date("2024-02-29").is_some() && parse_date("2026-02-29").is_none());
        assert_eq!(shift_date("2026-03-01", -1).as_deref(), Some("2026-02-28"));
        assert_eq!(shift_date("2026-12-31", 1).as_deref(), Some("2027-01-01"));
        assert_eq!(report_range("2026-10-24", 0), Some(("2026-10-24 00:00:00".into(), "2026-10-24 23:59:59".into())));
        // UTC+7: the local day starts at 17:00 UTC the day before
        assert_eq!(report_range("2026-10-24", 420), Some(("2026-10-23 17:00:00".into(), "2026-10-24 16:59:59".into())));
        assert!(report_range("not", 0).is_none());
        assert_eq!(odoo_unix("1970-01-02 00:00:10"), Some(86_410));
        assert!(odoo_unix("empty").is_none());
        assert_eq!(local_minute("2026-10-24 20:12:59", 7 * 3600), "2026-10-25 03:12");
        assert_eq!(local_minute("soon", 0), "soon");
    }

    #[test]
    fn diff_reports_new_and_moved_only() {
        let prev = vec![Snap { id: 1, stage_id: 19, stage: "TODO".into() }, Snap { id: 2, stage_id: 20, stage: "PROG".into() }, Snap { id: 3, stage_id: 20, stage: "PROG".into() }];
        let now = vec![task(1, 19, "TODO"), task(2, 21, "MERGE"), task(4, 19, "TODO")];
        let ch = diff(&prev, &now);
        assert_eq!(ch.len(), 2, "ticket 3 left the list: not news");
        assert_eq!((ch[0].task.id, ch[0].from.as_deref()), (2, Some("PROG")));
        assert_eq!((ch[1].task.id, ch[1].from.clone()), (4, None));
        assert!(diff(&snapshot_of(&now), &now).is_empty());
    }

    #[test]
    fn detail_keeps_last_three_comments() {
        let row = json!({"description": "<p>The <i>ticket</i></p>", "project_id": [2, "Project"], "date_deadline": "2026-11-01"});
        let msgs: Vec<Value> = (1..=5)
            .map(|i| json!({"message_type": "comment", "body": format!("<p>comment {i}</p>"), "author_id": [9, "Budi"], "date": format!("2026-10-2{i} 10:00:00"), "attachment_ids": [i]}))
            .chain([json!({"message_type": "notification", "body": "", "author_id": false, "date": "x", "attachment_ids": []}), json!({"message_type": "comment", "body": "<p><br></p>", "author_id": false, "date": "y", "attachment_ids": []})])
            .collect();
        let d = build_detail(task(7, 19, "TODO"), &row, &msgs);
        assert_eq!(d.description, "The ticket");
        assert_eq!(d.project, "Project");
        assert_eq!(d.comments.iter().map(|c| c.text.as_str()).collect::<Vec<_>>(), vec!["comment 3", "comment 4", "comment 5"]);
        assert_eq!(d.comments[0].author, "Budi");
        assert_eq!(d.attachments, 5);
    }

    #[test]
    fn apply_test_keeps_existing_choices() {
        let mut cfg = OdooCfg { stages: vec![StageCfg { id: 19, name: "TODO".into(), on: false }], ..Default::default() };
        let ok = TestOk { uid: 5, partner_id: 9, stage_subtype: 8, stages: vec![(19, "TODO".into()), (20, "IN PROGRESS".into()), (26, "RELEASED".into())] };
        apply_test(&mut cfg, &ok);
        assert_eq!((cfg.uid, cfg.partner_id, cfg.stage_subtype), (5, 9, 8));
        let on: Vec<bool> = cfg.stages.iter().map(|s| s.on).collect();
        assert_eq!(on, vec![false, true, false], "TODO stays off (chosen), RELEASED is off by itself");
    }

    #[test]
    fn report_text_lists_both_parts() {
        let r = Report { worked: vec![task(5, 20, "PROG")], queue: vec![task(6, 19, "TODO")] };
        assert_eq!(report_text("2026-11-01", &r), "Worked on 2026-11-01\n- #5 t5 (PROG)\n\nQueue today\n- #6 t6 (TODO)\n");
    }
}
