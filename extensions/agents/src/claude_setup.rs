//! Claude Code's `settings.json`: add our hooks (Connect), take them out (Remove), and tell whether they are there.
//! Everything else in the file stays as it is, in its order; the person's original file is kept once as
//! `settings.json.hesewake-backup`; a file that isn't valid JSON is never written.

use serde_json::Value;
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Link {
    Off,
    On,
    /// Our hooks point at another copy of the exe (moved or replaced).
    Moved,
    /// Hooks of the old built-in integration (`hesewake-notch*.exe --claude-hook`) are there.
    Old,
    /// settings.json can't be read as a JSON object; nothing is changed.
    Invalid,
}

/// The hook events we listen to, with their matcher.
pub const EVENTS: [(&str, Option<&str>); 8] = [
    ("UserPromptSubmit", None),
    ("PreToolUse", Some("*")),
    ("PostToolUse", Some("*")),
    ("Notification", Some("permission_prompt|elicitation_dialog|elicitation_url_dialog|agent_needs_input|idle_prompt")),
    ("Stop", None),
    ("StopFailure", None),
    ("SessionStart", None),
    ("SessionEnd", None),
];

pub const HOOK_ARGS: [&str; 2] = ["--hook", "claude"];

/// Add our hooks for `exe`; any older ones of ours (also of the old built-in integration) are replaced. A status line
/// is never touched here.
pub fn connect(v: &mut Value, exe: &str) {
    remove_hooks(v);
    let Some(root) = v.as_object_mut() else { return };
    let hooks = root.entry("hooks").or_insert_with(|| Value::Object(Default::default()));
    if !hooks.is_object() {
        *hooks = Value::Object(Default::default());
    }
    let Some(hooks) = hooks.as_object_mut() else { return };
    for (event, matcher) in EVENTS {
        let list = hooks.entry(event).or_insert_with(|| Value::Array(Vec::new()));
        if !list.is_array() {
            *list = Value::Array(Vec::new());
        }
        let mut group = serde_json::Map::new();
        if let Some(m) = matcher {
            group.insert("matcher".into(), Value::from(m));
        }
        group.insert("hooks".into(), serde_json::json!([{ "type": "command", "command": exe, "args": HOOK_ARGS, "async": true, "timeout": 10 }]));
        if let Some(a) = list.as_array_mut() {
            a.push(Value::Object(group));
        }
    }
}

/// Remove our hooks and status lines (also the old built-in integration's) and anything left empty by that.
pub fn disconnect(v: &mut Value) {
    remove_hooks(v);
    let ours = v.get("statusLine").is_some_and(is_our_status);
    if let (true, Some(root)) = (ours, v.as_object_mut()) {
        root.shift_remove("statusLine");
    }
}

pub fn link(v: &Value, exe: &str) -> Link {
    let mut found = Link::Off;
    let lists = v.get("hooks").and_then(Value::as_object).into_iter().flat_map(|h| h.values());
    for h in lists.filter_map(Value::as_array).flatten().filter_map(|g| g.get("hooks").and_then(Value::as_array)).flatten() {
        match whose(h) {
            Some(Mine::Old) => return Link::Old,
            Some(Mine::New) if h.get("command").and_then(Value::as_str).is_some_and(|c| c.eq_ignore_ascii_case(exe)) => found = Link::On,
            Some(Mine::New) if found == Link::Off => found = Link::Moved,
            _ => {}
        }
    }
    found
}

enum Mine {
    New,
    /// The old built-in integration of a Hesewake Notch build.
    Old,
}

/// Is this hook entry one of ours: `hesewake-agents*.exe --hook claude`, or the old `hesewake-notch*.exe --claude-hook`?
fn whose(h: &Value) -> Option<Mine> {
    let name = h.get("command").and_then(Value::as_str).and_then(|c| c.rsplit(['\\', '/']).next())?.to_ascii_lowercase();
    if !name.ends_with(".exe") {
        return None;
    }
    let args: Vec<&str> = h.get("args").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
    if name.starts_with("hesewake-agents") && args == HOOK_ARGS {
        Some(Mine::New)
    } else if name.starts_with("hesewake-notch") && args.contains(&"--claude-hook") {
        Some(Mine::Old)
    } else {
        None
    }
}

/// A status line of ours, recognised by its arguments alone: the command may hold the short 8.3 path
/// (`.../HESEWA~1/HESEWA~1.EXE`), which no longer shows the exe name.
fn is_our_status(sl: &Value) -> bool {
    let Some(cmd) = sl.get("command").and_then(Value::as_str) else { return false };
    let words: Vec<&str> = cmd.split_whitespace().collect();
    words.contains(&"--claude-statusline") || words.windows(2).any(|w| w == ["--statusline", "claude"])
}

