# Claude Code in the notch: design

Date: 2026-10-29. Status: reviewed (builder check done), waiting for approval.

## Why

People run Claude Code in a terminal and then switch to something else (a browser, WhatsApp, a fullscreen video) while
it works. They can't tell when Claude has finished, and worse, when it is stuck waiting for them (a permission prompt
or a question). They keep switching back to check, or they come back much later and find it has been waiting the whole
time.

The notch is always visible, even over fullscreen video (as the thin busy line), so it is the right place to show this.

## Goals

1. Show, at a glance, whether each running Claude Code session is **working**, **needs you** or **done**.
2. Make "needs you" hard to miss: it stays until the person deals with it.
3. On "done", show how long it took and the first line of Claude's last message.
4. One click takes the person to the terminal (or VS Code window) that runs that session.
5. Set up with one button in Settings, and removed just as easily.
6. Later (phase 2): a Claude slide with the session list and a "New session" launcher, and the plan's usage
   (5-hour and 7-day percentages) in the notch.

## Non-goals

- Sending prompts or commands to Claude from the notch. The terminal stays the place where Claude is driven.
- Voice input. Windows voice typing (Win+H) already types into the Claude Code prompt.
- Anything over the network. All data stays on the PC.

## Anti-goals (a failure even if it "works")

- The hooks slow Claude down, change what Claude does, or print anything into the Claude Code session.
- The person's `~/.claude/settings.json` is damaged or loses settings, or the integration can't be removed cleanly.
- The notch shows "working" forever for a session that crashed or was closed.
- Anthropic's logo or branding in the UI. The icon is neutral (for example a terminal prompt `>_` or a sparkle drawn
  with the existing gfx helpers); text says "Claude Code" where a name is needed.
- The feature costs anything (CPU, threads, files) for people who never connect it.

## Constraints

- Windows 10/11, Rust, the existing Win32 + tiny-skia app. No new runtime, no services.
- Smart App Control may block a freshly built exe on some PCs. If the hook can't start the exe, Claude Code shows a
  "hook error" notice; that is acceptable but the docs/Settings should say how to disconnect.
- All UI text goes through `i18n::tr()` with Indonesian translations, like the rest of the app.
- Repo text (code comments, commits, PR) in English.
- Release builds use `panic = "abort"`: nothing in the hook path or the payload parsing may panic on odd input.

## Facts about Claude Code hooks this design relies on

From the official docs (code.claude.com/docs/en/hooks.md, statusline.md), checked on 2026-10-29:

- Hooks live in `~/.claude/settings.json` under `"hooks": { "<Event>": [ { "matcher": "...", "hooks": [ { "type":
  "command", "command": "...", ... } ] } ] }`. Hooks for one event run in parallel.
- `"async": true` runs a command hook in the background without blocking Claude.
- Exec form (`"command"` = an `.exe` path, `"args"` = list) runs without a shell, so paths with spaces are safe.
- Exit code 0 with no stdout changes nothing. Notification hooks can't block anything.
- The hook gets one JSON object on stdin. Common fields: `session_id`, `transcript_path`, `cwd`, `hook_event_name`,
  `permission_mode`. Subagent calls also carry `agent_id`.
- Events used here:
  - `UserPromptSubmit`: the person sent a prompt → working.
  - `PreToolUse` / `PostToolUse`: tools are running → working.
  - `Notification` with `notification_type`: `permission_prompt` (fires if a permission prompt waits ~6 s),
    `elicitation_dialog`, `agent_needs_input` → needs you. Has `message` and optional `title`. `idle_prompt` (~60 s
    after every finished turn nobody answered) is **not** used: "Done" already covers a finished turn, and treating it
    as "needs you" would make that the normal state.
  - `Stop`: the turn finished. Has `last_assistant_message`. Not fired when the person interrupts.
  - `StopFailure`: the turn ended on an API error.
  - `SessionStart` (`source`), `SessionEnd` (`reason`).
- There is no event for "permission prompt answered". The next signal after approving is `PostToolUse` when that tool
  finishes, which can be minutes later.
- Plan usage is only available to the **status line** command, on stdin: `rate_limits.five_hour.used_percentage`,
  `rate_limits.seven_day.used_percentage`, `*.resets_at` (unix seconds). Present only for Pro/Max (or a gateway) and only
  after the first API response of a session. The status line is a separate `statusLine` setting, not a hook.

## How it works

### 1. The bridge: `hesewake-notch.exe --claude-hook`

The hooks run the app's own exe with `--claude-hook` (exec form, `async: true`). In this mode the exe:

