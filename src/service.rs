// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

pub mod glide_alpha;
pub mod glide_num;
pub mod grid;

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use anyhow::Result;
use log::{info, warn};

use crate::service::glide_alpha::{GlideAlpha, do_direction_alpha_tick, handle_alpha_event};
use crate::service::glide_num::{GlideNum, do_direction_num_tick, handle_key_event};
use crate::service::grid::{
    GridPhase, GridStateMut, MonitorList, enter_grid, handle_navigating, handle_selecting,
    init_grid_monitor, show_selection, watchdog,
};

use crate::{
    DrawState, GridCtx,
    config::{BTN_EXTRA, BTN_LEFT, BTN_MIDDLE, BTN_RIGHT, BTN_SIDE},
    keyboard::{KeyboardDev, KeyboardFilter},
    keymap::{
        KEY_CAPSLOCK, KEY_KPENTER, KEY_LEFTMETA, KEY_LEFTSHIFT, KEY_NUMLOCK, KEY_RIGHTMETA,
        KEY_RIGHTSHIFT, KEY_TAB, ModState, map as key_map,
    },
    overlay::Overlay,
    uinput::Mouse,
    uinput::{EV_KEY, EV_SYN, SYN_REPORT, create_virt_device, write_event, write_event_raw},
};

// ── Grid state ───────────────────────────────────────────────────────

struct GridEnv {
    active: bool,
    phase: GridPhase,
    overlay: Option<Overlay>,
    mouse: Option<Mouse>,
    cfg: Option<crate::service::grid::GridConfig>,
    cache: Option<crate::render::TextCache>,
    font_size: f32,
    states: Option<Vec<DrawState>>,
    ctx: Option<GridCtx>,
    monitors: MonitorList,
    monitor_idx: usize,
    select_hint: String,
}

impl GridEnv {
    fn new() -> Self {
        Self {
            active: false,
            phase: GridPhase::Navigating,
            overlay: None,
            mouse: None,
            cfg: None,
            cache: None,
            font_size: 0.0,
            states: None,
            ctx: None,
            monitors: Vec::new(),
            monitor_idx: 0,
            select_hint: String::new(),
        }
    }
}

// ── Key classification ──────────────────────────────────────────────

fn is_grid_key(code: u16) -> bool {
    key_map(code, &ModState::default()).is_some()
        || code == KEY_TAB
        || code == KEY_CAPSLOCK
        || code == KEY_LEFTMETA
        || code == KEY_RIGHTMETA
}

// ── Signal ───────────────────────────────────────────────────────────

extern "C" fn shutdown_signal(_: i32) {
    SHUTDOWN.store(true, Ordering::Relaxed);
}

static SHUTDOWN: AtomicBool = AtomicBool::new(false);

// ── Main entry point ─────────────────────────────────────────────────

pub fn run_service() -> Result<()> {
    info!("service — grid + glide-num + glide-alpha");

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

    let mut glide_num = GlideNum::new();
    let mut glide_alpha = GlideAlpha::new();
    let mut glide_alpha_active = false;

    for code in 1u16..=255 {
        write_event(&mut kbd_out, EV_KEY, code, 0)?;
    }
    write_event(&mut kbd_out, EV_SYN, SYN_REPORT, 0)?;

    let mut env = GridEnv::new();
    let mut mods = ModState::default();
    let mut meta_held = false;
    let mut shift_held = false;

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

                mods.update(code, is_press);
                match code {
                    KEY_LEFTMETA | KEY_RIGHTMETA => meta_held = is_press,
                    KEY_LEFTSHIFT | KEY_RIGHTSHIFT => shift_held = is_press,
                    _ => {}
                }

                // Only EV_KEY events carry mode-relevant information.
                if ev.type_ != EV_KEY {
                    write_event_raw(&mut kbd_out, &ev)?;
                    continue;
                }

                if toggle_glide_num(
                    code, value, is_press,
                    &mut glide_num,
                ) { continue; }

                if toggle_glide_alpha(
                    code, is_press, meta_held, shift_held,
                    &mut glide_alpha_active, &mut kbd_out,
                )? { continue; }

                if toggle_grid(
                    code, is_press, meta_held,
                    &mut env, &mut kbd_out,
                )? { continue; }

                if handle_glide_num_input(
                    code, value, is_press, &mut glide_num, &mut ptr_out,
                )? { continue; }

                if handle_glide_alpha_input(
                    code, value, is_press, &mut glide_alpha, &mut ptr_out,
                )? { continue; }

                if handle_grid_input(
                    code, value, &mut env, &mods,
                ) { continue; }

                write_event_raw(&mut kbd_out, &ev)?;
            }
            Ok(None) => {
                do_direction_num_tick(&mut glide_num, &mut ptr_out)?;
                do_direction_alpha_tick(&mut glide_alpha, &mut ptr_out)?;
            }
            Err(e) => return Err(e),
        }
        let t_poll = t_poll_start.elapsed();
        if t_poll > std::time::Duration::from_millis(40) {
            warn!("poll {t_poll:?}");
        }
    }
}

// ── Toggles ──────────────────────────────────────────────────────────