/// Remove our hook entries (new and old) and the groups, lists and `hooks` object left empty by that.
fn remove_hooks(v: &mut Value) {
    let Some(root) = v.as_object_mut() else { return };
    let Some(hooks) = root.get_mut("hooks").and_then(Value::as_object_mut) else { return };
    let mut changed = false;
    for list in hooks.values_mut() {
        let Some(groups) = list.as_array_mut() else { continue };
        let mut here = false;
        for g in groups.iter_mut() {
            if let Some(hs) = g.get_mut("hooks").and_then(Value::as_array_mut) {
                let before = hs.len();
                hs.retain(|h| whose(h).is_none());
                here |= hs.len() != before;
            }
        }
        changed |= here;
        if here {
            groups.retain(|g| g.get("hooks").and_then(Value::as_array).is_none_or(|hs| !hs.is_empty()));
        }
    }
    if changed {
        hooks.retain(|_, list| list.as_array().is_none_or(|a| !a.is_empty()));
        if hooks.is_empty() {
            root.shift_remove("hooks");
        }
    }
}

/// Claude Code's user settings file (`CLAUDE_CONFIG_DIR` wins over `~/.claude`, as in Claude Code).
pub fn settings_path() -> Option<PathBuf> {
    #[cfg(test)]
    if let Some(d) = TEST_DIR.with(|t| t.borrow().clone()) {
        return Some(d.join("settings.json"));
    }
    let dir = match std::env::var_os("CLAUDE_CONFIG_DIR").filter(|d| !d.is_empty()) {
        Some(d) => PathBuf::from(d),
        None => PathBuf::from(std::env::var_os("USERPROFILE")?).join(".claude"),
    };
    Some(dir.join("settings.json"))
}

pub fn read_link(exe: &str) -> Link {
    match load() {
        Ok(v) => link(&v, exe),
        Err(_) => Link::Invalid,
    }
}

/// Connect (`on`) or disconnect `exe` in the settings file. Nothing is written when nothing changes or the file
/// isn't a JSON object.
pub fn apply(on: bool, exe: &str) -> Result<Link, String> {
    let path = settings_path().ok_or("no settings folder")?;
    let mut v = load()?;
    let before = v.clone();
    if on {
        connect(&mut v, exe);
    } else {
        disconnect(&mut v);
    }
    if v != before {
        save(&path, &v)?;
    }
    Ok(link(&v, exe))
}

/// The settings as JSON: a missing or empty file is `{}`; a UTF-8 BOM is skipped.
fn load() -> Result<Value, String> {
    let path = settings_path().ok_or("no settings folder")?;
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.to_string()),
    };
    let text = text.trim_start_matches('\u{feff}');
    if text.trim().is_empty() {
        return Ok(Value::Object(Default::default()));
    }
    match serde_json::from_str::<Value>(text) {
        Ok(v) if v.is_object() => Ok(v),
        Ok(_) => Err("settings.json is not a JSON object".into()),
        Err(e) => Err(e.to_string()),
    }
}

