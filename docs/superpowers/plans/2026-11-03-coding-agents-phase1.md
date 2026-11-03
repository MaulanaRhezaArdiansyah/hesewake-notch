# Coding Agents extension, phase 1: implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A `hesewake-agents.exe` extension that shows Claude Code sessions on a notch slide (needs you first), with
toasts, a badge, a status line in Settings > Extensions, a settings page that connects Claude Code's hooks safely, and an
installer whose uninstaller removes those hooks.

**Architecture:** One Rust crate in `extensions/agents`, outside the notch's workspace, like `extensions/odoo`. As an
extension it owns the notch pipe and a message-only window. Claude Code runs the same exe with `--hook claude`, which
sends the event to that window with `WM_COPYDATA`. Pure modules (events, sessions, setup, UI, toasts) are unit-tested.
The Win32 parts (pipe, window, process tree) are ported from code that already works.

**Tech Stack:** Rust 2021, `serde` / `serde_json` (`preserve_order`), `windows` 0.58, `winresource`, Inno Setup 6,
GitHub Actions on `windows-latest`.

**Spec:** `docs/superpowers/specs/2026-11-03-coding-agents-extension-design.md` (sections 3, 4, 5.1, 5.3, 6.1, 6.2,
6.4, 6.5, 6.6, 6.7, 7, 10, 12). Phase 1 is spec section 11, item 1.