1. Runs before the single-instance check and creates no window, tray icon or threads.
2. Reads stdin (capped, for example 256 KB; anything larger is cut) and parses the JSON. Unknown or missing fields are
   ignored; invalid JSON means "do nothing".
3. Finds the window to come back to: walks up the parent processes (Toolhelp snapshot) and takes the first ancestor that
   owns a visible top-level window (Windows Terminal, conhost, VS Code, ...). For apps with many windows of one process
   (VS Code), prefer the window whose title contains the project folder name (`cwd`'s last part).
4. Sends a small message to the running notch (`FindWindowW` on class `HesewakeNotchWindow`, then `WM_COPYDATA`):
   event, session id, project name (`cwd` folder name), `cwd`, notification type, a short text (the notification
   message or the first ~200 characters of `last_assistant_message`), the window handle, and whether it came from a
   subagent.
5. Exits 0 with no output, always. If no notch is running, it just exits.

The payload is a small versioned struct (or compact JSON) so an older notch ignores fields it doesn't know.

### 2. Session state in the notch

`App` keeps a list of sessions keyed by `session_id`: project name, state (Working / NeedsYou / Done / Error), the time
the current turn started (from `UserPromptSubmit`), last event time, the preview text, and the window handle.

Transitions:

- `SessionStart` → add (state Done/idle, nothing shown).
- `UserPromptSubmit`, `PreToolUse`, `PostToolUse` → Working (start the turn timer on `UserPromptSubmit`).
- `Notification` (`permission_prompt`, `elicitation_dialog`, `agent_needs_input`) → NeedsYou with the message.
- `Stop` → Done, with duration and preview; `StopFailure` → Error.
- `SessionEnd` → remove.
- Subagent and background-agent events (`agent_id` present) never change the state: they neither start nor end a turn.
  A main-session `PreToolUse`/`PostToolUse` after `Stop` does mean a new turn started (for example a background task
  woke Claude), so it moves Done → Working; `Stop` follows.
- NeedsYou is cleared (the session becomes Answered: nothing shown, no animation, waiting for its next event) when:
  the person clicks the toast or the indicator; the session sends its next event; or the session's window becomes the
  foreground window **and no other session shares that window**. Claude Code sends no "answered" event, so this is
  the best signal available.
- Stale: a Working or Answered session with no event for 15 minutes, or whose window no longer exists, is dropped (covers crashes and
  closed terminals). Ctrl+C interrupts send no `Stop`; the same rule covers them.

Events are handled on the UI thread (`WM_COPYDATA` arrives there), so no new thread is needed.

Several sessions can share one window (Windows Terminal tabs, one VS Code window). Then a click brings the window to
the front but cannot pick the tab, the foreground rule above doesn't clear NeedsYou, and the "window already in front"
rule below doesn't hide Done toasts. This is a known limit, stated in Settings.

### 3. What the person sees (phase 1)

- **Working:** a small chip in the collapsed notch, in the same style as the timer chip: neutral icon, three animated
  dots, project name (shortened). With several sessions working, one chip with a count ("2 sessions"). It must not hide
  the music title or the clock; it takes room the way the timer chip and bell do (`*_extra()` widths).
- **Needs you:** two parts, so it can't be missed but costs nothing while it waits:
  1. A toast in orange with the project name and the message ("Claude needs your permission"), with the normal toast
     duration and queue (`src/toast.rs`: one at a time, others queued). Shown in busy mode too.
  2. After the toast, a **static orange indicator** in the collapsed notch (icon + project name, or a count) that stays
     until NeedsYou is cleared. It doesn't animate, so `anim::schedule` can stop the timer; waiting for hours costs no
     CPU. It takes room like the timer chip (`*_extra()` widths), is visible in the thin busy line as a small orange
     mark, and hovering still opens the notch as usual.
- **Done:** a toast in green: "Done · 6m 12s", project name, and the first line of the last message (markdown markers
  removed, ellipsized). Normal toast duration. Not shown if that session's window is already in front.
- **Error:** like Done but in red, "Stopped with an error".
- **Click on a toast or the chip:** bring the session's window to the front (`SetForegroundWindow`; the notch has just
  been clicked, so Windows allows it; restore it if minimized).
- Animations follow `anim::schedule`: the dots animate only while a session is Working and the notch is visible, and
  stop otherwise, so idle CPU doesn't change.

### 4. Connecting (Settings › Features › "Claude Code")

A row with a button:

- **Connect:** reads `~/.claude/settings.json` (creates it if missing), keeps every existing key and value and their
  order, adds our hook entries, and writes it back. Before the first change it saves a copy as
  `settings.json.hesewake-backup`. Our entries are recognised by the exe path plus the `--claude-hook` argument, so
  connecting twice adds nothing twice. The exe path is `std::env::current_exe()`.
