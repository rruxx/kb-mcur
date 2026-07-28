// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

// Shared uinput I/O: structs, ioctl constants, device-creation helpers.

use std::fs::File;
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;

use anyhow::{Context, Result};
use bytemuck::{Pod, Zeroable};

use crate::config::{UINPUT_CREATE_WAIT_MS, UINPUT_NAME_MAXLEN};

// ── Structs ──────────────────────────────────────────────────────────

#[derive(Copy, Clone)]
#[repr(C)]
pub struct InputEvent {
    pub time: libc::timeval,
    pub type_: u16,
    pub code: u16,
    pub value: i32,
}

// SAFETY: InputEvent is #[repr(C)] with all-primitive fields;
// every bit pattern is a valid value on Linux.
unsafe impl Zeroable for InputEvent {}
unsafe impl Pod for InputEvent {}

#[repr(C)]
pub struct UinputSetup {
    pub id: libc::input_id,
    pub name: [u8; UINPUT_NAME_MAXLEN],
    pub ff_effects_max: u32,
}

#[repr(C)]
pub struct UinputAbsSetup {
    pub code: u16,
    pub _pad: [u8; 2],
    pub absinfo: InputAbsinfo,
}

#[repr(C)]
pub struct InputAbsinfo {
    pub value: i32,
    pub minimum: i32,
    pub maximum: i32,
    pub fuzz: i32,
    pub flat: i32,
    pub resolution: i32,
}

// ── ioctl numbers ────────────────────────────────────────────────────

pub const UI_SET_EVBIT: u64 = 0x40045564;
pub const UI_SET_KEYBIT: u64 = 0x40045565;
pub const UI_SET_RELBIT: u64 = 0x40045566;
pub const UI_SET_ABSBIT: u64 = 0x40045567;
pub const UI_ABS_SETUP: u64 = 0x401C5504;
pub const UI_DEV_SETUP: u64 = 0x405C5503;
pub const UI_DEV_CREATE: u64 = 0x5501;
pub const UI_DEV_DESTROY: u64 = 0x5502;

// ── Event types & codes ──────────────────────────────────────────────

pub const EV_SYN: u16 = 0;
pub const EV_KEY: u16 = 1;
pub const EV_REL: u16 = 2;
pub const EV_ABS: u16 = 3;

pub const REL_X: u16 = 0;
pub const REL_Y: u16 = 1;

pub const ABS_X: u16 = 0;
pub const ABS_Y: u16 = 1;

pub const SYN_REPORT: u16 = 0;

// ── ioctl helpers ────────────────────────────────────────────────────

pub fn ioctl_val(fd: &File, request: u64, value: u32) -> io::Result<()> {
    let ret = unsafe { libc::ioctl(fd.as_raw_fd(), request, value as libc::c_ulong) };
    if ret < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

pub fn ioctl_ref<T>(fd: &File, request: u64, data: &T) -> io::Result<()> {
    let ret = unsafe { libc::ioctl(fd.as_raw_fd(), request, data as *const T as libc::c_ulong) };
    if ret < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

// ── Event writers ────────────────────────────────────────────────────

pub fn write_event(fd: &mut File, type_: u16, code: u16, value: i32) -> io::Result<()> {
    let ev = InputEvent {
        time: libc::timeval { tv_sec: 0, tv_usec: 0 },
        type_,
        code,
        value,
    };
    fd.write_all(bytemuck::bytes_of(&ev))
}

pub fn write_event_raw(fd: &mut File, ev: &InputEvent) -> io::Result<()> {
    fd.write_all(bytemuck::bytes_of(ev))
}

// ── Device creation ──────────────────────────────────────────────────

/// Open `/dev/uinput`, register a virtual device, and return its fd.
///
/// `name`    – device name (truncated to `UINPUT_NAME_MAXLEN` bytes).
/// `key_bits` – button/key codes to enable via `UI_SET_KEYBIT`.
/// `rel`     – if true, also enable `EV_REL`, `REL_X`, and `REL_Y`.
pub fn create_virt_device(name: &str, key_bits: &[u16], rel: bool) -> Result<File> {
    let fd = File::options()
        .write(true)
        .custom_flags(libc::O_NONBLOCK)
        .open("/dev/uinput")
        .context("open /dev/uinput")?;

    let mut n = [0u8; UINPUT_NAME_MAXLEN];
    n[..name.len().min(UINPUT_NAME_MAXLEN)].copy_from_slice(&name.as_bytes()[..name.len().min(UINPUT_NAME_MAXLEN)]);
    let setup = UinputSetup {
        id: libc::input_id { bustype: 0, vendor: 0, product: 0, version: 0 },
        name: n,
        ff_effects_max: 0,
    };
    ioctl_ref(&fd, UI_DEV_SETUP, &setup)?;
    ioctl_val(&fd, UI_SET_EVBIT, EV_KEY as u32)?;
    ioctl_val(&fd, UI_SET_EVBIT, EV_SYN as u32)?;
    if rel {
        ioctl_val(&fd, UI_SET_EVBIT, EV_REL as u32)?;
    }
    for &code in key_bits {
        ioctl_val(&fd, UI_SET_KEYBIT, code as u32)?;
    }
    if rel {
        ioctl_val(&fd, UI_SET_RELBIT, REL_X as u32)?;
        ioctl_val(&fd, UI_SET_RELBIT, REL_Y as u32)?;
    }
    ioctl_val(&fd, UI_DEV_CREATE, 0)?;
    std::thread::sleep(std::time::Duration::from_millis(UINPUT_CREATE_WAIT_MS));
    Ok(fd)
}
