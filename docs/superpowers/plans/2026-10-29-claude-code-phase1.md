# Claude Code in the notch (phase 1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Claude Code sessions report working / needs-you / done / error to the notch through hooks, with toasts, a collapsed-notch chip, click-to-focus, and Connect/Disconnect in Settings.

**Architecture:** Hooks run `hesewake-notch.exe --claude-hook` (exec form, async). That process parses the hook JSON, finds the terminal window among its ancestors, and sends a compact message to the running notch with `WM_COPYDATA`, then exits 0 silently. The notch keeps a pure session state machine (`claude.rs`), shows toasts/chip from it, and edits `~/.claude/settings.json` through pure merge functions (`claude_setup.rs`).

**Tech Stack:** Rust, `windows` 0.58 (Win32), tiny-skia, serde/serde_json (with `preserve_order`).

**Spec:** `docs/superpowers/specs/2026-10-29-claude-code-integration-design.md`

## Global Constraints

- Branch `feat/claude-code` from `origin/main` (0.8.0). Fork remote `fork`, PR to `dirgaastya/hesewake-notch` `main`.
- All repo text in English. No "Co-Authored-By Claude", "Generated with Claude Code" or session links in commits/PRs.
- Every UI string through `i18n::tr("English")` with an entry in `ID_TEXT` (the i18n test scans `src/` for `tr(` literals).
- Release build is `panic = "abort"`: hook-path and parsing code must not panic on any input (no `unwrap`/indexing on untrusted data, cut strings on char boundaries).
- The hook process always exits 0 and writes nothing to stdout or stderr.
- No network, no new threads in the notch for this feature, no cost when not connected.
- No Anthropic logo; neutral icon drawn with existing gfx helpers.
- Local `cargo` is blocked by Smart App Control on this PC: run tests in Windows Sandbox (`C:\Users\Astech\SandboxRust`, job files) or by pushing the branch to the fork (CI runs `cargo build --locked` with `-D warnings` and `cargo test --locked`). "Run" steps below mean either.
- Files are CRLF; keep them CRLF (`scratchpad/ed.py` or the Edit tool, never `sed -i`).

## Review Focus

1. `~/.claude/settings.json` starting with a UTF-8 BOM, empty, or with `"hooks": null` → Connect must still work (BOM stripped, null treated as empty) and must not lose other keys. Test in Task 3.
2. A `last_assistant_message` of 1 MB, or with multi-byte text (Indonesian/emoji/CJK) at the 200-char cut → preview cut on a char boundary, no panic. Test in Task 1.
3. Hook fires while no notch runs (or the notch is starting) → `deliver` returns false quickly (`SendMessageTimeoutW`, 500 ms), process exits 0. Test in Task 4.
4. The session's terminal window is closed while it shows NeedsYou → the orange indicator disappears within ~1 s (window gone ⇒ session dropped). Test in Task 2.
5. A turn that started before the notch did (no `UserPromptSubmit` seen) ends with `Stop` → Done toast without a duration, no panic. Test in Task 2.

---

### Task 1: Hook payload (`src/claude_hook.rs`, pure part)

**Files:**
- Create: `src/claude_hook.rs`
- Modify: `src/main.rs` (add `mod claude_hook;`)

