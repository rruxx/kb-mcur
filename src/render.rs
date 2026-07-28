// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashMap;

use fontdue::{Font, Metrics};
use tiny_skia::{
    Color, Paint, PathBuilder, Pixmap, PremultipliedColorU8, Rect, Shader, Stroke, Transform,
};

use crate::config::{BISECT_LABELS, SUBGRID_LABELS};
use crate::grid::{Grid, GridConfig, GridFilter};
use tiny_skia::IntRect;

pub struct TextCache {
    glyphs: HashMap<char, (Metrics, Vec<u8>)>,
}

impl TextCache {
    #[must_use]
    pub fn new(font: &Font, size: f32) -> Self {
        let mut glyphs = HashMap::new();
        for ch in 'a'..='z' {
            glyphs.insert(ch, font.rasterize(ch, size));
        }
        for ch in '0'..='9' {
            glyphs.insert(ch, font.rasterize(ch, size));
        }
        Self { glyphs }
    }

    fn get(&self, ch: char) -> Option<&(Metrics, Vec<u8>)> {
        self.glyphs.get(&ch)
    }
}

// ── Base layer (background + grid lines) ────────────────────────────

pub fn render_base(pixmap: &mut Pixmap, grid: &Grid, cfg: &GridConfig) {
    // Clear so repeated calls don't accumulate
    pixmap
        .pixels_mut()
        .fill(PremultipliedColorU8::from_rgba(0, 0, 0, 0).unwrap());
    let w = pixmap.width() as f32;
    let h = pixmap.height() as f32;
    fill_rect(pixmap, 0.0, 0.0, w, h, rgba(cfg.bg_color));
    let line = rgba(cfg.line_color);
    let stroke = Stroke {
        width: cfg.line_width,
        ..Default::default()
    };
    for row in 1..grid.rows {
        let y = (row as f32 / grid.rows as f32) * h;
        draw_line(pixmap, 0.0, y, w, y, &line, &stroke);
    }
    for col in 1..grid.cols {
        let x = (col as f32 / grid.cols as f32) * w;
        draw_line(pixmap, x, 0.0, x, h, &line, &stroke);
    }
}

pub fn render_labels(
    pixmap: &mut Pixmap,
    grid: &Grid,
    cfg: &GridConfig,
    cache: &TextCache,
    font_size: f32,
    filter: Option<&GridFilter>,
) {
    for cell in &grid.cells {
        if filter.is_some_and(|f| !f.matches(&cell.label)) {
            continue;
        }
        draw_text(
            pixmap,
            &cell.label,
            cell.center.0,
            cell.center.1,
            cache,
            font_size,
            cfg.label_color,
        );
    }
}

pub fn render_subgrid(
    pixmap: &mut Pixmap,
    rect: IntRect,
    cfg: &GridConfig,
    cache: &TextCache,
    font_size: f32,
) {
    let x = rect.x() as f32;
    let y = rect.y() as f32;
    let w = rect.width() as f32;
    let h = rect.height() as f32;

    draw_focus(pixmap, x, y, w, h, 2, 4, cfg);

    let cell_w = w / 4.0;
    let gap = font_size * 1.0;
    let top_y = y - gap - font_size * 0.5;
    let bot_y = y + h + gap + font_size * 0.5;
    for col in 0..4u32 {
        let cx = x + (col as f32 + 0.5) * cell_w;
        draw_char(
            pixmap,
            SUBGRID_LABELS[0][col as usize],
            cx,
            top_y,
            cache,
            cfg.label_color,
        );
        draw_char(
            pixmap,
            SUBGRID_LABELS[1][col as usize],
            cx,
            bot_y,
            cache,
            cfg.label_color,
        );
    }
}

pub fn render_bisect(
    pixmap: &mut Pixmap,
    rect: (f32, f32, f32, f32),
    cfg: &GridConfig,
    cache: &TextCache,
    font_size: f32,
) {
    let (x, y, w, h) = rect;
    draw_focus(pixmap, x, y, w, h, 2, 2, cfg);

    let gap = font_size * 1.0;
    let top_y = y - gap - font_size * 0.5;
    let bot_y = y + h + gap + font_size * 0.5;
    let pad = 12.0;
    let positions = [
        (BISECT_LABELS[0][0], x - pad, top_y),
        (BISECT_LABELS[0][1], x + w + pad, top_y),
        (BISECT_LABELS[1][0], x - pad, bot_y),
        (BISECT_LABELS[1][1], x + w + pad, bot_y),
    ];
    for &(ch, px, py) in &positions {
        draw_char(pixmap, ch, px, py, cache, cfg.label_color);
    }
}

// ── Low-level draw ─────────────────────────────────────────────────

fn rgba(color: [u8; 4]) -> Color {
    Color::from_rgba8(color[0], color[1], color[2], color[3])
}

fn highlight_color(label: [u8; 4]) -> Color {
    Color::from_rgba8(
        (f32::from(label[0]) * 0.25) as u8,
        (f32::from(label[1]) * 0.25) as u8,
        (f32::from(label[2]) * 0.25) as u8,
        32,
    )
}

