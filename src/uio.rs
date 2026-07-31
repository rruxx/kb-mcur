// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

// Shared uinput I/O: structs, ioctl definitions, device-creation helpers.

use std::fs::File;
use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;

use anyhow::{Context, Result};
use bytemuck::{Pod, Zeroable};
use nix::fcntl::OFlag;

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
    pub pad: [u8; 2],
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

// ── ioctl definitions (generated via nix, no magic numbers) ──────────

nix::ioctl_write_int!(ui_set_evbit, b'U', 0x64);
nix::ioctl_write_int!(ui_set_keybit, b'U', 0x65);
nix::ioctl_write_int!(ui_set_relbit, b'U', 0x66);
nix::ioctl_write_int!(ui_set_absbit, b'U', 0x67);
nix::ioctl_none!(ui_dev_create, b'U', 0x01);
nix::ioctl_none!(ui_dev_destroy, b'U', 0x02);
nix::ioctl_write_ptr!(ui_dev_setup, b'U', 0x03, UinputSetup);
nix::ioctl_write_ptr!(ui_abs_setup, b'U', 0x04, UinputAbsSetup);

// ── Event types & codes ──────────────────────────────────────────────

pub const EV_SYN: u16 = 0;
pub const EV_KEY: u16 = 1;
pub const EV_REL: u16 = 2;
pub const EV_ABS: u16 = 3;

pub const REL_X: u16 = 0;
pub const REL_Y: u16 = 1;
pub const REL_HWHEEL: u16 = 6;
pub const REL_WHEEL: u16 = 8;

pub const ABS_X: u16 = 0;
pub const ABS_Y: u16 = 1;

pub const SYN_REPORT: u16 = 0;

// ── Event writers ────────────────────────────────────────────────────

pub const ZERO_TIMEVAL: libc::timeval = libc::timeval {
    tv_sec: 0,
    tv_usec: 0,
};

pub fn write_event(fd: &mut File, type_: u16, code: u16, value: i32) -> std::io::Result<()> {
    let ev = InputEvent {
        time: ZERO_TIMEVAL,
        type_,
        code,
        value,
    };
    fd.write_all(bytemuck::bytes_of(&ev))
}

pub fn write_event_raw(fd: &mut File, ev: &InputEvent) -> std::io::Result<()> {
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
        .custom_flags(OFlag::O_NONBLOCK.bits())
        .open("/dev/uinput")
        .context("open /dev/uinput")?;

    let mut n = [0u8; UINPUT_NAME_MAXLEN];
    let len = name.len().min(UINPUT_NAME_MAXLEN);
    n[..len].copy_from_slice(&name.as_bytes()[..len]);
    let setup = UinputSetup {
        id: libc::input_id {
            bustype: 0,
            vendor: 0,
            product: 0,
            version: 0,
        },
        name: n,
        ff_effects_max: 0,
    };
    let raw = fd.as_raw_fd();
    unsafe { ui_dev_setup(raw, &raw const setup) }?;
    unsafe { ui_set_evbit(raw, EV_KEY.into()) }?;
    unsafe { ui_set_evbit(raw, EV_SYN.into()) }?;
    if rel {
        unsafe { ui_set_evbit(raw, EV_REL.into()) }?;
    }
    for &code in key_bits {
        unsafe { ui_set_keybit(raw, code.into()) }?;
    }
    if rel {
        unsafe { ui_set_relbit(raw, REL_X.into()) }?;
        unsafe { ui_set_relbit(raw, REL_Y.into()) }?;
        unsafe { ui_set_relbit(raw, REL_HWHEEL.into()) }?;
        unsafe { ui_set_relbit(raw, REL_WHEEL.into()) }?;
    }
    unsafe { ui_dev_create(raw) }?;
    std::thread::sleep(std::time::Duration::from_millis(UINPUT_CREATE_WAIT_MS));
    Ok(fd)
}
