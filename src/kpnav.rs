// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::io::{Read, Write};
use std::os::unix::net::UnixListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::Instant;

use crate::{
    config::{self, BTN_LEFT, BTN_MIDDLE, BTN_RIGHT},
    evdev::{KeyboardDev, KeyboardFilter},
    keymap::{
        KEY_KP0, KEY_KP5, KEY_KP7, KEY_KP8, KEY_KP9, KEY_KPASTERISK, KEY_KPDOT,
        KEY_KPENTER, KEY_KPMINUS, KEY_KPPLUS, KEY_KPSLASH, KEY_NUMLOCK,
    },
    uio::{
        EV_KEY, EV_REL, EV_SYN, REL_HWHEEL, REL_WHEEL, REL_X, REL_Y, SYN_REPORT,
        create_virt_device, write_event, write_event_raw,
    },
};
use anyhow::Result;
use log::{error, info, warn};

// ── 方向映射 ────────────────────────────────────────────────────────

bitflags::bitflags! {
    /// Keypad direction (one bit per physical key).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct Dir: u8 {
        const UP    = 0x01; // 8
        const DOWN  = 0x02; // 2
        const LEFT  = 0x04; // 4
        const RIGHT = 0x08; // 6
        const UP_LEFT    = 0x10; // 7
        const UP_RIGHT   = 0x20; // 9
        const DOWN_LEFT  = 0x40; // 1
        const DOWN_RIGHT = 0x80; // 3
    }
}

impl Dir {
    fn from_keypad(code: u16) -> Option<Self> {
        use crate::keymap::{
            KEY_KP1, KEY_KP2, KEY_KP3, KEY_KP4, KEY_KP6, KEY_KP7, KEY_KP8, KEY_KP9,
        };
        match code {
            KEY_KP8 => Some(Dir::UP),
            KEY_KP2 => Some(Dir::DOWN),
            KEY_KP4 => Some(Dir::LEFT),
            KEY_KP6 => Some(Dir::RIGHT),
            KEY_KP7 => Some(Dir::UP_LEFT),
            KEY_KP9 => Some(Dir::UP_RIGHT),
            KEY_KP1 => Some(Dir::DOWN_LEFT),
            KEY_KP3 => Some(Dir::DOWN_RIGHT),
            _ => None,
        }
    }

    fn to_vector(self) -> (i32, i32) {
        match self {
            Dir::UP => (0, -1),
            Dir::DOWN => (0, 1),
            Dir::LEFT => (-1, 0),
            Dir::RIGHT => (1, 0),
            Dir::UP_LEFT => (-1, -1),
            Dir::UP_RIGHT => (1, -1),
            Dir::DOWN_LEFT => (-1, 1),
            Dir::DOWN_RIGHT => (1, 1),
            _ => (0, 0),
        }
    }
}

// ── 主循环状态 ──────────────────────────────────────────────────────

struct Kpd {
    toggle: bool,
    btn_5: u8,      // 1=left, 2=middle, 3=right
    btn_held: bool, // true if 0 (hold) was used to press the button
    numlock_held: bool,

    // 方向键状态（同 mouse mode 加速模型）
    dir_held: u8,
    dir_mask: Dir,
    dir_count: u32,
}

impl Kpd {
    fn new() -> Self {
        Self {
            toggle: false,
            btn_5: 1,
            btn_held: false,
            numlock_held: false,
            dir_held: 0,
            dir_mask: Dir::empty(),
            dir_count: 0,
        }
    }

    fn active(&self) -> bool {
        self.toggle
    }

    fn btn_code(&self) -> u16 {
        match self.btn_5 {
            2 => BTN_MIDDLE,
            3 => BTN_RIGHT,
            _ => BTN_LEFT,
        }
    }
}

// ── Watchdog: fix uinput device ownership ─────────────────────────