#[allow(clippy::too_many_arguments)]
fn draw_focus(
    pixmap: &mut Pixmap,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    rows: u32,
    cols: u32,
    cfg: &GridConfig,
) {
    fill_rect(pixmap, x, y, w, h, highlight_color(cfg.label_color));
    let line = rgba(cfg.line_color);
    let stroke = Stroke {
        width: cfg.line_width,
        ..Default::default()
    };
    for row in 1..rows {
        let ly = y + (row as f32 / rows as f32) * h;
        draw_line(pixmap, x, ly, x + w, ly, &line, &stroke);
    }
    for col in 1..cols {
        let lx = x + (col as f32 / cols as f32) * w;
        draw_line(pixmap, lx, y, lx, y + h, &line, &stroke);
    }
}

fn fill_rect(pixmap: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, color: Color) {
    pixmap.fill_path(
        &PathBuilder::from_rect(Rect::from_xywh(x, y, w, h).unwrap()),
        &Paint {
            shader: Shader::SolidColor(color),
            ..Default::default()
        },
        tiny_skia::FillRule::Winding,
        Transform::identity(),
        None,
    );
}

fn draw_line(
    pixmap: &mut Pixmap,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    color: &Color,
    stroke: &Stroke,
) {
    let mut pb = PathBuilder::new();
    pb.move_to(x1, y1);
    pb.line_to(x2, y2);
    let path = pb.finish().unwrap();
    let paint = Paint {
        shader: Shader::SolidColor(*color),
        anti_alias: true,
        ..Default::default()
    };
    pixmap.stroke_path(&path, &paint, stroke, Transform::identity(), None);
}

fn draw_char(pixmap: &mut Pixmap, ch: char, cx: f32, cy: f32, cache: &TextCache, rgba: [u8; 4]) {
    let Some((m, bmp)) = cache.get(ch) else {
        return;
    };
    if bmp.is_empty() {
        return;
    }
    let gx = cx + m.xmin as f32 - m.advance_width * 0.5;
    let gy = cy - m.ymin as f32 - m.height as f32 * 0.5;
    blit_glyph(pixmap, bmp, m, gx, gy, rgba);
}

fn blit_glyph(pixmap: &mut Pixmap, bmp: &[u8], m: &Metrics, gx: f32, gy: f32, rgba: [u8; 4]) {
    let pw = pixmap.width() as usize;
    let pixels = pixmap.pixels_mut();
    for row in 0..m.height {
        let off = row * m.width;
        for col in 0..m.width {
            let cov = bmp[off + col];
            if cov == 0 {
                continue;
            }
            let ix = (gx + col as f32) as i32;
            let iy = (gy + row as f32) as i32;
            if ix < 0 || iy < 0 || ix as usize >= pw {
                continue;
            }
            let i = iy as usize * pw + ix as usize;
            if i >= pixels.len() {
                continue;
            }
            blend(&mut pixels[i], cov, rgba);
        }
    }
}

pub fn render_digit(
    pixmap: &mut Pixmap,
    digit: char,
    cx: f32,
    cy: f32,
    cache: &TextCache,
    rgba: [u8; 4],
) {
    draw_char(pixmap, digit, cx, cy, cache, rgba);
}

fn draw_text(
    pixmap: &mut Pixmap,
    text: &str,
    cx: f32,
    cy: f32,
    cache: &TextCache,
    size: f32,
    rgba: [u8; 4],
) {
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return;
    }
    let space = size * 0.12;

    let mut entries: Vec<&(Metrics, Vec<u8>)> = Vec::with_capacity(chars.len());
    let mut total_w = 0.0_f32;
    for &ch in &chars {
        let Some(g) = cache.get(ch) else {
            return;
        };
        total_w += g.0.advance_width;
        entries.push(g);
    }
    total_w += space * (chars.len() - 1) as f32;

    let mut pen = cx - total_w * 0.5;

    for &(m, bmp) in &entries {
        if bmp.is_empty() {
            pen += m.advance_width + space;
            continue;
        }
        let gx = pen + m.xmin as f32;
        let gy = cy - m.ymin as f32 - m.height as f32 * 0.5;
        blit_glyph(pixmap, bmp, m, gx, gy, rgba);
        pen += m.advance_width + space;
    }
}

fn blend(dst: &mut PremultipliedColorU8, coverage: u8, rgba: [u8; 4]) {
    let a = (u16::from(coverage) * u16::from(rgba[3])) / 255;
    let inv = 255 - a;
    let r = (u16::from(rgba[0]) * a + u16::from(dst.red()) * inv) / 255;
    let g = (u16::from(rgba[1]) * a + u16::from(dst.green()) * inv) / 255;
    let b = (u16::from(rgba[2]) * a + u16::from(dst.blue()) * inv) / 255;
    let alpha = (255 * a + u16::from(dst.alpha()) * inv) / 255;
    *dst = PremultipliedColorU8::from_rgba(r as u8, g as u8, b as u8, alpha as u8).unwrap();
}
