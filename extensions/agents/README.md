# Coding Agents for Hesewake Notch

An extension for [Hesewake Notch](https://github.com/dirgaastya/hesewake-notch) (0.13.0 or later) that shows your
Claude Code sessions on the notch, so you can see which one is waiting for you without going through every terminal.

## What it shows

- **Needs you**: sessions waiting for a permission or an answer, oldest first, with a dot on the row. A toast says
  which project and what it asks.
- **Working**: sessions running a turn, with how long they have been at it.
- **Recent**: sessions that finished, stopped with an error or have been idle, newest first. A finished turn gives a
  toast with how long it took.

Open a row to see the session's page: agent, state, time, branch, the tool it was using, its folder, and the last
message. **Copy path** copies the folder; **Dismiss** takes the session off the list. A dot on the collapsed notch
means a session needs you; it goes away when you open the slide or nothing is waiting any more.

Several sessions that finish at once become one toast ("3 sessions finished"). The notch shows at most one toast from
an extension every 5 seconds.

## Install and connect

1. Run `HesewakeAgents-Setup-<version>.exe`. It installs for you only, without administrator rights.
2. In Hesewake Notch: **Settings > Extensions**, press **Rescan** and switch **Coding Agents** on.
3. On the Coding Agents slide, press **Connect**.

Installing never changes Claude Code's settings. Only **Connect** does.

## What Connect changes

Connect adds one hook entry to each of these eight events in Claude Code's user settings
(`%USERPROFILE%\.claude\settings.json`, or the folder in `CLAUDE_CONFIG_DIR`):
`UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `Notification`, `Stop`, `StopFailure`, `SessionStart` and
`SessionEnd`. Each runs `hesewake-agents.exe --hook claude` asynchronously, so Claude Code never waits for it.

- Everything else in the file stays as it is. Before the first change the file is copied once to
  `settings.json.hesewake-backup`.
- If the file can't be read as JSON, nothing is written and the slide says so.
- Hooks from the old built-in Claude Code feature of Hesewake Notch are found and replaced.
- If the exe moves, the slide offers **Fix**.

**Remove** (in the extension's settings) takes our entries out again. **Uninstalling** does the same before the files go
away, and asks whether to delete the extension's own data as well (`%APPDATA%\hesewake-notch-agents`: the session list
and the options).

## Privacy

**Privacy mode** (in the extension's settings) hides message text and folder paths everywhere: rows, the session page
and toasts. Use it when you share your screen or stream.

Nothing goes over the network. The hook passes each event to the extension through a window message on this PC, and
the session list is a file in your own profile.

## Not in this version

The status line with usage (5-hour and 7-day limits), starting a new session or continuing one, recent folders, and
**Open terminal** come in a later version. Only Claude Code is supported for now.

## Building

```
cargo test --release
cargo build --release
ISCC installer.iss
```

The installer needs Inno Setup 6. CI (`.github/workflows/agents-extension.yml`) runs all three.
