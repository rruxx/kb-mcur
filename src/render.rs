use fontdue::{Font, Metrics};
use tiny_skia::{Color, Paint, PathBuilder, Pixmap, PremultipliedColorU8, Rect, Shader, Stroke, Transform};

use crate::grid::{Grid, GridConfig, GridFilter};

pub fn render_grid(
    pixmap: &mut Pixmap,
    grid: &Grid,
    cfg: &GridConfig,
    font: &Font,
    font_size: f32,
    filter: Option<&GridFilter>,
) {
    let w = pixmap.width() as f32;
    let h = pixmap.height() as f32;

    // background
    let bg = rgba_color(cfg.bg_color);
    pixmap.fill_path(
        &PathBuilder::from_rect(Rect::from_xywh(0.0, 0.0, w, h).unwrap()),
        &Paint { shader: Shader::SolidColor(bg), ..Default::default() },
        tiny_skia::FillRule::Winding,
        Transform::identity(),
        None,
    );

    // grid lines
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

    // labels — only for cells matching the current filter
    let label_c = rgba_color(cfg.label_color);
    for cell in &grid.cells {
        if filter.is_some_and(|f| !f.matches(&cell.label)) {
            continue;
        }
        draw_label(pixmap, &cell.label, cell.center.0, cell.center.1, font, font_size, &label_c);
    }
}

fn rgba_color(rgba: [u8; 4]) -> Color {
    Color::from_rgba8(rgba[0], rgba[1], rgba[2], rgba[3])
}

fn stroke_line(pixmap: &mut Pixmap, x1: f32, y1: f32, x2: f32, y2: f32, paint: &Paint, stroke: &Stroke) {
    let mut pb = PathBuilder::new();
    pb.move_to(x1, y1);
    pb.line_to(x2, y2);
    pixmap.stroke_path(&pb.finish().unwrap(), paint, stroke, Transform::identity(), None);
}

fn draw_label(
    pixmap: &mut Pixmap,
    text: &str,
    cx: f32,
    cy: f32,
    font: &Font,
    size: f32,
    color: &Color,
) {
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return;
    }

    let space = size * 0.12;

    // Rasterise every glyph, compute total advance width for horizontal centring.
    let mut entries: Vec<(Metrics, Vec<u8>)> = Vec::with_capacity(chars.len());
    let mut total_w = 0.0_f32;
    for &ch in &chars {
        let (m, bmp) = font.rasterize(ch, size);
        total_w += m.advance_width;
        entries.push((m, bmp));
    }
    total_w += space * (chars.len().saturating_sub(1)) as f32;

    let pw = pixmap.width() as usize;
    let pixels = pixmap.pixels_mut();
    let mut pen = cx - total_w * 0.5;

    for (m, bmp) in entries {
        if bmp.is_empty() {
            pen += m.advance_width + space;
            continue;
        }

        // Place glyph so its visual centre lands at (·, cy).
        let gx = pen + m.xmin as f32;
        let gy = cy - m.ymin as f32 - m.height as f32 * 0.5;

        for row in 0..m.height {
            for col in 0..m.width {
                let cov = bmp[row * m.width + col] as f32 / 255.0;
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

/// Premultiplied alpha blend of a single-colour source onto a pixel.
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
