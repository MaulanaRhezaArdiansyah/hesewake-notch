//! Microphone mute. Ctrl+Alt+M mutes the default capture device for every app (`IAudioEndpointVolume::SetMute`),
//! and the notch follows mute changes made anywhere else (Windows settings, a keyboard mic key, another app).
//! Windows has a default capture device for two roles (console and communications); when they are different
//! devices both are muted, so no sound goes out whichever one a meeting app uses.

use super::*;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Sender};
use windows::core::{implement, PCWSTR as WPCWSTR};
use windows::Win32::Media::Audio::Endpoints::{IAudioEndpointVolume, IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl};
use windows::Win32::Media::Audio::{
    eCapture, eCommunications, eConsole, EDataFlow, ERole, IMMDeviceEnumerator, IMMNotificationClient, IMMNotificationClient_Impl,
    MMDeviceEnumerator, AUDIO_VOLUME_NOTIFICATION_DATA, DEVICE_STATE,
};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED};
use windows::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY;

/// Message to the window: the mute state changed (`wParam` = `pack(state)`).
pub(crate) const WM_MIC: u32 = WM_APP + 13;

static TARGET: AtomicIsize = AtomicIsize::new(0);
static STARTED: AtomicBool = AtomicBool::new(false);
/// Last state sent to the window, so repeated notifications (e.g. a mic level change) are not sent again.
static LAST: AtomicUsize = AtomicUsize::new(usize::MAX);

/// Mute state as a `wParam`: None (no microphone) = 0, on = 1, muted = 2.
pub(crate) fn pack(s: Option<bool>) -> usize {
    match s {
        None => 0,
        Some(false) => 1,
        Some(true) => 2,
    }
}

pub(crate) fn unpack(v: usize) -> Option<bool> {
    match v {
        1 => Some(false),
        2 => Some(true),
        _ => None,
    }
}

/// State of the default devices together: muted only when every one of them is muted. None without a microphone.
fn combined(mutes: &[bool]) -> Option<bool> {
    (!mutes.is_empty()).then(|| mutes.iter().all(|&m| m))
}

/// The mute value a toggle sets: unmute when all are muted, otherwise mute all. None without a microphone.
fn toggled(mutes: &[bool]) -> Option<bool> {
    combined(mutes).map(|m| !m)
}

fn enumerator() -> Option<IMMDeviceEnumerator> {
    unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok() }
}

/// The default capture devices for the console and communications roles, each device once.
fn endpoints(en: &IMMDeviceEnumerator) -> Vec<IAudioEndpointVolume> {
    let mut ids: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for role in [eConsole, eCommunications] {
        unsafe {
            let Ok(dev) = en.GetDefaultAudioEndpoint(eCapture, role) else { continue };
            let id = match dev.GetId() {
                Ok(p) => {
                    let s = p.to_string().unwrap_or_default();
                    CoTaskMemFree(Some(p.0 as *const c_void));
                    s
                }
                Err(_) => String::new(),
            };
            if ids.contains(&id) {
                continue;
            }
            if let Ok(ep) = dev.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None) {
                ids.push(id);
                out.push(ep);
            }
        }
    }
    out
}

fn mutes(eps: &[IAudioEndpointVolume]) -> Vec<bool> {
    eps.iter().filter_map(|e| unsafe { e.GetMute() }.ok().map(|b| b.as_bool())).collect()
}

/// Mute every default microphone, or unmute them when all are muted. Returns the state afterwards.
fn real_toggle() -> Option<bool> {
    let eps = endpoints(&enumerator()?);
    let new = toggled(&mutes(&eps))?;
    for ep in &eps {
        let _ = unsafe { ep.SetMute(new, std::ptr::null()) };
    }
    combined(&mutes(&eps))
}

#[cfg(not(test))]
pub(crate) fn toggle() -> Option<bool> {
    real_toggle()
}

#[cfg(test)]
thread_local! {
    /// Mute state of the fake default devices used by tests (empty = no microphone).
    static FAKE: std::cell::RefCell<Vec<bool>> = const { std::cell::RefCell::new(Vec::new()) };
}

#[cfg(test)]
pub(crate) fn toggle() -> Option<bool> {
    FAKE.with(|f| {
        let mut f = f.borrow_mut();
        let new = toggled(&f)?;
        f.iter_mut().for_each(|m| *m = new);
        combined(&f)
    })
}

#[cfg(test)]
pub(crate) fn set_fake(mutes: &[bool]) {
    FAKE.with(|f| *f.borrow_mut() = mutes.to_vec());
}

#[cfg(test)]
pub(crate) fn fake() -> Vec<bool> {
    FAKE.with(|f| f.borrow().clone())
}

enum Ev {
    /// A device was added, removed or became the default: look up the default devices again.
    Devices,
    /// Mute or level of a watched device changed.
    Mute,
}

#[implement(IAudioEndpointVolumeCallback)]
struct MuteCb {
    tx: Sender<Ev>,
}

