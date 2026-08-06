// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::{Context, Result};
use log::{info, warn};
use tiny_skia::Pixmap;

use super::base::{render_bg, render_l1, render_labels};
use super::display::display_update;
use super::process::{cursor_action, cursor_warp, region_center};
use super::selection::redraw_select_hint;
use super::{Grid, GridConfig, GridFilter};
use crate::config::{
    FONT_BASE_HEIGHT, FONT_BASE_SIZE, FONT_SIZE_MIN, action_key, l1_key_pos, l2_key_pos, l3_key_pos,
};
use crate::device::Mouse;
use crate::font;
use crate::keymap::{KEY_H, KEY_J, KEY_K, KEY_L, KEY_TAB, ModState, key_map};
use crate::overlay::{Monitor, Overlay};
use crate::render::TextCache;

/// Ordered list of active monitors.
pub type MonitorList = Vec<Monitor>;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GridPhase {
    Selecting,
    Navigating,
}

/// Fully-initialized grid session state for a single monitor.
pub struct GridState {
    pub(crate) overlay: Overlay,
    pub(crate) mouse: Option<Mouse>,
    pub(crate) cfg: GridConfig,
    pub(crate) cache: TextCache,
    pub(crate) font_size: f32,
    pub(crate) draw_states: Vec<DrawState>,
}

/// Per-monitor state: the 27×27 grid, its base-layer RGBA bytes, and a
/// persistent pixmap that is re-uploaded on every redraw.
pub struct DrawState {
    pub(crate) name: String,
    pub(crate) grid: Grid,
    pub(crate) pixmap: Pixmap,
    pub(crate) bg_pixel: tiny_skia::PremultipliedColorU8,
    pub(crate) mask_idx: Vec<usize>,
    pub(crate) mask_px: Vec<tiny_skia::PremultipliedColorU8>,
    /// L3 glyph cache — shares the `DrawState` lifetime, so it always
    /// matches the current `font_size`.
    pub(crate) l3_cache: Option<TextCache>,
}

pub fn init_overlay(
    overlay: &mut Overlay,
    monitors: &[Monitor],
) -> Result<(GridConfig, f32, TextCache, Vec<DrawState>)> {
    let cfg = GridConfig::default();
    let base = Monitor::font_scale_base(monitors);
    let font_size = (FONT_BASE_SIZE * base / FONT_BASE_HEIGHT)
        .max(FONT_SIZE_MIN)
        .round();
    let cache = TextCache::new(font::font(), font_size);

    let mut draw_states = Vec::new();
    for (idx, m) in monitors.iter().enumerate() {
        let (x, y, w, h) = (m.x, m.y, m.w, m.h);
        let grid = Grid::new(u32::from(w), u32::from(h), &cfg);
        let mut pixmap = Pixmap::new(u32::from(w), u32::from(h)).context("pixmap")?;
        let bg_pixel = render_bg(&mut pixmap, &cfg);
        render_l1(&mut pixmap, &cfg);

        let all_px = pixmap.pixels();
        let mut mask_idx = Vec::new();
        let mut mask_px = Vec::new();
        for (i, &p) in all_px.iter().enumerate() {
            if p != bg_pixel {
                mask_idx.push(i);
                mask_px.push(p);
            }
        }

        render_labels(&mut pixmap, &grid, &cfg, &cache, font_size, None, 0);
        overlay.add_window(x, y, w, h)?;
        overlay.upload(idx, &pixmap)?;
        draw_states.push(DrawState {
            name: m.name.clone(),
            grid,
            pixmap,
            bg_pixel,
            mask_idx,
            mask_px,
            l3_cache: None,
        });
    }
    overlay.show_all()?;
    overlay.redraw_all()?;

    Ok((cfg, font_size, cache, draw_states))
}

pub fn init_grid_monitor(
    idx: usize,
    monitors: &[Monitor],
    overlay: Option<Overlay>,
) -> Result<GridState> {
    let single = vec![monitors[idx].clone()];
    let mut overlay = match overlay {
        Some(o) => o,
        None => super::init::connect_as_user()?,
    };
    let (cfg, font_size, cache, draw_states) = init_overlay(&mut overlay, &single)?;

    Ok(GridState {
        overlay,
        mouse: mouse_for_monitors(monitors),
        cfg,
        cache,
        font_size,
        draw_states,
    })
}

