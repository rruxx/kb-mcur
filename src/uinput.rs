// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::{Context, Result};
use std::fs::File;
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;

use crate::config::{btn_code as button_hid, CLICK_INTERVAL_MS};

#[repr(C)]
pub struct InputEvent {
    pub time: libc::timeval,
    pub type_: u16,
    pub code: u16,
    pub value: i32,
}

#[repr(C)]
pub struct UinputSetup {
    pub id: libc::input_id,
    pub name: [u8; 80],
    pub ff_effects_max: u32,
}

#[repr(C)]
struct UinputAbsSetup {
    code: u16,
    _pad: [u8; 2],
    absinfo: InputAbsinfo,
}

#[repr(C)]
struct InputAbsinfo {
    value: i32,
    minimum: i32,
    maximum: i32,
    fuzz: i32,
    flat: i32,
    resolution: i32,
}

pub const UI_SET_EVBIT: u64 = 0x40045564;
pub const UI_SET_KEYBIT: u64 = 0x40045565;
pub const UI_SET_RELBIT: u64 = 0x40045566;
pub const UI_SET_ABSBIT: u64 = 0x40045567;
pub const UI_ABS_SETUP: u64 = 0x401C5504;
pub const UI_DEV_SETUP: u64 = 0x405C5503;
pub const UI_DEV_CREATE: u64 = 0x5501;
pub const UI_DEV_DESTROY: u64 = 0x5502;

pub const EV_SYN: u16 = 0;
pub const EV_KEY: u16 = 1;
pub const EV_REL: u16 = 2;
pub const EV_ABS: u16 = 3;

pub const REL_X: u16 = 0;
pub const REL_Y: u16 = 1;

pub const BTN_LEFT: u16 = 0x110;
pub const BTN_MIDDLE: u16 = 0x112;
pub const BTN_RIGHT: u16 = 0x111;

pub const ABS_X: u16 = 0;
pub const ABS_Y: u16 = 1;

pub const SYN_REPORT: u16 = 0;

pub struct Mouse {
    fd: File,
    fd_rel: Option<File>,
    screen_w: u16,
    screen_h: u16,
}

fn ioctl(fd: &File, request: u64, value: u32) -> io::Result<()> {
    let ret = unsafe { libc::ioctl(fd.as_raw_fd(), request, value as libc::c_ulong) };
    if ret < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn ioctl_ref<T>(fd: &File, request: u64, data: &T) -> io::Result<()> {
    let ret = unsafe { libc::ioctl(fd.as_raw_fd(), request, data as *const T as libc::c_ulong) };
    if ret < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
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
                let mut n = [0u8; 80];
                n[..crate::project::DEV_ABS.len()].copy_from_slice(crate::project::DEV_ABS.as_bytes());
                n
            },
            ff_effects_max: 0,
        };
        ioctl_ref(&fd, UI_DEV_SETUP, &setup).context("UI_DEV_SETUP")?;

        ioctl(&fd, UI_SET_EVBIT, EV_KEY as u32).context("EV_KEY")?;
        ioctl(&fd, UI_SET_EVBIT, EV_ABS as u32).context("EV_ABS")?;
        ioctl(&fd, UI_SET_EVBIT, EV_SYN as u32).context("EV_SYN")?;

        ioctl(&fd, UI_SET_KEYBIT, BTN_LEFT as u32).context("BTN_LEFT")?;
        ioctl(&fd, UI_SET_KEYBIT, BTN_MIDDLE as u32).context("BTN_MIDDLE")?;
        ioctl(&fd, UI_SET_KEYBIT, BTN_RIGHT as u32).context("BTN_RIGHT")?;

        ioctl(&fd, UI_SET_ABSBIT, ABS_X as u32).context("ABS_X")?;
        ioctl(&fd, UI_SET_ABSBIT, ABS_Y as u32).context("ABS_Y")?;

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

        ioctl(&fd, UI_DEV_CREATE, 0).context("UI_DEV_CREATE")?;
        std::thread::sleep(std::time::Duration::from_millis(50));

        // Separate REL device for CLI move command
        let fd_rel = create_rel_device();
        if let Err(ref e) = fd_rel {
            eprintln!("warn: REL device unavailable — {e}");
        }

        Ok(Self { fd, fd_rel: fd_rel.ok(), screen_w, screen_h })
    }

    fn write_events(&mut self, events: &[InputEvent]) -> Result<()> {
        let bytes = unsafe {
            std::slice::from_raw_parts(
                events.as_ptr() as *const u8,
                events.len() * std::mem::size_of::<InputEvent>(),
            )
        };
        self.fd.write_all(bytes)?;
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
        let bytes = unsafe { std::slice::from_raw_parts(events.as_ptr() as *const u8, events.len() * std::mem::size_of::<InputEvent>()) };
        use std::io::Write;
        fd.write_all(bytes)?;
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
        let _ = ioctl(&self.fd, UI_DEV_DESTROY, 0);
        if let Some(ref fd) = self.fd_rel {
            let _ = ioctl(fd, UI_DEV_DESTROY, 0);
        }
    }
}

fn create_rel_device() -> Result<File> {
    let fd = std::fs::OpenOptions::new()
        .write(true).custom_flags(libc::O_NONBLOCK).open("/dev/uinput")?;
    let setup = UinputSetup {
        id: libc::input_id { bustype: 0, vendor: 0, product: 0, version: 0 },
        name: { let mut n = [0u8; 80]; n[..crate::project::DEV_REL.len()].copy_from_slice(crate::project::DEV_REL.as_bytes()); n },
        ff_effects_max: 0,
    };
    ioctl_ref(&fd, UI_DEV_SETUP, &setup)?;
    ioctl(&fd, UI_SET_EVBIT, EV_REL as u32)?;
    ioctl(&fd, UI_SET_EVBIT, EV_KEY as u32)?;
    ioctl(&fd, UI_SET_EVBIT, EV_SYN as u32)?;
    ioctl(&fd, UI_SET_RELBIT, REL_X as u32)?;
    ioctl(&fd, UI_SET_RELBIT, REL_Y as u32)?;
    ioctl(&fd, UI_SET_KEYBIT, BTN_LEFT as u32)?;
    ioctl(&fd, UI_DEV_CREATE, 0)?;
    std::thread::sleep(std::time::Duration::from_millis(50));
    Ok(fd)
}
