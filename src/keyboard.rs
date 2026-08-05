// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::os::fd::{AsFd, AsRawFd, OwnedFd};
use std::os::unix::fs::OpenOptionsExt;
use std::time::Duration;

use anyhow::{Context, Result};
use log::info;
use nix::fcntl::OFlag;
use nix::sys::inotify::{AddWatchFlags, InitFlags, Inotify};

use crate::uinput::raw::InputEvent;
use bytemuck::Zeroable;

// ── evdev ioctl definitions (generated via nix) ─────────────────────

nix::ioctl_write_int!(eviocgrab, b'E', 0x90);
nix::ioctl_read!(eviocgname, b'E', 0x06, [u8; 80]);
nix::ioctl_read!(eviocgkey, b'E', 0x21, [u8; 96]);

fn is_own_device(fd: &OwnedFd) -> bool {
    let mut buf = [0u8; 80];
    if unsafe { eviocgname(fd.as_raw_fd(), &raw mut buf) }.is_err() {
        return false;
    }
    buf.starts_with(crate::config::OWN_PREFIX)
}

// ── Keyboard detection ─────────────────────────────────────────────

use crate::keymap::{
    KEY_0, KEY_1, KEY_2, KEY_3, KEY_4, KEY_5, KEY_6, KEY_7, KEY_8, KEY_9, KEY_A, KEY_B,
    KEY_BACKSPACE, KEY_C, KEY_D, KEY_E, KEY_ENTER, KEY_ESC, KEY_F, KEY_G, KEY_H, KEY_I, KEY_J,
    KEY_K, KEY_KP0, KEY_KP1, KEY_KP2, KEY_KP3, KEY_KP4, KEY_KP5, KEY_KP6, KEY_KP7, KEY_KP8,
    KEY_KP9, KEY_KPASTERISK, KEY_KPDOT, KEY_KPENTER, KEY_KPMINUS, KEY_KPPLUS, KEY_KPSLASH, KEY_L,
    KEY_M, KEY_N, KEY_NUMLOCK, KEY_O, KEY_P, KEY_Q, KEY_R, KEY_S, KEY_SPACE, KEY_T, KEY_U, KEY_V,
    KEY_W, KEY_X, KEY_Y, KEY_Z,
};

/// a-z, 0-9, space, enter, backspace, esc
const GRID_REQUIRED: &[u16] = &[
    KEY_ESC,
    KEY_BACKSPACE,
    KEY_ENTER,
    KEY_SPACE,
    KEY_1, KEY_2, KEY_3, KEY_4, KEY_5, KEY_6, KEY_7, KEY_8, KEY_9, KEY_0, // 0-9
    KEY_Q, KEY_W, KEY_E, KEY_R, KEY_T, KEY_Y, KEY_U, KEY_I, KEY_O, KEY_P, // Q-P
    KEY_A, KEY_S, KEY_D, KEY_F, KEY_G, KEY_H, KEY_J, KEY_K, KEY_L, // A-L
    KEY_Z, KEY_X, KEY_C, KEY_V, KEY_B, KEY_N, KEY_M, // Z-M
];
const GRID_MIN_KEYS: u32 = 40;

/// numpad 0-9, /*-+., numlock, numpad enter
const PAD_REQUIRED: &[u16] = &[
    KEY_KPASTERISK,
    KEY_NUMLOCK,
    KEY_KP7, KEY_KP8, KEY_KP9, // KP7-KP9
    KEY_KPMINUS,
    KEY_KP4, KEY_KP5, KEY_KP6, // KP4-KP6
    KEY_KPPLUS,
    KEY_KP1, KEY_KP2, KEY_KP3, // KP1-KP3
    KEY_KP0,
    KEY_KPDOT,
    KEY_KPENTER,
    KEY_KPSLASH,
];
const PAD_MIN_KEYS: u32 = 17;

fn read_key_bits(fd: &OwnedFd) -> Option<[u8; 96]> {
    let mut bits = [0u8; 96];
    if unsafe { eviocgkey(fd.as_raw_fd(), &raw mut bits) }.is_err() {
        return None;
    }
    Some(bits)
}

fn has_key(bits: &[u8; 96], code: u16) -> bool {
    let ix = code as usize;
    (bits[ix / 8] & (1 << (ix % 8))) != 0
}

fn count_keys(bits: &[u8; 96]) -> u32 {
    bits.iter().map(|&byte| byte.count_ones()).sum()
}

fn is_suitable(fd: &OwnedFd) -> bool {
    let Some(bits) = read_key_bits(fd) else {
        return false;
    };
    let grid_ok =
        count_keys(&bits) >= GRID_MIN_KEYS && GRID_REQUIRED.iter().all(|&c| has_key(&bits, c));
    let pad_ok =
        count_keys(&bits) >= PAD_MIN_KEYS && PAD_REQUIRED.iter().all(|&c| has_key(&bits, c));
    grid_ok || pad_ok
}

// ── Device management ──────────────────────────────────────────────

struct DeviceFd {
    fd: OwnedFd,
    name: String, // e.g. "event3"
}

fn release_grab(raw: i32) {
    let _ = unsafe { eviocgrab(raw, 0) };
}