/// Write the settings: a backup of the person's original file once, then a temporary file renamed over it.
fn save(path: &std::path::Path, v: &Value) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let backup = path.with_file_name("settings.json.hesewake-backup");
    if path.exists() && !backup.exists() {
        std::fs::copy(path, &backup).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(v).map_err(|e| e.to_string())?;
    let tmp = path.with_file_name("settings.json.hesewake-tmp");
    std::fs::write(&tmp, text + "\n").map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

#[cfg(test)]
thread_local! {
    /// Tests point the settings folder here instead of the real one.
    pub(crate) static TEST_DIR: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const EXE: &str = r"C:\Users\a\AppData\Local\Programs\Hesewake Agents\hesewake-agents.exe";
    const OLD_EXE: &str = r"C:\Users\a\AppData\Local\Programs\Hesewake Notch\hesewake-notch.exe";

    fn user_file() -> Value {
        json!({
            "permissions": { "allow": ["Bash(ls:*)"] },
            "model": "opus",
            "hooks": { "Stop": [ { "hooks": [ { "type": "command", "command": "say done" } ] } ] },
            "theme": "dark"
        })
    }

    fn ours(v: &Value, event: &str) -> Vec<Value> {
        v["hooks"][event].as_array().into_iter().flatten().filter(|g| g["hooks"][0]["args"] == json!(HOOK_ARGS)).cloned().collect()
    }

    /// The old built-in integration's entries, as the Dev build wrote them.
    fn with_old_hooks(mut v: Value) -> Value {
        let hooks = v["hooks"].as_object_mut().unwrap();
        for (event, _) in EVENTS {
            let list = hooks.entry(event).or_insert_with(|| json!([]));
            list.as_array_mut().unwrap().push(json!({ "hooks": [ { "type": "command", "command": OLD_EXE, "args": ["--claude-hook"], "async": true, "timeout": 10 } ] }));
        }
        v["statusLine"] = json!({ "type": "command", "command": "C:/Users/a/AppData/Local/Programs/HESEWA~1/HESEWA~1.EXE --claude-statusline", "padding": 0 });
        v
    }

    /// A temporary settings folder for this test; removed when dropped.
    struct TempDir(PathBuf);
    impl TempDir {
        fn new(name: &str) -> TempDir {
            let d = std::env::temp_dir().join(format!("hesewake-agents-setup-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&d);
            std::fs::create_dir_all(&d).unwrap();
            TEST_DIR.with(|t| *t.borrow_mut() = Some(d.clone()));
            TempDir(d)
        }
        fn file(&self) -> PathBuf {
            self.0.join("settings.json")
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            TEST_DIR.with(|t| *t.borrow_mut() = None);
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn connect_keeps_everything_else() {
        let mut v = user_file();
        connect(&mut v, EXE);
        let keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(keys, ["permissions", "model", "hooks", "theme"], "order kept");
        assert_eq!(v["hooks"]["Stop"][0]["hooks"][0]["command"], "say done", "the user's hook stays first");
        for (event, matcher) in EVENTS {
            let mine = ours(&v, event);
            assert_eq!(mine.len(), 1, "{event}");
            let h = &mine[0]["hooks"][0];
            assert_eq!((h["type"].as_str(), h["command"].as_str(), h["async"].as_bool(), h["timeout"].as_u64()), (Some("command"), Some(EXE), Some(true), Some(10)), "{event}");
            assert_eq!(mine[0].get("matcher").and_then(Value::as_str), matcher, "{event}");
        }
        assert_eq!(link(&v, EXE), Link::On);
        assert_eq!(link(&user_file(), EXE), Link::Off);
    }

    #[test]
    fn connect_twice_adds_once() {
        let mut v = user_file();
        connect(&mut v, EXE);
        let once = v.clone();
        connect(&mut v, EXE);
        assert_eq!(v.to_string(), once.to_string());
    }

    #[test]
    fn connect_with_new_path_replaces_old() {
        let mut v = user_file();
        connect(&mut v, r"D:\old\HESEWAKE-AGENTS.EXE");
        assert_eq!(link(&v, EXE), Link::Moved);
        connect(&mut v, EXE);
        assert_eq!(link(&v, EXE), Link::On);
        assert_eq!(ours(&v, "Stop").len(), 1);
    }

    #[test]
    fn old_hesewake_notch_hooks_are_seen_and_replaced() {
        let mut v = with_old_hooks(user_file());
        assert_eq!(link(&v, EXE), Link::Old);
        connect(&mut v, EXE);
        assert_eq!(link(&v, EXE), Link::On);
        let all = v["hooks"].to_string();
        assert!(!all.contains("--claude-hook"), "no old entry left: {all}");
        assert_eq!(v["hooks"]["Stop"][0]["hooks"][0]["command"], "say done", "the user's hook stays");
        assert_eq!(ours(&v, "Stop").len(), 1);
        // the old entries next to new ones still ask to be replaced
        let mut both = with_old_hooks(user_file());
        let hooks = both["hooks"].clone();
        connect(&mut both, EXE);
        both["hooks"] = hooks;
        connect(&mut both, EXE);
        let mut mixed = with_old_hooks(both.clone());
        mixed.as_object_mut().unwrap().shift_remove("statusLine");
        assert_eq!(link(&mixed, EXE), Link::Old);
    }

    #[test]
    fn our_old_status_line_is_removed_but_a_foreign_one_stays() {
        let mut v = with_old_hooks(user_file());
        disconnect(&mut v);
        assert_eq!(v.to_string(), user_file().to_string(), "old hooks and old status line gone, the rest as it was");
        let mut foreign = user_file();
        foreign["statusLine"] = json!({ "type": "command", "command": "npx ccusage statusline" });
        let before = foreign.clone();
        connect(&mut foreign, EXE);
        assert_eq!(foreign["statusLine"], before["statusLine"], "connect keeps someone else's status line");
        disconnect(&mut foreign);
        assert_eq!(foreign.to_string(), before.to_string());
        let mut new_ours = user_file();
        new_ours["statusLine"] = json!({ "type": "command", "command": "C:/PROGRA~1/HESEWA~2/HESEWA~1.EXE --statusline claude" });
        disconnect(&mut new_ours);
        assert!(new_ours.get("statusLine").is_none(), "our own status line goes too");
    }

    #[test]
    fn disconnect_removes_only_ours() {
        let original = user_file();
        let mut v = original.clone();
        connect(&mut v, EXE);
        disconnect(&mut v);
        assert_eq!(v.to_string(), original.to_string());
        let mut bare = json!({ "model": "opus" });
        connect(&mut bare, EXE);
        disconnect(&mut bare);
        assert_eq!(bare.to_string(), json!({ "model": "opus" }).to_string(), "an empty hooks object is removed");
        let mut look_alike = json!({ "hooks": { "Stop": [ { "hooks": [ { "type": "command", "command": r"C:\x\other.exe", "args": ["--hook", "claude"] } ] } ] } });
        let before = look_alike.clone();
        disconnect(&mut look_alike);
        assert_eq!(look_alike, before, "another program with the same arguments is not ours");
    }

    #[test]
    fn odd_files_are_left_alone() {
        let d = TempDir::new("odd");
        let cases: [(&str, &[u8]); 4] = [
            ("bom", b"\xEF\xBB\xBF{\"model\":\"opus\"}"),
            ("empty", b""),
            ("null hooks", b"{\"hooks\":null}"),
            ("array hooks", b"{\"hooks\":[]}"),
        ];
        for (name, content) in cases {
            std::fs::write(d.file(), content).unwrap();
            assert_eq!(apply(true, EXE), Ok(Link::On), "{name}");
            assert_eq!(read_link(EXE), Link::On, "{name}");
            let _ = std::fs::remove_file(d.0.join("settings.json.hesewake-backup"));
        }
        std::fs::remove_file(d.file()).unwrap();
        assert_eq!(apply(true, EXE), Ok(Link::On), "missing file");
        for bad in ["{ not json", "// comment\n{}", "[1, 2]"] {
            std::fs::write(d.file(), bad).unwrap();
            assert!(apply(true, EXE).is_err(), "{bad}");
            assert!(apply(false, EXE).is_err(), "{bad}");
            assert_eq!(std::fs::read_to_string(d.file()).unwrap(), bad, "untouched");
            assert_eq!(read_link(EXE), Link::Invalid, "{bad}");
        }
    }

    /// Remove (and the uninstaller) must not touch Claude Code's settings when none of our hooks are there.
    #[test]
    fn nothing_to_remove_writes_nothing() {
        let d = TempDir::new("noop");
        assert_eq!(apply(false, EXE), Ok(Link::Off));
        assert!(!d.file().exists(), "no settings.json created");
        let original: &[u8] = b"\xEF\xBB\xBF{\"hooks\":{\"Stop\":[{\"hooks\":[{\"type\":\"command\",\"command\":\"say done\"}]}]},\"x\":[]}";
        std::fs::write(d.file(), original).unwrap();
        assert_eq!(apply(false, EXE), Ok(Link::Off));
        assert_eq!(std::fs::read(d.file()).unwrap(), original, "bytes unchanged");
        assert!(!d.0.join("settings.json.hesewake-backup").exists(), "no backup either");
    }

    #[test]
    fn backup_made_once() {
        let d = TempDir::new("backup");
        std::fs::write(d.file(), "{\"model\":\"opus\"}").unwrap();
        apply(true, EXE).unwrap();
        apply(false, EXE).unwrap();
        apply(true, EXE).unwrap();
        let backup = std::fs::read_to_string(d.0.join("settings.json.hesewake-backup")).unwrap();
        assert_eq!(backup, "{\"model\":\"opus\"}");
        assert_eq!(apply(false, EXE), Ok(Link::Off));
        let now: Value = serde_json::from_str(&std::fs::read_to_string(d.file()).unwrap()).unwrap();
        assert_eq!(now, json!({ "model": "opus" }));
    }

    #[test]
    fn the_settings_file_follows_claude_config_dir() {
        // without a test folder the path ends in settings.json under some folder
        assert!(settings_path().is_some_and(|p| p.ends_with("settings.json")));
    }
}
