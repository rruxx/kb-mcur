// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use crate::{
    config::{self, BTN_EXTRA, BTN_LEFT, BTN_MIDDLE, BTN_RIGHT, BTN_SIDE},
    evdev::{KeyboardDev, KeyboardFilter},
    keymap::{
        KEY_CAPSLOCK, KEY_KP0, KEY_KP5, KEY_KP7, KEY_KP8, KEY_KP9, KEY_KPASTERISK, KEY_KPDOT,
        KEY_KPENTER, KEY_KPMINUS, KEY_KPPLUS, KEY_KPSLASH, KEY_LEFTMETA, KEY_NUMLOCK,
        KEY_RIGHTMETA, ModState, map as key_map,
    },
    uio::{
        EV_KEY, EV_REL, EV_SYN, REL_HWHEEL, REL_WHEEL, REL_X, REL_Y, SYN_REPORT,
        create_virt_device, write_event, write_event_raw,
    },
};
use anyhow::Result;
use log::{info, warn};

use crate::{
    DrawState, FONT_DATA, GridCtx, init_overlay, overlay::Overlay, process_byte, uinput::Mouse,
};

// ── 方向映射 ────────────────────────────────────────────────────────

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct Dir: u8 {
        const UP    = 0x01;
        const DOWN  = 0x02;
        const LEFT  = 0x04;
        const RIGHT = 0x08;
        const UP_LEFT    = 0x10;
        const UP_RIGHT   = 0x20;
        const DOWN_LEFT  = 0x40;
        const DOWN_RIGHT = 0x80;
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

// ── kp-nav 状态 ─────────────────────────────────────────────────────

struct Kpd {
    toggle: bool,
    btn_5: u8,
    btn_held: bool,
    numlock_held: bool,
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
    let path = std::ffi::CString::new("/tmp/.X11-unix/X0").unwrap();
    if let Ok(st) = nix::sys::stat::stat(path.as_c_str())
        && st.st_uid != 0
    {
        return Some(st.st_uid);
    }
    None
}

fn setup_display_env(uid: u32) {
    let run_user = format!("/run/user/{uid}");

    for wn in ["wayland-1", "wayland-0"] {
        if std::path::Path::new(&format!("{run_user}/{wn}")).exists() {
            unsafe {
                std::env::set_var("WAYLAND_DISPLAY", wn);
                std::env::set_var("XDG_RUNTIME_DIR", &run_user);
            }
            return;
        }
    }

    let home = nix::unistd::User::from_uid(nix::unistd::Uid::from_raw(uid))
        .ok()
        .flatten()
        .map_or_else(
            || format!("/home/{uid}"),
            |u| u.dir.to_string_lossy().into_owned(),
        );
    unsafe {
        std::env::set_var("DISPLAY", ":0");
        std::env::set_var("HOME", &home);
    }
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

// ── kp-nav 事件处理 ─────────────────────────────────────────────────

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
            KEY_KPASTERISK => {
                if is_press {
                    write_event(ptr_out, EV_KEY, BTN_SIDE, 1)?;
                    write_event(ptr_out, EV_SYN, SYN_REPORT, 0)?;
                    write_event(ptr_out, EV_KEY, BTN_SIDE, 0)?;
                    write_event(ptr_out, EV_SYN, SYN_REPORT, 0)?;
                }
                return Ok(true);
            }
            KEY_KPMINUS => {
                if is_press {
                    write_event(ptr_out, EV_KEY, BTN_EXTRA, 1)?;
                    write_event(ptr_out, EV_SYN, SYN_REPORT, 0)?;
                    write_event(ptr_out, EV_KEY, BTN_EXTRA, 0)?;
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

// ── 统一服务 ────────────────────────────────────────────────────────

extern "C" fn shutdown_signal(_: libc::c_int) {
    SHUTDOWN.store(true, Ordering::Relaxed);
}

static SHUTDOWN: AtomicBool = AtomicBool::new(false);

pub fn run_service() -> Result<()> {
    info!("service — NumLock+KPEnter for mouse, Meta+CapsLock for grid");

    unsafe {
        libc::signal(
            libc::SIGINT,
            shutdown_signal as *const () as libc::sighandler_t,
        );
        libc::signal(
            libc::SIGTERM,
            shutdown_signal as *const () as libc::sighandler_t,
        );
    }

    let mut kbd = KeyboardDev::open_all(KeyboardFilter::Service)?;

    let kbd_bits: Vec<u16> = (1u16..=255).collect();
    let mut kbd_out = create_virt_device(crate::config::DEV_KBD, &kbd_bits, false)?;
    let mut ptr_out = create_virt_device(
        crate::config::DEV_PTR,
        &[BTN_LEFT, BTN_MIDDLE, BTN_RIGHT, BTN_SIDE, BTN_EXTRA],
        true,
    )?;

    let mut kpd = Kpd::new();

    for code in 1u16..=255 {
        write_event(&mut kbd_out, EV_KEY, code, 0)?;
    }
    write_event(&mut kbd_out, EV_SYN, SYN_REPORT, 0)?;

    // ── Grid 状态 ──
    let mut grid_active = false;
    let mut overlay: Option<Overlay> = None;
    let mut mouse: Option<Mouse> = None;
    let mut grid_cfg: Option<crate::grid::GridConfig> = None;
    let mut grid_cache: Option<crate::render::TextCache> = None;
    let mut grid_font_size: f32 = 0.0;
    let mut grid_states: Option<Vec<DrawState>> = None;
    let mut grid_ctx: Option<GridCtx> = None;
    let mut mods = ModState::default();
    let mut meta_held = false;

    let mut warn_is_done = false;
    let mut last_wd = Instant::now();

    loop {
        if SHUTDOWN.load(Ordering::Relaxed) {
            info!("shutting down");
            break Ok(());
        }

        let now = Instant::now();
        if now.duration_since(last_wd) >= std::time::Duration::from_secs(1) {
            watchdog();
            last_wd = now;
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

                // ── 修饰键追踪 ──
                mods.update(code, is_press);
                match code {
                    KEY_LEFTMETA | KEY_RIGHTMETA => meta_held = is_press,
                    _ => {}
                }

                // ── meta+capslock → 切换 grid ──
                if code == KEY_CAPSLOCK && is_press && meta_held {
                    if grid_active {
                        overlay = None;
                        mouse = None;
                        grid_cfg = None;
                        grid_cache = None;
                        grid_states = None;
                        grid_ctx = None;
                        grid_active = false;
                        info!("[grid] off");
                    } else {
                        match enter_grid() {
                            Ok(state) => {
                                overlay = Some(state.overlay);
                                mouse = state.mouse;
                                grid_cfg = Some(state.cfg);
                                grid_cache = Some(state.cache);
                                grid_font_size = state.font_size;
                                grid_states = Some(state.draw_states);
                                grid_ctx = Some(GridCtx::new());
                                grid_active = true;
                                info!("[grid] on");
                            }
                            Err(e) => {
                                warn!("[grid] init failed: {e}");
                            }
                        }
                    }
                    // Flush held modifier keys so compositor doesn't see
                    // stale modifiers on subsequent uinput actions.
                    for key in [KEY_LEFTMETA, KEY_RIGHTMETA, KEY_CAPSLOCK] {
                        write_event(&mut kbd_out, EV_KEY, key, 0)?;
                    }
                    write_event(&mut kbd_out, EV_SYN, SYN_REPORT, 0)?;
                    continue;
                }

                // ── grid 模式：主键盘区按键 → grid 处理 ──
                if grid_active {
                    if ev.type_ != EV_KEY {
                        continue;
                    }
                    if value == 0 {
                        continue;
                    }
                    let byte = key_map(code, &mods);
                    if let Some(b) = byte {
                        if let (Some(o), Some(gcfg), Some(gcache), Some(gstates), Some(gctx)) = (
                            &mut overlay,
                            grid_cfg.as_mut(),
                            grid_cache.as_mut(),
                            grid_states.as_mut(),
                            grid_ctx.as_mut(),
                        ) {
                            if let Err(e) = process_byte(
                                b,
                                o,
                                &mut mouse,
                                gcfg,
                                gcache,
                                grid_font_size,
                                gstates,
                                gctx,
                            ) {
                                warn!("[grid] error: {e}");
                            }
                        }
                        continue;
                    }
                }

                // ── kp-nav ──
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

// ── Grid 初始化 ────────────────────────────────────────────────────

struct GridState {
    overlay: Overlay,
    mouse: Option<Mouse>,
    cfg: crate::grid::GridConfig,
    cache: crate::render::TextCache,
    font_size: f32,
    draw_states: Vec<DrawState>,
}

fn enter_grid() -> Result<GridState> {
    use crate::config::FALLBACK_WIDTH;
    use anyhow::Context;

    let font = fontdue::Font::from_bytes(FONT_DATA, fontdue::FontSettings::default())
        .map_err(|e| anyhow::anyhow!("failed to parse embedded font: {e}"))?;

    let Some(session_uid) = display_session_uid() else {
        anyhow::bail!("no display session detected");
    };
    setup_display_env(session_uid);

    let saved_uid = nix::unistd::geteuid();
    nix::unistd::seteuid(nix::unistd::Uid::from_raw(session_uid))
        .context("seteuid to session user")?;

    let result = (|| -> Result<GridState> {
        let mut overlay = Overlay::connect()?;
        let named = overlay
            .named_monitors()
            .context("failed to query monitors")?;
        if named.is_empty() {
            anyhow::bail!("no active monitors detected");
        }
        let monitors: Vec<(i32, i32, u16, u16)> =
            named.iter().map(|n| (n.1, n.2, n.3, n.4)).collect();

        let monitor_idx = 0;
        let selected = &monitors[monitor_idx];

        let max_w = monitors
            .iter()
            .map(|m| m.0 + i32::from(m.2))
            .max()
            .unwrap_or(i32::from(FALLBACK_WIDTH)) as u16;
        let max_h = monitors
            .iter()
            .map(|m| m.1 + i32::from(m.3))
            .max()
            .unwrap_or(i32::from(crate::config::FALLBACK_HEIGHT)) as u16;
        let mouse = Mouse::new(max_w, max_h)
            .map_err(|e| {
                warn!("uinput unavailable — {e}");
                e
            })
            .ok();

        let single_monitors = vec![*selected];
        let (cfg, font_size, cache, draw_states) =
            init_overlay(&mut overlay, &font, &single_monitors)?;

        Ok(GridState {
            overlay,
            mouse,
            cfg,
            cache,
            font_size,
            draw_states,
        })
    })();

    let _ = nix::unistd::seteuid(saved_uid);
    result
}
