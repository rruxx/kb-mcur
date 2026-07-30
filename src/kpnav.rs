// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use crate::{
    config::{self, BTN_EXTRA, BTN_LEFT, BTN_MIDDLE, BTN_RIGHT, BTN_SIDE},
    evdev::{KeyboardDev, KeyboardFilter},
    keymap::{
        KEY_CAPSLOCK, KEY_KP0, KEY_KP5, KEY_KP7, KEY_KP8, KEY_KP9, KEY_KPASTERISK,
        KEY_KPDOT, KEY_KPENTER, KEY_KPMINUS, KEY_KPPLUS, KEY_KPSLASH, KEY_LEFTMETA,
        KEY_NUMLOCK, KEY_RIGHTMETA, KEY_TAB, ModState, map as key_map,
    },
    uio::{
        EV_KEY, EV_REL, EV_SYN, REL_HWHEEL, REL_WHEEL, REL_X, REL_Y, SYN_REPORT,
        create_virt_device, write_event, write_event_raw,
    },
};
use anyhow::{Context, Result};
use log::{info, warn};

use crate::{
    DrawState, FONT_DATA, GridCtx, init_overlay, process_byte,
    overlay::Overlay,
    uinput::Mouse,
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

    unsafe {
        std::env::set_var("DISPLAY", ":0");
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
    let mut grid_phase = GridPhase::Navigating;
    let mut overlay: Option<Overlay> = None;
    let mut mouse: Option<Mouse> = None;
    let mut grid_cfg: Option<crate::grid::GridConfig> = None;
    let mut grid_cache: Option<crate::render::TextCache> = None;
    let mut grid_font_size: f32 = 0.0;
    let mut grid_states_all: Option<Vec<DrawState>> = None;
    let mut grid_ctx: Option<GridCtx> = None;
    let mut grid_monitors: Vec<(i32, i32, u16, u16)> = Vec::new();
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
                                    if let Err(e) = show_selection(
                                        &mut overlay,
                                        &grid_monitors,
                                    ) {
                                        warn!("[grid] selection: {e}");
                                        grid_active = false;
                                    } else {
                                        grid_phase = GridPhase::Selecting;
                                        info!("[grid] select monitor (a-{})",
                                            (b'a' + (grid_monitors.len() - 1) as u8) as char);
                                    }
                                } else {
                                    overlay = Some(init_overlay_conn);
                                    mouse = init_mouse;
                                    // Single monitor: init grid overlay.
                                    if let Ok(state) =
                                        init_grid_monitor(0, &grid_monitors)
                                    {
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
                    // Flush held modifier keys
                    for key in [KEY_LEFTMETA, KEY_RIGHTMETA, KEY_CAPSLOCK] {
                        write_event(&mut kbd_out, EV_KEY, key, 0)?;
                    }
                    write_event(&mut kbd_out, EV_SYN, SYN_REPORT, 0)?;
                    continue;
                }

                // ── grid 模式 ──
                if grid_active {
                    if ev.type_ != EV_KEY {
                        continue;
                    }
                    if value == 0 {
                        continue;
                    }

                    // ── Selecting phase: letter → pick monitor ──
                    if grid_phase == GridPhase::Selecting {
                        let byte = key_map(code, &mods);
                        if let Some(b) = byte
                            && b.is_ascii_lowercase()
                        {
                            let idx = (b - b'a') as usize;
                            if idx < grid_monitors.len() {
                                grid_monitor_idx = idx;
                                // Drop selection overlay, init grid for selected monitor.
                                overlay = None;
                                if let Ok(state) =
                                    init_grid_monitor(grid_monitor_idx, &grid_monitors)
                                {
                                    overlay = Some(state.overlay);
                                    mouse = state.mouse;
                                    grid_cfg = Some(state.cfg);
                                    grid_cache = Some(state.cache);
                                    grid_font_size = state.font_size;
                                    grid_states_all = Some(state.draw_states);
                                    grid_ctx = Some(GridCtx::new());
                                    grid_phase = GridPhase::Navigating;
                                    info!("[grid] selected monitor {}", grid_monitor_idx + 1);
                                }
                            } else {
                                // Re-render with hint if partial match
                                select_hint = format!("{}", b as char);
                                if let Some(ref mut o) = overlay {
                                    let _ = redraw_select_hint(
                                        o,
                                        &grid_monitors,
                                        &select_hint,
                                    );
                                }
                            }
                        }
                        if let Some(ref mut o) = overlay
                            && b'\x1b' == byte.unwrap_or(0)
                        {
                            // Esc clears hint
                            select_hint.clear();
                            let _ = redraw_select_hint(o, &grid_monitors, "");
                        }
                        continue;
                    }

                    // ── Navigating phase: tab → 切换显示器 ──
                    if code == KEY_TAB && grid_monitors.len() > 1 {
                        grid_monitor_idx = (grid_monitor_idx + 1) % grid_monitors.len();
                        overlay = None;
                        if let Ok(state) =
                            init_grid_monitor(grid_monitor_idx, &grid_monitors)
                        {
                            overlay = Some(state.overlay);
                            mouse = state.mouse;
                            grid_cfg = Some(state.cfg);
                            grid_cache = Some(state.cache);
                            grid_font_size = state.font_size;
                            grid_states_all = Some(state.draw_states);
                            grid_ctx = Some(GridCtx::new());
                            info!(
                                "[grid] monitor {}/{}",
                                grid_monitor_idx + 1,
                                grid_monitors.len()
                            );
                        }
                        continue;
                    }

                    // ── 网格输入 ──
                    if grid_phase == GridPhase::Navigating {
                        let byte = key_map(code, &mods);
                        if let Some(b) = byte {
                            if let (
                                Some(o),
                                Some(gcfg),
                                Some(gcache),
                                Some(gstates),
                                Some(gctx),
                            ) = (
                                &mut overlay,
                                grid_cfg.as_mut(),
                                grid_cache.as_mut(),
                                grid_states_all.as_mut(),
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
                        }
                    }
                    continue;
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

                if kpd.active()
                    && handle_key_event(&mut kpd, &mut ptr_out, code, value, is_press)?
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

fn enter_grid() -> Result<(Overlay, Vec<(i32, i32, u16, u16)>, Option<Mouse>)> {
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
