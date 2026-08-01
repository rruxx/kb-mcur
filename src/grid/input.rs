// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::OnceLock;

use anyhow::{Context, Result};
use fontdue::Font;
use log::info;
use tiny_skia::Pixmap;

use crate::config::{
    FALLBACK_HEIGHT, FONT_ROW_DIVISOR, FONT_SIZE_MAX, FONT_SIZE_MIN, action_key, l1_key_pos,
    l2_key_pos,
};
use crate::grid::{Grid, GridConfig, GridFilter};
use crate::overlay::Overlay;
use crate::render::TextCache;
use crate::uinput::Mouse;

pub const FONT_DATA: &[u8] = include_bytes!("../../assets/font.ttf");

static MONITOR_NAME: OnceLock<String> = OnceLock::new();

/// Per-monitor state: the 27×27 grid, its base-layer RGBA bytes, and a
/// persistent pixmap that is re-uploaded on every redraw.
pub struct DrawState {
    grid: Grid,
    pixmap: Pixmap,
}

pub fn init_overlay(
    overlay: &mut Overlay,
    font: &Font,
    monitors: &[(i32, i32, u16, u16)],
) -> Result<(GridConfig, f32, TextCache, Vec<DrawState>)> {
    let cfg = GridConfig::default();
    let font_size = (f32::from(
        monitors
            .iter()
            .map(|m| m.3)
            .min()
            .unwrap_or(FALLBACK_HEIGHT),
    ) / cfg.rows as f32
        / FONT_ROW_DIVISOR)
        .clamp(FONT_SIZE_MIN, FONT_SIZE_MAX)
        .round();
    let cache = TextCache::new(font, font_size);

    let mut draw_states = Vec::new();
    for (idx, &(x, y, w, h)) in monitors.iter().enumerate() {
        let grid = Grid::new(u32::from(w), u32::from(h), &cfg);
        let mut pixmap = Pixmap::new(u32::from(w), u32::from(h)).context("pixmap")?;
        crate::render::render_base(&mut pixmap, &grid, &cfg);
        crate::render::render_labels(&mut pixmap, &grid, &cfg, &cache, font_size, None);
        overlay.add_window(x, y, w, h)?;
        overlay.upload(idx, &pixmap)?;
        draw_states.push(DrawState { grid, pixmap });
    }
    overlay.show_all()?;
    overlay.redraw_all()?;

    Ok((cfg, font_size, cache, draw_states))
}

// ── Grid context ────────────────────────────────────────────────────

pub struct GridCtx {
    pub(crate) filter: GridFilter,
    pub(crate) repeat: u32,
}

impl GridCtx {
    #[must_use]
    pub fn new() -> Self {
        Self {
            filter: GridFilter::new(),
            repeat: 0,
        }
    }
}

impl Default for GridCtx {
    fn default() -> Self {
        Self::new()
    }
}

// ── Grid-mode byte handler ──────────────────────────────────────────

/// Single-byte input handler for interactive grid mode.
#[allow(clippy::too_many_arguments)]
pub fn process_byte(
    byte: u8,
    overlay: &mut Overlay,
    mouse: &mut Option<Mouse>,
    cfg: &GridConfig,
    cache: &TextCache,
    font_size: f32,
    draw_states: &mut [DrawState],
    ctx: &mut GridCtx,
) -> Result<bool> {
    match byte {
        b'\r' | b'\n' | b' ' => {
            cursor_warp(mouse, &ctx.filter, draw_states)?;
            if let Some((cx, cy)) = region_center(&ctx.filter, draw_states) {
                overlay.pointer_warp(cx as i16, cy as i16)?;
            }
            ctx.filter.clear();
            ctx.repeat = 0;
            display_update(overlay, draw_states, cfg, cache, font_size, &ctx.filter)?;
            return Ok(false);
        }

        0x1b => {
            ctx.filter.clear();
            ctx.repeat = 0;
            display_update(overlay, draw_states, cfg, cache, font_size, &ctx.filter)?;
        }

        0x7f | b'\x08' => {
            ctx.filter.pop();
            ctx.repeat = 0;
            display_update(overlay, draw_states, cfg, cache, font_size, &ctx.filter)?;
        }

        ch => {
            let c = ch as char;

            if !ctx.filter.is_empty() && c.is_ascii_digit() {
                ctx.repeat = ctx
                    .repeat
                    .saturating_mul(10)
                    .saturating_add(u32::from(c as u8 - b'0'));
                return Ok(false);
            }

            if ctx.filter.len() >= 2
                && let Some(btn) = action_key(c)
            {
                cursor_action(mouse, &ctx.filter, draw_states, btn, ctx.repeat)?;
                if let Some((cx, cy)) = region_center(&ctx.filter, draw_states) {
                    overlay.pointer_warp(cx as i16, cy as i16)?;
                }
                ctx.filter.clear();
                ctx.repeat = 0;
                display_update(overlay, draw_states, cfg, cache, font_size, &ctx.filter)?;
                return Ok(false);
            }

            let valid = match ctx.filter.len() {
                0 => l1_key_pos(c).is_some(),
                1 => l2_key_pos(c).is_some(),
                _ => return Ok(false),
            };
            if !valid {
                return Ok(false);
            }

            ctx.filter.push(c);
            ctx.repeat = 0;
            display_update(overlay, draw_states, cfg, cache, font_size, &ctx.filter)?;
        }
    }
    Ok(false)
}

