// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::{fs::OpenOptionsExt, net::UnixListener};
use std::sync::mpsc;
use std::thread;
use std::time::Instant;

use anyhow::{Context, Result};
use libc::timeval;
use crate::{
    config,
    evdev::{KeyboardDev, KeyboardFilter},
    keymap::{
        KEY_KP1, KEY_KP2, KEY_KP3, KEY_KP4, KEY_KP5, KEY_KP6, KEY_KP7, KEY_KP8, KEY_KP9,
        KEY_KP0, KEY_KPASTERISK, KEY_KPENTER, KEY_KPDOT, KEY_KPMINUS, KEY_KPPLUS, KEY_KPSLASH, KEY_NUMLOCK,
    },
    uinput::{
        BTN_LEFT, BTN_MIDDLE, BTN_RIGHT, EV_KEY, EV_REL, EV_SYN, InputEvent, REL_X, REL_Y,
        SYN_REPORT, UI_DEV_CREATE, UI_DEV_SETUP, UI_SET_EVBIT, UI_SET_KEYBIT,
        UI_SET_RELBIT, UinputSetup,
    },
};

// ── uinput 设备创建 ─────────────────────────────────────────────────

fn ioctl_val(fd: &std::fs::File, request: u64, value: u32) -> io::Result<()> {
    let ret = unsafe { libc::ioctl(fd.as_raw_fd(), request, value as libc::c_ulong) };
    if ret < 0 { Err(io::Error::last_os_error()) } else { Ok(()) }
}

fn ioctl_ref<T>(fd: &std::fs::File, request: u64, data: &T) -> io::Result<()> {
    let ret = unsafe { libc::ioctl(fd.as_raw_fd(), request, data as *const T as libc::c_ulong) };
    if ret < 0 { Err(io::Error::last_os_error()) } else { Ok(()) }
}

fn create_virt_device(name: &str, key_bits: &[u16], rel: bool) -> Result<std::fs::File> {
    let fd = std::fs::OpenOptions::new()
        .write(true)
        .custom_flags(libc::O_NONBLOCK)
        .open("/dev/uinput")
        .context("open /dev/uinput")?;

    let mut n = [0u8; crate::project::UINPUT_NAME_MAXLEN];
    n[..name.len()].copy_from_slice(name.as_bytes());
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
    std::thread::sleep(std::time::Duration::from_millis(50));
    Ok(fd)
}

fn write_event(fd: &mut std::fs::File, type_: u16, code: u16, value: i32) -> io::Result<()> {
    let ev = InputEvent {
        time: timeval { tv_sec: 0, tv_usec: 0 },
        type_,
        code,
        value,
    };
    let bytes = unsafe {
        std::slice::from_raw_parts(&ev as *const _ as *const u8, std::mem::size_of::<InputEvent>())
    };
    fd.write_all(bytes)
}

fn write_event_raw(fd: &mut std::fs::File, ev: &InputEvent) -> io::Result<()> {
    let bytes = unsafe {
        std::slice::from_raw_parts(ev as *const _ as *const u8, std::mem::size_of::<InputEvent>())
    };
    fd.write_all(bytes)
}

// ── 方向映射 ────────────────────────────────────────────────────────

fn kp_direction(code: u16) -> Option<(i32, i32)> {
    match code {
        KEY_KP8 => Some((0, -1)),
        KEY_KP2 => Some((0, 1)),
        KEY_KP4 => Some((-1, 0)),
        KEY_KP6 => Some((1, 0)),
        KEY_KP7 => Some((-1, -1)),
        KEY_KP9 => Some((1, -1)),
        KEY_KP1 => Some((-1, 1)),
        KEY_KP3 => Some((1, 1)),
        _ => None,
    }
}

fn is_kp_direction(code: u16) -> bool {
    kp_direction(code).is_some()
}

// ── 主循环状态 ──────────────────────────────────────────────────────

struct Kpd {
    toggle: bool,
    btn_5: u8,           // 1=left, 2=middle, 3=right
    btn_held: bool,      // true if 0 (hold) was used to press the button
    numlock_held: bool,

    // 方向键状态（同 mouse mode 加速模型）
    dir_held: u8,
    dir_mask: u8,
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
            dir_mask: 0,
            dir_count: 0,
        }
    }

    fn active(&self) -> bool {
        self.toggle
    }

    fn kp_bit(code: u16) -> u8 {
        match code {
            KEY_KP8 => 0x01,
            KEY_KP2 => 0x02,
            KEY_KP4 => 0x04,
            KEY_KP6 => 0x08,
            KEY_KP7 => 0x10,
            KEY_KP9 => 0x20,
            KEY_KP1 => 0x40,
            KEY_KP3 => 0x80,
            _ => 0x00,
        }
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
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    if unsafe { libc::stat(path.as_ptr(), &mut st) } == 0 && st.st_uid != 0 {
        return Some(st.st_uid);
    }
    None
}