fn display_session_uid() -> Option<u32> {
    // Scan /run/user/* for Wayland sockets
    if let Ok(dir) = std::fs::read_dir("/run/user") {
        for entry in dir.flatten() {
            let uid_str = entry.file_name().to_string_lossy().into_owned();
            let uid: u32 = uid_str.parse().ok()?;
            for wn in ["wayland-0", "wayland-1"] {
                if entry.path().join(wn).exists() {
                    return Some(uid);
                }
            }
        }
    }
    // Fallback: X11
    let path = std::ffi::CString::new("/tmp/.X11-unix/X0").unwrap();
    if let Ok(st) = nix::sys::stat::stat(path.as_c_str())
        && st.st_uid != 0
    {
        return Some(st.st_uid);
    }
    None
}

fn watchdog() {
    let Some(session_uid) = display_session_uid() else {
        return;
    };

    let Ok(dir) = std::fs::read_dir("/sys/class/input/") else {
        return;
    };
    for entry in dir.flatten() {
        let ev_name = entry.file_name().to_string_lossy().into_owned();
        if !ev_name.starts_with("event") {
            continue;
        }

        let name_path = entry.path().join("device/name");
        let Ok(dev_name) = std::fs::read_to_string(&name_path) else {
            continue;
        };
        if !dev_name.trim().starts_with(crate::config::UINPUT_NAME) {
            continue;
        }

        let dev_path = format!("/dev/input/{ev_name}");
        let Ok(path_c) = std::ffi::CString::new(dev_path) else {
            continue;
        };
        let Ok(st) = nix::sys::stat::stat(path_c.as_c_str()) else {
            continue;
        };
        if st.st_uid != session_uid {
            let _ = nix::unistd::chown(
                path_c.as_c_str(),
                Some(nix::unistd::Uid::from_raw(session_uid)),
                Some(nix::unistd::Gid::from_raw(st.st_gid)),
            );
        }
    }
}

// ── 方向键事件处理 ──────────────────────────────────────────────────

/// Handle a single key event in mouse mode.
/// Returns `true` if the event was consumed (should not be forwarded).
fn handle_key_event(
    kpd: &mut Kpd,
    ptr_out: &mut std::fs::File,
    code: u16,
    value: i32,
    is_press: bool,
) -> Result<bool> {
    if kpd.numlock_held {
        match code {
            KEY_KPSLASH => {
                if is_press {
                    write_event(ptr_out, EV_REL, REL_WHEEL, 1)?;
                    write_event(ptr_out, EV_SYN, SYN_REPORT, 0)?;
                }
                return Ok(true);
            }
            KEY_KP8 => {
                if is_press {
                    write_event(ptr_out, EV_REL, REL_WHEEL, -1)?;
                    write_event(ptr_out, EV_SYN, SYN_REPORT, 0)?;
                }
                return Ok(true);
            }
            KEY_KP7 => {
                if is_press {
                    write_event(ptr_out, EV_REL, REL_HWHEEL, -1)?;
                    write_event(ptr_out, EV_SYN, SYN_REPORT, 0)?;
                }
                return Ok(true);
            }
            KEY_KP9 => {
                if is_press {
                    write_event(ptr_out, EV_REL, REL_HWHEEL, 1)?;
                    write_event(ptr_out, EV_SYN, SYN_REPORT, 0)?;
                }
                return Ok(true);
            }
            _ => {}
        }
    }

    match code {
        c if Dir::from_keypad(c).is_some() => {
            let flag = Dir::from_keypad(c).unwrap();
            if value == 0 {
                kpd.dir_mask.remove(flag);
                kpd.dir_held = kpd.dir_held.saturating_sub(1);
                if kpd.dir_held == 0 {
                    kpd.dir_count = 0;
                }
            } else if value == 1 {
                kpd.dir_mask.insert(flag);
                kpd.dir_held = kpd.dir_held.saturating_add(1);
            }
            Ok(true)
        }
        KEY_KP5 => {
            if value > 0 {
                write_event(ptr_out, EV_KEY, kpd.btn_code(), 1)?;
                write_event(ptr_out, EV_SYN, SYN_REPORT, 0)?;
                kpd.btn_held = true;
            } else if value == 0 && kpd.btn_held {
                write_event(ptr_out, EV_KEY, kpd.btn_code(), 0)?;
                write_event(ptr_out, EV_SYN, SYN_REPORT, 0)?;
                kpd.btn_held = false;
            }
            Ok(true)
        }
        KEY_KPDOT => {
            if is_press {
                write_event(ptr_out, EV_KEY, kpd.btn_code(), 0)?;
                write_event(ptr_out, EV_SYN, SYN_REPORT, 0)?;
                kpd.btn_held = false;
                info!("[release]");
            }
            Ok(true)
        }
        KEY_KP0 => {
            if value == 1 && !kpd.btn_held {
                write_event(ptr_out, EV_KEY, kpd.btn_code(), 1)?;
                write_event(ptr_out, EV_SYN, SYN_REPORT, 0)?;
                kpd.btn_held = true;
                info!("[hold]");
            }
            Ok(true)
        }
        KEY_KPASTERISK => {
            if is_press {
                kpd.btn_5 = 2;
                info!("[btn5=M]");
            }
            Ok(true)
        }
        KEY_KPSLASH => {
            if is_press {
                kpd.btn_5 = 1;
                info!("[btn5=L]");
            }
            Ok(true)
        }
        KEY_KPMINUS => {
            if is_press {
                kpd.btn_5 = 3;
                info!("[btn5=R]");
            }
            Ok(true)
        }
        KEY_KPPLUS => {
            if value == 1 {
                let code = kpd.btn_code();
                let half = std::time::Duration::from_millis(50);
                for _ in 0..2 {
                    write_event(ptr_out, EV_KEY, code, 1)?;
                    write_event(ptr_out, EV_SYN, SYN_REPORT, 0)?;
                    std::thread::sleep(half);
                    write_event(ptr_out, EV_KEY, code, 0)?;
                    write_event(ptr_out, EV_SYN, SYN_REPORT, 0)?;
                    std::thread::sleep(half);
                }
                info!("[dblclick]");
            }
            Ok(true)
        }
        _ => Ok(false),
    }
}