**Interfaces:**
- Produces:
  - `pub(crate) enum EventKind { SessionStart, SessionEnd, Prompt, Tool, NeedsYou, Stop, StopFailure }`
  - `#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)] pub(crate) struct Msg { pub v: u8 /*=1*/, pub kind: EventKind, pub session: String, pub project: String, pub cwd: String, pub text: String, pub hwnd: isize, pub subagent: bool }` (`EventKind` also Serialize/Deserialize/Copy/PartialEq).
  - `pub(crate) fn parse_hook(json: &str) -> Option<Msg>` (hwnd left 0; filled by Task 4).
  - `pub(crate) fn encode(m: &Msg) -> Vec<u8>` / `pub(crate) fn decode(b: &[u8]) -> Option<Msg>` (serde_json; `decode` rejects > 16 KB and unknown `v`).
  - `pub(crate) fn preview(s: &str) -> String`: first non-empty line, markdown markers (`#`, `*`, `_`, `` ` ``, `>`, leading `- `) removed, whitespace collapsed, at most 200 chars.
  - `pub(crate) fn project_name(cwd: &str) -> String`: last path component (`\` or `/`), trailing separators ignored, `"Claude Code"` if empty.
- Mapping in `parse_hook`: `hook_event_name` → `SessionStart`, `SessionEnd`, `UserPromptSubmit`⇒`Prompt`, `PreToolUse`/`PostToolUse`⇒`Tool`, `Notification` with `notification_type` in {`permission_prompt`, `elicitation_dialog`, `agent_needs_input`} ⇒`NeedsYou` (text = `preview(message)`), any other notification type ⇒ `None`, `Stop` ⇒ `Stop` (text = `preview(last_assistant_message)`), `StopFailure` ⇒ `StopFailure`; unknown event or missing `session_id` ⇒ `None`. `subagent = agent_id` present.

- [ ] **Step 1: Write the failing tests** in `src/claude_hook.rs` `#[cfg(test)] mod tests`:
  - `maps_each_event`: one JSON per event above → expected `kind`; `{"hook_event_name":"Notification","notification_type":"idle_prompt",...}` → `None`.
  - `stop_carries_a_preview`: `last_assistant_message` = `"## Done\n\n**PR #27** is up."` → `text == "Done"`; `"\n\n  - `fix` merged"` → `"fix merged"`.
  - `preview_cuts_on_char_boundaries`: 300 × `"é"` → 200 chars, valid UTF-8; 1 MB string → ≤ 200 chars, no panic.
  - `project_from_cwd`: `"C:\\Users\\a\\Projects\\rust\\hesewake-notch\\"` → `"hesewake-notch"`; `"/home/a/app"` → `"app"`; `""` → `"Claude Code"`.
  - `bad_input_is_ignored`: `""`, `"null"`, `"[]"`, `"{"`, `{"hook_event_name":5}`, missing `session_id` → `None`.
  - `round_trip`: `decode(&encode(&m)) == Some(m)`; `decode` of 17 KB or `v: 2` → `None`.
- [ ] **Step 2: Run** `cargo test claude_hook` → FAIL (module empty).
- [ ] **Step 3: Implement** the items above with `serde_json::Value` lookups (no panicking access).
- [ ] **Step 4: Run** `cargo test claude_hook` → PASS.
- [ ] **Step 5: Commit** `git add src/claude_hook.rs src/main.rs && git commit -m "Claude Code: parse hook events"`

### Task 2: Session state machine (`src/claude.rs`, pure part)

**Files:**
- Create: `src/claude.rs`
- Modify: `src/main.rs` (`mod claude;`)

**Interfaces:**
- Consumes: `claude_hook::{Msg, EventKind}`.
- Produces:
  - `#[derive(Clone, Copy, PartialEq, Debug)] pub(crate) enum State { Idle, Working, NeedsYou, Answered, Done, Error }`
  - `pub(crate) struct Session { pub id: String, pub project: String, pub state: State, pub turn_start: Option<Instant>, pub last: Instant, pub text: String, pub hwnd: isize }`
  - `pub(crate) enum Show { NeedsYou { session: String, project: String, text: String }, Done { session: String, project: String, text: String, secs: Option<u64> }, Error { session: String, project: String } }` (`Clone, PartialEq, Debug`)
  - `#[derive(Default)] pub(crate) struct Sessions { pub list: Vec<Session> }` with:
    - `pub fn apply(&mut self, m: &Msg, now: Instant, fg: isize) -> Option<Show>`: the transition table of the spec (§2), plus: `Stop`/`StopFailure` returns `None` when `fg != 0 && fg == hwnd && self.shares_window(hwnd) == 1` (person already looking); `subagent` messages never change state; at most `MAX_SESSIONS = 16` (oldest `last` dropped).
    - `pub fn tick(&mut self, now: Instant, fg: isize, alive: impl Fn(isize) -> bool) -> bool` (true if anything changed): NeedsYou → Answered when `fg == hwnd` and `shares_window(hwnd) == 1`; drop Working/Answered sessions idle for `STALE = 15 min`; drop any session with `hwnd != 0 && !alive(hwnd)`.
    - `pub fn clear_needs(&mut self, session: &str)` (NeedsYou → Answered).
    - `pub fn working(&self) -> usize`, `pub fn needing(&self) -> Option<&Session>` (oldest NeedsYou), `pub fn window_of(&self, session: &str) -> isize`, `fn shares_window(&self, hwnd: isize) -> usize`.

- [ ] **Step 1: Write the failing tests** (`#[cfg(test)] mod tests` in `claude.rs`, helper `msg(kind, session, hwnd)`):
  - `prompt_then_stop_shows_done_with_duration`: Prompt at t0, Stop at t0+372 s → `Show::Done { secs: Some(372), .. }`, state Done.
  - `stop_without_prompt_has_no_duration` (Review Focus 5) → `secs: None`.
  - `permission_prompt_needs_you_until_answered`: NeedsYou → `Show::NeedsYou`; `tick` with `fg == hwnd` → Answered; next Tool → Working.
  - `shared_window_does_not_clear_needs_you`: two sessions same hwnd, one NeedsYou; `tick(fg == hwnd)` keeps NeedsYou; `clear_needs` clears it.
  - `done_is_quiet_when_its_window_is_in_front`: Stop with `fg == hwnd`, sole session → `None`, state Done.
  - `subagents_never_change_state`: Done session + subagent Tool → still Done.
  - `tool_after_stop_starts_a_new_turn`: Done + main Tool → Working.
  - `stale_and_closed_windows_are_dropped` (Review Focus 4): Working idle 16 min → dropped; NeedsYou with `alive` = false → dropped.
  - `session_end_removes` and `at_most_16_sessions`.
- [ ] **Step 2: Run** `cargo test claude::` → FAIL.
- [ ] **Step 3: Implement** `Sessions` as specified (plain `Vec`, linear search).
- [ ] **Step 4: Run** `cargo test claude::` → PASS.
- [ ] **Step 5: Commit** `git commit -m "Claude Code: session states"`

### Task 3: settings.json Connect/Disconnect (`src/claude_setup.rs`)

**Files:**
- Create: `src/claude_setup.rs`
- Modify: `src/main.rs` (`mod claude_setup;`), `Cargo.toml` (`serde_json = { version = "1", features = ["preserve_order"] }`, update `Cargo.lock`)

**Interfaces:**
- Produces:
  - `#[derive(Clone, Copy, PartialEq, Debug)] pub(crate) enum Link { Off, On, Moved, Invalid }`
  - `pub(crate) const EVENTS: [(&str, Option<&str>); 8]` = `UserPromptSubmit`(None), `PreToolUse`(`"*"`), `PostToolUse`(`"*"`), `Notification`(`"permission_prompt|elicitation_dialog|agent_needs_input"`), `Stop`, `StopFailure`, `SessionStart`, `SessionEnd` (None).
  - `pub(crate) fn connect(v: &mut serde_json::Value, exe: &str)`: first removes ours, then appends per event `{ ["matcher": m,] "hooks": [{ "type": "command", "command": exe, "args": ["--claude-hook"], "async": true, "timeout": 10 }] }`.
  - `pub(crate) fn disconnect(v: &mut serde_json::Value)`: removes every hook object whose `command` file name is `hesewake-notch.exe` (case-insensitive) and whose `args` contains `"--claude-hook"`; drops matcher groups left with no hooks, events left empty, and `hooks` if empty.
  - `pub(crate) fn link(v: &serde_json::Value, exe: &str) -> Link` (On if ours exist with `command == exe` (case-insensitive), Moved if ours exist with another path, else Off).
  - `pub(crate) fn settings_path() -> Option<PathBuf>`: `%CLAUDE_CONFIG_DIR%\settings.json` if set, else `%USERPROFILE%\.claude\settings.json`; in tests a thread-local override `TEST_DIR` (same pattern as `calendar::TEST_CLIPBOARD`).
  - `pub(crate) fn read_link() -> Link` and `pub(crate) fn apply(on: bool) -> Result<Link, String>`: read file (missing ⇒ `{}`; strip BOM; invalid JSON ⇒ `Err` and `Link::Invalid`, nothing written), back up to `settings.json.hesewake-backup` only if that backup doesn't exist yet, write pretty JSON (2 spaces) via temp file + rename.

- [ ] **Step 1: Write the failing tests:**
  - `connect_keeps_everything_else`: input with `permissions`, `model`, an existing user hook on `Stop` → after `connect`, key order of top level unchanged, user hook still first in `Stop`, ours appended, all 8 events present with `async: true`.
  - `connect_twice_adds_once`; `connect_with_new_path_replaces_old` (old path ⇒ `Link::Moved` before, `On` after).
  - `disconnect_removes_only_ours`: user hook kept; events/`hooks` that only had ours removed; result equals the original input.
  - `odd_files` (Review Focus 1): BOM + JSON, empty file, `"hooks": null`, file missing → `apply(true)` succeeds; `"{ not json"` → `Err`, file unchanged, `read_link() == Invalid`.
  - `backup_made_once`: two `apply` calls → backup holds the original content.
- [ ] **Step 2: Run** `cargo test claude_setup` → FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test claude_setup` → PASS; `cargo build --locked` clean.
- [ ] **Step 5: Commit** `git commit -m "Claude Code: connect and disconnect hooks in settings.json"`

### Task 4: The bridge process (`--claude-hook`, `--claude-disconnect`)

**Files:**
- Modify: `src/claude_hook.rs` (runtime part), `src/main.rs` (dispatch at the very start of `main`, before the panic hook, `perf::init`, `pace::init` and `sys::single_instance`), `Cargo.toml` (`Win32_System_Diagnostics_ToolHelp`), `hesewake-notch.iss` (`[UninstallRun]`)

**Interfaces:**
- Consumes: `parse_hook`, `encode`, `claude_setup::apply`.
- Produces:
  - `pub(crate) const COPY_ID: usize = 0x434C_4155;` (`WM_COPYDATA` `dwData`).
  - `pub(crate) fn run_hook()`: read stdin up to 256 KB (`take`), `parse_hook`, set `hwnd = terminal_window(project)`, `deliver(find_notch(), &msg)`; never panics, returns normally.
  - `fn terminal_window(project: &str) -> isize`: Toolhelp snapshot pid→ppid; walk up from `GetCurrentProcessId()` at most 12 levels; for each ancestor, `EnumWindows` visible, unowned, titled top-level windows of that pid; prefer a title containing `project`; first match wins; 0 if none.
  - `pub(crate) fn find_notch() -> isize` (`FindWindowW(w!("HesewakeNotchWindow"), None)`).
  - `pub(crate) fn deliver(hwnd: isize, m: &Msg) -> bool`: `SendMessageTimeoutW(WM_COPYDATA, SMTO_ABORTIFHUNG, 500 ms)`; false when `hwnd == 0`.
  - `main`: `--claude-hook` ⇒ `run_hook(); return Ok(())`; `--claude-disconnect` ⇒ `let _ = claude_setup::apply(false); return Ok(())`.
  - Installer: `[UninstallRun]` `Filename: "{app}\{#AppExe}"; Parameters: "--claude-disconnect"; Flags: runhidden waituntilterminated; RunOnceId: "ClaudeDisconnect"`.

- [ ] **Step 1: Write the failing tests** (in `claude_hook.rs`):
  - `deliver_reaches_a_window`: register a test window class whose wndproc stores the decoded `WM_COPYDATA` (`dwData == COPY_ID`) in a thread-local; `deliver(test_hwnd, &m)` → true and stored == `m`.
  - `deliver_without_notch_is_quick` (Review Focus 3): `deliver(0, &m)` → false; a hung target is not needed, just assert the call returns in < 1 s.
  - `own_ancestors_are_walked`: `terminal_window("x")` returns without panicking (value not asserted; CI has no terminal window).
- [ ] **Step 2: Run** `cargo test claude_hook` → FAIL.
- [ ] **Step 3: Implement** the runtime functions and the `main` dispatch; add the Win32 feature; add `[UninstallRun]`.
- [ ] **Step 4: Run** `cargo test claude_hook` → PASS; `cargo build --release --locked` clean.
- [ ] **Step 5: Commit** `git commit -m "Claude Code: hook bridge process and uninstall cleanup"`

### Task 5: Notch reacts: toasts, focus, ticking

**Files:**
- Modify: `src/main.rs` (`App` fields `claude: claude::Sessions`, `claude_link: claude_setup::Link`; init), `src/snapshot.rs` (`test_app` fields), `src/window.rs` (`WM_COPYDATA` handler → `a.on_claude(&msg)`), `src/claude.rs` (App glue), `src/toast.rs` (new kind, size, duration, draw, click), `src/anim.rs` (`poll`: call `a.claude_tick()` once per second via `tick_n`), `src/i18n.rs`

**Interfaces:**
- Consumes: `Sessions`, `Show`, `claude_hook::{decode, COPY_ID}`.
- Produces:
  - `ToastKind::Claude(claude::Show)`; `TOAST_CLAUDE: (f32, f32) = (372.0, 66.0)`; durations: NeedsYou 8 s, Done 6 s, Error 6 s.
  - `impl App { pub(crate) fn on_claude(&mut self, m: &Msg); pub(crate) fn claude_tick(&mut self) -> bool; fn claude_focus(&mut self, session: &str) }`.
  - `on_claude`: `apply(m, now, GetForegroundWindow())`; `Some(show)` ⇒ `push_toast(ToastKind::Claude(show))`; redraw/schedule.
  - Click on a Claude toast ⇒ `claude_focus(session)`: `IsIconic` ⇒ `ShowWindow(SW_RESTORE)`, `SetForegroundWindow`, `clear_needs`, end toast.
  - Toast look: orange `[255, 159, 10]` "Claude needs you" / green `[60, 200, 110]` `"Done · {}"` with `fmt` m/s / red `[255, 69, 58]` "Stopped with an error"; second line project name; third line `text` ellipsized. Strings via `tr`, Indonesian: "Claude butuh kamu", "Selesai · {}", "Berhenti karena error".

- [ ] **Step 1: Write the failing tests** (`snapshot.rs`):
  - `claude_message_shows_needs_you_toast`: `a.on_claude(&needs_you_msg)` → `a.toast` is `ToastKind::Claude(Show::NeedsYou{..})`.
  - `claude_toast_click_focuses_and_clears`: with a fake session hwnd 0, clicking inside the toast ends it and the session becomes Answered.
  - `snap_claude_toasts`: `shot` for NeedsYou, Done (with and without duration), Error, in English and Indonesian (`i18n::restore_on_drop()`).
- [ ] **Step 2: Run** `cargo test claude` → FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test` (all) → PASS.
- [ ] **Step 5: Commit** `git commit -m "Claude Code: toasts and focus"`

### Task 6: Collapsed-notch chip and the thin-line mark

**Files:**
- Modify: `src/claude.rs` (`claude_extra`, `draw_claude_chip`, `draw_claude_thin`, `claude_animating`), `src/anim.rs` (`target` adds `claude_extra()` to the idle and playing widths; `schedule` uses `CLAUDE_DOTS_MS = 100` while `claude_animating()`), `src/render.rs` (`draw_collapsed` calls the chip; after the shape fill, when `a.thin()` and a session needs you, `draw_claude_thin`)

**Interfaces:**
- Produces:
  - `pub(crate) fn claude_extra(&self) -> f32`: 0 when expanded, thin, toast showing, or no Working/NeedsYou session; otherwise the chip width.
  - Chip content: NeedsYou wins over Working. NeedsYou: static orange icon + project (or `"{} need you"` count). Working: neutral icon + 3 dots stepping every 100 ms + project (or `"{} sessions"`). Project ellipsized to 90 px.
  - `claude_animating()`: a Working chip is visible (not expanded, not thin, no toast) — only then the 100 ms timer runs; NeedsYou is static.
  - Thin line: an orange 16 × 3 px rounded mark at the line's right end while any session needs you.
  - The chip is display-only; hovering opens the notch as before.

- [ ] **Step 1: Write the failing tests** (`snapshot.rs`): `snap_claude_chip` (idle working, idle needs-you, playing working, two sessions) and `thin_line_marks_needs_you` (thin + NeedsYou → orange pixels present; thin + Working only → none, reusing the coloured-pixel count from `thin_line_stays_empty_right_after_a_toast`), plus `claude_chip_does_not_animate_while_waiting` (`claude_animating()` false for NeedsYou-only).
- [ ] **Step 2: Run** → FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test` → PASS.
- [ ] **Step 5: Commit** `git commit -m "Claude Code: chip in the collapsed notch"`

### Task 7: Settings row, docs, test build

**Files:**
- Modify: `src/settingsview.rs` (Features row 7 = index 6: label `tr("Claude Code")`, sub from `claude_link`: "Not connected" / "Connected · new sessions" / "Exe moved, reconnect" / "settings.json is invalid"; button "Connect" / "Disconnect" / "Reconnect"; click arm `(2, 6)` calls `claude_setup::apply` and stores the result), `src/main.rs` (read `claude_setup::read_link()` when Settings opens, not per frame), `src/i18n.rs`, `README.md` (feature row: what it does, Connect, "WSL not supported", "several tabs in one window: brings the window, not the tab"), `CHANGELOG.md` (Unreleased)

- [ ] **Step 1: Write the failing test** `claude_row_connects_and_disconnects` (snapshot.rs, with `claude_setup::TEST_DIR` set to a temp dir): click the button → file has our hooks and the sub-label says connected; click again → our hooks gone. Extend `snap_settings`/`snap_indonesian` Features tab shots.
- [ ] **Step 2: Run** → FAIL. **Step 3: Implement.** **Step 4: Run** `cargo test` → PASS, `settings_text_fits_cells` still passes.
- [ ] **Step 5: Commit** `git commit -m "Claude Code: Settings row and docs"`
- [ ] **Step 6: Test build:** push `feat/claude-code` to `fork`; push a throwaway `ci-check/release-exe` (branch + `release-exe.yml` that runs `cargo test --locked`, `cargo build --release --locked`, uploads the exe); download with `gh run download` into `Downloads\hesewake-notch-ci\hesewake-notch-claude.exe` (separate name, do not overwrite the working exe); launch it once to see whether Smart App Control allows it (if blocked, retry once after ~15 min; never work around it). Ask the user to delete the throwaway branch.
- [ ] **Step 7: Manual check with the user:** Connect in Settings, start a new Claude Code session, verify working chip, permission toast, Done toast with preview, click-to-focus; Disconnect leaves `settings.json` as before.