**Code to port (read, don't guess):**
- `git show feat/claude-code:src/<file>` for `claude.rs`, `claude_hook.rs` and `claude_setup.rs`.
- `git show ext/odoo:extensions/odoo/<file>` for `src/main.rs` (pipe client and loop), `src/ui.rs` (the `Out` / `t()`
  / page patterns and tests), `build.rs`, `installer.iss`, and `.github/workflows/odoo-extension.yml`.
- In Git Bash, prefix these commands with `MSYS_NO_PATHCONV=1`.

## Global Constraints

- **Building and testing happen only in CI.** Smart App Control blocks local `cargo`, and the Windows Sandbox too.
  "Run the tests" means:
  1. commit;
  2. `git push fork ext/agents`;
  3. wait for the **Agents extension** workflow with Monitor (`gh run list -R MaulanaRhezaArdiansyah/hesewake-notch -w "Agents extension" -b ext/agents -L 1 --json status,conclusion`);
  4. read the failing log with `gh run view <id> --log-failed`.

  Batch work, because a cycle takes about 5 to 10 minutes. Never work around SAC.
- **CI flags**: `RUSTFLAGS=-C target-feature=+crt-static -D warnings`. No warnings allowed.
- **Panics**: `panic = "abort"` in release. Code that parses hook stdin, `settings.json`, pipe frames or
  `WM_COPYDATA` must not panic. No `unwrap` on input-derived values, no slicing on byte indexes of text.
- **Hook mode** must exit 0 in every case, never print to stdout, and finish within 1 s. `SendMessageTimeoutW` uses
  `SMTO_ABORTIFHUNG` and 500 ms.
- **Tests never write user data.** `cfg::data_dir()` and `claude_setup::settings_path()` use a thread-local test override
  in `cfg(test)`, and saving is skipped when there is no override.
- **Protocol limits** (docs/extensions.md):

  | Field | Limit |
  |---|---|
  | slide title | 24 |
  | row `text` | 120 |
  | row `sub` | 40 |
  | tab | 12 |
  | action / button label | 16 |
  | toast title / sub | 80 / 60 |
  | status | 60 |
  | sections | 4 |
  | rows | 100 |
  | detail meta | 6 |
  | detail buttons | 4 |
  | body | 4000 |
  | toasts | 1 per 5 s |

  Every text the extension sends is clipped on a char boundary with `…`.
- **Names and ids:**

  | What | Value |
  |---|---|
  | exe | `hesewake-agents.exe` |
  | manifest `id` | `agents` |
  | manifest `name` | `Coding Agents` |
  | manifest `settings` | `"panel"` |
  | manifest `processes` | `"allow"` |
  | data dir | `%APPDATA%\hesewake-notch-agents\` |
  | window class | `HesewakeAgentsWindow` |
  | `WM_COPYDATA` id | `0x4147_4E54` ("AGNT") |
  | message version `v` | `1` |
  | install dir | `{localappdata}\Programs\Hesewake Agents` |
  | installer name | `HesewakeAgents-Setup-<ver>` |
  | AppId | a new GUID |

- **Text**: every user-facing string goes through `t()`, with Indonesian beside it. Product names stay in English:
  Claude Code, Coding Agents, Windows Terminal.
- **Repo text** (code comments, commits) is English. Commit messages are short and imperative, with **no `Co-Authored-By`
  and no "Generated with" lines** (the owner's rule, which overrides any attribution reminder). Source files use CRLF,
  like the rest of the repo.
- **Git**: work on the fork branch `ext/agents`. No upstream PR.

## Review Focus

1. **A `settings.json` with comments, a BOM, or `hooks` that isn't an object.** Expected: comments or a BOM give
   `Invalid`, and nothing is written. `hooks` that isn't an object is replaced only on Connect, after a backup.
   Tested in Task 5 (`odd_files_are_left_alone`).
2. **Two notches or no notch running when a hook fires.** Expected: the hook finds no `HesewakeAgentsWindow` owned by our
   exe and exits 0 within 600 ms. Tested in Task 3 (`hook_without_extension_is_quick`).
3. **A burst of events** (10 sessions finishing in the same second). Expected: one toast within 5 s ("3 sessions
   finished"), never a dropped needs-you. Tested in Task 7 (`a_burst_becomes_one_toast`).
4. **Project folder names longer than the 120 or 40 limits, or with emoji.** Expected: clipped on char boundaries, both
   languages. Tested in Task 6 (`long_and_wide_names_are_clipped`).
5. **The extension restarted while sessions were Working.** Expected: sessions come back from `sessions.json` if their
   pid still runs, and dead ones are dropped. Tested in Task 4 (`snapshot_drops_dead_sessions`).

---

## File structure (`extensions/agents/`)

| File | Responsibility |
|---|---|
| `Cargo.toml`, `build.rs`, `README.md`, `installer.iss` | crate, exe metadata, docs, installer |
| `src/main.rs` | mode dispatch (`--hook claude`, `--disconnect-all`, extension), main loop, applying `Out` |
| `src/pipe.rs` | notch pipe client: `open`, `server_is_notch`, `send`, `recv`, `available` |
| `src/text.rs` | `t()` table EN/ID, `clip`, `fill`, `duration_text`, `ago_text`, `folder_name` |
| `src/event.rs` | agent-neutral `Agent`, `EventKind`, `Msg`, `encode` / `decode`, `preview`, `project_name` |
| `src/claude_hook.rs` | Claude hook stdin → `Msg` (`parse_hook`, `read_hook`) |
| `src/procs.rs` | process tree: `parent_map`, `ancestors`, `agent_pid`, `terminal_window`, `process_running`, `image_path` |
| `src/relay.rs` | `WM_COPYDATA`: extension-side message-only window, hook-side `find_ext` / `is_ours` / `send` |
| `src/sessions.rs` | `Session`, `State`, `Show`, the state machine, `branch_of`, snapshot load/save |
| `src/claude_setup.rs` | `settings.json`: `link`, `connect`, `disconnect`, backup, atomic save |
| `src/cfg.rs` | `Config` (privacy, toast toggles), `data_dir`, load/save |
| `src/toasts.rs` | toast queue, priority, merging, the 5 s gap |
| `src/ui.rs` | `Ui`: pages (home, detail, settings), messages in, `Out` out, badge, status |
| `.github/workflows/agents-extension.yml` (repo root) | test, build, installer, artifact |

---

### Task 1: Crate, text helpers and CI

**Files:**
- Create: `extensions/agents/Cargo.toml`, `build.rs`, `src/main.rs` (stub), `src/text.rs`
- Create: `.github/workflows/agents-extension.yml`

**Interfaces:**
- Produces:
  - `text::t(id_lang: bool, s: &'static str) -> &'static str`
  - `text::clip(s: &str, max: usize) -> String` (chars, ends with `…` when cut)
  - `text::fill(s: &str, args: &[&dyn Display]) -> String` (`{}` placeholders)
  - `text::duration_text(secs: u64) -> String` (`"45s"`, `"4m 12s"`, `"1h 5m"`)
  - `text::ago_text(secs: u64) -> String` (`"just now"` under 60 s, then `"4m"`, `"2h"`, `"3d"`)
  - `text::folder_name(path: &str) -> &str` (last component; trailing slashes ignored)

- [ ] **Step 1: Write `Cargo.toml` and `build.rs`.** Copy them from the Odoo crate:
  - package `hesewake-agents`, version `0.1.0`, description "Coding agent sessions for Hesewake Notch, as an extension
    (docs/extensions.md)";
  - `serde_json = { version = "1", features = ["preserve_order"] }`;
  - `windows` features: Foundation, Pipes, Threading, `UI_WindowsAndMessaging`, `System_Diagnostics_ToolHelp`,
    `System_ProcessStatus`, `System_LibraryLoader`, `Storage_FileSystem`;
  - same release profile (`panic = "abort"`) and an empty `[workspace]`.

  `build.rs` sets the icon `../../icon.ico` and the file description "Coding Agents for Hesewake Notch".
- [ ] **Step 2: Write the failing tests in `src/text.rs`.**

```rust
#[test] fn clip_counts_chars_and_marks_the_cut() {
    assert_eq!(clip("hesewake", 20), "hesewake");
    assert_eq!(clip("hesewake-notch", 8), "hesewak…");
    assert_eq!(clip("😀😀😀😀", 3).chars().count(), 3);
}
#[test] fn durations_read_naturally() {
    assert_eq!(duration_text(45), "45s"); assert_eq!(duration_text(252), "4m 12s"); assert_eq!(duration_text(3900), "1h 5m");
    assert_eq!(ago_text(30), "just now"); assert_eq!(ago_text(240), "4m"); assert_eq!(ago_text(7200), "2h");
}
#[test] fn folder_names() {
    assert_eq!(folder_name(r"C:\Users\a\Projects\hesewake-notch\"), "hesewake-notch");
    assert_eq!(folder_name("C:/x/y"), "y"); assert_eq!(folder_name("C:\\"), "C:");
}
#[test] fn every_text_has_indonesian() { /* every key in TEXTS has a non-empty, different-or-intentional ID entry, and `{}` counts match */ }
```

- [ ] **Step 3: Implement `text.rs`.** `TEXTS: &[(&str, &str)]` is the EN→ID table, filled in later tasks. `t` returns
  EN when `id_lang` is false, and also when the key is missing. In tests, `t` panics on a missing key.
- [ ] **Step 4: Write `src/main.rs` as a stub** with `mod text;` and an empty `fn main()`. Add `#![cfg_attr(not(test),
  windows_subsystem = "windows")]`.
- [ ] **Step 5: Write `agents-extension.yml`.** Copy `odoo-extension.yml` with these changes:
  - paths `extensions/agents/**` and the workflow file;
  - `working-directory: extensions/agents`;
  - concurrency group `agents-ext-${{ github.ref }}`;
  - artifact `hesewake-agents` (exe, `installer_output/*.exe`, `Cargo.lock`);
  - add `cargo generate-lockfile` before Test when `Cargo.lock` is missing;
  - skip the installer step until `installer.iss` exists (`if: hashFiles('extensions/agents/installer.iss') != ''`).
- [ ] **Step 6: Run the tests through CI** (see Global Constraints). Expected: Test passes with 4 tests, and Build
  passes.
- [ ] **Step 7: Commit the lockfile.** Download the `Cargo.lock` artifact into `extensions/agents/` and commit it: "Agents
  extension: lockfile". Later CI runs use `--locked`; update the workflow to do so in this commit.

### Task 2: Events and Claude hook parsing

**Files:**
- Create: `src/event.rs`, `src/claude_hook.rs`

**Interfaces:**
- Consumes: `text::folder_name`.
- Produces:
  - `event::Agent` (`Claude`; serde-renamed lowercase) with `fn label(self) -> &'static str` (`"Claude"`);
  - `event::EventKind` (`SessionStart`, `SessionEnd`, `Prompt`, `Tool`, `NeedsYou`, `Stop`, `StopFailure`, `Idle`);
  - `event::Msg { v: u8, agent: Agent, kind: EventKind, session: String, project: String, cwd: String, text: String, tool: String, hwnd: isize, subagent: bool, pid: u32 }`
    (serde; `#[serde(default)]` on `tool` and `pid`);
  - `event::VERSION: u8 = 1`;
  - `event::encode(&Msg) -> Vec<u8>`;
  - `event::decode(&[u8]) -> Option<Msg>`, which rejects input over 16 KiB, `v != 1` and bad JSON;
  - `event::preview(&str) -> String` (first meaningful line, 200 chars);
  - `event::project_name(cwd: &str) -> String`;
  - `claude_hook::parse_hook(json: &str) -> Option<Msg>`;
  - `claude_hook::read_hook(r: impl Read) -> Option<Msg>` (cap 16 MB).

- [ ] **Step 1: Port the tests from `feat/claude-code:src/claude_hook.rs`.** Port `maps_each_event`,
  `stop_carries_a_preview`, `preview_cuts_on_char_boundaries`, `long_final_message_still_reaches_the_notch`,
  `project_from_cwd`, `bad_input_is_ignored` and `round_trip`, then adapt them to `Msg.agent` and `Msg.tool`. Add:

```rust
#[test] fn the_tool_name_is_kept() {
    let m = parse_hook(&hook("PreToolUse", r#""tool_name":"Bash""#)).unwrap();
    assert_eq!((m.kind, m.tool.as_str()), (EventKind::Tool, "Bash"));
}
#[test] fn huge_and_broken_stdin_never_panics() {
    assert!(read_hook(std::io::repeat(b'{').take(20 * 1024 * 1024)).is_none());
    assert!(read_hook(&b"\xff\xfe{\"hook_event_name\":"[..]).is_none());
}
#[test] fn decode_rejects_other_versions_and_big_frames() { /* v=2 → None; 17 KiB → None */ }
```

- [ ] **Step 2: Run them through CI.** Expected: FAIL to compile, because the modules are missing.
- [ ] **Step 3: Port the code.** `parse_hook` and `preview` / `clean_line` come over as they are, plus:
  - `tool`, read from `tool_name` and clipped to 40;
  - `agent: Agent::Claude`.

  `hwnd` / `pid` stay 0 here; Task 3 fills them in.
- [ ] **Step 4: Run them through CI.** Expected: all pass.
- [ ] **Step 5: Commit**: "Agents extension: parse Claude Code hook events".

### Task 3: Process tree, relay window and `--hook claude`

**Files:**
- Create: `src/procs.rs`, `src/relay.rs`
- Modify: `src/main.rs` (mode dispatch)

**Interfaces:**
- Consumes: `event::{Msg, encode, decode}`, `claude_hook::read_hook`.
- Produces:
  - `procs::parent_map() -> Vec<(u32, u32, String)>`
  - `procs::ancestors(map, start: u32) -> Vec<(u32, String)>` (depth 12)
  - `procs::agent_pid(chain: &[(u32, String)]) -> u32` (the old `claude_pid`: the first ancestor that isn't a shell)
  - `procs::terminal_window(chain, project: &str) -> isize`
  - `procs::process_running(pid: u32) -> bool`
  - `procs::window_alive(hwnd: isize) -> bool`
  - `relay::COPY_ID: usize = 0x4147_4E54`
  - `relay::find_ext() -> isize` (`FindWindowExW(HWND_MESSAGE, …, "HesewakeAgentsWindow")`, then `is_ours`, at most 16 tries)
  - `relay::is_ours(hwnd) -> bool` (window's process image equals `current_exe`, case-insensitive)
  - `relay::send(hwnd, &[u8]) -> bool`
  - `relay::Receiver::start(tx: Sender<Msg>) -> io::Result<Receiver>`: a thread that owns the message-only window and
    passes every `WM_COPYDATA` with `COPY_ID` that `decode`s to `tx`; dropping it posts `WM_CLOSE`
  - `relay::run_hook()`: reads stdin, fills in the process data, sends, and always returns

- [ ] **Step 1: Port the tests.** From `claude_hook.rs`, port `the_walk_skips_shells_and_stops_at_explorer`,
  `own_ancestors_are_walked`, `deliver_reaches_a_window` (now against `Receiver::start`, with a message-only window) and
  `only_our_own_exe_is_the_notch`. Add:

```rust
#[test] fn hook_without_extension_is_quick() {
    let t = Instant::now(); assert!(!send(find_ext(), &encode(&sample())));
    assert!(t.elapsed() < Duration::from_millis(600));
}
#[test] fn other_ids_and_garbage_are_dropped() { /* WM_COPYDATA with id 0x434C_4155 or bytes "xx" → nothing on the channel */ }
```

- [ ] **Step 2: Run them through CI.** Expected: FAIL (missing).
- [ ] **Step 3: Port the code.** It comes from `claude_hook.rs` (`parent_map`, `ancestors`, `claude_pid`,
  `terminal_window`, `windows_of`, `is_ours`, `send`) and `claude.rs` (`process_running`). Then write `Receiver`:
  - `RegisterClassW` for `HesewakeAgentsWindow`, then `CreateWindowExW(…, HWND_MESSAGE, …)`;
  - the window procedure handles `WM_COPYDATA` and checks `dwData == COPY_ID` and `cbData <= 16 KiB`;
  - it then copies the bytes, decodes them, sends them, and returns 1.

  `run_hook` reads stdin, sets `pid = agent_pid(chain)` and `hwnd = terminal_window(chain, &m.project)`, then sends.
- [ ] **Step 4: Dispatch in `main.rs`.** `--hook claude` → `relay::run_hook()` then `exit(0)`. `--hook <other>` → `exit(0)`.
  Any other argument list falls through to extension mode (Task 8).
- [ ] **Step 5: Run them through CI.** Expected: all pass.
- [ ] **Step 6: Commit**: "Agents extension: hand hook events to the running extension".

### Task 4: Sessions

**Files:**
- Create: `src/sessions.rs`

**Interfaces:**
- Consumes: `event::{Msg, EventKind, Agent}`.
- Produces:
  - `sessions::State` (`Idle`, `Working`, `NeedsYou`, `Answered`, `Done`, `Error`; serde)
  - `sessions::Session { agent, id, project, cwd, branch: Option<String>, state, since: u64, turn_started: Option<u64>, tool: String, text: String, hwnd: isize, pid: u32 }` (serde)
  - `sessions::Show` (`NeedsYou { session, project, agent, tool, text }`, `Done { session, project, agent, secs: Option<u64> }`, `Error { session, project, agent }`)
  - `Sessions::apply(&mut self, m: &Msg, now: u64, fg: isize) -> Option<Show>`
  - `Sessions::tick(&mut self, now, fg, alive: impl Fn(isize) -> bool, running: impl Fn(u32) -> bool) -> bool` (true when something changed)
  - `Sessions::clear_needs(&mut self, id: &str)`
  - `Sessions::dismiss(&mut self, id: &str)`
  - `Sessions::get(&self, id) -> Option<&Session>`
  - `Sessions::list(&self) -> &[Session]`
  - `Sessions::needing_count(&self) -> usize`
  - `sessions::branch_of(cwd: &str) -> Option<String>`: reads `<cwd>/.git/HEAD` (≤ 1 KiB) and walks up to 6 parents.
    It returns the branch name for `ref: refs/heads/x`, the first 7 chars for a detached hash, and `None` otherwise.
    It also follows a `.git` *file* (`gitdir: …`, for worktrees).
  - `Sessions::load(path) -> Sessions` and `Sessions::save(&self, path)` (JSON; save skipped without a test dir in `cfg(test)`)
  - constants `STALE_SECS = 900`, `MAX_SESSIONS = 16`, `RECENT_KEEP_SECS = 7200`

- [ ] **Step 1: Port the tests from `feat/claude-code:src/claude.rs`.** All of them, with times as `u64` seconds instead
  of `Instant`. Add:

```rust
#[test] fn the_current_tool_is_remembered() { /* Tool "Edit" → session.tool == "Edit"; Stop keeps it for the summary */ }
#[test] fn done_sessions_leave_recent_after_two_hours() { /* Done at t, tick at t+7201 → removed */ }
#[test] fn snapshot_drops_dead_sessions() { /* save two, load, then tick with running = |pid| pid == 7 → only pid 7 stays */ }
#[test] fn branch_from_head_files() { /* temp dir: "ref: refs/heads/feat/x\n" → "feat/x"; 40-hex → 7 chars; gitdir file; garbage → None */ }
```

- [ ] **Step 2: Run them through CI.** Expected: FAIL (missing).
- [ ] **Step 3: Port the state machine.** The rules are in spec section 4. `apply` sets `branch = branch_of(cwd)` on the
  first message of a session and on `Prompt`. `Done` / `Error` / `Idle` older than `RECENT_KEEP_SECS` are removed in
  `tick`.
- [ ] **Step 4: Run them through CI.** Expected: all pass.
- [ ] **Step 5: Commit**: "Agents extension: session states".

### Task 5: Claude Code setup

**Files:**
- Create: `src/claude_setup.rs`

**Interfaces:**
- Produces:
  - `claude_setup::Link` (`Off`, `On`, `Moved`, `Old`, `Invalid`); `Old` means only old `hesewake-notch*.exe --claude-hook` entries
  - `claude_setup::EVENTS` (as in the old code)
  - `HOOK_ARGS = ["--hook", "claude"]`
  - `link(v: &Value, exe: &str) -> Link`
  - `connect(v: &mut Value, exe: &str)`
  - `disconnect(v: &mut Value)`
  - `settings_path() -> Option<PathBuf>` (`CLAUDE_CONFIG_DIR`, else `%USERPROFILE%\.claude\settings.json`)
  - `read_link(exe) -> Link`
  - `apply(on: bool, exe) -> Result<Link, String>`

- [ ] **Step 1: Port the tests from `feat/claude-code:src/claude_setup.rs`** (all of them, the new exe and args). Add:

```rust
#[test] fn old_hesewake_notch_hooks_are_seen_and_replaced() {
    // file with hesewake-notch.exe --claude-hook entries + a user hook → link == Old; connect → only new entries + the user hook
}
#[test] fn our_old_status_line_is_removed_but_a_foreign_one_stays() {
    // statusLine "…/hesewake-notch.exe --claude-statusline" → removed by disconnect; "ccusage statusline" → kept by connect and disconnect
}
#[test] fn odd_files_are_left_alone() {
    // "// comment\n{}" and "\u{feff}{}" → Invalid, file bytes unchanged after apply(true); "hooks": [] → replaced by an object on connect
}
```

- [ ] **Step 2: Run them through CI.** Expected: FAIL (missing).
- [ ] **Step 3: Port the code.** It comes over as it is, with these changes:
  - `is_ours` accepts `hesewake-agents.exe` + `HOOK_ARGS`, and `hesewake-notch*.exe` + `--claude-hook`;
  - `link` returns `Old` when only old entries exist;
  - `disconnect` also removes a `statusLine` whose command contains `hesewake-notch` and `--claude-statusline`, or
    `hesewake-agents` and `--statusline`.

  Phase 1 never writes a status line. Keep the backup (`settings.json.hesewake-backup`, made once) and the
  temp-file-then-rename save. Never write when `link` is `Invalid`.
- [ ] **Step 4: Run them through CI.** Expected: all pass.
- [ ] **Step 5: Commit**: "Agents extension: connect Claude Code without touching anything else".

### Task 6: Config and pages

**Files:**
- Create: `src/cfg.rs`, `src/ui.rs`
- Modify: `src/text.rs` (texts)

**Interfaces:**
- Consumes: `Sessions`, `Session`, `State`, `claude_setup::Link`, `text::*`.
- Produces:
  - `cfg::Config { privacy: bool, toast_needs: bool, toast_done: bool }` (default `false`, `true`, `true`)
  - `Config::load() -> Config` and `Config::save(&self)` (`config.json`)
  - `cfg::data_dir() -> Option<PathBuf>`
  - `ui::Out` (`Send(Value)`, `Save(Config)`, `SaveSessions`, `Connect(bool)`)
  - `ui::Ui::new(cfg: Config, link: Link, id_lang: bool) -> Ui`
  - fields `pub sessions: Sessions`, `pub now: u64`, `pub fg: isize`
  - `Ui::start(&mut self) -> Vec<Out>`
  - `Ui::on_msg(&mut self, m: &Value) -> Vec<Out>`
  - `Ui::on_event(&mut self, m: &Msg) -> Vec<Out>`
  - `Ui::linked(&mut self, link: Link) -> Vec<Out>` (after `Connect` ran)
  - `Ui::tick(&mut self, now_ms: u64, alive: impl Fn(isize) -> bool, running: impl Fn(u32) -> bool) -> Vec<Out>` (`now` = `now_ms / 1000`)
  - `Ui::page(&mut self) -> Value` (the current `slide` or `detail`, with `rev` growing)

**Pages** (slide title `Coding |Agents`; every text through `t()`):
- **Home.**
  - `actions: [{id:"settings", label:"Settings"}]`.
  - Sections in this order, empty ones left out: `Needs you`, `Working`, `Recent`.
  - Rows use spec 6.1's table. Row `id` = session id, `text` = `project · branch` (just `project` without a branch),
    `mark` on NeedsYou.
  - State `setup` with message `Connect Claude Code to see its sessions here` and `setup_label: "Connect"` when link is
    `Off` / `Old` / `Moved`.
  - With `Invalid`, state `error` with `Claude Code's settings.json could not be read. Nothing was changed.`.
  - When connected with no sessions, `empty` with `No agent sessions yet. They show up here when Claude Code runs.`
    (phase 1 has no `New`).
  - `foot`: `Privacy mode` when on.