/// Holds all grabbed keyboard devices.  Hot-plug via inotify on /dev/input/.
pub struct KeyboardDev {
    fds: Vec<DeviceFd>,
    inotify: Option<Inotify>,
    suspended: bool,
    pollfds: Vec<nix::poll::PollFd<'static>>,
}

impl KeyboardDev {
    pub fn open_all() -> Result<Self> {
        let ino = Inotify::init(InitFlags::IN_NONBLOCK).context("inotify_init")?;
        ino.add_watch(
            "/dev/input/",
            AddWatchFlags::IN_CREATE | AddWatchFlags::IN_DELETE,
        )
        .context("inotify watch /dev/input/")?;

        let mut devs = Self {
            fds: Vec::new(),
            inotify: Some(ino),
            suspended: false,
            pollfds: Vec::new(),
        };
        for name in event_device_names() {
            devs.try_add(&name);
        }
        if devs.fds.is_empty() {
            anyhow::bail!("no keyboard devices found in /dev/input/");
        }
        Ok(devs)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.fds.is_empty()
    }

    /// Poll for any input event with a timeout (ms).
    /// Hot-plug is driven by inotify — no periodic scanning.
    pub fn poll_event(&mut self, timeout_ms: i32) -> Result<Option<InputEvent>> {
        if self.fds.is_empty() {
            std::thread::sleep(Duration::from_millis(timeout_ms as u64));
            return Ok(None);
        }

        let nk = self.fds.len();
        let total = nk + 1;
        let ino_fd = self.inotify.as_ref().map(|i| i.as_fd().as_raw_fd());
        if self.pollfds.len() == total {
            for (i, d) in self.fds.iter().enumerate() {
                let fd = unsafe { std::os::fd::BorrowedFd::borrow_raw(d.fd.as_raw_fd()) };
                self.pollfds[i] = nix::poll::PollFd::new(fd, nix::poll::PollFlags::POLLIN);
            }
            if let Some(fd) = ino_fd {
                let fd = unsafe { std::os::fd::BorrowedFd::borrow_raw(fd) };
                self.pollfds[nk] = nix::poll::PollFd::new(fd, nix::poll::PollFlags::POLLIN);
            }
        } else {
            self.pollfds.clear();
            for d in &self.fds {
                let fd = unsafe { std::os::fd::BorrowedFd::borrow_raw(d.fd.as_raw_fd()) };
                self.pollfds
                    .push(nix::poll::PollFd::new(fd, nix::poll::PollFlags::POLLIN));
            }
            if let Some(fd) = ino_fd {
                let fd = unsafe { std::os::fd::BorrowedFd::borrow_raw(fd) };
                self.pollfds
                    .push(nix::poll::PollFd::new(fd, nix::poll::PollFlags::POLLIN));
            }
        }

        let ret = nix::poll::poll(&mut self.pollfds, timeout_ms as u16)?;
        if ret == 0 {
            return Ok(None);
        }

        for i in 0..total {
            let revents = self.pollfds[i]
                .revents()
                .unwrap_or(nix::poll::PollFlags::empty());
            if !revents.contains(nix::poll::PollFlags::POLLIN) {
                continue;
            }
            if i < nk {
                let mut ev: InputEvent = Zeroable::zeroed();
                let sz = std::mem::size_of::<InputEvent>();
                let bytes = bytemuck::bytes_of_mut(&mut ev);
                let fd = unsafe { std::os::fd::BorrowedFd::borrow_raw(self.fds[i].fd.as_raw_fd()) };
                let Ok(n) = nix::unistd::read(fd, bytes) else {
                    continue;
                };
                if n < sz {
                    continue;
                }
                return Ok(Some(ev));
            }
            // inotify fd (i == nk): read events, rescan only for event* files.
            if let Some(ref ino) = self.inotify {
                let events = ino.read_events()?;
                let has_event_device = events.iter().any(|e| {
                    e.name
                        .as_ref()
                        .is_some_and(|n| n.to_string_lossy().starts_with("event"))
                });
                if has_event_device {
                    self.maybe_rescan();
                }
            }
        }
        Ok(None)
    }

    // ── hot-plug ─────────────────────────────────────────────────

    fn maybe_rescan(&mut self) {
        if self.suspended {
            return;
        }

        let current = event_device_names();

        // Remove devices that disappeared
        self.fds.retain(|d| {
            if current.contains(&d.name) {
                true
            } else {
                info!("[evdev] lost {}", d.name);
                release_grab(d.fd.as_raw_fd());
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
        let Ok(fd) = open_device(&path) else { return };
        if is_own_device(&fd) || !is_suitable(&fd) {
            return;
        }
        if unsafe { eviocgrab(fd.as_raw_fd(), 1) }.is_err() {
            return;
        }
        info!("[evdev] added {name}");
        self.fds.push(DeviceFd {
            fd,
            name: name.to_owned(),
        });
        self.pollfds.clear(); // force rebuild next poll
    }
}

// ── helpers ────────────────────────────────────────────────────────

fn event_device_names() -> Vec<String> {
    let Ok(dir) = std::fs::read_dir("/dev/input/") else {
        return Vec::new();
    };
    dir.filter_map(std::result::Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("event"))
        .collect()
}

fn open_device(path: &str) -> Result<OwnedFd> {
    let fd = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(OFlag::O_NONBLOCK.bits())
        .open(path)
        .context(format!("open {path}"))?;
    Ok(fd.into())
}
