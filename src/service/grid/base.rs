// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

use tiny_skia::{Pixmap, PremultipliedColorU8, Stroke};

use super::{Grid, GridConfig, GridFilter};
use crate::render::{
    TextCache, char_width, draw_char_glyph, draw_line, draw_text, fill_rect, rgba,
};

// ── Base layer (background + grid lines + labels) ───────────────────

/// Fill pixmap with `BG_COLOR`, return the premultiplied pixel value.
pub(crate) fn render_bg(pixmap: &mut Pixmap, cfg: &GridConfig) -> PremultipliedColorU8 {
    pixmap
        .pixels_mut()
        .fill(PremultipliedColorU8::from_rgba(0, 0, 0, 0).unwrap());
    let w = pixmap.width() as f32;
    let h = pixmap.height() as f32;
    let bg = rgba(cfg.bg_color);
    fill_rect(pixmap, 0.0, 0.0, w, h, bg);
    pixmap.pixels()[0]
}

/// Draw L1 grid lines (9×3) onto a pixmap pre-filled with `BG_COLOR`.
pub(crate) fn render_l1(pixmap: &mut Pixmap, cfg: &GridConfig) {
    let w = pixmap.width() as f32;
    let h = pixmap.height() as f32;
    let line = rgba(cfg.line_color);
    let stroke = Stroke {
        width: 3.0,
        ..Default::default()
    };
    for col in 1..9 {
        let x = (col as f32 / 9.0) * w;
        draw_line(pixmap, x, 0.0, x, h, &line, &stroke);
    }
    for row in 1..3 {
        let y = (row as f32 / 3.0) * h;
        draw_line(pixmap, 0.0, y, w, y, &line, &stroke);
    }
}

pub(crate) fn render_labels(
    pixmap: &mut Pixmap,
    grid: &Grid,
    cfg: &GridConfig,
    cache: &TextCache,
    font_size: f32,
    filter: Option<&GridFilter>,
    phase: usize,
) {
    let highlight = cfg.label_color;
    let dim = [192, 255, 192, 64];

    for cell in &grid.cells {
        if filter.is_some_and(|f| !f.matches(&cell.label)) {
            continue;
        }
        let chars: Vec<char> = cell.label.chars().collect();
        if chars.len() == 2 {
            let (col0, col1) = match phase {
                0 => (highlight, dim),
                1 => (dim, highlight),
                _ => (dim, dim),
            };
            let gap = font_size * 0.10;
            let w0 = char_width(cache, chars[0]);
            let w1 = char_width(cache, chars[1]);
            let total = w0 + gap + w1;
            let cx0 = cell.center.0 - total * 0.5 + w0 * 0.5;
            let cx1 = cx0 + w0 * 0.5 + gap + w1 * 0.5;
            draw_char_glyph(pixmap, chars[0], cx0, cell.center.1, cache, col0);
            draw_char_glyph(pixmap, chars[1], cx1, cell.center.1, cache, col1);
        } else {
            draw_text(
                pixmap,
                &cell.label,
                cell.center.0,
                cell.center.1,
                cache,
                font_size,
                highlight,
            );
        }
    }
}
