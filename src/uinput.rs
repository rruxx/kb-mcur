// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::{Context, Result};
use log::warn;
use std::fs::File;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;

use crate::config::{
    btn_code as button_hid, BTN_LEFT, BTN_MIDDLE, BTN_RIGHT,
    CLICK_INTERVAL_MS, DEV_ABS, DEV_REL,
    UINPUT_CREATE_WAIT_MS, UINPUT_NAME_MAXLEN,
};
use crate::uio::{
    create_virt_device, ioctl_ref, ioctl_val,
    ABS_X, ABS_Y, EV_ABS, EV_KEY, EV_REL, EV_SYN,
    InputAbsinfo, InputEvent, REL_X, REL_Y, SYN_REPORT,
    UI_ABS_SETUP, UI_DEV_CREATE, UI_DEV_DESTROY, UI_DEV_SETUP,
    UI_SET_ABSBIT, UI_SET_EVBIT, UI_SET_KEYBIT, UinputAbsSetup, UinputSetup,
};

// Re-export shared types so existing callers still work.
pub use crate::uio::{InputEvent as UioInputEvent};
// Actually, InputEvent is already in scope via `use`, just re-export.
// Better: re-export all shared items for backward compat.

pub struct Mouse {
    fd: File,
    fd_rel: Option<File>,
    screen_w: u16,
    screen_h: u16,
}

impl Mouse {
    pub fn new(screen_w: u16, screen_h: u16) -> Result<Self> {
        let fd = std::fs::OpenOptions::new()
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open("/dev/uinput")
            .context("open /dev/uinput")?;

        // Write setup struct via ioctl (new API on kernel ≥5.x)
        let setup = UinputSetup {
            id: libc::input_id {
                bustype: 0,
                vendor: 0,
                product: 0,
                version: 0,
            },
            name: {
                let mut n = [0u8; UINPUT_NAME_MAXLEN];
                n[..DEV_ABS.len()].copy_from_slice(DEV_ABS.as_bytes());
                n
            },
            ff_effects_max: 0,
        };
        ioctl_ref(&fd, UI_DEV_SETUP, &setup).context("UI_DEV_SETUP")?;

        ioctl_val(&fd, UI_SET_EVBIT, EV_KEY as u32).context("EV_KEY")?;
        ioctl_val(&fd, UI_SET_EVBIT, EV_ABS as u32).context("EV_ABS")?;
        ioctl_val(&fd, UI_SET_EVBIT, EV_SYN as u32).context("EV_SYN")?;

        ioctl_val(&fd, UI_SET_KEYBIT, BTN_LEFT as u32).context("BTN_LEFT")?;
        ioctl_val(&fd, UI_SET_KEYBIT, BTN_MIDDLE as u32).context("BTN_MIDDLE")?;
        ioctl_val(&fd, UI_SET_KEYBIT, BTN_RIGHT as u32).context("BTN_RIGHT")?;

        ioctl_val(&fd, UI_SET_ABSBIT, ABS_X as u32).context("ABS_X")?;
        ioctl_val(&fd, UI_SET_ABSBIT, ABS_Y as u32).context("ABS_Y")?;

        let range = i16::MAX as i32;
        let abs_setup = |code: u16| UinputAbsSetup {
            code,
            _pad: [0; 2],
            absinfo: InputAbsinfo {
                value: 0,
                minimum: 0,
                maximum: range,
                fuzz: 0,
                flat: 0,
                resolution: 0,
            },
        };
        ioctl_ref(&fd, UI_ABS_SETUP, &abs_setup(ABS_X)).context("abs_setup X")?;
        ioctl_ref(&fd, UI_ABS_SETUP, &abs_setup(ABS_Y)).context("abs_setup Y")?;

        ioctl_val(&fd, UI_DEV_CREATE, 0).context("UI_DEV_CREATE")?;
        std::thread::sleep(std::time::Duration::from_millis(UINPUT_CREATE_WAIT_MS));

        // Separate REL device for CLI move command
        let fd_rel = create_virt_device(DEV_REL, &[BTN_LEFT], true);
        if let Err(ref e) = fd_rel {
            warn!("REL device unavailable — {e}");
        }

        Ok(Self { fd, fd_rel: fd_rel.ok(), screen_w, screen_h })
    }

    fn write_events(&mut self, events: &[InputEvent]) -> Result<()> {
        self.fd.write_all(bytemuck::cast_slice(events))?;
        Ok(())
    }

    fn make_event(type_: u16, code: u16, value: i32) -> InputEvent {
        InputEvent {
            time: libc::timeval {
                tv_sec: 0,
                tv_usec: 0,
            },
            type_,
            code,
            value,
        }
    }

    pub fn warp(&mut self, x: i16, y: i16) -> Result<()> {
        let abs_x = (x as f32 / self.screen_w as f32 * i16::MAX as f32) as i32;
        let abs_y = (y as f32 / self.screen_h as f32 * i16::MAX as f32) as i32;
        self.write_events(&[
            Self::make_event(EV_ABS, ABS_X, abs_x),
            Self::make_event(EV_ABS, ABS_Y, abs_y),
            Self::make_event(EV_SYN, SYN_REPORT, 0),
        ])?;
        Ok(())
    }

    pub fn move_rel(&mut self, dx: i32, dy: i32) -> Result<()> {
        let Some(ref mut fd) = self.fd_rel else { anyhow::bail!("REL device not available"); };
        let events = &[
            InputEvent { time: libc::timeval { tv_sec: 0, tv_usec: 0 }, type_: EV_REL, code: REL_X, value: dx },
            InputEvent { time: libc::timeval { tv_sec: 0, tv_usec: 0 }, type_: EV_REL, code: REL_Y, value: dy },
            InputEvent { time: libc::timeval { tv_sec: 0, tv_usec: 0 }, type_: EV_SYN, code: SYN_REPORT, value: 0 },
        ];
        fd.write_all(bytemuck::cast_slice(events))?;
        Ok(())
    }

    fn button_code(button: u8) -> u16 { button_hid(button) }

    pub fn button_press(&mut self, button: u8) -> Result<()> {
        let code = Self::button_code(button);
        self.write_events(&[
            Self::make_event(EV_KEY, code, 1),
            Self::make_event(EV_SYN, SYN_REPORT, 0),
        ])?;
        Ok(())
    }

    pub fn button_release(&mut self, button: u8) -> Result<()> {
        let code = Self::button_code(button);
        self.write_events(&[
            Self::make_event(EV_KEY, code, 0),
            Self::make_event(EV_SYN, SYN_REPORT, 0),
        ])?;
        Ok(())
    }

    pub fn click(&mut self, button: u8, count: u32) -> Result<()> {
        let half = std::time::Duration::from_millis(CLICK_INTERVAL_MS / 2);
        for _ in 0..count {
            self.button_press(button)?;
            std::thread::sleep(half);
            self.button_release(button)?;
            std::thread::sleep(half);
        }
        Ok(())
    }

    pub fn toggle(&mut self, button: u8) -> Result<()> {
        self.button_press(button)?;
        Ok(())
    }
}

impl Drop for Mouse {
    fn drop(&mut self) {
        let _ = ioctl_val(&self.fd, UI_DEV_DESTROY, 0);
        if let Some(ref fd) = self.fd_rel {
            let _ = ioctl_val(fd, UI_DEV_DESTROY, 0);
        }
    }
}
