// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::OnceLock;

use anyhow::{Context, Result};
use fontdue::Font;
use log::info;
use tiny_skia::Pixmap;

use crate::config::{
    FALLBACK_HEIGHT, FONT_ROW_DIVISOR, FONT_SIZE_MAX, FONT_SIZE_MIN, action_key, l1_key_pos,
    l2_key_pos, l3_key_pos,
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
    bg_pixel: tiny_skia::PremultipliedColorU8,
    mask_idx: Vec<usize>,
    mask_px: Vec<tiny_skia::PremultipliedColorU8>,
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
        let bg_pixel = crate::render::render_bg(&mut pixmap, &cfg);
        crate::render::render_l1(&mut pixmap, &cfg, bg_pixel);

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
        });
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
                2 | 3 => l3_key_pos(c).is_some(),
                _ => return Ok(false),
            };
            if !valid {
                return Ok(false);
            }

            if ctx.filter.len() >= 3 {
                ctx.filter.pop();
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
    let l2_rect = if filter.is_empty() {
        None
    } else {
        filter.input().chars().next().and_then(l1_key_pos).map(|(r, c)| {
            let w = states[0].pixmap.width() as f32;
            let h = states[0].pixmap.height() as f32;
            let cw = w / 9.0;
            let ch = h / 3.0;
            (c as f32 * cw, r as f32 * ch, cw, ch)
        })
    };
    let l3_rect = if filter.len() >= 2 {
        let input = filter.input();
        states
            .iter()
            .find_map(|ds| ds.grid.cell_by_label(&input[..2]))
            .map(|c| {
                (
                    c.rect.x() as f32,
                    c.rect.y() as f32,
                    c.rect.width() as f32,
                    c.rect.height() as f32,
                )
            })
    } else {
        None
    };
    let l3_sel = if filter.len() >= 3 {
        filter.input().chars().nth(2).and_then(l3_key_pos)
    } else {
        None
    };

    for (idx, ds) in states.iter_mut().enumerate() {
        ds.pixmap.pixels_mut().fill(ds.bg_pixel);
        for i in 0..ds.mask_idx.len() {
            ds.pixmap.pixels_mut()[ds.mask_idx[i]] = ds.mask_px[i];
        }
        crate::render::render_labels(
            &mut ds.pixmap,
            &ds.grid,
            cfg,
            cache,
            font_size,
            Some(filter),
            filter.len().min(2),
        );
        if let Some((x, y, w, h)) = l2_rect {
            render_l2_grid(&mut ds.pixmap, (x, y, w, h), cfg);
        }
        if let Some((x, y, w, h)) = l3_rect {
            render_l3_overlay(&mut ds.pixmap, (x, y, w, h), cfg, font_size * 0.75, l3_sel);
        }
        overlay.upload(idx, &ds.pixmap)?;
    }

    overlay.show_all()?;
    overlay.redraw_all()?;
    Ok(())
}

fn render_l2_grid(
    pixmap: &mut tiny_skia::Pixmap,
    (x, y, w, h): (f32, f32, f32, f32),
    cfg: &GridConfig,
) {
    use tiny_skia::{Color, Paint, PathBuilder, Shader, Stroke, Transform};
    let line = Color::from_rgba8(
        cfg.line_color[0],
        cfg.line_color[1],
        cfg.line_color[2],
        cfg.line_color[3],
    );
    let stroke = Stroke {
        width: cfg.line_width,
        ..Default::default()
    };
    let paint = Paint {
        shader: Shader::SolidColor(line),
        anti_alias: true,
        ..Default::default()
    };
    // L2: 3 cols × 9 rows within the L1 cell
    for col in 1..3 {
        let lx = x + col as f32 * w / 3.0;
        let mut pb = PathBuilder::new();
        pb.move_to(lx, y);
        pb.line_to(lx, y + h);
        pixmap.stroke_path(&pb.finish().unwrap(), &paint, &stroke, Transform::identity(), None);
    }
    for row in 1..9 {
        let ly = y + row as f32 * h / 9.0;
        let mut pb = PathBuilder::new();
        pb.move_to(x, ly);
        pb.line_to(x + w, ly);
        pixmap.stroke_path(&pb.finish().unwrap(), &paint, &stroke, Transform::identity(), None);
    }
}

