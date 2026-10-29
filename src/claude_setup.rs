//! Connects the notch to Claude Code: adds (or removes) our hooks in Claude Code's user settings
//! (`~/.claude/settings.json`), keeping everything else in the file as it was.

use serde_json::Value;
use std::path::PathBuf;

/// Whether our hooks are in Claude Code's settings.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Link {
    Off,
    On,
    /// Our hooks point at another copy of the exe (moved or replaced).
    Moved,
    /// settings.json can't be read as JSON; nothing is changed.
    Invalid,
}

/// The hook events we listen to, with their matcher.
pub(crate) const EVENTS: [(&str, Option<&str>); 8] = [
    ("UserPromptSubmit", None),
    ("PreToolUse", Some("*")),
    ("PostToolUse", Some("*")),
    ("Notification", Some("permission_prompt|elicitation_dialog|elicitation_url_dialog|agent_needs_input|idle_prompt")),
    ("Stop", None),
    ("StopFailure", None),
    ("SessionStart", None),
    ("SessionEnd", None),
];

pub(crate) const HOOK_ARG: &str = "--claude-hook";
/// Our hook commands run an exe whose name starts with this (also renamed copies and other editions).
const EXE_PREFIX: &str = "hesewake-notch";

/// Add our hooks for `exe` (any older ones of ours are replaced).
pub(crate) fn connect(v: &mut Value, exe: &str) {
    disconnect(v);
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
        group.insert(
            "hooks".into(),
            serde_json::json!([{ "type": "command", "command": exe, "args": [HOOK_ARG], "async": true, "timeout": 10 }]),
        );
        if let Some(a) = list.as_array_mut() {
            a.push(Value::Object(group));
        }
    }
}

/// Is this hook entry one of ours (any path to our exe, with the hook argument)?
fn is_ours(h: &Value) -> bool {
    let named = h.get("command").and_then(Value::as_str).and_then(|c| c.rsplit(['\\', '/']).next()).is_some_and(|n| {
        let n = n.to_ascii_lowercase();
        n.starts_with(EXE_PREFIX) && n.ends_with(".exe")
    });
    let arg = h.get("args").and_then(Value::as_array).is_some_and(|a| a.iter().any(|x| x == HOOK_ARG));
    named && arg
}

/// Remove our hooks and anything left empty by that; everything else stays.
pub(crate) fn disconnect(v: &mut Value) {
    let Some(root) = v.as_object_mut() else { return };
    let Some(hooks) = root.get_mut("hooks").and_then(Value::as_object_mut) else { return };
    for list in hooks.values_mut() {
        let Some(groups) = list.as_array_mut() else { continue };
        for g in groups.iter_mut() {
            if let Some(hs) = g.get_mut("hooks").and_then(Value::as_array_mut) {
                hs.retain(|h| !is_ours(h));
            }
        }
        groups.retain(|g| g.get("hooks").and_then(Value::as_array).is_none_or(|hs| !hs.is_empty()));
    }
    hooks.retain(|_, list| list.as_array().is_none_or(|a| !a.is_empty()));
    if hooks.is_empty() {
        root.shift_remove("hooks");
    }
}

pub(crate) fn link(v: &Value, exe: &str) -> Link {
    let mut found = Link::Off;
    let lists = v.get("hooks").and_then(Value::as_object).into_iter().flat_map(|h| h.values());
    for h in lists.filter_map(Value::as_array).flatten().filter_map(|g| g.get("hooks").and_then(Value::as_array)).flatten() {
        if is_ours(h) {
            if h.get("command").and_then(Value::as_str).is_some_and(|c| c.eq_ignore_ascii_case(exe)) {
                return Link::On;
            }
            found = Link::Moved;
        }
    }
    found
}