fn toggle_glide_num(
    code: u16,
    value: i32,
    is_press: bool,
    glide_num: &mut GlideNum,
) -> bool {
    if code == KEY_NUMLOCK {
        glide_num.numlock_held = value != 0;
    }
    if code == KEY_KPENTER && is_press && glide_num.numlock_held {
        glide_num.toggle = !glide_num.toggle;
        info!(
            "{}",
            if glide_num.active() { "[glide-num ON]" } else { "[pass-through]" }
        );
        return true;
    }
    false
}

fn toggle_glide_alpha(
    code: u16,
    is_press: bool,
    meta_held: bool,
    shift_held: bool,
    active: &mut bool,
    kbd_out: &mut std::fs::File,
) -> Result<bool> {
    if code != KEY_CAPSLOCK || !is_press || !meta_held || !shift_held {
        return Ok(false);
    }
    *active = !*active;
    info!("{}", if *active { "[glide-alpha ON]" } else { "[glide-alpha OFF]" });
    for key in [KEY_LEFTMETA, KEY_RIGHTMETA, KEY_LEFTSHIFT, KEY_RIGHTSHIFT, KEY_CAPSLOCK] {
        write_event(kbd_out, EV_KEY, key, 0)?;
    }
    write_event(kbd_out, EV_SYN, SYN_REPORT, 0)?;
    Ok(true)
}

fn toggle_grid(
    code: u16,
    is_press: bool,
    meta_held: bool,
    env: &mut GridEnv,
    kbd_out: &mut std::fs::File,
) -> Result<bool> {
    if code != KEY_CAPSLOCK || !is_press || !meta_held {
        return Ok(false);
    }
    if env.active {
        env.active = false;
        env.overlay = None;
        env.mouse = None;
        env.cfg = None;
        env.cache = None;
        env.states = None;
        env.ctx = None;
        info!("[grid] off");
    } else {
        match enter_grid() {
            Ok((init_overlay_conn, monitors_list, init_mouse)) => {
                env.monitors = monitors_list;
                env.monitor_idx = 0;
                env.active = true;

                if env.monitors.len() > 1 {
                    env.overlay = None;
                    env.select_hint.clear();
                    if let Err(e) = show_selection(&mut env.overlay, &env.monitors) {
                        warn!("[grid] selection: {e}");
                        env.active = false;
                    } else {
                        env.phase = GridPhase::Selecting;
                        info!(
                            "[grid] select monitor (a-{})",
                            (b'a' + (env.monitors.len() - 1) as u8) as char
                        );
                    }
                } else {
                    env.overlay = Some(init_overlay_conn);
                    env.mouse = init_mouse;
                    if let Ok(state) = init_grid_monitor(0, &env.monitors) {
                        env.overlay = Some(state.overlay);
                        env.mouse = state.mouse;
                        env.cfg = Some(state.cfg);
                        env.cache = Some(state.cache);
                        env.font_size = state.font_size;
                        env.states = Some(state.draw_states);
                    }
                    env.ctx = Some(GridCtx::new());
                    env.phase = GridPhase::Navigating;
                    info!("[grid] on");
                }
            }
            Err(e) => warn!("[grid] init failed: {e}"),
        }
    }
    for key in [KEY_LEFTMETA, KEY_RIGHTMETA, KEY_CAPSLOCK] {
        write_event(kbd_out, EV_KEY, key, 0)?;
    }
    write_event(kbd_out, EV_SYN, SYN_REPORT, 0)?;
    Ok(true)
}

// ── Input dispatch ───────────────────────────────────────────────────

fn handle_glide_num_input(
    code: u16,
    value: i32,
    is_press: bool,
    glide_num: &mut GlideNum,
    ptr_out: &mut std::fs::File,
) -> Result<bool> {
    if !glide_num.active() {
        return Ok(false);
    }
    handle_key_event(glide_num, ptr_out, code, value, is_press)
}

fn handle_glide_alpha_input(
    code: u16,
    value: i32,
    is_press: bool,
    glide_alpha: &mut GlideAlpha,
    ptr_out: &mut std::fs::File,
) -> Result<bool> {
    if !glide_alpha.active() {
        return Ok(false);
    }
    handle_alpha_event(glide_alpha, ptr_out, code, value, is_press)
}

fn handle_grid_input(
    code: u16,
    value: i32,
    env: &mut GridEnv,
    mods: &ModState,
) -> bool {
    if !env.active || !is_grid_key(code) || value == 0 {
        return false;
    }
    let state = GridStateMut {
        overlay: &mut env.overlay,
        cfg: &mut env.cfg,
        cache: &mut env.cache,
        font_size: &mut env.font_size,
        states: &mut env.states,
        ctx: &mut env.ctx,
        mouse: &mut env.mouse,
    };
    if env.phase == GridPhase::Selecting {
        let monitors = env.monitors.clone();
        handle_selecting(code, state, &mut env.monitor_idx, &mut env.phase, &monitors, mods, &mut env.select_hint);
    } else {
        handle_navigating(code, state, &env.monitors, &mut env.monitor_idx, mods, env.phase);
    }
    true
}
