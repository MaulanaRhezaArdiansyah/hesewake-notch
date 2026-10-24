//! Status koneksi internet (Windows.Networking.Connectivity), dibaca dari media thread tiap beberapa detik.

use std::sync::atomic::{AtomicU8, Ordering};
use windows::Networking::Connectivity::{NetworkConnectivityLevel, NetworkInformation};

pub(crate) const UNKNOWN: u8 = 0;
pub(crate) const CONNECTED: u8 = 1;
pub(crate) const NO_INTERNET: u8 = 2;
pub(crate) const DISCONNECTED: u8 = 3;

static STATE: AtomicU8 = AtomicU8::new(UNKNOWN);

pub(crate) fn state() -> u8 {
    STATE.load(Ordering::Relaxed)
}

pub(crate) fn label(s: u8) -> &'static str {
    match s {
        CONNECTED => "Connected",
        NO_INTERNET => "No internet access",
        DISCONNECTED => "Disconnected",
        _ => "",
    }
}

fn read() -> u8 {
    let Ok(p) = NetworkInformation::GetInternetConnectionProfile() else {
        return DISCONNECTED;
    };
    match p.GetNetworkConnectivityLevel() {
        Ok(NetworkConnectivityLevel::InternetAccess) | Ok(NetworkConnectivityLevel::ConstrainedInternetAccess) => CONNECTED,
        Ok(_) => NO_INTERNET,
        Err(_) => UNKNOWN,
    }
}

/// Baca ulang status. True bila berubah (perlu redraw).
pub(crate) fn poll() -> bool {
    let now = read();
    STATE.swap(now, Ordering::Relaxed) != now
}
