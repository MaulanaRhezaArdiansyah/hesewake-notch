//! Odoo tickets for Hesewake Notch, as an extension (docs/extensions.md): my tickets per stage, a ticket's page, the
//! daily report, and toasts for new tickets and tickets that changed stage. It does what the Office edition's Tickets
//! slide did, with that edition's settings (`odoo.json`) and API key.
//!
//! One thread owns the pipe: it reads only when `PeekNamedPipe` says a frame is there, so it can also write on its own
//! schedule (the monitor) without the blocked-read problem of a plain pipe handle. The network runs on a second thread.

#![cfg_attr(not(test), windows_subsystem = "windows")]

mod cfg;
mod cred;
mod odoo;
mod tickets;
mod ui;

use serde_json::{json, Value};
use std::collections::VecDeque;
use std::ffi::c_void;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::windows::io::AsRawHandle;
use std::sync::mpsc::{channel, Sender};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE, SYSTEMTIME};
use windows::Win32::System::Pipes::{GetNamedPipeServerProcessId, PeekNamedPipe};
use windows::Win32::System::SystemInformation::{GetLocalTime, GetSystemTime};
use windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION};

const MAX_FRAME: usize = 64 * 1024;

fn send(pipe: &mut File, v: Value) -> io::Result<()> {
    let body = v.to_string().into_bytes();
    if body.len() > MAX_FRAME {
        // never happens with the limits ui.rs keeps; a page this large would be dropped by the notch anyway
        return Ok(());
    }
    pipe.write_all(&(body.len() as u32).to_le_bytes())?;
    pipe.write_all(&body)?;
    pipe.flush()
}

fn recv(pipe: &mut File) -> io::Result<Value> {
    let mut len = [0u8; 4];
    pipe.read_exact(&mut len)?;
    let n = u32::from_le_bytes(len) as usize;
    if n == 0 || n > MAX_FRAME {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "bad frame length"));
    }
    let mut body = vec![0u8; n];
    pipe.read_exact(&mut body)?;
    serde_json::from_slice(&body).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

fn handle(pipe: &File) -> HANDLE {
    HANDLE(pipe.as_raw_handle() as *mut c_void)
}

/// Bytes waiting in the pipe, or None when it is closed.
fn available(pipe: &File) -> Option<u32> {
    let mut n = 0u32;
    unsafe { PeekNamedPipe(handle(pipe), None, 0, None, Some(&mut n), None) }.ok()?;
    Some(n)
}

fn open_pipe() -> io::Result<File> {
    let name = std::env::var("HESEWAKE_NOTCH_PIPE").map_err(|_| io::Error::new(io::ErrorKind::NotFound, "HESEWAKE_NOTCH_PIPE is not set"))?;
    OpenOptions::new().read(true).write(true).open(name)
}

/// The other end must be the notch (`hesewake-notch.exe`) before anything about tickets is sent (docs/extensions.md, section 3).
fn server_is_notch(pipe: &File) -> bool {
    unsafe {
        let mut pid = 0u32;
        if GetNamedPipeServerProcessId(handle(pipe), &mut pid).is_err() {
            return false;
        }
        let Ok(p) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return false };
        let mut buf = [0u16; 1024];
        let mut n = buf.len() as u32;
        let r = QueryFullProcessImageNameW(p, PROCESS_NAME_FORMAT(0), PWSTR(buf.as_mut_ptr()), &mut n);
        let _ = CloseHandle(p);
        if r.is_err() {
            return false;
        }
        let path = String::from_utf16_lossy(&buf[..n as usize]);
        path.rsplit('\\').next().is_some_and(|f| f.eq_ignore_ascii_case("hesewake-notch.exe"))
    }
}

fn stamp(t: &SYSTEMTIME) -> String {
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond)
}

/// The clock: Unix seconds, the local day, and the local zone against UTC (to the quarter hour).
fn set_time(ui: &mut ui::Ui) {
    ui.now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let (local, utc) = unsafe { (GetLocalTime(), GetSystemTime()) };
    let (l, u) = (stamp(&local), stamp(&utc));
    if let (Some(a), Some(b)) = (tickets::odoo_unix(&l), tickets::odoo_unix(&u)) {
        ui.tz_secs = ((a - b) as f64 / 900.0).round() as i64 * 900;
    }
    ui.today = l[..10].to_string();
}

fn apply(pipe: &mut File, ui: &mut ui::Ui, work: &Sender<tickets::Cmd>, out: Vec<ui::Out>) -> io::Result<()> {
    let mut q: VecDeque<ui::Out> = out.into();
    while let Some(o) = q.pop_front() {
        match o {
            ui::Out::Send(v) => send(pipe, v)?,
            ui::Out::Work(c) => {
                let _ = work.send(c);
            }
            ui::Out::Save(c) => c.save(),
            ui::Out::Reload => q.extend(ui.reloaded(cfg::OdooCfg::load(), cred::has_key())),
        }
    }
    Ok(())
}

fn run() -> io::Result<()> {
    let mut pipe = open_pipe()?;
    if !server_is_notch(&pipe) {
        return Ok(());
    }
    send(&mut pipe, json!({"t": "hello", "proto": 1, "id": "odoo", "name": "Odoo", "version": env!("CARGO_PKG_VERSION")}))?;
    let welcome = recv(&mut pipe)?;
    if welcome["t"] != "welcome" {
        return Ok(());
    }
    let (work_tx, work_rx) = channel();
    let (evt_tx, evt_rx) = channel();
    std::thread::spawn(move || tickets::worker(work_rx, evt_tx));

    let mut ui = ui::Ui::new(cfg::OdooCfg::load(), cred::has_key(), welcome["lang"] == "id");
    set_time(&mut ui);
    let out = ui.start();
    apply(&mut pipe, &mut ui, &work_tx, out)?;
    let mut last_tick = Instant::now();
    loop {
        let mut busy = false;
        match available(&pipe) {
            None => return Ok(()),
            // a whole frame follows its length: the notch writes each one in one go
            Some(n) if n >= 4 => {
                busy = true;
                let msg = recv(&mut pipe)?;
                if msg["t"] == "quit" {
                    let _ = send(&mut pipe, json!({"t": "bye"}));
                    return Ok(());
                }
                set_time(&mut ui);
                let out = ui.on_msg(&msg);
                apply(&mut pipe, &mut ui, &work_tx, out)?;
            }
            Some(_) => {}
        }
        while let Ok(e) = evt_rx.try_recv() {
            busy = true;
            set_time(&mut ui);
            let out = ui.on_evt(e);
            apply(&mut pipe, &mut ui, &work_tx, out)?;
        }
        if last_tick.elapsed() >= Duration::from_secs(1) {
            last_tick = Instant::now();
            set_time(&mut ui);
            let out = ui.tick();
            apply(&mut pipe, &mut ui, &work_tx, out)?;
        }
        if !busy {
            std::thread::sleep(Duration::from_millis(40));
        }
    }
}

fn main() {
    // the notch shows the status; there is nobody to print an error to
    let _ = run();
    // the network thread may still be in a call: end now
    std::process::exit(0);
}
