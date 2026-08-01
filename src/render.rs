// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashMap;

use fontdue::{Font, Metrics};
use tiny_skia::{
    Color, Paint, PathBuilder, Pixmap, PremultipliedColorU8, Rect, Shader, Stroke, Transform,
};

use crate::grid::{Grid, GridConfig, GridFilter};

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
        for ch in ['0', '1', '2', '3', '4', '5', '6', '7', '8', '9', ',', '.'] {
            glyphs.insert(ch, font.rasterize(ch, size));
        }
        Self { glyphs }
    }

    pub(crate) fn get(&self, ch: char) -> Option<&(Metrics, Vec<u8>)> {
        self.glyphs.get(&ch)
    }
}

// ── Base layer (background + grid lines) ────────────────────────────

/// Fill pixmap with `BG_COLOR`, return the premultiplied pixel value.
pub fn render_bg(pixmap: &mut Pixmap, cfg: &GridConfig) -> PremultipliedColorU8 {
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
pub fn render_l1(pixmap: &mut Pixmap, cfg: &GridConfig) {
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

pub fn render_labels(
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

fn char_width(cache: &TextCache, ch: char) -> f32 {
    cache.get(ch).map_or(0.0, |(m, _)| m.advance_width)
}

fn draw_char_glyph(
    pixmap: &mut Pixmap,
    ch: char,
    cx: f32,
    cy: f32,
    cache: &TextCache,
    rgba: [u8; 4],
) {
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

// ── Low-level draw ─────────────────────────────────────────────────

fn rgba(color: [u8; 4]) -> Color {
    Color::from_rgba8(color[0], color[1], color[2], color[3])
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

pub fn draw_text(
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