/// Claude Code's user settings file (`CLAUDE_CONFIG_DIR` wins over `~/.claude`, as in Claude Code).
pub(crate) fn settings_path() -> Option<PathBuf> {
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

fn current_exe() -> String {
    // The test binary has another name; tests act as the installed app.
    if cfg!(test) {
        return r"C:\Program Files\Hesewake Notch\hesewake-notch.exe".to_string();
    }
    std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default()
}

pub(crate) fn read_link() -> Link {
    read_link_with(&current_exe())
}

pub(crate) fn read_link_with(exe: &str) -> Link {
    match load() {
        Ok(v) => link(&v, exe),
        Err(_) => Link::Invalid,
    }
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

/// Connect (`on`) or disconnect the running exe.
pub(crate) fn apply(on: bool) -> Result<Link, String> {
    apply_with(on, &current_exe())
}

/// Connect or disconnect `exe` in the settings file.
pub(crate) fn apply_with(on: bool, exe: &str) -> Result<Link, String> {
    let path = settings_path().ok_or("no settings folder")?;
    let mut v = load()?;
    if !on && link(&v, exe) == Link::Off {
        // None of ours: leave Claude Code's file (and folder) exactly as they are.
        return Ok(Link::Off);
    }
    let before = v.clone();
    if on {
        connect(&mut v, exe);
    } else {
        disconnect(&mut v);
    }
    if v == before {
        return Ok(link(&v, exe));
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    // Keep the person's original file once, before our first change.
    let backup = path.with_file_name("settings.json.hesewake-backup");
    if path.exists() && !backup.exists() {
        std::fs::copy(&path, &backup).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?;
    let tmp = path.with_file_name("settings.json.hesewake-tmp");
    std::fs::write(&tmp, text + "\n").map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    Ok(link(&v, exe))
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

    const EXE: &str = r"C:\Users\a\AppData\Local\Programs\Hesewake Notch\hesewake-notch.exe";

    fn user_file() -> Value {
        json!({
            "permissions": { "allow": ["Bash(ls:*)"] },
            "model": "opus",
            "hooks": { "Stop": [ { "hooks": [ { "type": "command", "command": "say done" } ] } ] },
            "theme": "dark"
        })
    }

    fn ours(v: &Value, event: &str) -> Vec<Value> {
        v["hooks"][event].as_array().into_iter().flatten().filter(|g| g["hooks"][0]["args"][0] == HOOK_ARG).cloned().collect()
    }

    /// A temporary settings folder for this test; removed when dropped.
    struct TempDir(PathBuf);
    impl TempDir {
        fn new(name: &str) -> TempDir {
            let d = std::env::temp_dir().join(format!("hesewake-claude-{name}-{}", std::process::id()));
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
            assert_eq!((h["type"].as_str(), h["command"].as_str(), h["async"].as_bool()), (Some("command"), Some(EXE), Some(true)), "{event}");
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
        connect(&mut v, r"D:\old\HESEWAKE-NOTCH.EXE");
        assert_eq!(link(&v, EXE), Link::Moved);
        connect(&mut v, EXE);
        assert_eq!(link(&v, EXE), Link::On);
        assert_eq!(ours(&v, "Stop").len(), 1);
    }

    /// A copy of the exe under another name (a test build, the Office edition) is still recognised: connecting again
    /// replaces its hooks instead of adding more, and Remove takes them out.
    #[test]
    fn renamed_exe_is_still_ours() {
        let renamed = r"C:\Users\a\Downloads\hesewake-notch-claude.exe";
        let original = user_file();
        let mut v = original.clone();
        connect(&mut v, renamed);
        assert_eq!(link(&v, renamed), Link::On);
        connect(&mut v, renamed);
        assert_eq!(ours(&v, "Stop").len(), 1, "connecting twice adds once");
        connect(&mut v, EXE);
        assert_eq!(ours(&v, "Stop").len(), 1, "another copy replaces it");
        disconnect(&mut v);
        assert_eq!(v.to_string(), original.to_string());
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
    }

    #[test]
    fn odd_files() {
        let d = TempDir::new("odd");
        let cases: [(&str, &[u8]); 3] = [("bom", b"\xEF\xBB\xBF{\"model\":\"opus\"}"), ("empty", b""), ("null hooks", b"{\"hooks\":null}")];
        for (name, content) in cases {
            std::fs::write(d.file(), content).unwrap();
            assert_eq!(apply_with(true, EXE), Ok(Link::On), "{name}");
            assert_eq!(read_link_with(EXE), Link::On, "{name}");
            let _ = std::fs::remove_file(d.0.join("settings.json.hesewake-backup"));
        }
        std::fs::remove_file(d.file()).unwrap();
        assert_eq!(apply_with(true, EXE), Ok(Link::On), "missing file");
        std::fs::write(d.file(), "{ not json").unwrap();
        assert!(apply_with(true, EXE).is_err());
        assert_eq!(std::fs::read_to_string(d.file()).unwrap(), "{ not json", "untouched");
        assert_eq!(read_link_with(EXE), Link::Invalid);
    }

    /// Remove (and the uninstaller) must not touch Claude Code's settings when none of our hooks are there.
    #[test]
    fn nothing_to_remove_writes_nothing() {
        let d = TempDir::new("noop");
        assert_eq!(apply_with(false, EXE), Ok(Link::Off));
        assert!(!d.file().exists(), "no settings.json created");
        let original: &[u8] = b"\xEF\xBB\xBF{\"hooks\":{\"Stop\":[{\"hooks\":[{\"type\":\"command\",\"command\":\"say done\"}]}]},\"x\":[]}";
        std::fs::write(d.file(), original).unwrap();
        assert_eq!(apply_with(false, EXE), Ok(Link::Off));
        assert_eq!(std::fs::read(d.file()).unwrap(), original, "bytes unchanged");
        assert!(!d.0.join("settings.json.hesewake-backup").exists(), "no backup either");
    }

    #[test]
    fn backup_made_once() {
        let d = TempDir::new("backup");
        std::fs::write(d.file(), "{\"model\":\"opus\"}").unwrap();
        apply_with(true, EXE).unwrap();
        apply_with(false, EXE).unwrap();
        apply_with(true, EXE).unwrap();
        let backup = std::fs::read_to_string(d.0.join("settings.json.hesewake-backup")).unwrap();
        assert_eq!(backup, "{\"model\":\"opus\"}");
        assert_eq!(apply_with(false, EXE), Ok(Link::Off));
        let now: Value = serde_json::from_str(&std::fs::read_to_string(d.file()).unwrap()).unwrap();
        assert_eq!(now, json!({ "model": "opus" }));
    }
}
