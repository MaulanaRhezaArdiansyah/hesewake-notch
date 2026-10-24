//! Volume master perangkat audio default (IAudioEndpointVolume). Dipakai untuk kontrol volume lewat scroll di slide Music
//! dan (nanti) HUD volume. Dipanggil di UI thread; objek dibuat per panggilan (murah, dan mengikuti perangkat default).

use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};

fn endpoint() -> Option<IAudioEndpointVolume> {
    unsafe {
        let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let dev = en.GetDefaultAudioEndpoint(eRender, eConsole).ok()?;
        dev.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None).ok()
    }
}

/// (volume 0..1, bisu). Dipakai HUD volume (0.3.0).
#[allow(dead_code)]
pub(crate) fn get() -> Option<(f32, bool)> {
    let ep = endpoint()?;
    unsafe { Some((ep.GetMasterVolumeLevelScalar().ok()?, ep.GetMute().ok()?.as_bool())) }
}

/// Ubah volume sebesar `delta` (-1..1) lalu kembalikan keadaan baru. Menyalakan kembali suara bila sedang bisu dan
/// volume dinaikkan.
pub(crate) fn adjust(delta: f32) -> Option<(f32, bool)> {
    let ep = endpoint()?;
    unsafe {
        let cur = ep.GetMasterVolumeLevelScalar().ok()?;
        let new = (cur + delta).clamp(0.0, 1.0);
        ep.SetMasterVolumeLevelScalar(new, std::ptr::null()).ok()?;
        if delta > 0.0 && ep.GetMute().ok()?.as_bool() {
            ep.SetMute(false, std::ptr::null()).ok()?;
        }
        Some((new, ep.GetMute().ok()?.as_bool()))
    }
}

/// Langkah volume (0..1) untuk satu muatan roda mouse (`delta` Windows: 120 = satu takik).
pub(crate) fn wheel_step(delta: i32) -> f32 {
    (delta as f32 / 120.0) * 0.02
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wheel_step_is_two_percent_per_notch() {
        assert!((wheel_step(120) - 0.02).abs() < 1e-6);
        assert!((wheel_step(-240) + 0.04).abs() < 1e-6);
        assert!((wheel_step(30) - 0.005).abs() < 1e-6);
    }
}