fn watchdog() {
    let Some(session_uid) = display_session_uid() else { return };

    let Ok(dir) = std::fs::read_dir("/sys/class/input/") else { return };
    for entry in dir.flatten() {
        let ev_name = entry.file_name().to_string_lossy().into_owned();
        if !ev_name.starts_with("event") { continue; }

        let name_path = entry.path().join("device/name");
        let Ok(dev_name) = std::fs::read_to_string(&name_path) else { continue };
        if !dev_name.trim().starts_with(crate::project::UINPUT_NAME) { continue; }

        let dev_path = format!("/dev/input/{ev_name}");
        let Ok(path_c) = std::ffi::CString::new(dev_path) else { continue };
        let mut st: libc::stat = unsafe { std::mem::zeroed() };
        if unsafe { libc::stat(path_c.as_ptr(), &mut st) } != 0 { continue; }
        if st.st_uid != session_uid {
            unsafe { libc::chown(path_c.as_ptr(), session_uid, st.st_gid) };
        }
    }
}

// ── 主入口 ──────────────────────────────────────────────────────────

pub fn socket_path() -> String {
    crate::project::SOCKET.to_string()
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
            let cpath = std::ffi::CString::new(path.as_str()).unwrap();
            unsafe { libc::chmod(cpath.as_ptr(), 0o666) };
            l
        }
        Err(e) => {
            eprintln!("[socket] failed to bind {path}: {e}");
            return;
        }
    };

    for stream in listener.incoming() {
        let mut stream = match stream {
            Ok(s) => s,
            Err(_) => continue,
        };

        let mut buf = [0u8; 16];
        let n = match stream.read(&mut buf) {
            Ok(0) => continue,
            Ok(n) => n,
            Err(_) => continue,
        };

        if buf[..n].trim_ascii() != b"grid" {
            let _ = stream.write_all(b"ERR\n");
            continue;
        }

        eprintln!("[socket] grid session requested");
        let _ = cmd_tx.send(Cmd::Release);
        let _ = ack_rx.recv(); // wait for main thread to release
        let _ = stream.write_all(b"OK\n");

        // Wait for client to disconnect (grid exits)
        let _ = stream.read(&mut buf);

        let _ = cmd_tx.send(Cmd::Reacquire);
        eprintln!("[socket] grid session ended");
    }
}