- **Detail** (`for` = session id).
  - `title` = project.
  - `meta`: Agent, State, Time, Branch, Tool, Folder; Folder is left out in privacy mode.
  - `body` = `text`, empty in privacy mode.
  - `buttons`: `Copy path` (`copy` = cwd, left out in privacy mode) and `Dismiss` (`id:"dismiss"`).
  - Opening a NeedsYou session's detail calls `clear_needs`.
- **Settings** (a `slide` with `back: true`, title `Coding |Agents`). Rows are in two sections:
  - **Claude Code**: one row `id:"claude"` whose `text` is the action (`Connect`, `Fix`, `Replace old hooks`, `Remove`)
    and whose `sub` is the state (`Not connected`, `Connected`, `Moved, press to fix`, `Old Hesewake hooks`,
    `settings.json unreadable`).
  - **Options**: `privacy`, `toast_needs`, `toast_done`, with `sub` `On` / `Off` and tone `good` / `muted`.

  A row click toggles, sends `Save`, and sends the page again.
- `back` from detail or settings → home. `action` `setup` → `Connect(true)`. Row `claude` → `Connect(link != On)`.
- **Deviation from spec 6.5, to be told to the user.** The protocol sends the same `visible` for the slide and the
  panel, so the extension can't open its settings page straight in the panel. Both show Home, and Settings is one click
  away.

