// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::os::fd::{IntoRawFd, RawFd};
use std::os::unix::fs::OpenOptionsExt;

use anyhow::{Context, Result};

use crate::uinput::InputEvent;

const EVIOCGRAB: u64 = 0x40044590;

fn eviocgname(len: u16) -> u64 {
    (2u64 << 30) | ((len as u64) << 16) | (0x45u64 << 8) | 0x06
}

fn eviocgbit(ev_type: u32, len: usize) -> u64 {
    (2u64 << 30) | ((len as u64) << 16) | (0x45u64 << 8) | (0x20 + ev_type as u64)
}

fn is_own_device(fd: RawFd) -> bool {
    let mut buf = [0u8; 8];
    let ret = unsafe { libc::ioctl(fd, eviocgname(8), buf.as_mut_ptr()) };
    ret >= 0 && buf.starts_with(b"kb-")
}

/// Returns true if the device supports standard keyboard keycodes
/// (letter 'a', keypad '1', or arrow 'up').
fn is_keyboard(fd: RawFd) -> bool {
    let mut bits = [0u8; 96];
    let req = eviocgbit(1, 96); // EV_KEY = 1
    if unsafe { libc::ioctl(fd, req, bits.as_mut_ptr()) } < 0 {
        return false;
    }
    const KEY_A: usize = 30;
    const KEY_KP1: usize = 79;
    const KEY_UP: usize = 103;
    let has = |code: usize| -> bool { (bits[code / 8] & (1 << (code % 8))) != 0 };
    has(KEY_A) || has(KEY_KP1) || has(KEY_UP)
}

struct DeviceFd {
    fd: RawFd,
}

/// Holds all grabbed keyboard devices.
pub struct KeyboardDev {
    fds: Vec<DeviceFd>,
}

impl KeyboardDev {
    pub fn open_all() -> Result<Self> {
        let mut fds = Vec::new();
        for entry in glob_input_devices()? {
            match open_device(&entry) {
                Ok(fd) => {
                    if is_own_device(fd) {
                        unsafe { libc::close(fd) };
                        continue;
                    }
                    if !is_keyboard(fd) {
                        unsafe { libc::close(fd) };
                        continue;
                    }
                    if unsafe { libc::ioctl(fd, EVIOCGRAB, 1) } == 0 {
                        fds.push(DeviceFd { fd });
                    } else {
                        unsafe { libc::close(fd) };
                    }
                }
                Err(_) => {}
            }
        }
        if fds.is_empty() {
            anyhow::bail!("no keyboard devices found in /dev/input/");
        }
        Ok(Self { fds })
    }

    /// Release all grabs, then close.
    pub fn release(&mut self) {
        for d in &self.fds {
            unsafe {
                libc::ioctl(d.fd, EVIOCGRAB, 0);
            };
        }
    }

    /// Block until a key event arrives, returns (code, value).
    pub fn next_keypress(&self) -> Result<(u16, i32)> {
        loop {
            if let Some(ev) = self.poll_event(16)? {
                if ev.type_ == crate::uinput::EV_KEY {
                    return Ok((ev.code, ev.value));
                }
            }
        }
    }

    /// Poll for any input event with a timeout (ms). Returns the full
    /// event (including EV_SYN, EV_MSC, etc.).
    pub fn poll_event(&self, timeout_ms: i32) -> Result<Option<InputEvent>> {
        let n = self.fds.len();
        let mut pfds: Vec<libc::pollfd> = self
            .fds
            .iter()
            .map(|d| libc::pollfd {
                fd: d.fd,
                events: libc::POLLIN,
                revents: 0,
            })
            .collect();

        let ret = unsafe { libc::poll(pfds.as_mut_ptr(), n as libc::nfds_t, timeout_ms) };
        if ret < 0 {
            anyhow::bail!("poll failed");
        }
        if ret == 0 {
            return Ok(None);
        }

        for p in pfds {
            if p.revents & libc::POLLIN == 0 {
                continue;
            }
            let mut ev: InputEvent = unsafe { std::mem::zeroed() };
            let sz = std::mem::size_of::<InputEvent>();
            let bytes_read =
                unsafe { libc::read(p.fd, &mut ev as *mut _ as *mut libc::c_void, sz) };
            if (bytes_read as usize) < sz {
                continue;
            }
            return Ok(Some(ev));
        }
        Ok(None)
    }
}

impl Drop for KeyboardDev {
    fn drop(&mut self) {
        for d in &self.fds {
            unsafe { libc::close(d.fd) };
        }
    }
}

// ── helpers ────────────────────────────────────────────────────────

fn glob_input_devices() -> Result<Vec<String>> {
    let mut paths = Vec::new();
    let dir = std::fs::read_dir("/dev/input/").context("read /dev/input")?;
    for entry in dir {
        let name = entry?.file_name().to_string_lossy().into_owned();
        if name.starts_with("event") {
            paths.push(format!("/dev/input/{name}"));
        }
    }
    Ok(paths)
}

fn open_device(path: &str) -> Result<RawFd> {
    let f = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
        .context(format!("open {path}"))?;
    Ok(f.into_raw_fd())
}
