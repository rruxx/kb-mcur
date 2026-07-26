// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::os::fd::{IntoRawFd, RawFd};
use std::os::unix::fs::OpenOptionsExt;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use crate::uinput::InputEvent;

const EVIOCGRAB: u64 = 0x40044590;
const RESCAN_INTERVAL: Duration = Duration::from_secs(1);

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
    name: String, // e.g. "event3"
}

/// Holds all grabbed keyboard devices.  Supports hot-plug via periodic
/// re-scan of /dev/input/.
pub struct KeyboardDev {
    fds: Vec<DeviceFd>,
    last_rescan: Instant,
}

impl KeyboardDev {
    pub fn open_all() -> Result<Self> {
        let mut devs = Self {
            fds: Vec::new(),
            last_rescan: Instant::now(),
        };
        for name in event_device_names() {
            devs.try_add(&name);
        }
        if devs.fds.is_empty() {
            anyhow::bail!("no keyboard devices found in /dev/input/");
        }
        Ok(devs)
    }

    pub fn is_empty(&self) -> bool {
        self.fds.is_empty()
    }

    /// Release all grabs, then close.
    pub fn release(&mut self) {
        for d in &self.fds {
            unsafe { libc::ioctl(d.fd, EVIOCGRAB, 0); }
        }
    }

    /// Block until a key event arrives, returns (code, value).
    pub fn next_keypress(&mut self) -> Result<(u16, i32)> {
        loop {
            if self.is_empty() {
                anyhow::bail!("all keyboards disconnected");
            }
            if let Some(ev) = self.poll_event(16)? {
                if ev.type_ == crate::uinput::EV_KEY {
                    return Ok((ev.code, ev.value));
                }
            }
        }
    }

    /// Poll for any input event with a timeout (ms).  Also performs
    /// periodic hot-plug rescan.
    pub fn poll_event(&mut self, timeout_ms: i32) -> Result<Option<InputEvent>> {
        self.maybe_rescan();

        if self.fds.is_empty() {
            std::thread::sleep(Duration::from_millis(timeout_ms as u64));
            return Ok(None);
        }

        let mut pfds: Vec<libc::pollfd> = self
            .fds
            .iter()
            .map(|d| libc::pollfd {
                fd: d.fd,
                events: libc::POLLIN,
                revents: 0,
            })
            .collect();

        let ret = unsafe { libc::poll(pfds.as_mut_ptr(), pfds.len() as libc::nfds_t, timeout_ms) };
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

    // ── hot-plug ─────────────────────────────────────────────────

    fn maybe_rescan(&mut self) {
        let now = Instant::now();
        if now.duration_since(self.last_rescan) < RESCAN_INTERVAL {
            return;
        }
        self.last_rescan = now;

        let current = event_device_names();

        // Remove devices that disappeared
        self.fds.retain(|d| {
            if current.contains(&d.name) {
                true
            } else {
                eprintln!("[evdev] lost {}", d.name);
                unsafe { libc::ioctl(d.fd, EVIOCGRAB, 0); }
                unsafe { libc::close(d.fd); }
                false
            }
        });

        // Add new devices
        for name in &current {
            if !self.fds.iter().any(|d| &d.name == name) {
                self.try_add(name);
            }
        }
    }

    fn try_add(&mut self, name: &str) {
        let path = format!("/dev/input/{name}");
        let fd = match open_device(&path) {
            Ok(f) => f,
            Err(_) => return,
        };
        if is_own_device(fd) || !is_keyboard(fd) {
            unsafe { libc::close(fd) };
            return;
        }
        if unsafe { libc::ioctl(fd, EVIOCGRAB, 1) } != 0 {
            unsafe { libc::close(fd) };
            return;
        }
        eprintln!("[evdev] added {}", name);
        self.fds.push(DeviceFd { fd, name: name.to_owned() });
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

fn event_device_names() -> Vec<String> {
    let Ok(dir) = std::fs::read_dir("/dev/input/") else {
        return Vec::new();
    };
    dir.filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("event"))
        .collect()
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
