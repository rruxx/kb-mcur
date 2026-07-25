// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::os::fd::{IntoRawFd, RawFd};
use std::os::unix::fs::OpenOptionsExt;

use anyhow::{Context, Result};

use crate::uinput::{EV_KEY, InputEvent};

const EVIOCGRAB: u64 = 0x40044590;

fn eviocgname(len: u16) -> u64 {
    (2u64 << 30) | ((len as u64) << 16) | (0x45u64 << 8) | 0x06
}

fn device_name(fd: RawFd) -> String {
    let mut buf = [0u8; 80];
    let ret = unsafe { libc::ioctl(fd, eviocgname(80), buf.as_mut_ptr()) };
    if ret < 0 {
        return String::new();
    }
    let len = buf.iter().position(|&b| b == 0).unwrap_or(80);
    String::from_utf8_lossy(&buf[..len]).into_owned()
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
                    if device_name(fd) == "kb-mcur" {
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

            if unsafe { libc::poll(pfds.as_mut_ptr(), n as libc::nfds_t, -1) } < 0 {
                anyhow::bail!("poll failed");
            }

            for p in pfds {
                if p.revents & libc::POLLIN == 0 {
                    continue;
                }
                let mut ev: InputEvent = unsafe { std::mem::zeroed() };
                let sz = std::mem::size_of::<InputEvent>();
                let n = unsafe { libc::read(p.fd, &mut ev as *mut _ as *mut libc::c_void, sz) };
                if (n as usize) < sz {
                    continue;
                }
                if ev.type_ == EV_KEY {
                    return Ok((ev.code, ev.value));
                }
            }
        }
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
