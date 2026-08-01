// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use anyhow::Result;
use log::{info, warn};

use crate::glide::{Glide, do_direction_tick, handle_key_event};
use crate::grid::{
    GridPhase, GridStateMut, MonitorList, enter_grid, handle_navigating, handle_selecting,
    init_grid_monitor, show_selection, watchdog,
};

use crate::{
    DrawState, GridCtx,
    config::{BTN_EXTRA, BTN_LEFT, BTN_MIDDLE, BTN_RIGHT, BTN_SIDE},
    evdev::{KeyboardDev, KeyboardFilter},
    keymap::{
        KEY_CAPSLOCK, KEY_KPENTER, KEY_LEFTMETA, KEY_NUMLOCK, KEY_RIGHTMETA, KEY_TAB, ModState,
        map as key_map,
    },
    overlay::Overlay,
    uinput::Mouse,
    uio::{EV_KEY, EV_SYN, SYN_REPORT, create_virt_device, write_event, write_event_raw},
};

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
                            handle_selecting(
                                code,
                                state,
                                &mut grid_monitor_idx,
                                &mut grid_phase,
                                &monitors,
                                &mods,
                                &mut select_hint,
                            );
                        } else {
                            handle_navigating(
                                code,
                                state,
                                &grid_monitors,
                                &mut grid_monitor_idx,
                                &mods,
                                grid_phase,
                            );
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
