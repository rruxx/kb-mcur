use std::collections::HashMap;

use fontdue::{Font, Metrics};
use tiny_skia::{Color, Paint, PathBuilder, Pixmap, PremultipliedColorU8, Rect, Shader, Stroke, Transform};

use crate::grid::{Cell, Grid, GridConfig, GridFilter, SUBGRID_LABELS};

// ── Text cache ─────────────────────────────────────────────────────

/// Pre-rasterised lowercase letters so labels never hit `fontdue` at runtime.
pub struct TextCache {
    glyphs: HashMap<char, (Metrics, Vec<u8>)>,
}

impl TextCache {
    pub fn new(font: &Font, size: f32) -> Self {
        let mut glyphs = HashMap::new();
        for ch in ('a'..='z').chain([';']) {
            glyphs.insert(ch, font.rasterize(ch, size));
        }
        Self { glyphs }
    }

    fn get(&self, ch: char) -> Option<&(Metrics, Vec<u8>)> {
        self.glyphs.get(&ch)
    }
}

// ── Base layer (background + grid lines, no labels) ─────────────────

pub fn render_base(pixmap: &mut Pixmap, grid: &Grid, cfg: &GridConfig) {
    let w = pixmap.width() as f32;
    let h = pixmap.height() as f32;

    let bg = rgba_color(cfg.bg_color);
    pixmap.fill_path(
        &PathBuilder::from_rect(Rect::from_xywh(0.0, 0.0, w, h).unwrap()),
        &Paint { shader: Shader::SolidColor(bg), ..Default::default() },
        tiny_skia::FillRule::Winding,
        Transform::identity(),
        None,
    );

    let line_c = rgba_color(cfg.line_color);
    let paint = Paint { shader: Shader::SolidColor(line_c), anti_alias: true, ..Default::default() };
    let stroke = Stroke { width: cfg.line_width, ..Default::default() };
    for row in 1..grid.rows {
        let y = (row as f32 / grid.rows as f32) * h;
        stroke_line(pixmap, 0.0, y, w, y, &paint, &stroke);
    }
    for col in 1..grid.cols {
        let x = (col as f32 / grid.cols as f32) * w;
        stroke_line(pixmap, x, 0.0, x, h, &paint, &stroke);
    }
}

// ── Labels (cached glyphs) ─────────────────────────────────────────

pub fn render_labels(
    pixmap: &mut Pixmap,
    grid: &Grid,
    cfg: &GridConfig,
    cache: &TextCache,
    font_size: f32,
    filter: Option<&GridFilter>,
) {
    let label_c = rgba_color(cfg.label_color);
    for cell in &grid.cells {
        if filter.is_some_and(|f| !f.matches(&cell.label)) {
            continue;
        }
        draw_text(pixmap, &cell.label, cell.center.0, cell.center.1, cache, font_size, &label_c);
    }
}

// ── Region focus (generic — used by sub-grid and bisect levels) ─────

/// Highlight a region, draw grid lines inside, and labels outside.
/// `labels` is row-major: labels[r][c].
fn render_region(
    pixmap: &mut Pixmap,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    rows: u32,
    cols: u32,
    labels: &[&[char]],
    cfg: &GridConfig,
    cache: &TextCache,
    font_size: f32,
) {
    // Light fill
    let hl = rgba_color(cfg.label_color);
    let hl_paint = Paint {
        shader: Shader::SolidColor(Color::from_rgba8(
            (hl.red() * 64.0) as u8,
            (hl.green() * 64.0) as u8,
            (hl.blue() * 64.0) as u8,
            32,
        )),
        ..Default::default()
    };
    pixmap.fill_path(
        &PathBuilder::from_rect(Rect::from_xywh(x, y, w, h).unwrap()),
        &hl_paint,
        tiny_skia::FillRule::Winding,
        Transform::identity(),
        None,
    );

    // Grid lines
    let line_c = rgba_color(cfg.line_color);
    let paint = Paint { shader: Shader::SolidColor(line_c), anti_alias: true, ..Default::default() };
    let stroke = Stroke { width: cfg.line_width, ..Default::default() };
    for row in 1..rows {
        let ly = y + (row as f32 / rows as f32) * h;
        stroke_line(pixmap, x, ly, x + w, ly, &paint, &stroke);
    }
    for col in 1..cols {
        let lx = x + (col as f32 / cols as f32) * w;
        stroke_line(pixmap, lx, y, lx, y + h, &paint, &stroke);
    }

    // Labels outside
    let label_c = rgba_color(cfg.label_color);
    let gap = font_size * 1.0;
    let cell_w = w / cols as f32;
    let top_y = y - gap - font_size * 0.5;
    let bot_y = y + h + gap + font_size * 0.5;

    for col in 0..cols {
        let cx = x + (col as f32 + 0.5) * cell_w;
        if let Some(ch) = labels.get(0).and_then(|r| r.get(col as usize)) {
            draw_text(pixmap, &ch.to_string(), cx, top_y, cache, font_size, &label_c);
        }
        if let Some(ch) = labels.get(1).and_then(|r| r.get(col as usize)) {
            draw_text(pixmap, &ch.to_string(), cx, bot_y, cache, font_size, &label_c);
        }
    }
}

