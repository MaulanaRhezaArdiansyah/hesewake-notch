# Claude Code in the notch: phase 2 design

Date: 2026-10-30. Builds on `2026-10-29-claude-code-integration-design.md` (phase 1, built). Fork only for now: no
upstream issue or PR until the person says so.

## Decisions taken with the person

- Status line text in the terminal: **model · context · usage**, for example
  `Opus 4.7 · ctx 42% · 5h 41% (reset 15:20) · 7d 18%`. Parts that are missing are left out.
- Usage in the notch: **in the Claude slide, plus one warning toast** when the 5-hour window passes 90%. The small
  notch stays as it is.

## Facts this relies on (code.claude.com/docs/en/statusline.md, read 2026-10-30)

- `statusLine` in `~/.claude/settings.json`: `{ "type": "command", "command": "<shell command>", "padding": 0 }`. No
  `args`: the command is a shell string. On Windows it runs through Git Bash when installed, else PowerShell. Paths must
  use forward slashes; a path with spaces needs quotes (bash) and `&` (PowerShell), so the 8.3 short path is used when
  it has no spaces.
- It runs at session start and after each assistant message (debounced 300 ms), and when a `resets_at` passes. A
  still-running command is cancelled when the next update comes: it must be fast.
- stdin JSON: `model.display_name`, `context_window.used_percentage` (may be null), `session_id`,
  `rate_limits.five_hour` / `seven_day` with `used_percentage` (0-100) and `resets_at` (unix seconds). `rate_limits`
  is only there for Pro/Max (or a gateway) and only after the session's first API response; each window may be
  missing on its own.

## 1. Usage

- `hesewake-notch.exe --claude-statusline` (before the single-instance check, no window): reads stdin (capped at
  1 MB), prints the one line, and when there are rate limits sends them to the notch with `WM_COPYDATA` (its own
  `dwData`, same "only a window of this exe" rule as the hooks). Never panics, always exits 0. Invalid JSON prints
  nothing.
- The notch keeps the newest numbers. When the 5-hour window reaches 90% it shows one toast ("Claude usage 91%",
  "5-hour limit · resets 15:20") per window (keyed by its `resets_at`). A window whose `resets_at` has passed shows
  as reset.
- Settings › Alerts › **Claude usage** (shown while Claude Code is connected): a toggle that adds or removes our
  `statusLine`. When the person already has a status line of their own, it is never replaced: the row says so and the
  toggle does nothing. Remove (Claude Code row) and the uninstaller also remove our status line. A moved exe is fixed
  together with the hooks.

## 2. Claude slide (optional, off by default)

- Header "Claude Code", and **+ New session** at the top right.
- Left: the sessions (3 rows, the wheel scrolls): a coloured dot (working accent, needs you orange, done green, error
  red, idle grey), the project, the state ("Working · 3m", "Needs you", "Done", "Error", "Idle") and **Open** (brings
  its window to the front) when the window is known. A click on a row does the same as Open.
- Right: **Usage**: 5h and 7d, each with the percentage, a thin bar and when it resets ("reset 15:20", "reset in 3d").
  Without numbers: "Turn on in Settings" (status line off) or "After Claude's first reply" (on, nothing yet).
- No sessions: "No Claude Code sessions" and a hint (New session, or "Connect in Settings › Alerts" when not
  connected).
- **New session** opens a list over the sessions: up to 3 recent project folders (from the `cwd`s the hooks report,
  kept in `%APPDATA%\hesewake-notch\claude_recent.txt`, newest first, at most 8) and **Other folder…** (the Windows
  folder picker, on its own STA thread). Picking one opens Windows Terminal there running `claude`
  (`wt.exe -d "<dir>" claude`), or `cmd /k claude` in that folder when Windows Terminal is missing. A click outside
  closes the list.
- Nothing on the slide animates, so it costs no frames.

## Testing

- Unit: status JSON parsing (missing / null / odd values, invalid JSON), the line, the command string (spaces, short
  path, forward slashes), statusLine add / remove / "someone else's" / moved exe in settings.json, usage messages
  (newest wins, one warning per window), recent folders (newest first, no duplicates, cap), the launch command and its
  fallback.
- App-level: slide clicks (Open, New session, a recent folder, Other folder, outside), wheel scroll, the Settings row.
- Snapshots: the slide with sessions in every state and usage, empty, not connected, the New session list, the usage
  toast, the Settings rows; English and Indonesian.