fn mouse_for_monitors(monitors: &[Monitor]) -> Option<Mouse> {
    use crate::config::FALLBACK_WIDTH;
    let max_w = monitors
        .iter()
        .map(|m| m.x + i32::from(m.w))
        .max()
        .unwrap_or(i32::from(FALLBACK_WIDTH)) as u16;
    let max_h = monitors
        .iter()
        .map(|m| m.y + i32::from(m.h))
        .max()
        .unwrap_or(i32::from(crate::config::FALLBACK_HEIGHT)) as u16;
    Mouse::new(max_w, max_h).ok()
}

// ── Grid context ────────────────────────────────────────────────────

pub struct GridCtx {
    pub(crate) filter: GridFilter,
    pub(crate) repeat: u32,
    pub(crate) l4_dx: i32,
    pub(crate) l4_dy: i32,
}

impl GridCtx {
    #[must_use]
    pub fn new() -> Self {
        Self {
            filter: GridFilter::new(),
            repeat: 0,
            l4_dx: 0,
            l4_dy: 0,
        }
    }
}

impl Default for GridCtx {
    fn default() -> Self {
        Self::new()
    }
}

// ── Mutable session view ────────────────────────────────────────────

/// Bundles every piece of mutable grid state for the active monitor so
/// that event handlers can operate on it as a unit.
///
/// `sel_overlay` holds the multi-monitor selection hint (used only during the
/// `Selecting` phase); the grid session itself is held whole in `state`.
pub struct GridStateMut<'a> {
    sel_overlay: &'a mut Option<Overlay>,
    state: &'a mut Option<GridState>,
    ctx: &'a mut Option<GridCtx>,
}

impl<'a> GridStateMut<'a> {
    #[must_use]
    pub fn new(
        sel_overlay: &'a mut Option<Overlay>,
        state: &'a mut Option<GridState>,
        ctx: &'a mut Option<GridCtx>,
    ) -> Self {
        Self {
            sel_overlay,
            state,
            ctx,
        }
    }

    fn load(&mut self, s: GridState) {
        *self.state = Some(s);
    }

    fn reset_ctx(&mut self) {
        *self.ctx = Some(GridCtx::new());
    }

    // ── Selecting phase ──

