// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::{Context, Result};
use log::warn;
use std::fs::File;
use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;

use crate::config::{
    BTN_LEFT, BTN_MIDDLE, BTN_RIGHT, CLICK_INTERVAL_MS, DEV_ABS, DEV_REL, UINPUT_CREATE_WAIT_MS,
    UINPUT_NAME_MAXLEN, btn_code as button_hid,
};
use crate::uio::{
    ABS_X, ABS_Y, EV_ABS, EV_KEY, EV_REL, EV_SYN, InputAbsinfo, InputEvent, REL_X, REL_Y,
    SYN_REPORT, UinputAbsSetup, UinputSetup, create_virt_device, ui_abs_setup, ui_dev_create,
    ui_dev_destroy, ui_dev_setup, ui_set_absbit, ui_set_evbit, ui_set_keybit,
};

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
        let raw = fd.as_raw_fd();
        unsafe { ui_dev_setup(raw, &raw const setup) }.context("UI_DEV_SETUP")?;

        unsafe { ui_set_evbit(raw, EV_KEY.into()) }.context("EV_KEY")?;
        unsafe { ui_set_evbit(raw, EV_ABS.into()) }.context("EV_ABS")?;
        unsafe { ui_set_evbit(raw, EV_SYN.into()) }.context("EV_SYN")?;

        unsafe { ui_set_keybit(raw, BTN_LEFT.into()) }.context("BTN_LEFT")?;
        unsafe { ui_set_keybit(raw, BTN_MIDDLE.into()) }.context("BTN_MIDDLE")?;
        unsafe { ui_set_keybit(raw, BTN_RIGHT.into()) }.context("BTN_RIGHT")?;

        unsafe { ui_set_absbit(raw, ABS_X.into()) }.context("ABS_X")?;
        unsafe { ui_set_absbit(raw, ABS_Y.into()) }.context("ABS_Y")?;

        let range = i32::from(i16::MAX);
        let abs_setup = |code: u16| UinputAbsSetup {
            code,
            pad: [0; 2],
            absinfo: InputAbsinfo {
                value: 0,
                minimum: 0,
                maximum: range,
                fuzz: 0,
                flat: 0,
                resolution: 0,
            },
        };
        unsafe { ui_abs_setup(raw, &abs_setup(ABS_X)) }.context("abs_setup X")?;
        unsafe { ui_abs_setup(raw, &abs_setup(ABS_Y)) }.context("abs_setup Y")?;

        unsafe { ui_dev_create(raw) }?;
        std::thread::sleep(std::time::Duration::from_millis(UINPUT_CREATE_WAIT_MS));

        let fd_rel = create_virt_device(DEV_REL, &[BTN_LEFT], true);
        if let Err(ref e) = fd_rel {
            warn!("REL device unavailable — {e}");
        }

        Ok(Self {
            fd,
            fd_rel: fd_rel.ok(),
            screen_w,
            screen_h,
        })
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
        let abs_x = (f32::from(x) / f32::from(self.screen_w) * f32::from(i16::MAX)) as i32;
        let abs_y = (f32::from(y) / f32::from(self.screen_h) * f32::from(i16::MAX)) as i32;
        self.write_events(&[
            Self::make_event(EV_ABS, ABS_X, abs_x),
            Self::make_event(EV_ABS, ABS_Y, abs_y),
            Self::make_event(EV_SYN, SYN_REPORT, 0),
        ])?;
        Ok(())
    }

    pub fn move_rel(&mut self, dx: i32, dy: i32) -> Result<()> {
        let Some(ref mut fd) = self.fd_rel else {
            anyhow::bail!("REL device not available");
        };
        let events = &[
            InputEvent {
                time: libc::timeval {
                    tv_sec: 0,
                    tv_usec: 0,
                },
                type_: EV_REL,
                code: REL_X,
                value: dx,
            },
            InputEvent {
                time: libc::timeval {
                    tv_sec: 0,
                    tv_usec: 0,
                },
                type_: EV_REL,
                code: REL_Y,
                value: dy,
            },
            InputEvent {
                time: libc::timeval {
                    tv_sec: 0,
                    tv_usec: 0,
                },
                type_: EV_SYN,
                code: SYN_REPORT,
                value: 0,
            },
        ];
        fd.write_all(bytemuck::cast_slice(events))?;
        Ok(())
    }

    fn button_code(button: u8) -> u16 {
        button_hid(button)
    }

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
        let _ = unsafe { ui_dev_destroy(self.fd.as_raw_fd()) };
        if let Some(ref fd) = self.fd_rel {
            let _ = unsafe { ui_dev_destroy(fd.as_raw_fd()) };
        }
    }
}