- [ ] **Step 1: Write the failing tests in `ui.rs`.** Use a fixed `now` and helpers `sess(state, project, branch)`.

```rust
#[test] fn needs_you_comes_first_with_the_mark() { /* Working + NeedsYou + Done → sections ["Needs you","Working","Recent"]; first row mark=true, tone "warn", sub "Claude · Needs you · 2m" */ }
#[test] fn not_connected_asks_to_connect() { /* Link::Off → state "setup", setup_label "Connect"; action setup → [Connect(true)] */ }
#[test] fn privacy_hides_text_and_paths() { /* privacy on → detail has no body, no Folder meta, no Copy path; foot "Privacy mode" */ }
#[test] fn settings_rows_toggle_and_save() { /* action settings → back:true page; row privacy → Save(cfg.privacy == true), page again */ }
#[test] fn long_and_wide_names_are_clipped() { /* project = "x".repeat(300), branch = emoji ×50 → every text ≤120, sub ≤40, title ≤24, labels ≤16, both langs */ }
#[test] fn every_page_fits_in_both_languages() { /* home/detail/settings for each Link and State, EN and ID: limits above, ≤4 sections, ≤6 meta, ≤4 buttons */ }
#[test] fn revs_grow_and_language_follows_the_notch() { /* {"t":"lang","lang":"id"} → next page in Indonesian, rev larger */ }
```