// ── Cursor & button actions ────────────────────────────────────────

/// Move the cursor to the centre of the currently-selected region.
fn cursor_warp(mouse: &mut Option<Mouse>, filter: &GridFilter, states: &[DrawState]) -> Result<()> {
    let Some(m) = mouse else {
        return Ok(());
    };
    if let Some((cx, cy)) = region_center(filter, states) {
        m.warp(cx as i16, cy as i16)?;
        info!(
            "=> {} ({cx:.0}, {cy:.0})",
            MONITOR_NAME.get().map_or("?", |s| s.as_str())
        );
    }
    Ok(())
}

/// Warp the cursor, then click N times.
fn cursor_action(
    mouse: &mut Option<Mouse>,
    filter: &GridFilter,
    states: &[DrawState],
    button: u8,
    repeat: u32,
) -> Result<()> {
    let Some(m) = mouse else {
        return Ok(());
    };
    let center = region_center(filter, states);
    if let Some((cx, cy)) = center {
        m.warp(cx as i16, cy as i16)?;
    }
    let name = MONITOR_NAME.get().map_or("?", |s| s.as_str());
    let n = if repeat == 0 { 1 } else { repeat };
    m.click(button, n)?;
    if let Some((cx, cy)) = center {
        info!("click btn{button} x{n}  {name} ({cx:.0}, {cy:.0})");
    }
    Ok(())
}

// ── Display update ──────────────────────────────────────────────────

fn display_update(
    overlay: &Overlay,
    states: &mut [DrawState],
    cfg: &GridConfig,
    cache: &TextCache,
    font_size: f32,
    filter: &GridFilter,
) -> Result<()> {
    for (idx, ds) in states.iter_mut().enumerate() {
        crate::render::render_base(&mut ds.pixmap, &ds.grid, cfg);
        crate::render::render_labels(
            &mut ds.pixmap,
            &ds.grid,
            cfg,
            cache,
            font_size,
            Some(filter),
        );
        overlay.upload(idx, &ds.pixmap)?;
    }

    overlay.show_all()?;
    overlay.redraw_all()?;
    Ok(())
}

// ── Region geometry ─────────────────────────────────────────────────

/// Get the currently-selected cell's rect from the 2‑character filter.
fn region_rect(filter: &GridFilter, states: &[DrawState]) -> Option<(f32, f32, f32, f32)> {
    let input = filter.input();
    if input.len() < 2 {
        return None;
    }
    let cell = states
        .iter()
        .find_map(|ds| ds.grid.cell_by_label(&input[..2]))?;
    Some((
        cell.rect.x() as f32,
        cell.rect.y() as f32,
        cell.rect.width() as f32,
        cell.rect.height() as f32,
    ))
}

/// Centre pixel of the current region.
fn region_center(filter: &GridFilter, states: &[DrawState]) -> Option<(f32, f32)> {
    let (x, y, w, h) = region_rect(filter, states)?;
    Some((x + w * 0.5, y + h * 0.5))
}