pub fn run() -> Result<()> {
    eprintln!("kp-nav — NumLock+KPEnter to toggle");

    let (cmd_tx, cmd_rx) = mpsc::channel::<Cmd>();
    let (ack_tx, ack_rx) = mpsc::channel::<()>();
    thread::spawn(move || socket_thread(cmd_tx, ack_rx));

    let mut kbd = KeyboardDev::open_all(KeyboardFilter::KpNav)?;

    let kbd_bits: Vec<u16> = (1u16..=255).collect();
    let mut kbd_out = create_virt_device(crate::project::DEV_KBD, &kbd_bits, false)?;
    let mut ptr_out = create_virt_device(
        crate::project::DEV_PTR,
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
                eprintln!("[socket] released keyboards for grid session");
                let _ = ack_tx.send(());
            }
            Ok(Cmd::Reacquire) => {
                if let Ok(k) = KeyboardDev::open_all(KeyboardFilter::KpNav) {
                    kbd = k;
                    eprintln!("[socket] re-acquired keyboards");
                }
            }
            Err(mpsc::TryRecvError::Disconnected) => {}  // socket thread exited, keep serving
            Err(mpsc::TryRecvError::Empty) => {}
        }

        if kbd.is_empty() {
            if !warn_is_done {
                eprintln!("[warn] all keyboards gone");
            }
            warn_is_done = true;
        } else {
            warn_is_done = false;
        }
        match kbd.poll_event(32) {
            Ok(Some(ev)) => {
                let code = ev.code;
                let value = ev.value;
                let is_press = value > 0;

                // ── NumLock 追踪 ──
                if code == KEY_NUMLOCK {
                    if value == 1 {
                        kpd.numlock_held = true;
                    } else if value == 0 {
                        kpd.numlock_held = false;
                    }
                }

                // ── NumLock+KPEnter 切换 ──
                if code == KEY_KPENTER && is_press && kpd.numlock_held {
                    kpd.toggle = !kpd.toggle;
                    if kpd.active() {
                        eprintln!("[mouse mode ON]");
                    } else {
                        eprintln!("[pass-through]");
                    }
                    // 吃掉组合键，不转发
                    continue;
                }

                let active = kpd.active();

                // ── 鼠标模式下处理 NumPad ──
                if active {
                    let handled = match code {
                        // 方向键
                        c if is_kp_direction(c) => {
                            if value == 0 {
                                let bit = Kpd::kp_bit(c);
                                kpd.dir_mask &= !bit;
                                kpd.dir_held = kpd.dir_held.saturating_sub(1);
                                if kpd.dir_held == 0 {
                                    kpd.dir_count = 0;
                                }
                            } else if value == 1 {
                                let bit = Kpd::kp_bit(c);
                                kpd.dir_mask |= bit;
                                kpd.dir_held = kpd.dir_held.saturating_add(1);
                            }
                            true
                        }
                        // 5 键：按/松鼠标按钮
                        KEY_KP5 => {
                            if value > 0 {
                                write_event(&mut ptr_out, EV_KEY, kpd.btn_code(), 1)?;
                                write_event(&mut ptr_out, EV_SYN, SYN_REPORT, 0)?;
                                kpd.btn_held = true;
                            } else if value == 0 {
                                // Only release if 5 was the one that pressed
                                // (0-hold releases via . instead)
                                if kpd.btn_held {
                                    write_event(&mut ptr_out, EV_KEY, kpd.btn_code(), 0)?;
                                    write_event(&mut ptr_out, EV_SYN, SYN_REPORT, 0)?;
                                    kpd.btn_held = false;
                                }
                            }
                            true
                        }
                        // . * / - 切换 5 的按钮模式
                        KEY_KPDOT => {
                            if is_press {
                                write_event(&mut ptr_out, EV_KEY, kpd.btn_code(), 0)?;
                                write_event(&mut ptr_out, EV_SYN, SYN_REPORT, 0)?;
                                kpd.btn_held = false;
                                eprintln!("[release]");
                            }
                            true
                        }
                        KEY_KP0 => {
                            // Hold: press button down and keep it held
                            if value == 1 && !kpd.btn_held {
                                write_event(&mut ptr_out, EV_KEY, kpd.btn_code(), 1)?;
                                write_event(&mut ptr_out, EV_SYN, SYN_REPORT, 0)?;
                                kpd.btn_held = true;
                                eprintln!("[hold]");
                            }
                            true
                        }
                        KEY_KPASTERISK => {
                            if is_press {
                                kpd.btn_5 = 2;
                                eprintln!("[btn5=M]");
                            }
                            true
                        }
                        KEY_KPSLASH => {
                            if is_press {
                                kpd.btn_5 = 1;
                                eprintln!("[btn5=L]");
                            }
                            true
                        }
                        KEY_KPMINUS => {
                            if is_press {
                                kpd.btn_5 = 3;
                                eprintln!("[btn5=R]");
                            }
                            true
                        }
                        KEY_KPPLUS => {
                            if value == 1 {
                                let code = kpd.btn_code();
                                let half = std::time::Duration::from_millis(50);
                                write_event(&mut ptr_out, EV_KEY, code, 1)?;
                                write_event(&mut ptr_out, EV_SYN, SYN_REPORT, 0)?;
                                std::thread::sleep(half);
                                write_event(&mut ptr_out, EV_KEY, code, 0)?;
                                write_event(&mut ptr_out, EV_SYN, SYN_REPORT, 0)?;
                                std::thread::sleep(half);
                                write_event(&mut ptr_out, EV_KEY, code, 1)?;
                                write_event(&mut ptr_out, EV_SYN, SYN_REPORT, 0)?;
                                std::thread::sleep(half);
                                write_event(&mut ptr_out, EV_KEY, code, 0)?;
                                write_event(&mut ptr_out, EV_SYN, SYN_REPORT, 0)?;
                                eprintln!("[dblclick]");
                            }
                            true
                        }
                        _ => false,
                    };

                    if handled {
                        continue;
                    }
                }

                // ── 直通模式 / 非 NumPad 键：原样转发 ──
                write_event_raw(&mut kbd_out, &ev)?;
            }
            Ok(None) => {
                // 32 ms 定时器：单方向键移动
                if kpd.active() && kpd.dir_held == 1 {
                    let (dx, dy) = direction_from_mask(kpd.dir_mask);
                    kpd.dir_count = kpd.dir_count.saturating_add(1);
                    let step = config::cursor_speed(kpd.dir_count) as f32;
                    let mx = (dx as f32 * step) as i32;
                    let my = (dy as f32 * step) as i32;
                    write_event(&mut ptr_out, EV_REL, REL_X, mx)?;
                    write_event(&mut ptr_out, EV_REL, REL_Y, my)?;
                    write_event(&mut ptr_out, EV_SYN, SYN_REPORT, 0)?;
                }
            }
            Err(e) => return Err(e),
        }
    }
}

fn direction_from_mask(mask: u8) -> (i32, i32) {
    match mask {
        0x01 => (0, -1),   // 8
        0x02 => (0, 1),    // 2
        0x04 => (-1, 0),   // 4
        0x08 => (1, 0),    // 6
        0x10 => (-1, -1),  // 7
        0x20 => (1, -1),   // 9
        0x40 => (-1, 1),   // 1
        0x80 => (1, 1),    // 3
        _ => (0, 0),
    }
}
