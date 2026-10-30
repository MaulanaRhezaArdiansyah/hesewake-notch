//! Claude Code plan usage (5-hour and 7-day limits). Claude Code only gives these numbers to its status line command,
//! so Settings can set ours: `hesewake-notch.exe --claude-statusline` prints the line under the Claude Code prompt
//! (model, context, usage) and passes the limits to the running notch, the same way the hooks do.

use super::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{Read, Write};

/// `dwData` of the `WM_COPYDATA` message that carries a `Usage`.
pub(crate) const COPY_USAGE: usize = 0x434C_5553;
pub(crate) const VERSION: u8 = 1;
/// The status line JSON is small; anything beyond this is not read.
const MAX_STDIN: u64 = 1024 * 1024;
/// The 5-hour window warns once it reaches this.
pub(crate) const WARN_PCT: f32 = 90.0;

/// One rate-limit window: percentage used (0-100) and when it resets (unix seconds).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub(crate) struct Window {
    pub(crate) pct: f32,
    pub(crate) resets_at: i64,
}

/// The limits as sent to the notch.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub(crate) struct Usage {
    pub(crate) v: u8,
    pub(crate) five: Option<Window>,
    pub(crate) seven: Option<Window>,
    /// When the status line ran (unix seconds): the newest numbers win.
    pub(crate) at: i64,
}

/// What the status line shows.
#[derive(Clone, Debug, PartialEq, Default)]
pub(crate) struct Status {
    pub(crate) model: String,
    /// Context window used, in percent (missing early in a session).
    pub(crate) ctx: Option<f32>,
    pub(crate) usage: Usage,
}

fn window(v: &Value) -> Option<Window> {
    let pct = v.get("used_percentage")?.as_f64()?;
    let resets_at = v.get("resets_at")?.as_f64()?;
    (pct.is_finite() && resets_at.is_finite()).then(|| Window { pct: pct.clamp(0.0, 1000.0) as f32, resets_at: resets_at as i64 })
}

/// The status line JSON from Claude Code. None when it is not a JSON object.
pub(crate) fn parse_status(json: &str, now: i64) -> Option<Status> {
    let v: Value = serde_json::from_str(json).ok()?;
    if !v.is_object() {
        return None;
    }
    let model = v.pointer("/model/display_name").and_then(Value::as_str).unwrap_or_default();
    let ctx = v.pointer("/context_window/used_percentage").and_then(Value::as_f64).filter(|p| p.is_finite()).map(|p| p.clamp(0.0, 100.0) as f32);
    let rl = v.get("rate_limits");
    let usage = Usage {
        v: VERSION,
        five: rl.and_then(|r| r.get("five_hour")).and_then(window),
        seven: rl.and_then(|r| r.get("seven_day")).and_then(window),
        at: now,
    };
    Some(Status { model: model.chars().filter(|c| !c.is_control()).take(40).collect(), ctx, usage })
}

/// The line under the Claude Code prompt: "Opus 4.7 · ctx 42% · 5h 41% (reset 15:20) · 7d 18%". `offset` is the local
/// time zone in seconds east of UTC.
pub(crate) fn status_line(s: &Status, offset: i32) -> String {
    let mut parts = Vec::new();
    if !s.model.is_empty() {
        parts.push(s.model.clone());
    }
    if let Some(c) = s.ctx {
        parts.push(format!("ctx {c:.0}%"));
    }
    if let Some(w) = s.usage.five {
        parts.push(format!("5h {:.0}% (reset {})", w.pct, calendar::hhmm(w.resets_at, offset)));
    }
    if let Some(w) = s.usage.seven {
        parts.push(format!("7d {:.0}% (reset {})", w.pct, when(w.resets_at, s.usage.at, offset)));
    }
    parts.join(" \u{b7} ")
}

/// Day of the week of `t` (unix seconds) at `offset`. 1 January 1970 was a Thursday.
fn weekday(t: i64, offset: i32) -> &'static str {
    const DAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    DAYS[(t + offset as i64).div_euclid(86_400).rem_euclid(7) as usize]
}

/// "18:10" when `t` is today, else "Thu 18:10".
fn when(t: i64, now: i64, offset: i32) -> String {
    let day = |x: i64| (x + offset as i64).div_euclid(86_400);
    if day(t) == day(now) {
        calendar::hhmm(t, offset)
    } else {
        format!("{} {}", weekday(t, offset), calendar::hhmm(t, offset))
    }
}

