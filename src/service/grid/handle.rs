// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

use log::{info, warn};

use super::display::display_update;
use super::init::{GridPhase, GridStateMut, MonitorList, init_grid_monitor};
use super::process::process_byte;
use super::selection::redraw_select_hint;
use super::state::GridCtx;
use crate::keymap::{KEY_H, KEY_J, KEY_K, KEY_L, KEY_TAB, ModState, map as key_map};

// ── Grid 事件处理 ─────────────────────────────────────────────────

pub(crate) fn handle_selecting(
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

pub(crate) fn handle_navigating(
    code: u16,
    state: GridStateMut<'_>,
    monitors: &MonitorList,
    grid_monitor_idx: &mut usize,
    mods: &ModState,
    grid_phase: GridPhase,
) {
    // ── L4: alt + hjkl = micro-adjust within L3 cell ──
    if mods.alt && (code == KEY_H || code == KEY_J || code == KEY_K || code == KEY_L) {
        if let (Some(o), Some(gcfg), Some(gcache), Some(gstats), Some(gctx)) = (
            state.overlay.as_ref(),
            state.cfg.as_ref(),
            state.cache.as_ref(),
            state.states.as_mut(),
            state.ctx.as_mut(),
        ) && gctx.filter.len() >= 3
        {
            match code {
                KEY_H => gctx.l4_dx = (gctx.l4_dx - 1).max(-3),
                KEY_L => gctx.l4_dx = (gctx.l4_dx + 1).min(3),
                KEY_K => gctx.l4_dy = (gctx.l4_dy - 1).max(-3),
                KEY_J => gctx.l4_dy = (gctx.l4_dy + 1).min(3),
                _ => {}
            }
            if let Err(e) = display_update(
                o,
                gstats,
                gcfg,
                gcache,
                *state.font_size,
                &gctx.filter,
                Some((gctx.l4_dx, gctx.l4_dy)),
            ) {
                warn!("[grid] l4 display error: {e}");
            }
        }
        return;
    }

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