- **Disconnect:** removes only our entries (and empty containers they leave behind) and keeps everything else.
- **State shown:** "Connected", "Not connected", or "Exe moved, reconnect" when the saved path no longer matches the
  running exe.
- If `settings.json` is not valid JSON, nothing is written and the row says so.
- Key order is kept with serde_json's `preserve_order` feature (to be enabled).
- **Uninstall:** a `--claude-disconnect` mode does the same as Disconnect without a window; the installer runs it from
  `[UninstallRun]` (`hesewake-notch.iss`), so uninstalling leaves no hooks pointing at a missing exe.
- A test build (for example in `Downloads\hesewake-notch-ci`) may connect; if it is deleted or replaced, the row of the
  next exe shows "Exe moved, reconnect" and reconnecting replaces the old entries.
- Claude Code running inside WSL reads its own Linux `~/.claude`, so it is not supported; Settings says so.
- Changes apply to new Claude Code sessions (Claude Code reads settings at start); the row says that.

Hook entries written (exec form, async, short timeout):

```json
"UserPromptSubmit": [{ "hooks": [{ "type": "command", "command": "<exe>", "args": ["--claude-hook"], "async": true, "timeout": 10 }] }],
"PreToolUse":       [{ "matcher": "*", "hooks": [ ...same... ] }],
"PostToolUse":      [{ "matcher": "*", "hooks": [ ...same... ] }],
"Notification":     [{ "matcher": "permission_prompt|elicitation_dialog|agent_needs_input", "hooks": [ ...same... ] }],
"Stop":             [{ "hooks": [ ...same... ] }],
"StopFailure":      [{ "hooks": [ ...same... ] }],
"SessionStart":     [{ "hooks": [ ...same... ] }],
"SessionEnd":       [{ "hooks": [ ...same... ] }]
```

(The exact JSON is the builder's call as long as it is exec form, async, and recognisable for removal.)

### 5. Phase 2

- **Claude slide** (optional slide, off by default, like Clipboard/Agenda): the sessions with project, state and time,
  an "Open" button per row (focus its window), and **New session**: a short list of recent project folders (from the
  `cwd`s seen) that opens Windows Terminal there running `claude` (`wt.exe -d <dir> claude`, falling back to
  `cmd /k claude` when `wt.exe` is missing).
- **Usage:** Connect also offers to set the status line. Our status line command (`--claude-statusline`) reads the
  status line JSON, sends the `rate_limits` numbers to the notch (same `WM_COPYDATA` path), and prints a short line
  for the terminal (model and context use). It is only set when the person has **no** `statusLine` of their own; if they
  have one, it is left alone and the usage part is simply not available (Settings says why). Disconnect removes ours.
  The usage option is a separate switch in the row, off until turned on. The notch shows "5h 41% · resets 15:20" in the Claude
  slide and a small warning toast when the 5-hour window passes 90%. Numbers come only from Claude Code, nothing is
  estimated.

## Testing

- Unit tests (no Windows needed): payload parsing with odd input (no panics), the state machine (all transitions,
  subagents, stale rules), project name from `cwd`, preview text cleanup, and the settings.json merge/unmerge
  (keeps other keys and order, idempotent, removes only ours, refuses invalid JSON, backup made once).
- Snapshot tests (existing harness): Working chip (idle and with music), several sessions, Needs-you toast, Done toast,
  Error toast, both languages.
- CI-only tests (GitHub windows runner, gated on `CI`): start the real exe with `--claude-hook` and a JSON on stdin
  against a test window, and check the message arrives; Connect/Disconnect against a temporary settings file.
- Manual: a real Claude Code session on the person's PC with the test build (via GitHub Actions artifact).

## Delivery

- Branch `feat/claude-code` on the fork, from `main` (0.8.0). Phase 1 first, as its own PR with screenshots; phase 2 as
  follow-up PRs.
- A test build from GitHub Actions is downloaded to `Downloads\hesewake-notch-ci` for the person to try before the PR.

## Left to the builder

- The icon drawing, exact colours (within the existing palette), chip layout and toast sizes.
- The payload encoding over `WM_COPYDATA`.
- How the parent-process walk is done and its limits (depth, timeouts).
- Whether `PreToolUse` and `PostToolUse` both need hooks, if one is enough to keep "working" accurate.
- Parent-process walk details, UIPI handling (the notch may need `ChangeWindowMessageFilterEx` for `WM_COPYDATA` if
  the terminal runs elevated), and the payload format.
- Settings row placement within Features.
