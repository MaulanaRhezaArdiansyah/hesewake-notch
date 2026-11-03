//! Coding agent sessions for Hesewake Notch, as an extension (docs/extensions.md): which Claude Code sessions need
//! you, which are working and which are done, with toasts. The same exe is Claude Code's hook (`--hook claude`).
//!
//! One thread owns the pipe: it reads only when `PeekNamedPipe` says a frame is there, so it can also write on its own
//! schedule. Hook events arrive on the window thread of `relay::Receiver` and come here through a channel.

#![cfg_attr(not(test), windows_subsystem = "windows")]

mod claude_hook;
mod cfg;
mod claude_setup;
mod event;
mod pipe;
mod procs;
mod relay;
mod sessions;
mod text;
mod toasts;
mod ui;

use claude_setup::Link;
use serde_json::json;
use std::collections::VecDeque;
use std::fs::File;
use std::io;
use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

/// This exe's full path, as Claude Code's hooks name it.
fn exe() -> String {
    std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default()
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// The clock and the foreground window, read before each call into `Ui`.
fn set_time(ui: &mut ui::Ui) -> u64 {
    let ms = now_ms();
    ui.now = ms / 1000;
    ui.fg = unsafe { GetForegroundWindow() }.0 as isize;
    ms
}

struct Run {
    pipe: File,
    exe: String,
    sessions_file: Option<PathBuf>,
    /// The sessions changed since they were last written.
    dirty: bool,
}

impl Run {
    fn save_sessions(&mut self, ui: &ui::Ui) {
        if !self.dirty {
            return;
        }
        self.dirty = false;
        if let Some(f) = &self.sessions_file {
            if let Some(dir) = f.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            ui.sessions.save(f);
        }
    }

    fn apply(&mut self, ui: &mut ui::Ui, out: Vec<ui::Out>) -> io::Result<()> {
        let mut q: VecDeque<ui::Out> = out.into();
        while let Some(o) = q.pop_front() {
            match o {
                ui::Out::Send(v) => pipe::send(&mut self.pipe, v)?,
                ui::Out::Save(c) => c.save(),
                // written at most once a second, by the loop
                ui::Out::SaveSessions => self.dirty = true,
                ui::Out::Connect(on) => {
                    let link = claude_setup::apply(on, &self.exe).unwrap_or(Link::Invalid);
                    q.extend(ui.linked(link));
                }
                ui::Out::Recheck => q.extend(ui.linked(claude_setup::read_link(&self.exe))),
            }
        }
        Ok(())
    }
}

fn run() -> io::Result<()> {
    let mut pipe = pipe::open_pipe()?;
    if !pipe::server_is_notch(&pipe) {
        return Ok(());
    }
    pipe::send(&mut pipe, json!({"t": "hello", "proto": 1, "id": "agents", "name": "Coding Agents", "version": env!("CARGO_PKG_VERSION")}))?;
    let welcome = pipe::recv(&mut pipe)?;
    if welcome["t"] != "welcome" {
        return Ok(());
    }
    let exe = exe();
    let sessions_file = cfg::data_dir().map(|d| d.join("sessions.json"));
    let mut ui = ui::Ui::new(cfg::Config::load(), claude_setup::read_link(&exe), welcome["lang"] == "id");
    if let Some(f) = &sessions_file {
        ui.sessions = sessions::Sessions::load(f);
    }

    // the hooks reach whichever copy owns the window; a second copy only says so
    let (tx, events) = channel();
    let receiver = if relay::find_ext() != 0 { None } else { relay::Receiver::start(tx).ok() };
    ui.other_copy = receiver.is_none();

    let mut r = Run { pipe, exe, sessions_file, dirty: false };
    set_time(&mut ui);
    let out = ui.start();
    r.apply(&mut ui, out)?;
    let mut last_tick = Instant::now();
    loop {
        let mut busy = false;
        match pipe::available(&r.pipe) {
            None => break,
            // a whole frame follows its length: the notch writes each one in one go
            Some(n) if n >= 4 => {
                busy = true;
                let msg = pipe::recv(&mut r.pipe)?;
                if msg["t"] == "quit" {
                    let _ = pipe::send(&mut r.pipe, json!({"t": "bye"}));
                    break;
                }
                set_time(&mut ui);
                let out = ui.on_msg(&msg);
                r.apply(&mut ui, out)?;
            }
            Some(_) => {}
        }
        while let Ok(m) = events.try_recv() {
            busy = true;
            set_time(&mut ui);
            let out = ui.on_event(&m);
            r.apply(&mut ui, out)?;
        }
        if last_tick.elapsed() >= Duration::from_secs(1) {
            last_tick = Instant::now();
            let ms = set_time(&mut ui);
            let out = ui.tick(ms, procs::window_alive, procs::process_running);
            r.apply(&mut ui, out)?;
            r.save_sessions(&ui);
        }
        if !busy {
            std::thread::sleep(Duration::from_millis(40));
        }
    }
    r.save_sessions(&ui);
    drop(receiver);
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["--hook", "claude"] => relay::run_hook(),
        // the uninstaller: take our hooks and status line out of Claude Code's settings
        ["--disconnect-all"] => {
            let _ = claude_setup::apply(false, &exe());
        }
        // the notch shows the status; there is nobody to print an error to
        _ => {
            let _ = run();
        }
    }
    std::process::exit(0);
}