impl IAudioEndpointVolumeCallback_Impl for MuteCb_Impl {
    fn OnNotify(&self, _data: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> windows::core::Result<()> {
        let _ = self.tx.send(Ev::Mute);
        Ok(())
    }
}

#[implement(IMMNotificationClient)]
struct DevCb {
    tx: Sender<Ev>,
}

impl IMMNotificationClient_Impl for DevCb_Impl {
    fn OnDeviceStateChanged(&self, _id: &WPCWSTR, _state: DEVICE_STATE) -> windows::core::Result<()> {
        let _ = self.tx.send(Ev::Devices);
        Ok(())
    }
    fn OnDeviceAdded(&self, _id: &WPCWSTR) -> windows::core::Result<()> {
        let _ = self.tx.send(Ev::Devices);
        Ok(())
    }
    fn OnDeviceRemoved(&self, _id: &WPCWSTR) -> windows::core::Result<()> {
        let _ = self.tx.send(Ev::Devices);
        Ok(())
    }
    fn OnDefaultDeviceChanged(&self, flow: EDataFlow, _role: ERole, _id: &WPCWSTR) -> windows::core::Result<()> {
        if flow == eCapture {
            let _ = self.tx.send(Ev::Devices);
        }
        Ok(())
    }
    fn OnPropertyValueChanged(&self, _id: &WPCWSTR, _key: &PROPERTYKEY) -> windows::core::Result<()> {
        Ok(())
    }
}

/// Watch the default microphones on a background thread (no polling) and send `WM_MIC` whenever the state
/// changes, starting with the current state.
pub(crate) fn start_watch(hwnd: isize) {
    if STARTED.swap(true, Ordering::Relaxed) {
        return;
    }
    TARGET.store(hwnd, Ordering::Relaxed);
    std::thread::spawn(|| unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let Some(en) = enumerator() else { return };
        let (tx, rx) = channel();
        let client: IMMNotificationClient = DevCb { tx: tx.clone() }.into();
        let _ = en.RegisterEndpointNotificationCallback(&client);
        let cb: IAudioEndpointVolumeCallback = MuteCb { tx }.into();
        let mut eps: Vec<IAudioEndpointVolume> = Vec::new();
        let mut ev = Ev::Devices;
        loop {
            if let Ev::Devices = ev {
                for ep in eps.drain(..) {
                    let _ = ep.UnregisterControlChangeNotify(&cb);
                }
                eps = endpoints(&en);
                for ep in &eps {
                    let _ = ep.RegisterControlChangeNotify(&cb);
                }
            }
            let s = pack(combined(&mutes(&eps)));
            if LAST.swap(s, Ordering::Relaxed) != s {
                let _ = PostMessageW(HWND(TARGET.load(Ordering::Relaxed) as *mut c_void), WM_MIC, WPARAM(s), LPARAM(0));
            }
            match rx.recv() {
                Ok(e) => ev = e,
                Err(_) => break, // cannot happen while `client` and `cb` are alive
            }
        }
    });
}

impl App {
    /// The mute state changed (from the hotkey or elsewhere): update the notch, without a toast.
    pub(crate) fn mic_apply(&mut self, s: Option<bool>) {
        if self.mic_muted != s {
            self.mic_muted = s;
            self.priv_t = Instant::now();
        }
    }

    /// Ctrl+Alt+M: toggle the mute and show the HUD (which also says when there is no microphone).
    pub(crate) fn mic_hotkey(&mut self) {
        let s = toggle();
        self.mic_apply(s);
        self.push_toast(toast::ToastKind::Hud { kind: toast::HudKind::Mic(s), on: s == Some(false) });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_packs_into_wparam() {
        for s in [None, Some(false), Some(true)] {
            assert_eq!(unpack(pack(s)), s);
        }
        assert_eq!(unpack(7), None, "unknown value = no microphone");
    }

    #[test]
    fn toggle_mutes_all_unless_all_are_muted() {
        assert_eq!(combined(&[]), None, "no microphone");
        assert_eq!(combined(&[true]), Some(true));
        assert_eq!(combined(&[true, false]), Some(false), "one device still on = not muted");
        assert_eq!(toggled(&[]), None, "no microphone: nothing to toggle");
        assert_eq!(toggled(&[false]), Some(true));
        assert_eq!(toggled(&[true, false]), Some(true), "mute the one that is still on");
        assert_eq!(toggled(&[true, true]), Some(false));
    }

    /// The real devices (GitHub Actions only, so a local run never mutes the developer's mic): toggling twice
    /// returns to the starting state, and a machine without a microphone is left alone.
    #[test]
    fn real_microphone_toggles_and_back() {
        if std::env::var_os("CI").is_none() {
            return;
        }
        let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let en = enumerator().expect("audio device enumerator");
        let before = combined(&mutes(&endpoints(&en)));
        eprintln!("microphone state on this runner: {before:?}");
        match before {
            None => assert_eq!(real_toggle(), None, "no microphone: nothing changes"),
            Some(m) => {
                assert_eq!(real_toggle(), Some(!m));
                assert_eq!(real_toggle(), Some(m), "second toggle restores the state");
            }
        }
    }
}
