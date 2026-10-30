<div align="center">

<img src="icon.ico" alt="Hesewake Notch" width="96" height="96">

# Hesewake Notch

*Personal fork of [Poch](https://github.com/klawcodes/poch) by Commonly Works (MIT).*

**A tiny notch for the top of your Windows screen. Music, clock, water, to-do, file shelf and notes, all in one place.**

![Platform](https://img.shields.io/badge/platform-Windows%2010%20%7C%2011-0078D4?logo=windows&logoColor=white)
![Language](https://img.shields.io/badge/built%20with-Rust-DEA584?logo=rust&logoColor=black)
![License](https://img.shields.io/badge/license-MIT-green)

</div>

---

## <img src="https://api.iconify.design/lucide/info.svg?color=%23ff453a" width="20" align="top"> About

Hesewake Notch sits flush against the top edge of your screen as a small black notch. Hover over it and it expands into a carousel of mini apps. Move away and it shrinks back down. It is a single, lightweight `.exe` with no runtime to install.

- Layered per-pixel alpha window, software rendered with `tiny-skia`
- Small memory footprint: embedded fonts, bounded glyph cache, working set trimmed when idle
- Your data stays local in `%APPDATA%\hesewake-notch`

---

## <img src="https://api.iconify.design/lucide/plus.svg?color=%23ff453a" width="20" align="top"> What this fork adds to Poch

Synced lyrics (with a scrolling lyrics mode), cover art in the notch, a redesigned Water slide, Windows notifications with contact photos, toasts (charger, Bluetooth, Caps/Num/Scroll Lock, microphone/camera in use), volume control by scrolling, a **Settings** slide (show/hide/reorder slides, display and feature toggles), a tray menu, a blue theme, and an installer. See [CHANGELOG.md](CHANGELOG.md) for details.

---

## <img src="https://api.iconify.design/lucide/layout-grid.svg?color=%23ff453a" width="20" align="top"> Slides

Swipe, scroll, drag, use the arrow buttons or click the dots to move through an endless carousel.

|                                           Icon                                            | Slide     | What it does                                                                                                                                                      |
| :---------------------------------------------------------------------------------------: | --------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- |
|    <img src="https://api.iconify.design/lucide/music.svg?color=%23ff453a" width="22">     | **Music** | Now Playing from Spotify, browsers and other media apps, with artwork, progress bar and playback controls. The clock and date sit on the right of the same slide. |
|   <img src="https://api.iconify.design/lucide/droplets.svg?color=%23ff453a" width="22">   | **Water** | Daily water tracker with quick add buttons, a daily goal (1.5 to 3.0 L) and optional reminders.                                                                   |
| <img src="https://api.iconify.design/lucide/list-checks.svg?color=%23ff453a" width="22">  | **To-do** | Simple checklist with add, edit and delete. Scrolls when the list gets long.                                                                                      |
| <img src="https://api.iconify.design/lucide/folder-open.svg?color=%23ff453a" width="22">  | **Ledge** | A shelf for up to 5 files or folders. Drop them in, click to open, drag them back out (copy by default, hold `Shift` to move).                                    |
| <img src="https://api.iconify.design/lucide/notebook-pen.svg?color=%23ff453a" width="22"> | **Scrap** | Mini notepad, up to 3,333 characters, with caret, selection, clipboard and a live character counter.                                                              |
| <img src="https://api.iconify.design/lucide/clipboard-list.svg?color=%23ff453a" width="22"> | **Clipboard** | Optional (turn it on in Settings > Slides). The last 10 texts you copied in any app; click one to copy it again. Kept in memory only, and copies that password managers mark as private are skipped. |
| <img src="https://api.iconify.design/lucide/calendar-days.svg?color=%23ff453a" width="22"> | **Agenda** | Optional. What is left on today's calendar, with a **Join** button for Meet / Zoom / Teams links. Needs the calendar link (see Calendar below). |

---

## <img src="https://api.iconify.design/lucide/sparkles.svg?color=%23ff453a" width="20" align="top"> Features

|                                          Icon                                           | Feature        | Details                                                                                                             |
| :-------------------------------------------------------------------------------------: | -------------- | ------------------------------------------------------------------------------------------------------------------- |
|   <img src="https://api.iconify.design/lucide/music.svg?color=%23ff453a" width="22">    | Media control  | Reads Windows media sessions (GSMTC). Artwork comes from the session, with a fallback to the iTunes Search API.     |
|   <img src="https://api.iconify.design/lucide/clock.svg?color=%23ff453a" width="22">    | Idle clock     | When nothing is playing, the notch shows the time, then switches to a blinking eye with a red glow, then loops.     |
| <img src="https://api.iconify.design/lucide/minimize-2.svg?color=%23ff453a" width="22"> | Busy mode      | With a maximized or fullscreen window (or Adobe apps in front), the notch shrinks to a thin line. Hover to open it. |
|  <img src="https://api.iconify.design/lucide/keyboard.svg?color=%23ff453a" width="22">  | Hotkey         | `Ctrl` + `Alt` + `H` toggles between the clock and the eye.                                                         |
| <img src="https://api.iconify.design/lucide/mic-off.svg?color=%23ff453a" width="22"> | Mic mute       | `Ctrl` + `Alt` + `M` mutes the default microphone for every app, and again to unmute. While muted, a crossed-out mic stays in the notch, also when the mute was made elsewhere. Turn the key off in Settings > Alerts > Mic mute key. |
|   <img src="https://api.iconify.design/lucide/waves.svg?color=%23ff453a" width="22">    | Drop animation | Dropping a file onto the notch makes its bottom edge ripple like water.                                             |
|  <img src="https://api.iconify.design/lucide/palette.svg?color=%23ff453a" width="22">   | Artwork tint   | The Music slide picks up colors from the album cover for a soft glow.                                               |
| <img src="https://api.iconify.design/lucide/languages.svg?color=%23ff453a" width="22"> | Romanize lyrics | Optional (Settings > Widgets): Japanese lyrics shown in romaji (Lindera IPADIC and WanaKana) and Korean lyrics in Revised Romanization (no dictionary needed). |
| <img src="https://api.iconify.design/lucide/globe.svg?color=%23ff453a" width="22"> | Language       | English or Indonesian (Settings > General > Language). Applies right away, including the right-click menu.     |
| <img src="https://api.iconify.design/lucide/download.svg?color=%23ff453a" width="22"> | Update check   | At startup and every 24 hours the app asks GitHub for the latest release and shows a toast once per new version, with **Download** to open the release page. Nothing is installed automatically. Right-click menu: **Check for updates**. Turn it off in Settings > Alerts > Update notice. |
| <img src="https://api.iconify.design/lucide/calendar-clock.svg?color=%23ff453a" width="22"> | Calendar       | Settings > Widgets > Calendar: copy your calendar's secret ICS address (Google: "Secret address in iCal format", Outlook: "Publish calendar"), then press **Paste**. The notch counts down the last 10 minutes before a meeting and shows a toast with **Join** when it starts. The link is kept in Windows Credential Manager (not in a file) and the feed is refreshed every 10 minutes. |
| <img src="https://api.iconify.design/lucide/square-terminal.svg?color=%23ff453a" width="22"> | Claude Code    | Settings > Alerts > Claude Code: press **Connect** to add the notch's hooks to Claude Code's user settings (`~/.claude/settings.json`; your other settings stay, and the original file is kept once as `settings.json.hesewake-backup`). New Claude Code sessions then show a small `>_` chip while Claude works, an orange toast and mark when it needs your permission or an answer, and a toast with the time taken and the first line of its reply when it is done. Click a toast to bring that terminal to the front. Several sessions in one window (terminal tabs) bring the window, not the tab. Claude Code in WSL is not supported. **Remove** takes the hooks out again; uninstalling does too. Nothing leaves your PC. |

---

## <img src="https://api.iconify.design/lucide/download.svg?color=%23ff453a" width="20" align="top"> Install

| Requirement                             | Version                                                           |
| --------------------------------------- | ----------------------------------------------------------------- |
| Operating system                        | Windows 10 or Windows 11                                          |
| Build tools (only to build from source) | [Rust](https://www.rust-lang.org/tools/install) (stable)          |
| Installer (optional)                    | [Inno Setup](https://jrsoftware.org/isinfo.php), using `hesewake-notch.iss` |

---

## <img src="https://api.iconify.design/lucide/hammer.svg?color=%23ff453a" width="20" align="top"> Build from source

```powershell
git clone <your-repo-url>
cd hesewake-notch
cargo build --release
```

The executable is created at `target\release\hesewake-notch.exe`. Japanese romaji needs the Lindera IPADIC dictionary: press **Get** on the "Japanese dictionary" row in Settings > Widgets (it goes to `%APPDATA%\hesewake-notch\dict\ipadic`), or for development run `scripts\fetch-dict.ps1` to put it in the repo's `dict\ipadic`. To make a setup installer, open `hesewake-notch.iss` in Inno Setup and compile it.

---

## <img src="https://api.iconify.design/lucide/folder-tree.svg?color=%23ff453a" width="20" align="top"> Project layout

| File                                         | Purpose                                                            |
| -------------------------------------------- | ------------------------------------------------------------------ |
| `main.rs`                                    | App state, window creation, message loop                           |
| `window.rs`                                  | Win32 window procedure (mouse, keyboard, timer, hotkey, file drop) |
| `render.rs`, `gfx.rs`                        | Frame drawing, notch shape, text and shape helpers                 |
| `anim.rs`                                    | Spring animation, timers, hover, busy and media polling            |
| `music.rs`                                   | Media thread, artwork and artwork tint, Music slide                |
| `clock.rs`                                   | Clock and date helpers                                             |
| `water.rs`, `todo.rs`, `shelf.rs`, `note.rs` | Water, To-do, Ledge and Scrap slides                               |
| `i18n.rs`                                    | UI language: English text as the key, Indonesian table             |
| `clipboard.rs`                               | Clipboard history slide and clipboard listener                     |
| `ics.rs`, `calendar.rs`                      | iCalendar reader (time zones, recurring events), calendar feed, Agenda slide |
| `claude.rs`, `claude_hook.rs`, `claude_setup.rs` | Claude Code sessions in the notch, the `--claude-hook` bridge process, Connect/Remove in `settings.json` |
| `update.rs`                                  | Update check against the latest GitHub release                     |
| `dragdrop.rs`                                | OLE drag out of the Ledge                                          |
| `eye.rs`, `busy.rs`, `icon.rs`               | Eye animation, busy detection, app icons                           |
| `nav.rs`, `input.rs`, `config.rs`, `data.rs` | Carousel, clicks and menu, shared sizes, data storage              |

---

## <img src="https://api.iconify.design/lucide/database.svg?color=%23ff453a" width="20" align="top"> Your data

Everything is saved as plain text in `%APPDATA%\hesewake-notch`:

| File          | Content                                  |
| ------------- | ---------------------------------------- |
| `todo.txt`    | To-do items                              |
| `water.txt`   | Water intake, goal and reminder settings |
| `shelf.txt`   | Files on the Ledge                       |
| `notepad.txt` | Scrap note                               |

Right click the notch for water options and to exit the app.

---

## <img src="https://api.iconify.design/lucide/scale.svg?color=%23ff453a" width="20" align="top"> License

Released under the [MIT License](LICENSE).

**Hesewake Notch is a personal fork of [Poch](https://github.com/klawcodes/poch)** (c) 2026 **Commonly Works**, used under the MIT License. The original copyright notice is kept in [LICENSE](LICENSE), as the license requires; the modifications in this fork are (c) 2026 dirga and released under the same terms.

> Copyright (c) 2026 **Commonly Works**

Any copy or fork, whether source or compiled, must keep the [LICENSE](LICENSE) file with this credit.

The embedded font is **Instrument Sans**, used under its own open font license.

---

<div align="center">

Made by **Commonly Works**

</div>
