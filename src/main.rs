// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

//! GUI subsystem on Windows so double-clicking the .exe opens no console
//! window; CLI output is restored by attaching to the parent console.

#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use anyhow::Result;
use clap::{CommandFactory, Parser};
use kursor::cli;

#[derive(Parser)]
#[command(
    name = env!("CARGO_PKG_NAME"),
    version,
    about = "Keyboard-driven mouse-cursor control.",
)]
struct Cli {
    #[command(subcommand)]
    cmd: Option<cli::Cmd>,
}

fn main() -> Result<()> {
    #[cfg(target_os = "windows")]
    let console = {
        let attached = console::attach_parent_console();
        init_logger(attached);
        attached
    };
    #[cfg(not(target_os = "windows"))]
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let cli = Cli::parse();
    match cli.cmd {
        Some(cmd) => cli::dispatch(cmd),
        None => {
            #[cfg(target_os = "windows")]
            {
                // Double-click with no arguments: run the service in the
                // background (no console); the tray icon exits it.
                if console {
                    Cli::command().print_help()?;
                    Ok(())
                } else {
                    cli::dispatch(cli::Cmd::Service)
                }
            }
            #[cfg(not(target_os = "windows"))]
            {
                Cli::command().print_help()?;
                Ok(())
            }
        }
    }
}

#[cfg(target_os = "windows")]
mod console {
    use windows_sys::Win32::Foundation::GENERIC_WRITE;
    use windows_sys::Win32::Storage::FileSystem::{CreateFileW, FILE_SHARE_WRITE, OPEN_EXISTING};
    use windows_sys::Win32::System::Console::{
        ATTACH_PARENT_PROCESS, AttachConsole, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE, SetStdHandle,
    };

    /// Attach to the parent console (terminal run) and rebind stdout/stderr so
    /// Rust stdio and `println!` work from a GUI-subsystem process. Returns
    /// `true` when attached (i.e. launched from a terminal, not by double-click).
    pub fn attach_parent_console() -> bool {
        unsafe {
            if AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
                return false;
            }
            bind_handle(STD_OUTPUT_HANDLE, "CONOUT$");
            bind_handle(STD_ERROR_HANDLE, "CONERR$");
            true
        }
    }

    unsafe fn bind_handle(which: u32, name: &str) {
        let wide = encode_wide(name);
        let h = unsafe {
            CreateFileW(
                wide.as_ptr(),
                GENERIC_WRITE,
                FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        if h as isize != -1 {
            unsafe { SetStdHandle(which, h) };
        }
    }

    fn encode_wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }
}

#[cfg(target_os = "windows")]
fn init_logger(console: bool) {
    use env_logger::{Builder, Env, Target};

    let env = Env::default().default_filter_or("info");
    if console {
        Builder::from_env(env).target(Target::Stderr).init();
        return;
    }
    // Background (double-click) run: there is no console, so log to a file.
    let dir = std::env::var("LOCALAPPDATA")
        .unwrap_or_else(|_| std::env::temp_dir().to_string_lossy().into_owned());
    let log_dir = std::path::Path::new(&dir).join("kursor");
    let _ = std::fs::create_dir_all(&log_dir);
    if let Ok(file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_dir.join("kursor.log"))
    {
        Builder::from_env(env)
            .target(Target::Pipe(Box::new(file)))
            .init();
    } else {
        Builder::from_env(env).target(Target::Stderr).init();
    }
}