fn render_l3_overlay(
    pixmap: &mut tiny_skia::Pixmap,
    rect: (f32, f32, f32, f32),
    cfg: &GridConfig,
    font_size: f32,
    sel: Option<(usize, usize)>,
) {
    use crate::config::L3_KEYS;
    use crate::render::TextCache;
    use tiny_skia::{Color, Paint, PathBuilder, Shader, Stroke, Transform};

    let (x, y, w, h) = rect;

    let font = fontdue::Font::from_bytes(crate::FONT_DATA, fontdue::FontSettings::default()).unwrap();
    let cache = TextCache::new(&font, font_size);

    // Clear L3 region to transparent (remove L2 content, keep desktop visible).
    let pw = pixmap.width() as usize;
    let ph = pixmap.height() as usize;
    {
        let pixels = pixmap.pixels_mut();
        let transparent = tiny_skia::PremultipliedColorU8::from_rgba(0, 0, 0, 0).unwrap();
        let y0 = (y.max(0.0) as usize).min(ph);
        let y1 = ((y + h).max(0.0) as usize).min(ph);
        let x0 = (x.max(0.0) as usize).min(pw);
        let x1 = ((x + w).max(0.0) as usize).min(pw);
        for py in y0..y1 {
            let off = py * pw;
            for px in x0..x1 {
                pixels[off + px] = transparent;
            }
        }
    }

    // Fill with background (same as render_base)
    let bg = Color::from_rgba8(
        cfg.bg_color[0],
        cfg.bg_color[1],
        cfg.bg_color[2],
        cfg.bg_color[3],
    );
    pixmap.fill_path(
        &PathBuilder::from_rect(tiny_skia::Rect::from_xywh(x, y, w, h).unwrap()),
        &Paint {
            shader: Shader::SolidColor(bg),
            ..Default::default()
        },
        tiny_skia::FillRule::Winding,
        Transform::identity(),
        None,
    );

    let line_color = Color::from_rgba8(
        cfg.line_color[0],
        cfg.line_color[1],
        cfg.line_color[2],
        cfg.line_color[3],
    );
    let label_color = cfg.label_color;

    // Grid lines: 5 cols, 3 rows
    let stroke = Stroke {
        width: cfg.line_width,
        ..Default::default()
    };
    let line_paint = Paint {
        shader: Shader::SolidColor(line_color),
        anti_alias: true,
        ..Default::default()
    };

    for col in 1..5 {
        let lx = x + col as f32 * w / 5.0;
        let mut pb = PathBuilder::new();
        pb.move_to(lx, y);
        pb.line_to(lx, y + h);
        pixmap.stroke_path(
            &pb.finish().unwrap(),
            &line_paint,
            &stroke,
            Transform::identity(),
            None,
        );
    }
    for row in 1..3 {
        let ly = y + row as f32 * h / 3.0;
        let mut pb = PathBuilder::new();
        pb.move_to(x, ly);
        pb.line_to(x + w, ly);
        pixmap.stroke_path(
            &pb.finish().unwrap(),
            &line_paint,
            &stroke,
            Transform::identity(),
            None,
        );
    }

    // Labels (skip selected cell)
    for (row, krow) in L3_KEYS.iter().enumerate() {
        for (col, &ch) in krow.iter().enumerate() {
            if sel == Some((row, col)) {
                continue;
            }
            let cx = x + (col as f32 + 0.5) * w / 5.0;
            let cy = y + (row as f32 + 0.5) * h / 3.0;
            crate::render::draw_text(
                pixmap,
                &ch.to_string(),
                cx,
                cy,
                &cache,
                font_size,
                label_color,
            );
        }
    }

    // Highlight selected L3 sub-cell
    if let Some((sr, sc)) = sel {
        let sx = x + sc as f32 * w / 5.0;
        let sy = y + sr as f32 * h / 3.0;
        let sw = w / 5.0;
        let sh = h / 3.0;
        let hl_color = Color::from_rgba8(192, 255, 192, 128);
        let hl_stroke = Stroke {
            width: 2.0,
            ..Default::default()
        };
        let hl_paint = Paint {
            shader: Shader::SolidColor(hl_color),
            anti_alias: true,
            ..Default::default()
        };
        let mut hl_pb = PathBuilder::new();
        hl_pb.move_to(sx, sy);
        hl_pb.line_to(sx + sw, sy);
        hl_pb.line_to(sx + sw, sy + sh);
        hl_pb.line_to(sx, sy + sh);
        hl_pb.close();
        pixmap.stroke_path(
            &hl_pb.finish().unwrap(),
            &hl_paint,
            &hl_stroke,
            Transform::identity(),
            None,
        );
    }
}

// ── Region geometry ─────────────────────────────────────────────────

/// Get the currently-selected rect from the filter.
fn region_rect(filter: &GridFilter, states: &[DrawState]) -> Option<(f32, f32, f32, f32)> {
    let input = filter.input();
    if input.len() < 2 {
        return None;
    }
    let cell = states
        .iter()
        .find_map(|ds| ds.grid.cell_by_label(&input[..2]))?;
    let (px, py, pw, ph) = (
        cell.rect.x() as f32,
        cell.rect.y() as f32,
        cell.rect.width() as f32,
        cell.rect.height() as f32,
    );
    if let Some(ch) = input.chars().nth(2) {
        let (r, c) = l3_key_pos(ch)?;
        Some((
            px + c as f32 * pw / 5.0,
            py + r as f32 * ph / 3.0,
            pw / 5.0,
            ph / 3.0,
        ))
    } else {
        Some((px, py, pw, ph))
    }
}

/// Centre pixel of the current region.
fn region_center(filter: &GridFilter, states: &[DrawState]) -> Option<(f32, f32)> {
    let (x, y, w, h) = region_rect(filter, states)?;
    Some((x + w * 0.5, y + h * 0.5))
}