/// Emit a relative cursor movement for held direction keys.
fn do_direction_tick(kpd: &mut Kpd, ptr_out: &mut std::fs::File) -> Result<()> {
    if kpd.dir_held != 1 {
        return Ok(());
    }
    let (dx, dy) = kpd.dir_mask.to_vector();
    kpd.dir_count = kpd.dir_count.saturating_add(1);
    let step = config::cursor_speed(kpd.dir_count) as f32;
    let mx = (dx as f32 * step) as i32;
    let my = (dy as f32 * step) as i32;
    write_event(ptr_out, EV_REL, REL_X, mx)?;
    write_event(ptr_out, EV_REL, REL_Y, my)?;
    write_event(ptr_out, EV_SYN, SYN_REPORT, 0)?;
    Ok(())
}

// ── 主入口 ──────────────────────────────────────────────────────────

#[must_use]
pub fn socket_path() -> String {
    crate::config::SOCKET.to_string()
}

enum Cmd {
    Release,
    Reacquire,
}

fn socket_thread(cmd_tx: mpsc::Sender<Cmd>, ack_rx: mpsc::Receiver<()>) {
    let path = socket_path();
    let _ = std::fs::remove_file(&path);

    let listener = match UnixListener::bind(&path) {
        Ok(l) => {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o666)).ok();
            l
        }
        Err(e) => {
            error!("[socket] failed to bind {path}: {e}");
            return;
        }
    };

    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };

        let mut buf = [0u8; 16];
        let n = match stream.read(&mut buf) {
            Ok(0) | Err(_) => continue,
            Ok(n) => n,
        };

        if buf[..n].trim_ascii() != b"grid" {
            let _ = stream.write_all(b"ERR\n");
            continue;
        }

        info!("[socket] grid session requested");
        let _ = cmd_tx.send(Cmd::Release);
        let _ = ack_rx.recv(); // wait for main thread to release
        let _ = stream.write_all(b"OK\n");

        // Wait for client to disconnect (grid exits)
        let _ = stream.read(&mut buf);

        let _ = cmd_tx.send(Cmd::Reacquire);
        info!("[socket] grid session ended");
    }
}

