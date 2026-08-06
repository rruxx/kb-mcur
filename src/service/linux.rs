// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Linux service main loop: evdev keyboard grab + uinput output.

use std::fs::File;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use anyhow::Result;
use log::{info, warn};

use crate::config::{
    BTN_EXTRA, BTN_LEFT, BTN_MIDDLE, BTN_RIGHT, BTN_SIDE, DEV_KBD, DEV_PTR, MouseButton,
    hid_button_code,
};
use crate::device::linux::abi::{
    EV_KEY, EV_REL, EV_SYN, REL_HWHEEL, REL_WHEEL, REL_X, REL_Y, SYN_REPORT, create_virt_device,
    write_event, write_event_raw,
};
use crate::device::linux::input::KeyboardDev;
use crate::device::pointer::{KeyboardOut, Pointer, ScrollAxis, SideButton};
use crate::keymap::KEYCODE_MAX;
use crate::service::Service;

/// uinput-backed keyboard passthrough.
struct UinputKbd<'a>(&'a mut File);

impl KeyboardOut for UinputKbd<'_> {
    fn key(&mut self, code: u16, value: i32) -> Result<()> {
        write_event(self.0, EV_KEY, code, value)?;
        Ok(())
    }
    fn sync(&mut self) -> Result<()> {
        write_event(self.0, EV_SYN, SYN_REPORT, 0)?;
        Ok(())
    }
}

/// REL-only pointer over the uinput pointer device (glide output).
struct RelPointer<'a>(&'a mut File);

impl Pointer for RelPointer<'_> {
    fn move_rel(&mut self, dx: i32, dy: i32) -> Result<()> {
        write_event(self.0, EV_REL, REL_X, dx)?;
        write_event(self.0, EV_REL, REL_Y, dy)?;
        write_event(self.0, EV_SYN, SYN_REPORT, 0)?;
        Ok(())
    }

    fn button(&mut self, button: MouseButton, press: bool) -> Result<()> {
        write_event(self.0, EV_KEY, hid_button_code(button), i32::from(press))?;
        write_event(self.0, EV_SYN, SYN_REPORT, 0)?;
        Ok(())
    }

    fn scroll(&mut self, axis: ScrollAxis, dir: i32) -> Result<()> {
        let code = match axis {
            ScrollAxis::Vertical => REL_WHEEL,
            ScrollAxis::Horizontal => REL_HWHEEL,
        };
        write_event(self.0, EV_REL, code, dir)?;
        write_event(self.0, EV_SYN, SYN_REPORT, 0)?;
        Ok(())
    }

    fn side(&mut self, button: SideButton) -> Result<()> {
        let code = match button {
            SideButton::Back => BTN_SIDE,
            SideButton::Forward => BTN_EXTRA,
        };
        write_event(self.0, EV_KEY, code, 1)?;
        write_event(self.0, EV_SYN, SYN_REPORT, 0)?;
        write_event(self.0, EV_KEY, code, 0)?;
        write_event(self.0, EV_SYN, SYN_REPORT, 0)?;
        Ok(())
    }

    fn warp(&mut self, _x: i32, _y: i32) -> Result<()> {
        Ok(())
    }
}

// ── Signal ───────────────────────────────────────────────────────────

extern "C" fn shutdown_signal(_: i32) {
    SHUTDOWN.store(true, Ordering::Relaxed);
}

static SHUTDOWN: AtomicBool = AtomicBool::new(false);

// ── Main loop ────────────────────────────────────────────────────────

pub fn run() -> Result<()> {
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

    let kbd_bits: Vec<u16> = (1u16..=KEYCODE_MAX).collect();
    let mut kbd_out = create_virt_device(DEV_KBD, &kbd_bits, false)?;
    let mut ptr_out = create_virt_device(
        DEV_PTR,
        &[BTN_LEFT, BTN_MIDDLE, BTN_RIGHT, BTN_SIDE, BTN_EXTRA],
        true,
    )?;

    for code in 1u16..=KEYCODE_MAX {
        write_event(&mut kbd_out, EV_KEY, code, 0)?;
    }
    write_event(&mut kbd_out, EV_SYN, SYN_REPORT, 0)?;

    let mut svc = Service::new();

    let mut warn_is_done = false;
    let mut last_wd = Instant::now();
    let mut last_resize = Instant::now();

    loop {
        if SHUTDOWN.load(Ordering::Relaxed) {
            info!("shutting down");
            break Ok(());
        }

        let now = Instant::now();
        if now.duration_since(last_wd) >= std::time::Duration::from_secs(1) {
            crate::service::grid::fix_device_permissions();
            last_wd = now;
        }
        if now.duration_since(last_resize)
            >= std::time::Duration::from_millis(crate::config::GRID_RESIZE_CHECK_MS)
        {
            if let Err(e) = svc.poll_grid_resize() {
                warn!("grid resize check: {e}");
            }
            last_resize = now;
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
                if ev.type_ != EV_KEY {
                    write_event_raw(&mut kbd_out, &ev)?;
                    continue;
                }
                let consumed = svc.dispatch(
                    ev.code,
                    ev.value,
                    &mut RelPointer(&mut ptr_out),
                    &mut UinputKbd(&mut kbd_out),
                )?;
                if !consumed {
                    write_event_raw(&mut kbd_out, &ev)?;
                }
            }
            Ok(None) => {
                svc.direction_tick(&mut RelPointer(&mut ptr_out))?;
            }
            Err(e) => return Err(e),
        }
        let t_poll = t_poll_start.elapsed();
        if t_poll > std::time::Duration::from_millis(40) {
            warn!("poll {t_poll:?}");
        }
    }
}
