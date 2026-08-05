// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::OnceLock;

use anyhow::{Context, Result};
use fontdue::Font;
use tiny_skia::Pixmap;

use super::{Grid, GridConfig, GridFilter};
use crate::config::{FALLBACK_HEIGHT, FONT_ROW_DIVISOR, FONT_SIZE_MAX, FONT_SIZE_MIN};
use crate::overlay::Overlay;
use crate::render::TextCache;

pub const FONT_DATA: &[u8] = include_bytes!("../../../assets/font.ttf");

/// Parsed once per process; `Font` is `Send + Sync` (plain data).
pub(crate) static FONT: OnceLock<Font> = OnceLock::new();

pub(crate) fn font() -> &'static Font {
    FONT.get_or_init(|| {
        fontdue::Font::from_bytes(FONT_DATA, fontdue::FontSettings::default())
            .expect("embedded font data corrupted")
    })
}

pub(crate) static MONITOR_NAME: OnceLock<String> = OnceLock::new();

/// Per-monitor state: the 27×27 grid, its base-layer RGBA bytes, and a
/// persistent pixmap that is re-uploaded on every redraw.
pub struct DrawState {
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
    let cache = TextCache::new(font(), font_size);

    let mut draw_states = Vec::new();
    for (idx, &(x, y, w, h)) in monitors.iter().enumerate() {
        let grid = Grid::new(u32::from(w), u32::from(h), &cfg);
        let mut pixmap = Pixmap::new(u32::from(w), u32::from(h)).context("pixmap")?;
        let bg_pixel = crate::render::render_bg(&mut pixmap, &cfg);
        crate::render::render_l1(&mut pixmap, &cfg);

        let all_px = pixmap.pixels();
        let mut mask_idx = Vec::new();
        let mut mask_px = Vec::new();
        for (i, &p) in all_px.iter().enumerate() {
            if p != bg_pixel {
                mask_idx.push(i);
                mask_px.push(p);
            }
        }

        crate::render::render_labels(&mut pixmap, &grid, &cfg, &cache, font_size, None, 0);
        overlay.add_window(x, y, w, h)?;
        overlay.upload(idx, &pixmap)?;
        draw_states.push(DrawState {
            grid,
            pixmap,
            bg_pixel,
            mask_idx,
            mask_px,
            l3_cache: None,
        });
        MONITOR_NAME.get_or_init(|| format!("mon {idx}"));
    }
    overlay.show_all()?;
    overlay.redraw_all()?;

    Ok((cfg, font_size, cache, draw_states))
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