pub(crate) fn encode(u: &Usage) -> Vec<u8> {
    serde_json::to_vec(u).unwrap_or_default()
}

pub(crate) fn decode(b: &[u8]) -> Option<Usage> {
    if b.len() > 4096 {
        return None;
    }
    serde_json::from_slice::<Usage>(b).ok().filter(|u| u.v == VERSION)
}

const SAVED: &str = "claude_usage.json";

/// The last numbers, saved so the Claude slide shows them right after the notch starts.
pub(crate) fn load() -> Option<Usage> {
    if cfg!(test) {
        return None;
    }
    let text = std::fs::read_to_string(data_path(SAVED)).ok()?;
    decode(text.as_bytes())
}

fn save(u: &Usage) {
    if cfg!(test) {
        return;
    }
    write_data(SAVED, String::from_utf8_lossy(&encode(u)).into_owned());
}

/// The 5-hour window to count as already warned about at start: a saved one at or past `WARN_PCT` (its toast came
/// before the restart).
pub(crate) fn warned_at_start(u: Option<&Usage>) -> i64 {
    u.and_then(|u| u.five).filter(|w| w.pct >= WARN_PCT).map_or(0, |w| w.resets_at)
}

/// `--claude-statusline`: print the line and pass the limits to the running notch. Never fails.
pub(crate) fn run_statusline() {
    let mut buf = Vec::new();
    if std::io::stdin().take(MAX_STDIN).read_to_end(&mut buf).is_err() {
        return;
    }
    let Some(s) = parse_status(&String::from_utf8_lossy(&buf), notif::now_unix() as i64) else { return };
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{}", status_line(&s, calendar::local_offset()));
    let _ = out.flush();
    if s.usage.five.is_some() || s.usage.seven.is_some() {
        let notch = claude_hook::find_notch();
        claude_hook::send(notch, COPY_USAGE, &encode(&s.usage));
    }
}

/// "15:20" today, otherwise "in 3d" / "in 5h".
pub(crate) fn reset_text(resets_at: i64, now: i64, offset: i32) -> String {
    let left = resets_at - now;
    if left <= 0 {
        return String::new();
    }
    let day = |t: i64| (t + offset as i64).div_euclid(86_400);
    if day(resets_at) == day(now) {
        fill(tr("reset {}"), &[&calendar::hhmm(resets_at, offset)])
    } else if left >= 86_400 {
        fill(tr("reset in {}d"), &[&(left / 86_400)])
    } else {
        fill(tr("reset in {}h"), &[&(left / 3600).max(1)])
    }
}

impl App {
    /// New numbers from the status line. The newest wins; the 5-hour window warns once per window at `WARN_PCT`.
    pub(crate) fn on_claude_usage(&mut self, u: Usage) {
        if self.claude_usage.as_ref().is_some_and(|old| old.at > u.at) {
            return;
        }
        if let Some(w) = u.five.filter(|w| w.pct >= WARN_PCT && w.resets_at > notif::now_unix() as i64) {
            if self.claude_warned != w.resets_at {
                self.claude_warned = w.resets_at;
                self.push_toast(ToastKind::Claude(claude::Show::Usage { pct: w.pct.round() as u16, resets_at: w.resets_at }));
            }
        }
        if self.claude_usage.as_ref() != Some(&u) {
            save(&u);
        }
        self.claude_usage = Some(u);
    }

    /// A window's numbers if it has not reset yet.
    pub(crate) fn claude_window(&self, seven: bool, now: i64) -> Option<Window> {
        let u = self.claude_usage.as_ref()?;
        if seven { u.seven } else { u.five }.filter(|w| w.resets_at > now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_793_000_000;

    fn full() -> String {
        format!(
            r#"{{"model":{{"id":"claude-opus-4-7","display_name":"Opus 4.7"}},"session_id":"s1",
               "context_window":{{"used_percentage":42.4,"context_window_size":200000}},
               "rate_limits":{{"five_hour":{{"used_percentage":41.2,"resets_at":{}}},"seven_day":{{"used_percentage":18,"resets_at":{}}}}}}}"#,
            NOW + 3600,
            NOW + 3 * 86_400
        )
    }