- [ ] **Step 2: Run them through CI.** Expected: FAIL (missing).
- [ ] **Step 3: Implement `cfg.rs` and `ui.rs`.** Follow the shapes above and the Odoo `Ui` pattern: a `View` enum
  (`Home`, `Detail(String)`, `Settings`), `fn t(&self, s)`, and one `page()` builder per view. Add each new string to
  `TEXTS` with its Indonesian.
- [ ] **Step 4: Run them through CI.** Expected: all pass, including `every_text_has_indonesian`.
- [ ] **Step 5: Commit**: "Agents extension: slide, session page and settings".

### Task 7: Toasts, badge and status

**Files:**
- Create: `src/toasts.rs`
- Modify: `src/ui.rs`

**Interfaces:**
- Consumes: `sessions::Show`, `cfg::Config`, `text::*`.
- Produces:
  - `toasts::Toast { title: String, sub: String, needs: bool }`
  - `toasts::Queue::push(&mut self, t: Toast)`
  - `Queue::next(&mut self, now_ms: u64) -> Option<Value>` (a `toast` message, at most one per 5000 ms; needs-you first;
    two or more waiting of one kind merge into `"{} sessions need you"` / `"{} sessions finished"`)
  - `Ui` sends `{"t":"badge","on":true}` when a NeedsYou appears while the slide isn't visible, and
    `{"t":"badge","on":false}` once none is left
  - `Ui` sends `{"t":"status","text":…}` when it changes: `Claude connected · {} sessions · {} need you`,
    `Claude connected · {} sessions`, `Claude not connected`, or `Claude settings unreadable`

