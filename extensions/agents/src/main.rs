//! Coding agent sessions for Hesewake Notch, as an extension (docs/extensions.md): which Claude Code sessions need
//! you, which are working and which are done, with toasts. The same exe is Claude Code's hook (`--hook claude`).

#![cfg_attr(not(test), windows_subsystem = "windows")]
// the modules are wired into `main` one by one; until then most of them are unused
#![allow(dead_code)]

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

fn main() {}
