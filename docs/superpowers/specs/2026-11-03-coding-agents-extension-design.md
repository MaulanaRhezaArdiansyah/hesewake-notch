# Coding Agents extension: design

Status: draft for approval (2026-11-03). Lives on the fork branch `ext/agents`, code in `extensions/agents`.

## 1. Intent

Coding agents (Claude Code today, maybe Codex, Antigravity and others later) work for minutes at a time in a terminal,
then stop because they are done or because they **need the user**: a permission prompt or a question. Today the user
finds out by checking terminals. The notch is always in view, so it should say when an agent needs you, when it is done,
and how much quota is left, and get you back to the right terminal in one click.

The fork already had this for Claude Code, built into the notch itself (`feat/claude-code`). It worked, but it only ran
on a private build, and that build was retired on 2026-11-03 for the official installer. Since 0.13.0 the notch has
**extensions**, separate programs that talk to it over a pipe (`docs/extensions.md`). This design moves the feature
there, so it runs on the official notch and survives its updates. It also makes the feature **general for coding
agents**: one session model and one UI, with one **adapter** per agent. Claude Code comes first.

## 2. Goals, non-goals, anti-goals

**Goals (agreed with the user)**
1. **Needs you first.** Sessions waiting for permission or input are listed at the top and get a toast that names the
   project and what is asked.
2. **One click back to that session's terminal**, not a new one.
3. **A summary when an agent finishes**: project and duration.
4. **Informative rows**: project, branch, agent, state, duration, and the current tool.
5. **Usage**: the 5-hour and 7-day limits with reset times, plus context use per session, with warnings at 80% and 90%.
6. **Quick start**: recent projects, start a new session, or continue the last one.
7. **Privacy mode** for screen sharing and recording: no message text and no full paths.
8. **A neutral icon and agent labels**. No vendor logos.
9. **Part of the notch** (a slide, toasts, and an indicator in the small notch), not a separate pill next to it.

**Non-goals for the first version**
- Adapters other than Claude Code. The design leaves room for them (section 9), but they are built only after their
  integration points have been checked.
- Picking the exact Windows Terminal **tab** or VS Code terminal. The first version focuses the right window.
- Claude Code under WSL.
- Anything over the network. The extension does not phone home.

**Anti-goals** (failure even if it works)
- **Approving or denying a permission from the notch.** Toasts can't carry buttons, and a one-click "allow" from a toast
  is the wrong thing to make easy. The notch only takes you to the terminal.
- **Breaking or silently taking over the user's Claude Code settings.** Today's lesson: hooks pointing to a deleted exe.
  Every change to `settings.json` is a merge with a backup. Someone else's status line is never replaced. Uninstalling
  removes what we added.
- **Slowing Claude Code down.** Hooks are async, finish within milliseconds, never block, and always exit 0.
- **Getting around the notch's security model.** For example, starting programs through Explorer to escape the
  extension's process limit. If the notch must change, we ask its maintainer (section 8).

## 3. Shape

One program, `hesewake-agents.exe`, in four modes:

| Mode | Started by | Job |
|---|---|---|
| (none) | the notch, as an extension | Keeps the sessions, talks to the notch, launches and focuses terminals |
| `--hook claude` | Claude Code, for every hook event | Reads the event from stdin and hands it to the running extension |
| `--statusline claude` | Claude Code, for its status line | Prints the status line and hands the usage numbers to the extension |
| `--disconnect-all` | the uninstaller | Removes our hooks and status line from every agent's settings |

```
Claude Code ──hook──▶ hesewake-agents --hook claude ──WM_COPYDATA──▶ hesewake-agents (extension) ──pipe──▶ notch
            ──status line──▶ hesewake-agents --statusline claude ─┘        │
                                                                    launch / focus terminals
```

**Hook to extension: `WM_COPYDATA`** to a message-only window of class `HesewakeAgentsWindow` that the extension
creates. The code is reused from `claude_hook.rs`, where it is proven: `find_notch`, `send` and `decode` become
`find_ext`, `send` and `decode`. The hook finds the window with `FindWindowExW(HWND_MESSAGE, …)`. It sends only if
that window's process image is our own exe, so session data never goes to a look-alike window. The send uses
`SendMessageTimeoutW` with `SMTO_ABORTIFHUNG` and 500 ms. With no extension running, the hook exits silently with code
0. Builder's call: a user-only named pipe is fine instead, if it turns out simpler.

**Session snapshot.** The extension saves its sessions to `sessions.json` when they change, and loads them at start,
dropping sessions whose process is gone. A restart of the notch or the extension does not lose the picture.

## 4. Session model (agent-neutral)

