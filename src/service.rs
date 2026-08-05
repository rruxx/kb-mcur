// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

pub mod dir;
pub mod glide_alpha;
pub mod glide_num;
pub mod grid;

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use anyhow::Result;
use log::{info, warn};

use crate::service::glide_alpha::GlideAlpha;
use crate::service::glide_num::GlideNum;
use crate::service::grid::{GridEnv, fix_device_permissions};

use crate::{
    config::{BTN_EXTRA, BTN_LEFT, BTN_MIDDLE, BTN_RIGHT, BTN_SIDE},
    device::abi::{EV_KEY, EV_SYN, SYN_REPORT, create_virt_device, write_event, write_event_raw},
    device::input::KeyboardDev,
    keymap::{
        KEY_CAPSLOCK, KEY_KPENTER, KEY_LEFTMETA, KEY_LEFTSHIFT, KEY_NUMLOCK, KEY_RIGHTMETA,
        KEY_RIGHTSHIFT, KEY_TAB, ModState, key_map,
    },
};

// ── Key classification ───────────────────────────────────────────────

fn is_grid_key(code: u16) -> bool {
    key_map(code, &ModState::default()).is_some() || code == KEY_TAB
}

// ── Signal ───────────────────────────────────────────────────────────

extern "C" fn shutdown_signal(_: i32) {
    SHUTDOWN.store(true, Ordering::Relaxed);
}

static SHUTDOWN: AtomicBool = AtomicBool::new(false);

// ── Main entry point ─────────────────────────────────────────────────

pub fn run_service() -> Result<()> {
    info!("service — grid + glide-num + glide-alpha");

    unsafe {
        let _ = nix::sys::signal::signal(
            nix::sys::signal::Signal::SIGINT,
            nix::sys::signal::SigHandler::Handler(shutdown_signal),
        );
        let _ = nix::sys::signal::signal(
            nix::sys::signal::Signal::SIGTERM,
            nix::sys::signal::SigHandler::Handler(shutdown_signal),
        );
    }

    let mut kbd = KeyboardDev::open_all()?;

    let kbd_bits: Vec<u16> = (1u16..=255).collect();
    let mut kbd_out = create_virt_device(crate::config::DEV_KBD, &kbd_bits, false)?;
    let mut ptr_out = create_virt_device(
        crate::config::DEV_PTR,
        &[BTN_LEFT, BTN_MIDDLE, BTN_RIGHT, BTN_SIDE, BTN_EXTRA],
        true,
    )?;

    let mut glide_num = GlideNum::new();
    let mut glide_alpha = GlideAlpha::new();

    for code in 1u16..=255 {
        write_event(&mut kbd_out, EV_KEY, code, 0)?;
    }
    write_event(&mut kbd_out, EV_SYN, SYN_REPORT, 0)?;

    let mut grid = GridEnv::new();
    let mut mods = ModState::default();

    let mut warn_is_done = false;
    let mut last_wd = Instant::now();
    // A held modifier whose key-down has not yet been forwarded to the desktop.
    // It is consumed when a mode-toggle chord follows; otherwise it is replayed.
    let mut pending: Option<u16> = None;

    loop {
        if SHUTDOWN.load(Ordering::Relaxed) {
            info!("shutting down");
            break Ok(());
        }

        let now = Instant::now();
        if now.duration_since(last_wd) >= std::time::Duration::from_secs(1) {
            fix_device_permissions();
            last_wd = now;
        }

        if kbd.is_empty() {
            if !warn_is_done {
                warn!("all keyboards gone");
            }
            warn_is_done = true;
        } else {
            warn_is_done = false;
        }
        let t_poll_start = Instant::now();
        match kbd.poll_event(32) {
            Ok(Some(ev)) => {
                let code = ev.code;
                let value = ev.value;
                let is_press = value > 0;

                mods.update(code, is_press);

                // Only EV_KEY events carry mode-relevant information.
                if ev.type_ != EV_KEY {
                    write_event_raw(&mut kbd_out, &ev)?;
                    continue;
                }

                // Precisely match a mode-toggle chord so extra modifiers
                // (e.g. meta+ctrl+capslock) are not mistaken for one.
                // grid:        meta, no shift/ctrl/alt
                // glide-alpha: meta+shift, no ctrl/alt
                // glide-num:   numlock, no meta/shift/ctrl/alt
                let chord = is_press
                    && match code {
                        KEY_CAPSLOCK => mods.meta && !mods.ctrl && !mods.alt,
                        KEY_KPENTER => {
                            glide_num.numlock_held()
                                && !mods.meta
                                && !mods.shift
                                && !mods.ctrl
                                && !mods.alt
                        }
                        _ => false,
                    };
                if let Some(p) = pending.take()
                    && !chord
                {
                    write_event(&mut kbd_out, EV_KEY, p, 1)?;
                    write_event(&mut kbd_out, EV_SYN, SYN_REPORT, 0)?;
                }

                // Hold Meta/NumLock presses: forward only if no chord follows.
                if is_press
                    && (code == KEY_LEFTMETA || code == KEY_RIGHTMETA || code == KEY_NUMLOCK)
                {
                    pending = Some(code);
                    continue;
                }

                if toggle_glide_num(&mut glide_num, code, value, is_press, &mods) {
                    continue;
                }

                if toggle_glide_alpha(&mut glide_alpha, code, is_press, &mods, &mut kbd_out)? {
                    continue;
                }

                if grid.toggle(code, is_press, &mods, &mut kbd_out)? {
                    continue;
                }

                if handle_glide_num_input(code, value, is_press, &mut glide_num, &mut ptr_out)? {
                    continue;
                }

                if handle_glide_alpha_input(code, value, is_press, &mut glide_alpha, &mut ptr_out)?
                {
                    continue;
                }

                if handle_grid_input(code, value, &mut grid, &mods) {
                    continue;
                }

                write_event_raw(&mut kbd_out, &ev)?;
            }
            Ok(None) => {
                glide_num.direction_tick(&mut ptr_out)?;
                glide_alpha.direction_tick(&mut ptr_out)?;
            }
            Err(e) => return Err(e),
        }
        let t_poll = t_poll_start.elapsed();
        if t_poll > std::time::Duration::from_millis(40) {
            warn!("poll {t_poll:?}");
        }
    }
}