    pub fn handle_selecting(
        &mut self,
        code: u16,
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
                *self.sel_overlay = None;
                if let Ok(s) = init_grid_monitor(*grid_monitor_idx, monitors, None) {
                    self.load(s);
                    self.reset_ctx();
                    *grid_phase = GridPhase::Navigating;
                    info!("[grid] selected monitor {}", *grid_monitor_idx + 1);
                }
            } else {
                *select_hint = format!("{}", b as char);
                if let Some(o) = self.sel_overlay.as_mut() {
                    let _ = redraw_select_hint(o, monitors, select_hint);
                }
            }
        }
    }

    // ── Navigating phase ──

    pub fn handle_navigating(
        &mut self,
        code: u16,
        monitors: &MonitorList,
        grid_monitor_idx: &mut usize,
        mods: &ModState,
        grid_phase: GridPhase,
    ) {
        // L4: shift + hjkl = micro-adjust within L3 cell.
        if mods.shift && (code == KEY_H || code == KEY_J || code == KEY_K || code == KEY_L) {
            let (Some(s), Some(gctx)) = (self.state.as_mut(), self.ctx.as_mut()) else {
                return;
            };
            if gctx.filter.len() >= 3 {
                match code {
                    KEY_H => gctx.l4_dx = (gctx.l4_dx - 1).max(-3),
                    KEY_L => gctx.l4_dx = (gctx.l4_dx + 1).min(3),
                    KEY_K => gctx.l4_dy = (gctx.l4_dy - 1).max(-3),
                    KEY_J => gctx.l4_dy = (gctx.l4_dy + 1).min(3),
                    _ => {}
                }
                if let Err(e) = Self::redraw(
                    &s.overlay,
                    &s.cfg,
                    &s.cache,
                    &mut s.draw_states,
                    s.font_size,
                    gctx,
                ) {
                    warn!("[grid] l4 display error: {e}");
                }
            }
            return;
        }

        if code == KEY_TAB && monitors.len() > 1 {
            *grid_monitor_idx = (*grid_monitor_idx + 1) % monitors.len();
            *self.state = None;
            if let Ok(s) = init_grid_monitor(*grid_monitor_idx, monitors, None) {
                self.load(s);
                self.reset_ctx();
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
                && let Err(e) = self.process_byte(b)
            {
                warn!("[grid] error: {e}");
            }
        }
    }

    // ── Byte-level input handler ──

    /// Re-render the active monitor from its current filter + L4 offset.
    fn redraw(
        o: &Overlay,
        gcfg: &GridConfig,
        gcache: &TextCache,
        gstats: &mut [DrawState],
        font_size: f32,
        gctx: &GridCtx,
    ) -> Result<()> {
        display_update(
            o,
            gstats,
            gcfg,
            gcache,
            font_size,
            &gctx.filter,
            Some((gctx.l4_dx, gctx.l4_dy)),
        )
    }

    /// Clear the L4 micro-adjust offset whenever the selection changes, so a
    /// previous shift+hjkl nudge never carries over to a new region.
    fn reset_l4(gctx: &mut GridCtx) {
        gctx.l4_dx = 0;
        gctx.l4_dy = 0;
    }

    fn process_byte(&mut self, byte: u8) -> Result<()> {
        let (Some(s), Some(gctx)) = (self.state.as_mut(), self.ctx.as_mut()) else {
            return Ok(());
        };

        match byte {
            // ';' = warp & reset (was Enter).
            b';' => {
                cursor_warp(&mut s.mouse, &gctx.filter, &s.draw_states, gctx)?;
                if let Some((cx, cy)) = region_center(&gctx.filter, &s.draw_states, gctx) {
                    s.overlay.pointer_warp(cx as i32, cy as i32)?;
                }
                gctx.filter.clear();
                gctx.repeat = 0;
                Self::reset_l4(gctx);
                Self::redraw(
                    &s.overlay,
                    &s.cfg,
                    &s.cache,
                    &mut s.draw_states,
                    s.font_size,
                    gctx,
                )?;
            }

            // 'p' = step back L3 → L2 → L1 (was Backspace).
            b'p' => {
                gctx.filter.pop();
                if gctx.filter.len() >= 2 {
                    gctx.filter.pop();
                }
                gctx.repeat = 0;
                Self::reset_l4(gctx);
                Self::redraw(
                    &s.overlay,
                    &s.cfg,
                    &s.cache,
                    &mut s.draw_states,
                    s.font_size,
                    gctx,
                )?;
            }

            ch => {
                let c = ch as char;

                if !gctx.filter.is_empty() && c.is_ascii_digit() {
                    gctx.repeat = gctx
                        .repeat
                        .saturating_mul(10)
                        .saturating_add(u32::from(c as u8 - b'0'));
                    return Ok(());
                }

                if gctx.filter.len() >= 2
                    && let Some(btn) = action_key(c)
                {
                    cursor_action(
                        &mut s.mouse,
                        &gctx.filter,
                        &s.draw_states,
                        btn,
                        gctx.repeat,
                        gctx,
                    )?;
                    if let Some((cx, cy)) = region_center(&gctx.filter, &s.draw_states, gctx) {
                        s.overlay.pointer_warp(cx as i32, cy as i32)?;
                    }
                    gctx.filter.clear();
                    gctx.repeat = 0;
                    Self::reset_l4(gctx);
                    Self::redraw(
                        &s.overlay,
                        &s.cfg,
                        &s.cache,
                        &mut s.draw_states,
                        s.font_size,
                        gctx,
                    )?;
                    return Ok(());
                }

                let valid = match gctx.filter.len() {
                    0 => l1_key_pos(c).is_some(),
                    1 => l2_key_pos(c).is_some(),
                    2 | 3 => l3_key_pos(c).is_some(),
                    _ => return Ok(()),
                };
                if !valid {
                    return Ok(());
                }

                if gctx.filter.len() >= 3 {
                    gctx.filter.pop();
                }
                gctx.filter.push(c);
                gctx.repeat = 0;
                Self::reset_l4(gctx);
                Self::redraw(
                    &s.overlay,
                    &s.cfg,
                    &s.cache,
                    &mut s.draw_states,
                    s.font_size,
                    gctx,
                )?;
            }
        }
        Ok(())
    }
}
