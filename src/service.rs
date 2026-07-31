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
        KEY_RIGHTMETA, KEY_TAB, ModState, map as key_map,
    },
    uio::{
        EV_KEY, EV_REL, EV_SYN, REL_HWHEEL, REL_WHEEL, REL_X, REL_Y, SYN_REPORT,
        create_virt_device, write_event, write_event_raw,
    },
};
use anyhow::{Context, Result};
use log::{info, warn};

use crate::{
    DrawState, FONT_DATA, GridCtx, init_overlay, overlay::Overlay, process_byte, uinput::Mouse,
};

// ── Grid 状态阶段 ──────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
enum GridPhase {
    Selecting,
    Navigating,
}

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
    fn from_numpad(code: u16) -> Option<Self> {
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

// ── glide 状态 ─────────────────────────────────────────────────────

struct Glide {
    toggle: bool,
    btn_5: u8,
    btn_held: bool,
    numlock_held: bool,
    dir_held: u8,
    dir_mask: Dir,
    dir_count: u32,
}

impl Glide {
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

// ── Watchdog ─────────────────────────────────────────────────────

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
        .map_or_else(|| format!("/home/{uid}"), |u| u.dir.to_string_lossy().into_owned());
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

// ── glide 事件处理 ─────────────────────────────────────────────────

fn handle_key_event(
    glide: &mut Glide,
    ptr_out: &mut std::fs::File,
    code: u16,
    value: i32,
    is_press: bool,
) -> Result<bool> {
    if glide.numlock_held {
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
        c if Dir::from_numpad(c).is_some() => {
            let flag = Dir::from_numpad(c).unwrap();
            if value == 0 {
                glide.dir_mask.remove(flag);
                glide.dir_held = glide.dir_held.saturating_sub(1);
                if glide.dir_held == 0 {
                    glide.dir_count = 0;
                }
            } else if value == 1 {
                glide.dir_mask.insert(flag);
                glide.dir_held = glide.dir_held.saturating_add(1);
            }
            Ok(true)
        }
        KEY_KP5 => {
            if value > 0 {
                write_event(ptr_out, EV_KEY, glide.btn_code(), 1)?;
                write_event(ptr_out, EV_SYN, SYN_REPORT, 0)?;
                glide.btn_held = true;
            } else if value == 0 && glide.btn_held {
                write_event(ptr_out, EV_KEY, glide.btn_code(), 0)?;
                write_event(ptr_out, EV_SYN, SYN_REPORT, 0)?;
                glide.btn_held = false;
            }
            Ok(true)
        }
        KEY_KPDOT => {
            if is_press {
                write_event(ptr_out, EV_KEY, glide.btn_code(), 0)?;
                write_event(ptr_out, EV_SYN, SYN_REPORT, 0)?;
                glide.btn_held = false;
                info!("[release]");
            }
            Ok(true)
        }
        KEY_KP0 => {
            if value == 1 && !glide.btn_held {
                write_event(ptr_out, EV_KEY, glide.btn_code(), 1)?;
                write_event(ptr_out, EV_SYN, SYN_REPORT, 0)?;
                glide.btn_held = true;
                info!("[hold]");
            }
            Ok(true)
        }
        KEY_KPASTERISK => {
            if is_press {
                glide.btn_5 = 2;
                info!("[btn5=M]");
            }
            Ok(true)
        }
        KEY_KPSLASH => {
            if is_press {
                glide.btn_5 = 1;
                info!("[btn5=L]");
            }
            Ok(true)
        }
        KEY_KPMINUS => {
            if is_press {
                glide.btn_5 = 3;
                info!("[btn5=R]");
            }
            Ok(true)
        }
        KEY_KPPLUS => {
            if value == 1 {
                let code = glide.btn_code();
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

fn do_direction_tick(glide: &mut Glide, ptr_out: &mut std::fs::File) -> Result<()> {
    if glide.dir_held != 1 {
        return Ok(());
    }
    let (dx, dy) = glide.dir_mask.to_vector();
    glide.dir_count = glide.dir_count.saturating_add(1);
    let step = config::cursor_speed(glide.dir_count) as f32;
    let mx = (dx as f32 * step) as i32;
    let my = (dy as f32 * step) as i32;
    write_event(ptr_out, EV_REL, REL_X, mx)?;
    write_event(ptr_out, EV_REL, REL_Y, my)?;
    write_event(ptr_out, EV_SYN, SYN_REPORT, 0)?;
    Ok(())
}

// ── 双模服务 ────────────────────────────────────────────────────────

fn is_grid_key(code: u16) -> bool {
    key_map(code, &ModState::default()).is_some()
        || code == KEY_TAB
        || code == KEY_CAPSLOCK
        || code == KEY_LEFTMETA
        || code == KEY_RIGHTMETA
}

extern "C" fn shutdown_signal(_: i32) {
    SHUTDOWN.store(true, Ordering::Relaxed);
}

static SHUTDOWN: AtomicBool = AtomicBool::new(false);

pub fn run_service() -> Result<()> {
    info!("service — NumLock+KPEnter for glide, Meta+CapsLock for grid");

    unsafe {
        let _ = nix::sys::signal::signal(
            nix::sys::signal::Signal::SIGINT,
            nix::sys::signal::SigHandler::Handler(shutdown_signal),
        );
        let _ = nix::sys::signal::signal(
            nix::sys::signal::Signal::SIGTERM,
            nix::sys::signal::SigHandler::Handler(shutdown_signal),
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

    let mut glide = Glide::new();

    for code in 1u16..=255 {
        write_event(&mut kbd_out, EV_KEY, code, 0)?;
    }
    write_event(&mut kbd_out, EV_SYN, SYN_REPORT, 0)?;

    // ── Grid 状态 ──
    let mut grid_active = false;
    let mut grid_phase = GridPhase::Navigating;
    let mut overlay: Option<Overlay> = None;
    let mut mouse: Option<Mouse> = None;
    let mut grid_cfg: Option<crate::grid::GridConfig> = None;
    let mut grid_cache: Option<crate::render::TextCache> = None;
    let mut grid_font_size: f32 = 0.0;
    let mut grid_states_all: Option<Vec<DrawState>> = None;
    let mut grid_ctx: Option<GridCtx> = None;
    let mut grid_monitors: MonitorList = Vec::new();
    let mut grid_monitor_idx: usize = 0;
    let mut select_hint: String = String::new();
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
                        grid_states_all = None;
                        grid_ctx = None;
                        grid_active = false;
                        info!("[grid] off");
                    } else {
                        match enter_grid() {
                            Ok((init_overlay_conn, monitors_list, init_mouse)) => {
                                grid_monitors = monitors_list;
                                grid_monitor_idx = 0;
                                grid_active = true;

                                if grid_monitors.len() > 1 {
                                    overlay = None;
                                    select_hint.clear();
                                    if let Err(e) = show_selection(&mut overlay, &grid_monitors) {
                                        warn!("[grid] selection: {e}");
                                        grid_active = false;
                                    } else {
                                        grid_phase = GridPhase::Selecting;
                                        info!(
                                            "[grid] select monitor (a-{})",
                                            (b'a' + (grid_monitors.len() - 1) as u8) as char
                                        );
                                    }
                                } else {
                                    overlay = Some(init_overlay_conn);
                                    mouse = init_mouse;
                                    if let Ok(state) = init_grid_monitor(0, &grid_monitors) {
                                        overlay = Some(state.overlay);
                                        mouse = state.mouse;
                                        grid_cfg = Some(state.cfg);
                                        grid_cache = Some(state.cache);
                                        grid_font_size = state.font_size;
                                        grid_states_all = Some(state.draw_states);
                                    }
                                    grid_ctx = Some(GridCtx::new());
                                    grid_phase = GridPhase::Navigating;
                                    info!("[grid] on");
                                }
                            }
                            Err(e) => {
                                warn!("[grid] init failed: {e}");
                            }
                        }
                    }
                    for key in [KEY_LEFTMETA, KEY_RIGHTMETA, KEY_CAPSLOCK] {
                        write_event(&mut kbd_out, EV_KEY, key, 0)?;
                    }
                    write_event(&mut kbd_out, EV_SYN, SYN_REPORT, 0)?;
                    continue;
                }

                // ── grid 模式 ──
                if grid_active && ev.type_ == EV_KEY && is_grid_key(code) {
                    if value != 0 {
                        let state = GridStateMut {
                            overlay: &mut overlay,
                            cfg: &mut grid_cfg,
                            cache: &mut grid_cache,
                            font_size: &mut grid_font_size,
                            states: &mut grid_states_all,
                            ctx: &mut grid_ctx,
                            mouse: &mut mouse,
                        };
                        if grid_phase == GridPhase::Selecting {
                            let monitors = grid_monitors.clone();
                            handle_selecting(code, state, &mut grid_monitor_idx, &mut grid_phase, &monitors, &mods, &mut select_hint);
                        } else {
                            handle_navigating(code, state, &grid_monitors, &mut grid_monitor_idx, &mods, grid_phase);
                        }
                    }
                    continue;
                }

                // ── glide ──
                if ev.type_ == EV_KEY {
                    if code == KEY_NUMLOCK {
                        glide.numlock_held = value != 0;
                    }

                    if code == KEY_KPENTER && is_press && glide.numlock_held {
                        glide.toggle = !glide.toggle;
                        info!(
                            "{}",
                            if glide.active() {
                                "[glide ON]"
                            } else {
                                "[pass-through]"
                            }
                        );
                        continue;
                    }

                    if glide.active()
                        && handle_key_event(&mut glide, &mut ptr_out, code, value, is_press)?
                    {
                        continue;
                    }
                }

                write_event_raw(&mut kbd_out, &ev)?;
            }
            Ok(None) => {
                do_direction_tick(&mut glide, &mut ptr_out)?;
            }
            Err(e) => return Err(e),
        }
        let t_poll = t_poll_start.elapsed();
        if t_poll > std::time::Duration::from_millis(40) {
            warn!("poll {t_poll:?}");
        }
    }
}

// ── Grid 事件处理 ─────────────────────────────────────────────────

struct GridStateMut<'a> {
    overlay: &'a mut Option<Overlay>,
    cfg: &'a mut Option<crate::grid::GridConfig>,
    cache: &'a mut Option<crate::render::TextCache>,
    font_size: &'a mut f32,
    states: &'a mut Option<Vec<DrawState>>,
    ctx: &'a mut Option<GridCtx>,
    mouse: &'a mut Option<Mouse>,
}

fn handle_selecting(
    code: u16,
    state: GridStateMut<'_>,
    grid_monitor_idx: &mut usize,
    grid_phase: &mut GridPhase,
    monitors: &MonitorList,
    mods: &ModState,
    select_hint: &mut String,
) {
    let byte = key_map(code, mods);
    if let Some(b) = byte
        && b.is_ascii_lowercase()
    {
        let idx = (b - b'a') as usize;
        if idx < monitors.len() {
            *grid_monitor_idx = idx;
            *state.overlay = None;
            if let Ok(s) = init_grid_monitor(*grid_monitor_idx, monitors) {
                *state.overlay = Some(s.overlay);
                *state.mouse = s.mouse;
                *state.cfg = Some(s.cfg);
                *state.cache = Some(s.cache);
                *state.font_size = s.font_size;
                *state.states = Some(s.draw_states);
                *state.ctx = Some(GridCtx::new());
                *grid_phase = GridPhase::Navigating;
                info!("[grid] selected monitor {}", *grid_monitor_idx + 1);
            }
        } else {
            *select_hint = format!("{}", b as char);
            if let Some(o) = state.overlay.as_mut() {
                let _ = redraw_select_hint(o, monitors, select_hint);
            }
        }
    }
    if let Some(o) = state.overlay.as_mut()
        && b'\x1b' == byte.unwrap_or(0)
    {
        select_hint.clear();
        let _ = redraw_select_hint(o, monitors, "");
    }
}

fn handle_navigating(
    code: u16,
    state: GridStateMut<'_>,
    monitors: &MonitorList,
    grid_monitor_idx: &mut usize,
    mods: &ModState,
    grid_phase: GridPhase,
) {
    if code == KEY_TAB && monitors.len() > 1 {
        *grid_monitor_idx = (*grid_monitor_idx + 1) % monitors.len();
        *state.overlay = None;
        if let Ok(s) = init_grid_monitor(*grid_monitor_idx, monitors) {
            *state.overlay = Some(s.overlay);
            *state.mouse = s.mouse;
            *state.cfg = Some(s.cfg);
            *state.cache = Some(s.cache);
            *state.font_size = s.font_size;
            *state.states = Some(s.draw_states);
            *state.ctx = Some(GridCtx::new());
            info!(
                "[grid] monitor {}/{}",
                *grid_monitor_idx + 1,
                monitors.len()
            );
        }
        return;
    }

    if grid_phase == GridPhase::Navigating {
        let byte = key_map(code, mods);
        if let Some(b) = byte
            && let (Some(o), Some(gcfg), Some(gcache), Some(gstates), Some(gctx)) = (
                state.overlay.as_mut(),
                state.cfg.as_mut(),
                state.cache.as_mut(),
                state.states.as_mut(),
                state.ctx.as_mut(),
            )
            && let Err(e) = process_byte(
                b,
                o,
                state.mouse,
                gcfg,
                gcache,
                *state.font_size,
                gstates,
                gctx,
            )
        {
            warn!("[grid] error: {e}");
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

type MonitorList = Vec<(i32, i32, u16, u16)>;

fn connect_as_user() -> Result<Overlay> {
    let Some(session_uid) = display_session_uid() else {
        anyhow::bail!("no display session detected");
    };
    setup_display_env(session_uid);

    let saved = nix::unistd::geteuid();
    nix::unistd::seteuid(nix::unistd::Uid::from_raw(session_uid))
        .context("seteuid")?;
    let result = Overlay::connect();
    let _ = nix::unistd::seteuid(saved);
    result
}

fn mouse_for_monitors(monitors: &[(i32, i32, u16, u16)]) -> Option<Mouse> {
    use crate::config::FALLBACK_WIDTH;
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
    Mouse::new(max_w, max_h).ok()
}

fn enter_grid() -> Result<(Overlay, MonitorList, Option<Mouse>)> {
    let overlay = connect_as_user()?;
    let named = overlay
        .named_monitors()
        .context("failed to query monitors")?;
    if named.is_empty() {
        anyhow::bail!("no active monitors detected");
    }
    let monitors: Vec<(i32, i32, u16, u16)> =
        crate::debug::clone_monitors(
            named.iter().map(|n| (n.1, n.2, n.3, n.4)).collect(),
        );

    let m = mouse_for_monitors(&monitors);
    Ok((overlay, monitors, m))
}

fn init_grid_monitor(
    idx: usize,
    monitors: &[(i32, i32, u16, u16)],
) -> Result<GridState> {
    let font = fontdue::Font::from_bytes(FONT_DATA, fontdue::FontSettings::default())
        .map_err(|e| anyhow::anyhow!("failed to parse embedded font: {e}"))?;

    let single = vec![monitors[idx]];
    let mut overlay = connect_as_user()?;
    let (cfg, font_size, cache, draw_states) =
        init_overlay(&mut overlay, &font, &single)?;

    Ok(GridState {
        overlay,
        mouse: mouse_for_monitors(monitors),
        cfg,
        cache,
        font_size,
        draw_states,
    })
}

// ── 多屏选屏 ──────────────────────────────────────────────────────

fn show_selection(
    overlay: &mut Option<Overlay>,
    monitors: &[(i32, i32, u16, u16)],
) -> Result<()> {
    let bbox_x = monitors.iter().map(|m| m.0).min().unwrap_or(0);
    let bbox_y = monitors.iter().map(|m| m.1).min().unwrap_or(0);
    let bbox_w = monitors.iter().map(|m| m.0 + m.2 as i32).max().unwrap_or(0) - bbox_x;
    let bbox_h = monitors.iter().map(|m| m.1 + m.3 as i32).max().unwrap_or(0) - bbox_y;

    let mut new_overlay = connect_as_user()?;
    new_overlay.add_window(bbox_x, bbox_y, bbox_w as u16, bbox_h as u16)?;
    new_overlay.show_all()?;
    redraw_select_hint(&mut new_overlay, monitors, "")?;
    *overlay = Some(new_overlay);
    Ok(())
}

fn redraw_select_hint(
    overlay: &mut Overlay,
    monitors: &[(i32, i32, u16, u16)],
    hint: &str,
) -> Result<()> {
    use tiny_skia::{Color, Paint, PathBuilder, Pixmap, PremultipliedColorU8, Shader, Transform};

    let bbox_x = monitors.iter().map(|m| m.0).min().unwrap_or(0);
    let bbox_y = monitors.iter().map(|m| m.1).min().unwrap_or(0);
    let bbox_w = monitors.iter().map(|m| m.0 + m.2 as i32).max().unwrap_or(0) - bbox_x;
    let bbox_h = monitors.iter().map(|m| m.1 + m.3 as i32).max().unwrap_or(0) - bbox_y;

    let mut pixmap = Pixmap::new(bbox_w as u32, bbox_h as u32).context("pixmap")?;
    pixmap
        .pixels_mut()
        .fill(PremultipliedColorU8::from_rgba(0, 0, 0, 0).unwrap());

    let font = fontdue::Font::from_bytes(FONT_DATA, fontdue::FontSettings::default())
        .map_err(|e| anyhow::anyhow!("font: {e}"))?;
    let font_size = 128.0;
    let cache = crate::render::TextCache::new(&font, font_size);

    let bg = Color::from_rgba8(0, 0, 0, 144);
    let paint = Paint {
        shader: Shader::SolidColor(bg),
        anti_alias: true,
        ..Default::default()
    };
    let pw = font_size * 1.8;
    let ph = font_size * 1.8;

    for (i, &(mx, my, mw, mh)) in monitors.iter().enumerate() {
        let label = format!("{}", (b'a' + i as u8) as char);
        if !hint.is_empty() && !label.starts_with(hint) {
            continue;
        }
        let cx = (mx - bbox_x) as f32 + mw as f32 * 0.5;
        let cy = (my - bbox_y) as f32 + mh as f32 * 0.5;
        let x = cx - pw * 0.5;
        let y = cy - ph * 0.5;

        let mut pb = PathBuilder::new();
        pb.push_oval(tiny_skia::Rect::from_xywh(x, y, pw, ph).unwrap());
        let oval = pb.finish().unwrap();
        pixmap.fill_path(
            &oval,
            &paint,
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );

        crate::render::draw_text(
            &mut pixmap,
            &label,
            cx,
            cy,
            &cache,
            font_size,
            [192, 255, 192, 192],
        );
    }

    overlay.upload(0, &pixmap)?;
    overlay.redraw_all()?;
    Ok(())
}
