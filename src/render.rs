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
    pub fn new(font: &Font, size: f32) -> Self {
        let mut glyphs = HashMap::new();
        for ch in 'a'..='z' {
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
    pixmap.pixels_mut().fill(PremultipliedColorU8::from_rgba(0, 0, 0, 0).unwrap());
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
        draw_text(
            pixmap,
            &SUBGRID_LABELS[0][col as usize].to_string(),
            cx,
            top_y,
            cache,
            font_size,
            cfg.label_color,
        );
        draw_text(
            pixmap,
            &SUBGRID_LABELS[1][col as usize].to_string(),
            cx,
            bot_y,
            cache,
            font_size,
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

    draw_text(
        pixmap,
        &BISECT_LABELS[0][0].to_string(),
        x - pad,
        top_y,
        cache,
        font_size,
        cfg.label_color,
    );
    draw_text(
        pixmap,
        &BISECT_LABELS[0][1].to_string(),
        x + w + pad,
        top_y,
        cache,
        font_size,
        cfg.label_color,
    );
    draw_text(
        pixmap,
        &BISECT_LABELS[1][0].to_string(),
        x - pad,
        bot_y,
        cache,
        font_size,
        cfg.label_color,
    );
    draw_text(
        pixmap,
        &BISECT_LABELS[1][1].to_string(),
        x + w + pad,
        bot_y,
        cache,
        font_size,
        cfg.label_color,
    );
}

// ── Low-level draw ─────────────────────────────────────────────────

fn rgba(rgb: [u8; 4]) -> Color {
    Color::from_rgba8(rgb[0], rgb[1], rgb[2], rgb[3])
}

fn highlight_color(label: [u8; 4]) -> Color {
    Color::from_rgba8(
        (label[0] as f32 * 0.25) as u8,
        (label[1] as f32 * 0.25) as u8,
        (label[2] as f32 * 0.25) as u8,
        32,
    )
}

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

pub fn render_digit(
    pixmap: &mut Pixmap,
    digit: char,
    cx: f32,
    cy: f32,
    cache: &TextCache,
    font_size: f32,
    rgba: [u8; 4],
) {
    draw_text(pixmap, &digit.to_string(), cx, cy, cache, font_size, rgba);
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

    let pw = pixmap.width() as usize;
    let pixels = pixmap.pixels_mut();
    let mut pen = cx - total_w * 0.5;

    for &(m, bmp) in entries.iter() {
        if bmp.is_empty() {
            pen += m.advance_width + space;
            continue;
        }
        let gx = pen + m.xmin as f32;
        let gy = cy - m.ymin as f32 - m.height as f32 * 0.5;
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
        pen += m.advance_width + space;
    }
}

fn blend(dst: &mut PremultipliedColorU8, coverage: u8, rgba: [u8; 4]) {
    let a = (coverage as u16 * rgba[3] as u16) / 255;
    let inv = 255 - a;
    let r = (rgba[0] as u16 * a + dst.red() as u16 * inv) / 255;
    let g = (rgba[1] as u16 * a + dst.green() as u16 * inv) / 255;
    let b = (rgba[2] as u16 * a + dst.blue() as u16 * inv) / 255;
    let alpha = (255 * a + dst.alpha() as u16 * inv) / 255;
    *dst = PremultipliedColorU8::from_rgba(r as u8, g as u8, b as u8, alpha as u8).unwrap();
}
