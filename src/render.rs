// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashMap;

use fontdue::{Font, Metrics};
use tiny_skia::{
    Color, Paint, PathBuilder, Pixmap, PremultipliedColorU8, Rect, Shader, Stroke, Transform,
};

pub struct TextCache {
    glyphs: HashMap<char, (Metrics, Vec<u8>)>,
}

impl TextCache {
    #[must_use]
    pub fn new(font: &Font, size: f32) -> Self {
        let ss = crate::config::FONT_SUPERSAMPLE as f32;
        let mut glyphs = HashMap::new();
        for ch in 'a'..='z' {
            glyphs.insert(ch, supersample_glyph(font, ch, size, ss));
        }
        for ch in ['0', '1', '2', '3', '4', '5', '6', '7', '8', '9', ',', '.'] {
            glyphs.insert(ch, supersample_glyph(font, ch, size, ss));
        }
        Self { glyphs }
    }

    #[must_use]
    pub fn get(&self, ch: char) -> Option<&(Metrics, Vec<u8>)> {
        self.glyphs.get(&ch)
    }
}

/// Rasterize `ch` at `size × ss`, normalizing the metrics back to 1× (`xmin`/
/// `ymin`/advance ÷ ss) while keeping the ss× bitmap for `blit_glyph`.
fn supersample_glyph(font: &Font, ch: char, size: f32, ss: f32) -> (Metrics, Vec<u8>) {
    let (m, bmp) = font.rasterize(ch, size * ss);
    let m = Metrics {
        advance_width: m.advance_width / ss,
        advance_height: m.advance_height / ss,
        xmin: (m.xmin as f32 / ss).round() as i32,
        ymin: (m.ymin as f32 / ss).round() as i32,
        width: m.width,
        height: m.height,
        bounds: m.bounds,
    };
    (m, bmp)
}

// ── Glyph metrics & drawing ─────────────────────────────────────────

#[must_use]
pub fn char_width(cache: &TextCache, ch: char) -> f32 {
    cache.get(ch).map_or(0.0, |(m, _)| m.advance_width)
}

pub fn draw_char_glyph(
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
    let ss = crate::config::FONT_SUPERSAMPLE as f32;
    let gx = cx + m.xmin as f32 - m.advance_width * 0.5;
    let gy = cy - m.ymin as f32 - m.height as f32 / ss * 0.5;
    blit_glyph(pixmap, bmp, m, gx, gy, rgba);
}

// ── Low-level draw ─────────────────────────────────────────────────

#[must_use]
pub fn rgba(color: [u8; 4]) -> Color {
    Color::from_rgba8(color[0], color[1], color[2], color[3])
}

pub fn fill_rect(pixmap: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, color: Color) {
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

pub fn draw_line(
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
    let ss = crate::config::FONT_SUPERSAMPLE as usize;
    let pw = pixmap.width() as usize;
    let pixels = pixmap.pixels_mut();
    // The bitmap is ss×; each target pixel averages the ss×ss source block.
    let mut row = 0usize;
    while row < m.height {
        let mut col = 0usize;
        while col < m.width {
            let mut sum = 0u32;
            let mut cnt = 0u32;
            for dy in 0..ss {
                let r = row + dy;
                if r >= m.height {
                    break;
                }
                let off = r * m.width;
                for dx in 0..ss {
                    let c = col + dx;
                    if c >= m.width {
                        break;
                    }
                    sum += u32::from(bmp[off + c]);
                    cnt += 1;
                }
            }
            let cov = (sum / cnt.max(1)) as u8;
            if cov != 0 {
                let ix = (gx + col as f32 / ss as f32) as i32;
                let iy = (gy + row as f32 / ss as f32) as i32;
                if ix >= 0 && iy >= 0 && (ix as usize) < pw {
                    let i = iy as usize * pw + ix as usize;
                    if i < pixels.len() {
                        blend(&mut pixels[i], cov, rgba);
                    }
                }
            }
            col += ss;
        }
        row += ss;
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
    let ss = crate::config::FONT_SUPERSAMPLE as f32;
    let space = size * crate::config::CHAR_SPACE_RATIO;

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
        let gy = cy - m.ymin as f32 - m.height as f32 / ss * 0.5;
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