// ── Toggles ──────────────────────────────────────────────────────────

fn toggle_glide_num(
    glide_num: &mut GlideNum,
    code: u16,
    value: i32,
    is_press: bool,
    mods: &ModState,
) -> bool {
    if code == KEY_NUMLOCK {
        glide_num.set_numlock(value != 0);
    }
    if code == KEY_KPENTER
        && is_press
        && glide_num.numlock_held()
        && !mods.meta
        && !mods.shift
        && !mods.ctrl
        && !mods.alt
    {
        glide_num.toggle();
        warn!(
            "{}",
            if glide_num.active() {
                "[glide-num ON]"
            } else {
                "[glide-num OFF]"
            }
        );
        return true;
    }
    false
}

fn toggle_glide_alpha(
    glide_alpha: &mut GlideAlpha,
    code: u16,
    is_press: bool,
    mods: &ModState,
    kbd_out: &mut std::fs::File,
) -> Result<bool> {
    if code != KEY_CAPSLOCK || !is_press || !mods.meta || !mods.shift || mods.ctrl || mods.alt {
        return Ok(false);
    }
    glide_alpha.toggle();
    warn!(
        "{}",
        if glide_alpha.active() {
            "[glide-alpha ON]"
        } else {
            "[glide-alpha OFF]"
        }
    );
    for key in [
        KEY_LEFTMETA,
        KEY_RIGHTMETA,
        KEY_LEFTSHIFT,
        KEY_RIGHTSHIFT,
        KEY_CAPSLOCK,
    ] {
        write_event(kbd_out, EV_KEY, key, 0)?;
    }
    write_event(kbd_out, EV_SYN, SYN_REPORT, 0)?;
    Ok(true)
}

// ── Input dispatch ───────────────────────────────────────────────────

fn handle_glide_num_input(
    code: u16,
    value: i32,
    is_press: bool,
    glide_num: &mut GlideNum,
    ptr_out: &mut std::fs::File,
) -> Result<bool> {
    if !glide_num.active() {
        return Ok(false);
    }
    glide_num.handle_event(ptr_out, code, value, is_press)
}

fn handle_glide_alpha_input(
    code: u16,
    value: i32,
    is_press: bool,
    glide_alpha: &mut GlideAlpha,
    ptr_out: &mut std::fs::File,
) -> Result<bool> {
    if !glide_alpha.active() {
        return Ok(false);
    }
    glide_alpha.handle_event(ptr_out, code, value, is_press)
}

fn handle_grid_input(code: u16, value: i32, grid: &mut GridEnv, mods: &ModState) -> bool {
    if !grid.active() || !is_grid_key(code) || value == 0 {
        return false;
    }
    grid.handle_input(code, value, mods)
}