// ── Level-3 sub-grid (4×2 within parent cell) ──────────────────────

pub fn render_subgrid(
    pixmap: &mut Pixmap,
    parent: &Cell,
    cfg: &GridConfig,
    cache: &TextCache,
    font_size: f32,
) {
    let top: &[char] = &SUBGRID_LABELS[0];
    let bot: &[char] = &SUBGRID_LABELS[1];
    render_region(
        pixmap,
        parent.rect.x() as f32,
        parent.rect.y() as f32,
        parent.rect.width() as f32,
        parent.rect.height() as f32,
        2,
        4,
        &[top, bot],
        cfg,
        cache,
        font_size,
    );
}

// ── Levels 4-7  quadrant bisection (2×2, labels at corners) ─────────

pub fn render_bisect(
    pixmap: &mut Pixmap,
    rect: (f32, f32, f32, f32), // x, y, w, h
    cfg: &GridConfig,
    cache: &TextCache,
    font_size: f32,
) {
    let (x, y, w, h) = rect;
    let hw = w * 0.5;
    let hh = h * 0.5;
    let gap = font_size * 1.0;

    // highlight
    let hl = rgba_color(cfg.label_color);
    let hl_paint = Paint {
        shader: Shader::SolidColor(Color::from_rgba8(
            (hl.red() * 64.0) as u8,
            (hl.green() * 64.0) as u8,
            (hl.blue() * 64.0) as u8,
            32,
        )),
        ..Default::default()
    };
    pixmap.fill_path(
        &PathBuilder::from_rect(Rect::from_xywh(x, y, w, h).unwrap()),
        &hl_paint,
        tiny_skia::FillRule::Winding,
        Transform::identity(),
        None,
    );

    // grid lines
    let line_c = rgba_color(cfg.line_color);
    let paint = Paint { shader: Shader::SolidColor(line_c), anti_alias: true, ..Default::default() };
    let stroke = Stroke { width: cfg.line_width, ..Default::default() };
    stroke_line(pixmap, x, y + hh, x + w, y + hh, &paint, &stroke);
    stroke_line(pixmap, x + hw, y, x + hw, y + h, &paint, &stroke);

    // labels at corners
    let label_c = rgba_color(cfg.label_color);
    let top_y = y - gap - font_size * 0.5;
    let bot_y = y + h + gap + font_size * 0.5;
    let pad = 12.0;

    draw_text(pixmap, "e", x - pad, top_y, cache, font_size, &label_c);
    draw_text(pixmap, "r", x + w + pad, top_y, cache, font_size, &label_c);
    draw_text(pixmap, "d", x - pad, bot_y, cache, font_size, &label_c);
    draw_text(pixmap, "f", x + w + pad, bot_y, cache, font_size, &label_c);
}

// ── Helpers ────────────────────────────────────────────────────────

fn rgba_color(rgba: [u8; 4]) -> Color {
    Color::from_rgba8(rgba[0], rgba[1], rgba[2], rgba[3])
}

fn stroke_line(pixmap: &mut Pixmap, x1: f32, y1: f32, x2: f32, y2: f32, paint: &Paint, stroke: &Stroke) {
    let mut pb = PathBuilder::new();
    pb.move_to(x1, y1);
    pb.line_to(x2, y2);
    pixmap.stroke_path(&pb.finish().unwrap(), paint, stroke, Transform::identity(), None);
}

fn draw_text(
    pixmap: &mut Pixmap,
    text: &str,
    cx: f32,
    cy: f32,
    cache: &TextCache,
    size: f32,
    color: &Color,
) {
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return;
    }

    let space = size * 0.12;

    // Collect cached glyphs, compute total advance width for horizontal centring.
    let mut entries: Vec<&(Metrics, Vec<u8>)> = Vec::with_capacity(chars.len());
    let mut total_w = 0.0_f32;
    for &ch in &chars {
        let Some(g) = cache.get(ch) else {
            return;
        };
        total_w += g.0.advance_width;
        entries.push(g);
    }
    total_w += space * (chars.len().saturating_sub(1)) as f32;

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
            let row_off = row * m.width;
            for col in 0..m.width {
                let cov = bmp[row_off + col] as f32 / 255.0;
                if cov <= 0.0 {
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
                blend(&mut pixels[i], cov, color);
            }
        }
        pen += m.advance_width + space;
    }
}

fn blend(dst: &mut PremultipliedColorU8, coverage: f32, color: &Color) {
    let src_a = coverage * color.alpha();
    let src_r = color.red() * src_a;
    let src_g = color.green() * src_a;
    let src_b = color.blue() * src_a;

    let inv = 1.0 - src_a;
    *dst = PremultipliedColorU8::from_rgba(
        ((src_r + dst.red() as f32 / 255.0 * inv) * 255.0).round() as u8,
        ((src_g + dst.green() as f32 / 255.0 * inv) * 255.0).round() as u8,
        ((src_b + dst.blue() as f32 / 255.0 * inv) * 255.0).round() as u8,
        ((src_a + dst.alpha() as f32 / 255.0 * inv) * 255.0).round() as u8,
    )
    .unwrap();
}
