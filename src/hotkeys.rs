//! Global hotkeys Ctrl+Alt+1..8: open the notch on the Nth enabled slide (the order in Settings > Slides).
//! Pressing the same hotkey again while that slide is showing closes the notch.

use super::*;
use crate::slides::SlideId;
use windows::Win32::UI::Input::KeyboardAndMouse::UnregisterHotKey;

/// Number of slide hotkeys (Ctrl+Alt+1 .. Ctrl+Alt+8).
pub(crate) const SLIDE_KEYS: usize = 8;
/// Hotkey ids 10..=17; id 1 is Ctrl+Alt+H (eye.rs).
const FIRST_ID: i32 = 10;
/// How long the notch stays open after a slide hotkey (longer while hovered).
const PEEK_SECS: u64 = 8;

/// Slide position for a `WM_HOTKEY` id, if it is one of the slide hotkeys.
pub(crate) fn slide_of(id: i32) -> Option<usize> {
    (FIRST_ID..FIRST_ID + SLIDE_KEYS as i32).contains(&id).then(|| (id - FIRST_ID) as usize)
}

/// Register (or release) Ctrl+Alt+1..8. A key another app already holds is skipped and noted in perf.log.
pub(crate) fn set_slide_hotkeys(hwnd: isize, on: bool) {
    if cfg!(test) {
        return; // tests must not grab global hotkeys
    }
    let hwnd = HWND(hwnd as _);
    let mods = HOT_KEY_MODIFIERS(MOD_ALT.0 | MOD_CONTROL.0 | MOD_NOREPEAT.0);
    for i in 0..SLIDE_KEYS {
        let id = FIRST_ID + i as i32;
        let _ = unsafe { UnregisterHotKey(hwnd, id) };
        if on && unsafe { RegisterHotKey(hwnd, id, mods, '1' as u32 + i as u32) }.is_err() {
            perf::note(&format!("hotkey: Ctrl+Alt+{} is already taken by another app; that slide hotkey is off", i + 1));
        }
    }
}

#[derive(Debug, PartialEq)]
enum Action {
    Show,
    Close,
    Ignore,
}

/// What hotkey `n` should do, given the carousel order, whether the notch is open, and the current slide.
fn action(order: &[SlideId], n: usize, expanded: bool, cur: usize) -> Action {
    match order.get(n) {
        None | Some(SlideId::Settings) => Action::Ignore,
        Some(_) if expanded && cur == n => Action::Close,
        Some(_) => Action::Show,
    }
}

/// Text under a slide's name in Settings > Slides: its hotkey, if it has one.
pub(crate) fn hint(cfg: &[(SlideId, bool)], i: usize) -> Option<String> {
    if !cfg.get(i)?.1 {
        return None;
    }
    let pos = cfg[..i].iter().filter(|(_, on)| *on).count();
    (pos < SLIDE_KEYS).then(|| format!("Ctrl+Alt+{}", pos + 1))
}

impl App {
    /// Handle slide hotkey `n` (0-based). True if the notch changed.
    pub(crate) fn slide_hotkey(&mut self, n: usize) -> bool {
        match action(&self.order, n, self.expanded(), self.cur()) {
            Action::Ignore => false,
            Action::Close => {
                self.peek_until = None;
                self.reminding = false;
                true
            }
            Action::Show => {
                self.show(n);
                self.peek_until = Some(Instant::now() + Duration::from_secs(PEEK_SECS));
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use SlideId::*;

    #[test]
    fn ids_map_to_slide_positions() {
        assert_eq!(slide_of(1), None, "Ctrl+Alt+H is not a slide hotkey");
        assert_eq!(slide_of(FIRST_ID), Some(0));
        assert_eq!(slide_of(FIRST_ID + 7), Some(7));
        assert_eq!(slide_of(FIRST_ID + 8), None);
    }

    #[test]
    fn hotkey_shows_then_closes_its_slide() {
        let order = [Music, Todo, Timer];
        assert_eq!(action(&order, 1, false, 0), Action::Show, "closed notch: open on that slide");
        assert_eq!(action(&order, 1, true, 0), Action::Show, "open on another slide: switch");
        assert_eq!(action(&order, 1, true, 1), Action::Close, "same hotkey again: close");
        assert_eq!(action(&order, 1, false, 1), Action::Show, "closed but last on that slide: open");
        assert_eq!(action(&order, 3, false, 0), Action::Ignore, "no 4th slide");
        assert_eq!(action(&[Music, Todo, Settings], 2, false, 0), Action::Ignore, "Settings has no hotkey");
    }

    #[test]
    fn hints_count_only_enabled_slides() {
        let cfg = vec![(Music, true), (Water, false), (Todo, true), (Ledge, true)];
        assert_eq!(hint(&cfg, 0).as_deref(), Some("Ctrl+Alt+1"));
        assert_eq!(hint(&cfg, 1), None, "disabled slide");
        assert_eq!(hint(&cfg, 2).as_deref(), Some("Ctrl+Alt+2"));
        assert_eq!(hint(&cfg, 3).as_deref(), Some("Ctrl+Alt+3"));
        assert_eq!(hint(&cfg, 9), None);
        let many: Vec<_> = SlideId::USER.iter().map(|&s| (s, true)).chain([(Music, true)]).collect();
        assert_eq!(hint(&many, 7).as_deref(), Some("Ctrl+Alt+8"));
        assert_eq!(hint(&many, 8), None, "only 8 hotkeys");
    }
}