    #[test]
    fn reads_model_context_and_limits() {
        let s = parse_status(&full(), NOW).unwrap();
        assert_eq!(s.model, "Opus 4.7");
        assert_eq!(s.ctx, Some(42.4));
        assert_eq!(s.usage.five, Some(Window { pct: 41.2, resets_at: NOW + 3600 }));
        assert_eq!(s.usage.seven.map(|w| w.pct), Some(18.0));
        assert_eq!(s.usage.at, NOW);
    }

    #[test]
    fn missing_or_odd_fields_are_left_out() {
        let s = parse_status(r#"{"model":{"display_name":"Sonnet"},"context_window":{"used_percentage":null}}"#, NOW).unwrap();
        assert_eq!((s.ctx, s.usage.five, s.usage.seven), (None, None, None), "no limits before the first reply");
        let s = parse_status(r#"{"rate_limits":{"five_hour":{"used_percentage":"x","resets_at":1},"seven_day":{"used_percentage":5}}}"#, NOW).unwrap();
        assert_eq!((s.usage.five, s.usage.seven), (None, None), "wrong types and missing resets_at");
        assert_eq!(parse_status("not json", NOW), None);
        assert_eq!(parse_status("[1,2]", NOW), None);
        assert_eq!(parse_status("", NOW), None);
        let s = parse_status(r#"{"model":{"display_name":"Op\u001b[31mus"}}"#, NOW).unwrap();
        assert_eq!(s.model, "Op[31mus", "no control characters reach the terminal");
    }

    #[test]
    fn the_line_has_what_is_known() {
        let s = parse_status(&full(), NOW).unwrap();
        let reset = calendar::hhmm(NOW + 3600, 7 * 3600);
        // NOW + 3 days at UTC+7 is Thursday 14:33
        assert_eq!(status_line(&s, 7 * 3600), format!("Opus 4.7 \u{b7} ctx 42% \u{b7} 5h 41% (reset {reset}) \u{b7} 7d 18% (reset Thu 14:33)"));
        let s = parse_status(r#"{"model":{"display_name":"Haiku"}}"#, NOW).unwrap();
        assert_eq!(status_line(&s, 0), "Haiku");
        assert_eq!(status_line(&Status::default(), 0), "");
    }

    #[test]
    fn weekly_reset_shows_the_day_unless_it_is_today() {
        assert_eq!(weekday(0, 0), "Thu", "1 January 1970");
        assert_eq!(weekday(4 * 86_400, 0), "Mon");
        assert_eq!(weekday(86_400 - 1, 7 * 3600), "Fri", "the local day, not UTC");
        assert_eq!(when(NOW + 60, NOW, 0), calendar::hhmm(NOW + 60, 0), "today: just the time");
    }

    #[test]
    fn usage_messages_roundtrip() {
        let u = parse_status(&full(), NOW).unwrap().usage;
        assert_eq!(decode(&encode(&u)), Some(u.clone()));
        assert_eq!(decode(b"{}"), None);
        assert_eq!(decode(&[b'x'; 5000]), None, "too long");
        let old = Usage { v: 0, ..u };
        assert_eq!(decode(&encode(&old)), None, "other version");
    }

    #[test]
    fn a_saved_window_past_90_was_already_warned_about() {
        let u = parse_status(&full(), NOW).unwrap().usage;
        assert_eq!(warned_at_start(Some(&u)), 0, "41%: nothing warned yet");
        let mut hot = u.clone();
        hot.five = Some(Window { pct: 93.0, resets_at: NOW + 600 });
        assert_eq!(warned_at_start(Some(&hot)), NOW + 600, "no second toast for the same window after a restart");
        assert_eq!(warned_at_start(None), 0);
    }

    #[test]
    fn reset_times_read_naturally() {
        let off = 7 * 3600;
        let midnight = (NOW + off as i64).div_euclid(86_400) * 86_400 - off as i64;
        let later_today = midnight + 23 * 3600;
        if later_today > NOW {
            assert_eq!(reset_text(later_today, NOW, off), format!("reset {}", calendar::hhmm(later_today, off)));
        }
        assert_eq!(reset_text(NOW + 3 * 86_400 + 60, NOW, off), "reset in 3d");
        assert_eq!(reset_text(NOW - 1, NOW, off), "", "already reset");
    }
}
