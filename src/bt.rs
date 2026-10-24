//! Deteksi perangkat audio Bluetooth yang tersambung/terputus. Event datang dari thread WinRT, dikirim ke UI thread
//! lewat slot bersama + `WM_BT` (pola yang sama dengan `post_refresh` di music.rs).

use super::*;
use toast::ToastKind;
use windows::Devices::Bluetooth::{BluetoothConnectionStatus, BluetoothDevice, BluetoothMajorClass};
use windows::Devices::Enumeration::DeviceInformation;
use windows::Foundation::TypedEventHandler;

/// Pesan ke window: ada toast Bluetooth baru di slot.
pub(crate) const WM_BT: u32 = WM_APP + 2;

pub(crate) type BtSlot = Arc<Mutex<Vec<ToastKind>>>;

const RESCAN_SECS: u64 = 60;

fn push(slot: &BtSlot, hwnd: isize, kind: ToastKind) {
    {
        let mut q = slot.lock().unwrap_or_else(|p| p.into_inner());
        if q.len() < 4 {
            q.push(kind);
        }
    }
    unsafe {
        let _ = PostMessageW(HWND(hwnd as *mut c_void), WM_BT, WPARAM(0), LPARAM(0));
    }
}

/// Thread latar belakang: pasang pemantau pada tiap perangkat audio yang sudah dipasangkan (re-scan tiap menit
/// untuk perangkat yang baru dipasangkan). Event awal saat start tidak menghasilkan toast.
pub(crate) fn bt_thread(hwnd: isize, slot: BtSlot) {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let mut known: Vec<String> = Vec::new();
    let mut keep: Vec<BluetoothDevice> = Vec::new(); // handle harus hidup agar event tetap datang
    loop {
        scan(hwnd, &slot, &mut known, &mut keep);
        std::thread::sleep(Duration::from_secs(RESCAN_SECS));
    }
}

fn scan(hwnd: isize, slot: &BtSlot, known: &mut Vec<String>, keep: &mut Vec<BluetoothDevice>) {
    let Ok(sel) = BluetoothDevice::GetDeviceSelectorFromPairingState(true) else { return };
    let Ok(found) = DeviceInformation::FindAllAsyncAqsFilter(&sel).and_then(|a| a.get()) else { return };
    for info in found {
        let Ok(id) = info.Id() else { continue };
        let id_s = id.to_string();
        if known.contains(&id_s) {
            continue;
        }
        known.push(id_s);
        let Ok(dev) = BluetoothDevice::FromIdAsync(&id).and_then(|a| a.get()) else { continue };
        let is_audio = dev
            .ClassOfDevice()
            .and_then(|c| c.MajorClass())
            .map_or(false, |m| m == BluetoothMajorClass::AudioVideo);
        if !is_audio {
            continue;
        }
        let name = dev.Name().map(|n| n.to_string()).unwrap_or_default();
        let (slot2, name2) = (slot.clone(), name.clone());
        let mut last = Instant::now() - Duration::from_secs(60);
        let handler = TypedEventHandler::<BluetoothDevice, windows::core::IInspectable>::new(move |d, _| {
            let Some(d) = d.as_ref() else { return Ok(()) };
            let connected = d.ConnectionStatus()? == BluetoothConnectionStatus::Connected;
            // Redam kedipan koneksi (flap) yang datang beruntun.
            if last.elapsed() < Duration::from_secs(3) {
                return Ok(());
            }
            last = Instant::now();
            push(&slot2, hwnd, ToastKind::Bluetooth { name: name2.clone(), pct: None, connected });
            Ok(())
        });
        if dev.ConnectionStatusChanged(&handler).is_ok() {
            keep.push(dev);
        }
    }
}
