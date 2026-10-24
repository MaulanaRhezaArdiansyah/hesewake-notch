//! Log waktu frame (aktif kalau env HESEWAKE_PERF=1) ke %APPDATA%\hesewake-notch\perf.log, satu baris per detik.

use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

static ENABLED: AtomicBool = AtomicBool::new(false);
static STATS: Mutex<Stats> = Mutex::new(Stats::new());

struct Stats {
    start: Option<Instant>,
    frames: u32,
    draw_us: u64,
    blit_us: u64,
    max_us: u64,
    last: Option<Instant>,
    gap_max_us: u64,
}

impl Stats {
    const fn new() -> Self {
        Stats { start: None, frames: 0, draw_us: 0, blit_us: 0, max_us: 0, last: None, gap_max_us: 0 }
    }
}

pub(crate) fn init() {
    if std::env::var("HESEWAKE_PERF").map_or(false, |v| v == "1") {
        ENABLED.store(true, Ordering::Relaxed);
    }
}

pub(crate) fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Catat satu frame: durasi `draw` dan durasi blit + UpdateLayeredWindow.
pub(crate) fn frame(draw: Duration, blit: Duration) {
    if !enabled() {
        return;
    }
    let mut s = STATS.lock().unwrap_or_else(|p| p.into_inner());
    let now = Instant::now();
    let t0 = *s.start.get_or_insert(now);
    if let Some(l) = s.last {
        s.gap_max_us = s.gap_max_us.max((now - l).as_micros() as u64);
    }
    s.last = Some(now);
    s.frames += 1;
    s.draw_us += draw.as_micros() as u64;
    s.blit_us += blit.as_micros() as u64;
    s.max_us = s.max_us.max((draw + blit).as_micros() as u64);
    if now - t0 >= Duration::from_secs(1) {
        let n = s.frames.max(1) as u64;
        let line = format!(
            "fps={} draw_avg={}us blit_avg={}us frame_max={}us gap_max={}us\n",
            s.frames,
            s.draw_us / n,
            s.blit_us / n,
            s.max_us,
            s.gap_max_us
        );
        *s = Stats::new();
        s.last = Some(now);
        drop(s);
        let p = data_path("perf.log");
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p) {
            let _ = f.write_all(line.as_bytes());
        }
    }
}

/// Catatan bebas ke perf.log (hanya bila HESEWAKE_PERF=1), mis. kegagalan pengambilan cover.
pub(crate) fn note(msg: &str) {
    if !enabled() {
        return;
    }
    let p = data_path("perf.log");
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p) {
        let _ = writeln!(f, "{}", msg);
    }
}

/// Memori committed proses (MB), untuk diagnosa lonjakan memori.
pub(crate) fn mem_mb() -> f32 {
    use windows::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    let mut c = PROCESS_MEMORY_COUNTERS::default();
    c.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
    unsafe {
        let _ = K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb);
    }
    c.PagefileUsage as f32 / (1024.0 * 1024.0)
}