```
Session { agent, id, project, cwd, branch?, state, since, turn_started?, tool?, text?, model?, context_pct?,
          window?, pid?, subagent_seen }
State   = Working | NeedsYou | Answered | Done | Error | Idle
Usage   { agent, five_hour?: {pct, resets_at}, seven_day?: {pct, resets_at}, at }
```

The state machine is `claude.rs Sessions::apply/tick/clear_needs`, moved as it is. It is pure, and its "alive" and
"running" checks are closures. The rules:

| Event | Result |
|---|---|
| prompt | Working, turn timer starts |
| tool | Working (a new turn after Done/Error), remembers the tool name |
| needs-you | NeedsYou, toast |
| stop / stop-failure | Done / Error, toast with the duration (none if that session's window is in front and not shared) |
| idle | Idle, only from Working/Answered (Esc and Ctrl+C send no stop) |
| session-end | removed |
| NeedsYou while its window comes to the front (not shared), or after its row/toast is clicked | Answered |
| silent for more than 15 min while Working/Answered, window gone, or process gone | removed |

Subagent events update the session without changing its state, except needs-you. At most 16 sessions, the oldest
removed. New to the old code:

- `tool` (the `tool_name` of PreToolUse, e.g. "Bash", "Edit").
- `branch`, read by the extension from `<cwd>/.git/HEAD`, never by running git.
- `model` and `context_pct` per session, from the status line (section 5.2), matched by `session_id`.

## 5. Claude Code adapter

### 5.1 Hooks

These are the old ones, with the new exe and arguments: `{"type":"command","command":"<exe>","args":["--hook","claude"],"async":true,"timeout":10}`.

- Events and no matcher: UserPromptSubmit, Stop, StopFailure, SessionStart, SessionEnd.
- PreToolUse and PostToolUse: matcher `*`.
- Notification: matcher `permission_prompt|elicitation_dialog|elicitation_url_dialog|agent_needs_input|idle_prompt`.

Parsing is `parse_hook` as it is, plus `tool_name`. That means:
- stdin up to 16 MB;
- never panic (release builds abort);
- `preview()` keeps the first meaningful line, at most 200 characters.

Window and pid come from the process tree, using `ancestors`, `claude_pid` and `terminal_window`. The variables of the
parent session are removed before any launch (`PARENT_SESSION_VARS`).

### 5.2 Status line

`model.display_name`, `context_window.used_percentage`, and `rate_limits.five_hour` / `rate_limits.seven_day`
(`used_percentage`, `resets_at`), as `parse_status` reads them now, plus `session_id`. It prints the same line as
before, for example `Opus 4.7 · ctx 42% · 5h 41% (reset 15:20) · 7d 18% (reset Thu 14:33)`.

Usage is saved in `usage.json`, newest `at` wins. Warnings fire once per `resets_at`, at 80% (row tone only) and 90%
(toast).

### 5.3 Setup (connect, fix, remove)

`claude_setup.rs`, moved:
- the path comes from `CLAUDE_CONFIG_DIR`, otherwise `~/.claude/settings.json`;
- an invalid file is never written;
- a one-time backup is made, and writes go to a temp file that is renamed over the original;
- someone else's `statusLine` is **taken** and left alone. The panel then says "Status line used by another tool", and
  usage is unavailable;
- the status line command uses forward slashes, and an 8.3 short path only when it removes spaces.

Changes from the old code:
- **Recognising ours**: `hesewake-agents.exe` with `--hook` / `--statusline`, or the **old** `hesewake-notch*.exe`
  with `--claude-hook` / `--claude-statusline`. Old entries show as "Old Hesewake hooks: replace", and Connect replaces
  them.
- **Connect is always the user's click** (the `setup` button or the panel). Installing never edits `settings.json`.
- **Link state on start**: Off, On, Moved (exe path changed → "Fix") or Invalid. Shown in the panel and in `status`.

## 6. In the notch

### 6.1 Slide (protocol v1, no host changes needed)

- **Title** "Coding |Agents".
- **Action**: `New` (≤ 16 characters).
- **Sections** (at most 4) in this order:
  1. **Needs you**
  2. **Working**
  3. **Recent**: Done, Error and Idle, newest first. Done sessions older than 2 h are dropped from the list.
  4. **Usage**
- **Session rows** (text ≤ 120, sub ≤ 40):

  | State | `text` | `sub` | `tone` |
  |---|---|---|---|
  | NeedsYou | `hesewake-notch · main` | `Claude · Needs you · 2m` | `warn` |
  | Working | same | `Claude · Working 3m · Bash` | none |
  | Done | same | `Claude · Done 4m ago` | `good` |
  | Error | same | `Claude · Error 1m ago` | `warn` |
  | Idle | same | `Claude · Idle 12m` | `muted` |

  The badge (`mark`) is set on rows that need you.
- **Usage rows**: `text` `Claude · 5h 62% · reset 14:00`, `sub` `7d 18% · reset Thu`. `tone` is `warn` at 80% or more.
  Without numbers the sub says "Connect the status line in Settings" or "After the first reply".
- **States**:
  - `setup` while Claude Code isn't connected: "Connect Claude Code to see its sessions here", with the button
    `Connect`.
  - `empty` when connected but there are no sessions: "No agent sessions. Start one with New."
- **foot**: "Privacy mode" while privacy mode is on.

### 6.2 Session detail (row click)

- `title`: the project.
- `meta` (at most 6): Agent, State, Time, Branch, Model, Context. In privacy mode, Folder is left out, and it isn't
  shown when space runs out.
- `body`: the needs-you message or the final reply preview. Hidden in privacy mode.
- `buttons` (at most 4):
  - `Open terminal`, if a window is known;
  - `Continue here`: `claude --continue` in the session's folder, offered only for Done/Error/Idle;
  - `Copy path`: `copy`, hidden in privacy mode;
  - `Dismiss`: removes the row.
- Clicking the row or the toast of a NeedsYou session also marks it Answered (`clear_needs`).

### 6.3 New session

`New` opens a list page with `back: true`:
- the recent folders (at most 8, from `recent.json`; text is the folder name, sub is the shortened path, or "hidden" in
  privacy mode);
- `Other folder…` (an `IFileOpenDialog` folder picker run by the extension).

Clicking a folder opens a detail page with `New session` and `Continue last` buttons. With more than one adapter
installed later, there is one button per agent.

Launching uses `find_on_path` and `launch_plan` from the old code:
- absolute paths only;
- Windows Terminal first, then `cmd /k`;
- `NoDefaultCurrentDirectoryInExePath=1`;
- npm shims only through our own cmd.

### 6.4 Toasts, badge, status

| When | Title (≤ 80) | Sub (≤ 60) |
|---|---|---|
| NeedsYou | `hesewake-notch needs you` | `Claude · Bash: cargo test` (privacy: `Claude · Permission`) |
| Done | `Done · hesewake-notch` | `Claude · 4m 12s` |
| Error | `Stopped with an error` | `hesewake-notch · Claude` |
| 5h usage ≥ 90% | `Claude usage 90%` | `5-hour limit · reset 14:00` |

- **Rate limit.** The notch allows one toast per 5 s per extension. The extension queues them, and puts needs-you
  first. If more are waiting, it merges them ("2 sessions need you").
- **Badge** on while any session needs you and the slide hasn't been seen.
- **`status`**, for example "Claude connected · 2 sessions · 1 needs you" or "Claude not connected".

### 6.5 Settings panel (`"settings": "panel"`)

One page:
- **Claude Code**: Connect / Fix / Remove / Replace old hooks, plus status line state.
- **Privacy mode**: on/off.
- **Toasts**: needs-you, done and usage, each on/off.
- **Terminal**: Windows Terminal or Command Prompt.

Toggles are rows with `sub` On/Off and a tone. A click sends `row`, and the page is sent again.

### 6.6 Privacy mode (my call on the details)

Off by default. When on:
- no message or reply text anywhere (rows, detail, toasts);
- no paths, only folder names;
- no `Copy path`.

Project folder names stay, because without them the list says nothing. Hiding those too ("Project 1") is a possible
later option.

### 6.7 Language

English and Indonesian, chosen from `welcome.lang` and `lang`. Every label is checked against the protocol limits in
both languages by a test, as the Odoo extension does.

## 7. Packaging and data

- **Install folder**: `%LOCALAPPDATA%\Programs\Hesewake Agents\hesewake-agents.exe`. Inno Setup, lowest privileges, like
  `HesewakeOdoo-Setup`.
- **Manifest** `%APPDATA%\hesewake-notch\extensions\agents\extension.json`: `id` `agents`, `name` `Coding Agents`,
  `slide: true`, `settings: "panel"`, `processes: "allow"` (launching terminals; the description says so).
- **Data** in `%APPDATA%\hesewake-notch-agents\`: `config.json`, `recent.json`, `usage.json` and `sessions.json`.
- **Uninstaller** runs `hesewake-agents.exe --disconnect-all` before deleting files, then removes the manifest. It asks
  whether to delete the data.
- **CI** gets its own workflow like `odoo-extension.yml`: test, build, installer, artifact. Smart App Control blocks
  local `cargo` on the user's PC, so CI builds.

## 8. Notch changes to propose to the maintainer (an issue, like #67)

The extension works without any of these. Each one makes it better. The notch announces what it supports in `welcome`,
and the extension adapts.

| # | Change | Why | Without it |
|---|---|---|---|
| H0 | `welcome.caps`: a list such as `["live","focus","breakaway","icon","tag"]` | so extensions can adapt | the extension assumes none |
| H1 | **`live`** message: a short text (≤ 24) with a tone, shown in the **small notch** and rotating with weather and prayer. `{"t":"live","text":"1 needs you","tone":"warn"}`, cleared with an empty text. The notch draws it with its own icon and may animate "working". Hovering it opens the slide. | goal 9: being seen without opening the notch | only toasts and the badge |
| H2 | **Foreground permission on click**: when the notch forwards a click (`row`, `action`), it first calls `AllowSetForegroundWindow(<ext pid>)` | Windows doesn't let a background program bring a window to the front; the click is the user's, made in the notch | `Open terminal` may only flash the taskbar button |
| H3 | **Breakaway for `"processes": "allow"`**: add `JOB_OBJECT_LIMIT_BREAKAWAY_OK` to the job when the manifest allows processes, so an extension can start a program with `CREATE_BREAKAWAY_FROM_JOB` | today the job has `KILL_ON_JOB_CLOSE`: a terminal started by an extension **dies when the notch closes or the extension restarts**, taking the Claude session with it | `New` / `Continue` are hidden, and the slide shows the folder and a `Copy` command instead |
| H4 | **Built-in icon** in the manifest: `"icon": "agent"`, chosen from icons the notch draws itself (no images) | goal 8: a neutral, recognisable slide icon | the generic extension icon |
| H5 | **Row `tag`**: `{"text":"Claude","color":"orange"}` from a fixed palette of 6 names | goal 8: coloured agent labels | the agent name stays in `sub` text |

Priority: H3 and H2 (they make goals 2 and 6 work), then H1, then H4/H5. H3 widens only what `"processes": "allow"`
already allows, and only for programs the extension starts itself on purpose.

## 9. Other agents (later)

An adapter supplies:
- **identity**: an id, a display name and a short label;
- **setup**: link state, connect, fix and disconnect;
- **event parsing**: hook or notify input, turned into the common events;
- **usage, if any**;
- **launch commands**: new and continue;
- **process-name rules** for finding its window.

The UI shows only what an adapter supplies: no usage rows if the agent has no usage.

**Codex CLI** and **Antigravity** are research tasks. Before writing an adapter, check what each one offers:
- for Codex, the `notify` program setting, hooks if any, the session files in `~/.codex/sessions`, and limits;
- for Antigravity, its CLI and any event hooks.

Nothing about them in this document is verified.

## 10. Security

- **Hook input is untrusted.** Parsers are bounded and never panic. Everything sent to the notch is already clipped to
  the protocol limits.
- **`WM_COPYDATA` is checked both ways.** The hook sends only to a window whose process is our exe. The extension
  accepts only its two ids, a bounded size and `v` 1. Another program of the same user could still fake events; that is
  the same boundary as the notch's own (`docs/extensions.md` §8).
- **Launching** uses absolute paths only, never a `claude.cmd` from the project folder, and never takes text from
  untrusted input as a command. Folders come from hook `cwd`s and the picker, and are checked to exist.
- **No network. No secrets.** Prompt and reply previews stay on the PC and are hidden in privacy mode.

## 11. Phases

1. **Core**:
   - the crate and pipe client (from the Odoo extension);
   - the message-only window and hook mode;
   - the session model;
   - the slide, detail, toasts, badge and status;
   - the settings panel with connect, fix, remove and privacy;
   - the installer and uninstaller cleanup.
2. **Usage and quick start**:
   - the status line;
   - usage rows and warnings;
   - recent folders, `New` and `Continue`, using H3 when the notch has it;
   - `Open terminal`, using H2 when the notch has it.
3. **Notch issue**: H0 to H5, filed upstream once phase 1 shows the need. Each change is adopted in the extension as
   it lands.
4. **More agents**: research first, then one adapter at a time.

## 12. Testing

- **Unit tests** for:
  - hook and status line parsing (including huge and broken input);
  - state machine transitions (the old tests, moved);
  - settings merge (ours, old ours, someone else's status line, invalid file, Moved);
  - privacy-mode output;
  - every page and toast checked against the protocol limits in EN and ID.
- **An `#[ignore]` end-to-end test** that starts a notch and checks the slide arrives.
- **Manual trial on the user's PC with a CI-built installer**: a real Claude session, then permission, done, usage and
  Open terminal.

## 13. Left to the builder

- Named pipe instead of `WM_COPYDATA` for hook delivery (section 3), if simpler.
- How long Done and Error stay in Recent (2 h suggested).
- Exact wording, as long as it fits the limits in both languages.
- How the focus fallback behaves without H2: restore and flash, or just flash.
