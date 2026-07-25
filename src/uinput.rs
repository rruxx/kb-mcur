use anyhow::{Context, Result};
use std::fs::File;
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;

use crate::config::CLICK_INTERVAL_MS;

#[repr(C)]
struct InputEvent {
    time: libc::timeval,
    type_: u16,
    code: u16,
    value: i32,
}

#[repr(C)]
struct UinputSetup {
    id: libc::input_id,
    name: [u8; 80],
    ff_effects_max: u32,
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

const UI_SET_EVBIT: u64 = 0x40045564;
const UI_SET_KEYBIT: u64 = 0x40045565;
const UI_SET_RELBIT: u64 = 0x40045566;
const UI_SET_ABSBIT: u64 = 0x40045567;
const UI_ABS_SETUP: u64 = 0x401C5504;
const UI_DEV_SETUP: u64 = 0x405C5503;
const UI_DEV_CREATE: u64 = 0x5501;
const UI_DEV_DESTROY: u64 = 0x5502;

const EV_SYN: u16 = 0;
const EV_KEY: u16 = 1;
const EV_REL: u16 = 2;
const EV_ABS: u16 = 3;

const REL_X: u16 = 0;
const REL_Y: u16 = 1;

const BTN_LEFT: u16 = 0x110;
const BTN_MIDDLE: u16 = 0x112;
const BTN_RIGHT: u16 = 0x111;
const BTN_TOUCH: u16 = 0x14a;

const ABS_X: u16 = 0;
const ABS_Y: u16 = 1;

const SYN_REPORT: u16 = 0;

pub struct Mouse {
    fd: File,
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
            id: libc::input_id { bustype: 0, vendor: 0, product: 0, version: 0 },
            name: {
                let mut n = [0u8; 80];
                n[..7].copy_from_slice(b"kb-mcur");
                n
            },
            ff_effects_max: 0,
        };
        ioctl_ref(&fd, UI_DEV_SETUP, &setup).context("UI_DEV_SETUP")?;

        ioctl(&fd, UI_SET_EVBIT, EV_KEY as u32).context("EV_KEY")?;
        ioctl(&fd, UI_SET_EVBIT, EV_REL as u32).context("EV_REL")?;
        ioctl(&fd, UI_SET_EVBIT, EV_ABS as u32).context("EV_ABS")?;

        ioctl(&fd, UI_SET_RELBIT, REL_X as u32).context("REL_X")?;
        ioctl(&fd, UI_SET_RELBIT, REL_Y as u32).context("REL_Y")?;
        ioctl(&fd, UI_SET_EVBIT, EV_SYN as u32).context("EV_SYN")?;

        ioctl(&fd, UI_SET_KEYBIT, BTN_LEFT as u32).context("BTN_LEFT")?;
        ioctl(&fd, UI_SET_KEYBIT, BTN_MIDDLE as u32).context("BTN_MIDDLE")?;
        ioctl(&fd, UI_SET_KEYBIT, BTN_RIGHT as u32).context("BTN_RIGHT")?;
        ioctl(&fd, UI_SET_KEYBIT, BTN_TOUCH as u32).context("BTN_TOUCH")?;

        ioctl(&fd, UI_SET_ABSBIT, ABS_X as u32).context("ABS_X")?;
        ioctl(&fd, UI_SET_ABSBIT, ABS_Y as u32).context("ABS_Y")?;

        let range = i16::MAX as i32;
        let abs_setup = |code: u16| UinputAbsSetup {
            code,
            _pad: [0; 2],
            absinfo: InputAbsinfo { value: 0, minimum: 0, maximum: range, fuzz: 0, flat: 0, resolution: 0 },
        };
        ioctl_ref(&fd, UI_ABS_SETUP, &abs_setup(ABS_X)).context("abs_setup X")?;
        ioctl_ref(&fd, UI_ABS_SETUP, &abs_setup(ABS_Y)).context("abs_setup Y")?;

        ioctl(&fd, UI_DEV_CREATE, 0).context("UI_DEV_CREATE")?;
        std::thread::sleep(std::time::Duration::from_millis(50)); // wait for compositor to recognise the new device

        Ok(Self { fd, screen_w, screen_h })
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

    fn event(type_: u16, code: u16, value: i32) -> InputEvent {
        InputEvent {
            time: libc::timeval { tv_sec: 0, tv_usec: 0 },
            type_,
            code,
            value,
        }
    }

    pub fn warp(&mut self, x: i16, y: i16) -> Result<()> {
        let abs_x = (x as f32 / self.screen_w as f32 * i16::MAX as f32) as i32;
        let abs_y = (y as f32 / self.screen_h as f32 * i16::MAX as f32) as i32;
        self.write_events(&[
            Self::event(EV_ABS, ABS_X, abs_x),
            Self::event(EV_ABS, ABS_Y, abs_y),
            Self::event(EV_SYN, SYN_REPORT, 0),
        ])?;
        Ok(())
    }

    pub fn move_rel(&mut self, dx: i32, dy: i32) -> Result<()> {
        self.write_events(&[
            Self::event(EV_REL, REL_X, dx),
            Self::event(EV_REL, REL_Y, dy),
            Self::event(EV_SYN, SYN_REPORT, 0),
        ])?;
        Ok(())
    }

    fn button_code(button: u8) -> u16 {
        match button {
            1 => BTN_LEFT,
            2 => BTN_MIDDLE,
            _ => BTN_RIGHT,
        }
    }

    pub fn button_press(&mut self, button: u8) -> Result<()> {
        let code = Self::button_code(button);
        self.write_events(&[
            Self::event(EV_KEY, code, 1),
            Self::event(EV_SYN, SYN_REPORT, 0),
        ])?;
        Ok(())
    }

    pub fn button_release(&mut self, button: u8) -> Result<()> {
        let code = Self::button_code(button);
        self.write_events(&[
            Self::event(EV_KEY, code, 0),
            Self::event(EV_SYN, SYN_REPORT, 0),
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
    }
}