**Toast texts** (spec 6.4; privacy replaces the `sub` detail):

| Kind | Title | Sub |
|---|---|---|
| NeedsYou | `{project} needs you` | `Claude · {tool}: {text}`, or `Claude · Permission` in privacy mode / without text |
| Done | `Done · {project}` | `Claude · {duration}`, or `Claude` without a duration |
| Error | `Stopped with an error` | `{project} · Claude` |

No toast when its toggle is off. Done is already quiet when its window is in front, because `Sessions::apply` returns no
`Show` then.

- [ ] **Step 1: Write the failing tests.**

```rust
#[test] fn one_toast_per_five_seconds() { /* push 2 Done of different projects at 0 ms → next(0) Some, next(4999) None, next(5000) Some */ }
#[test] fn a_burst_becomes_one_toast() { /* 3 Done + 1 NeedsYou queued → next = NeedsYou; then next = "3 sessions finished" */ }
#[test] fn toggles_and_privacy_shape_toasts() { /* toast_done off → no Done; privacy on → NeedsYou sub "Claude · Permission" */ }
#[test] fn badge_and_status_follow_the_sessions() { /* NeedsYou while not visible → badge on + status "… · 1 need you"; clear_needs → badge off */ }
```

- [ ] **Step 2: Run them through CI.** Expected: FAIL (missing).
- [ ] **Step 3: Implement `toasts.rs` and wire it into `Ui`.** `Ui::on_event` turns `Show` into `Toast`. `Ui::tick` flushes
  the queue with its `now_ms`.