extern "C" fn shutdown_signal(_: libc::c_int) {
    SHUTDOWN.store(true, Ordering::Relaxed);
}

static SHUTDOWN: AtomicBool = AtomicBool::new(false);

pub fn run() -> Result<()> {
    info!("kp-nav — NumLock+KPEnter to toggle");

    unsafe {
        libc::signal(libc::SIGINT, shutdown_signal as *const () as libc::sighandler_t);
        libc::signal(libc::SIGTERM, shutdown_signal as *const () as libc::sighandler_t);
    }

    let (cmd_tx, cmd_rx) = mpsc::channel::<Cmd>();
    let (ack_tx, ack_rx) = mpsc::channel::<()>();
    thread::spawn(move || socket_thread(cmd_tx, ack_rx));

    let mut kbd = KeyboardDev::open_all(KeyboardFilter::KpNav)?;

    let kbd_bits: Vec<u16> = (1u16..=255).collect();
    let mut kbd_out = create_virt_device(crate::config::DEV_KBD, &kbd_bits, false)?;
    let mut ptr_out = create_virt_device(
        crate::config::DEV_PTR,
        &[BTN_LEFT, BTN_MIDDLE, BTN_RIGHT],
        true,
    )?;

    let mut kpd = Kpd::new();

    // Release all keys via uinput to clear any "stuck key" state
    // on the compositor from the grab→uinput transition window.
    for code in 1u16..=255 {
        write_event(&mut kbd_out, EV_KEY, code, 0)?;
    }
    write_event(&mut kbd_out, EV_SYN, SYN_REPORT, 0)?;

    let mut warn_is_done = false;
    let mut last_wd = Instant::now();

    loop {
        if SHUTDOWN.load(Ordering::Relaxed) {
            info!("shutting down");
            break Ok(());
        }

        // ── Watchdog: fix uinput device ownership every second ──
        let now = Instant::now();
        if now.duration_since(last_wd) >= std::time::Duration::from_secs(1) {
            watchdog();
            last_wd = now;
        }

        // ── Socket commands (grid takeover) ──
        match cmd_rx.try_recv() {
            Ok(Cmd::Release) => {
                kbd.close_all();
                info!("[socket] released keyboards for grid session");
                let _ = ack_tx.send(());
            }
            Ok(Cmd::Reacquire) => {
                if let Ok(k) = KeyboardDev::open_all(KeyboardFilter::KpNav) {
                    kbd = k;
                    info!("[socket] re-acquired keyboards");
                }
            }
            Err(mpsc::TryRecvError::Disconnected | mpsc::TryRecvError::Empty) => {}
        }

        if kbd.is_empty() {
            if !warn_is_done {
                warn!("all keyboards gone");
            }
            warn_is_done = true;
        } else {
            warn_is_done = false;
        }
        let t_poll_start = Instant::now();
        match kbd.poll_event(32) {
            Ok(Some(ev)) => {
                let code = ev.code;
                let value = ev.value;
                let is_press = value > 0;

                if code == KEY_NUMLOCK {
                    kpd.numlock_held = value != 0;
                }

                if code == KEY_KPENTER && is_press && kpd.numlock_held {
                    kpd.toggle = !kpd.toggle;
                    info!(
                        "{}",
                        if kpd.active() {
                            "[mouse mode ON]"
                        } else {
                            "[pass-through]"
                        }
                    );
                    continue;
                }

                if kpd.active() && handle_key_event(&mut kpd, &mut ptr_out, code, value, is_press)?
                {
                    continue;
                }

                write_event_raw(&mut kbd_out, &ev)?;
            }
            Ok(None) => {
                do_direction_tick(&mut kpd, &mut ptr_out)?;
            }
            Err(e) => return Err(e),
        }
        let t_poll = t_poll_start.elapsed();
        if t_poll > std::time::Duration::from_millis(40) {
            warn!("poll {t_poll:?}");
        }
    }
}
