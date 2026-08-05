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
    FALLBACK_HEIGHT, FONT_ROW_DIVISOR, FONT_SIZE_MAX, FONT_SIZE_MIN, action_key, l1_key_pos,
    l2_key_pos, l3_key_pos,
};
use crate::font;
use crate::keymap::{KEY_H, KEY_J, KEY_K, KEY_L, KEY_TAB, ModState, key_map};
use crate::overlay::{Monitor, Overlay};
use crate::render::TextCache;
use crate::uinput::Mouse;

/// Ordered list of active monitors.
pub type MonitorList = Vec<Monitor>;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GridPhase {
    Selecting,
    Navigating,
}

/// Fully-initialized grid session state for a single monitor.
pub(crate) struct GridState {
    pub(crate) overlay: Overlay,
    pub(crate) mouse: Option<Mouse>,
    pub(crate) cfg: GridConfig,
    pub(crate) cache: TextCache,
    pub(crate) font_size: f32,
    pub(crate) draw_states: Vec<DrawState>,
}

/// Per-monitor state: the 27×27 grid, its base-layer RGBA bytes, and a
/// persistent pixmap that is re-uploaded on every redraw.
pub(crate) struct DrawState {
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

pub(crate) fn init_overlay(
    overlay: &mut Overlay,
    monitors: &[Monitor],
) -> Result<(GridConfig, f32, TextCache, Vec<DrawState>)> {
    let cfg = GridConfig::default();
    let font_size = (f32::from(
        monitors
            .iter()
            .map(|m| m.h)
            .min()
            .unwrap_or(FALLBACK_HEIGHT),
    ) / cfg.rows as f32
        / FONT_ROW_DIVISOR)
        .clamp(FONT_SIZE_MIN, FONT_SIZE_MAX)
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

pub(crate) fn init_grid_monitor(
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

// ── Mutable session view ────────────────────────────────────────────

/// Bundles every piece of mutable grid state for the active monitor so
/// that event handlers can operate on it as a unit.
pub struct GridStateMut<'a> {
    overlay: &'a mut Option<Overlay>,
    cfg: &'a mut Option<GridConfig>,
    cache: &'a mut Option<TextCache>,
    font_size: &'a mut f32,
    states: &'a mut Option<Vec<DrawState>>,
    ctx: &'a mut Option<GridCtx>,
    mouse: &'a mut Option<Mouse>,
}

impl<'a> GridStateMut<'a> {
    #[must_use]
    pub fn new(
        overlay: &'a mut Option<Overlay>,
        cfg: &'a mut Option<GridConfig>,
        cache: &'a mut Option<TextCache>,
        font_size: &'a mut f32,
        states: &'a mut Option<Vec<DrawState>>,
        ctx: &'a mut Option<GridCtx>,
        mouse: &'a mut Option<Mouse>,
    ) -> Self {
        Self {
            overlay,
            cfg,
            cache,
            font_size,
            states,
            ctx,
            mouse,
        }
    }

    fn load(&mut self, s: GridState) {
        *self.overlay = Some(s.overlay);
        *self.mouse = s.mouse;
        *self.cfg = Some(s.cfg);
        *self.cache = Some(s.cache);
        *self.font_size = s.font_size;
        *self.states = Some(s.draw_states);
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
                *self.overlay = None;
                if let Ok(s) = init_grid_monitor(*grid_monitor_idx, monitors, None) {
                    self.load(s);
                    self.reset_ctx();
                    *grid_phase = GridPhase::Navigating;
                    info!("[grid] selected monitor {}", *grid_monitor_idx + 1);
                }
            } else {
                *select_hint = format!("{}", b as char);
                if let Some(o) = self.overlay.as_mut() {
                    let _ = redraw_select_hint(o, monitors, select_hint);
                }
            }
        }
        if let Some(o) = self.overlay.as_mut()
            && b'\x1b' == byte.unwrap_or(0)
        {
            select_hint.clear();
            let _ = redraw_select_hint(o, monitors, "");
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
        // L4: alt + hjkl = micro-adjust within L3 cell.
        if mods.alt && (code == KEY_H || code == KEY_J || code == KEY_K || code == KEY_L) {
            if let (Some(o), Some(gcfg), Some(gcache), Some(gstats), Some(gctx)) = (
                self.overlay.as_ref(),
                self.cfg.as_ref(),
                self.cache.as_ref(),
                self.states.as_mut(),
                self.ctx.as_mut(),
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
                    *self.font_size,
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
            *self.overlay = None;
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

    fn process_byte(&mut self, byte: u8) -> Result<()> {
        let (Some(o), Some(gcfg), Some(gcache), Some(gstats), Some(gctx)) = (
            self.overlay.as_mut(),
            self.cfg.as_mut(),
            self.cache.as_mut(),
            self.states.as_mut(),
            self.ctx.as_mut(),
        ) else {
            return Ok(());
        };

        match byte {
            b'\r' | b'\n' => {
                cursor_warp(self.mouse, &gctx.filter, gstats, gctx)?;
                if let Some((cx, cy)) = region_center(&gctx.filter, gstats, gctx) {
                    o.pointer_warp(cx as i16, cy as i16)?;
                }
                gctx.filter.clear();
                gctx.repeat = 0;
                display_update(
                    o,
                    gstats,
                    gcfg,
                    gcache,
                    *self.font_size,
                    &gctx.filter,
                    Some((gctx.l4_dx, gctx.l4_dy)),
                )?;
            }

            0x1b => {
                gctx.filter.clear();
                gctx.repeat = 0;
                display_update(
                    o,
                    gstats,
                    gcfg,
                    gcache,
                    *self.font_size,
                    &gctx.filter,
                    Some((gctx.l4_dx, gctx.l4_dy)),
                )?;
            }

            0x7f | b'\x08' => {
                gctx.filter.pop();
                if gctx.filter.len() >= 2 {
                    gctx.filter.pop();
                }
                gctx.repeat = 0;
                display_update(
                    o,
                    gstats,
                    gcfg,
                    gcache,
                    *self.font_size,
                    &gctx.filter,
                    Some((gctx.l4_dx, gctx.l4_dy)),
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
                    cursor_action(self.mouse, &gctx.filter, gstats, btn, gctx.repeat, gctx)?;
                    if let Some((cx, cy)) = region_center(&gctx.filter, gstats, gctx) {
                        o.pointer_warp(cx as i16, cy as i16)?;
                    }
                    gctx.filter.clear();
                    gctx.repeat = 0;
                    display_update(
                        o,
                        gstats,
                        gcfg,
                        gcache,
                        *self.font_size,
                        &gctx.filter,
                        Some((gctx.l4_dx, gctx.l4_dy)),
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
                display_update(
                    o,
                    gstats,
                    gcfg,
                    gcache,
                    *self.font_size,
                    &gctx.filter,
                    Some((gctx.l4_dx, gctx.l4_dy)),
                )?;
            }
        }
        Ok(())
    }
}