- [ ] **Step 4: Run them through CI.** Expected: all pass.
- [ ] **Step 5: Commit**: "Agents extension: toasts, badge and status".

### Task 8: The extension loop and `--disconnect-all`

**Files:**
- Create: `src/pipe.rs`
- Modify: `src/main.rs`

**Interfaces:**
- Consumes: everything above.
- Produces:
  - extension mode: hello `{"t":"hello","proto":1,"id":"agents","name":"Coding Agents","version":<pkg>}`;
  - `--disconnect-all`: `claude_setup::apply(false, …)`, then exit 0.

- [ ] **Step 1: Port `pipe.rs`.** `open_pipe`, `server_is_notch`, `send`, `recv`, `available` and `MAX_FRAME` come from
  Odoo's `main.rs` as they are.
- [ ] **Step 2: Write the loop in `main.rs`.** It follows Odoo's `run()`:
  1. connect and hello, and get `welcome` (`lang`);
  2. `Ui::new(Config::load(), claude_setup::read_link(exe), lang == "id")`, then `ui.sessions = Sessions::load(…)`;
  3. `relay::Receiver::start(tx)`;
  4. loop:
     - read pipe frames when `available >= 4`;
     - drain the `Msg` channel into `ui.on_event`;
     - every 1 s, `ui.tick(procs::window_alive, procs::process_running)`, which also flushes the toast queue;
     - sleep 40 ms when idle.

  `Out` handling:
  - `Send` → `pipe::send`;
  - `Save` → `cfg.save()`;
  - `SaveSessions` → `sessions.save` (at most once per second);
  - `Connect(on)` → `claude_setup::apply(on, exe)`, then `ui.linked(result)`; an `Err` gives `Invalid`.

  `fg` comes from `GetForegroundWindow` before each `on_event` / `tick`. On `quit`, send `bye`, save sessions and exit.
  If `Receiver::start` fails (a second copy is running), the extension still runs, but the status says
  `Another copy is running`.
- [ ] **Step 3: Write `fn main()`.** Dispatch: `--hook claude` → hook; `--disconnect-all` → disconnect; anything else →
  `run()`. Each path ends with `exit(0)`.
- [ ] **Step 4: Run it through CI.** Expected: all tests still pass, and Build passes without warnings.
- [ ] **Step 5: Commit**: "Agents extension: run as an extension".

### Task 9: Installer, README and trial

**Files:**
- Create: `extensions/agents/installer.iss`, `extensions/agents/README.md`

- [ ] **Step 1: Write `installer.iss`** from Odoo's, with these changes:
  - the names and paths from Global Constraints, and a new AppId GUID (generate it once with `[guid]::NewGuid()`);
  - the manifest has `"id": "agents"`, `"name": "Coding Agents"`, `"settings": "panel"`, `"processes": "allow"`;
  - FinishedLabel: "In Hesewake Notch (0.13.0 or later): Settings > Extensions, Rescan, switch Coding Agents on. Then
    press Connect on its slide to link Claude Code.";
  - `InitializeUninstall` stops the extension, then runs `{app}\hesewake-agents.exe --disconnect-all` and waits for it;
  - the data question asks "Also delete the Coding Agents data (session list and options)?" and removes
    `{userappdata}\hesewake-notch-agents`.
- [ ] **Step 2: Write `README.md`.** Cover:
  - what it shows;
  - how to install and connect;
  - what Connect changes in `settings.json` (our 8 hook entries, a one-time backup) and what Remove and uninstall undo;
  - privacy mode;
  - that phase 1 has no usage, New or Open terminal yet;
  - that nothing goes over the network.
- [ ] **Step 3: Run it through CI.** Expected: Test, Build and Installer pass, and the artifact holds
  `HesewakeAgents-Setup-0.1.0.exe`. Note its SHA-256 from the log.
- [ ] **Step 4: Commit**: "Agents extension: installer that undoes its hooks on uninstall".
- [ ] **Step 5: Trial on the user's PC.**
  1. `gh run download` into `Downloads\hesewake-agents-ci`, and compare the installer's hash with the log.
  2. Ask the user to run the installer from Explorer. If SAC blocks it, wait about 30 minutes and retry once; don't
     rebuild.
  3. The user then enables the extension in Settings, presses Connect, and runs one Claude Code session that asks a
     permission and then finishes.
  4. Check: Needs you row and toast; Done toast with its duration; `settings.json` keeps everything else; Remove leaves
     the file as before Connect, apart from its key order.
  5. Report the result. Don't start the notch or Claude from this session.

---

## Spec coverage (phase 1)

| Spec | Task |
|---|---|
| §3 modes | 3 (`--hook claude`), 8 (extension, `--disconnect-all`) |
| §3 `WM_COPYDATA` and the is-ours check | 3 |
| §3 snapshot | 4, 8 |
| §4 model and state machine (`tool`, `branch`) | 4 |
| §5.1 hooks | 2, 3, 5 |
| §5.3 setup (link states, old hooks, never on install) | 5, 6, 9 |
| §6.1 slide, §6.2 detail (Copy path, Dismiss) | 6 |
| §6.4 toasts, badge, status | 7 |
| §6.5 panel (privacy and toast toggles) | 6 (see the deviation above) |
| §6.6 privacy, §6.7 language | 6, 7 |
| §7 packaging, uninstall cleanup, CI | 1, 9 |
| §10 security | 2, 3, 5 |
| §12 unit tests and manual trial | each task, 9 |

**Left for phase 2:**
- status line and usage (§5.2, Usage section, usage toast);
- `New` / `Continue` and recent folders (§6.3);
- `Open terminal` and focus;
- the Terminal choice in settings;
- the `#[ignore]` end-to-end test.
