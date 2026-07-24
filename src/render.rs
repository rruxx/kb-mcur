use fontdue::Font;
use tiny_skia::{Color, Paint, PathBuilder, Pixmap, PremultipliedColorU8, Shader, Stroke, Transform};

use crate::grid::{Grid, GridConfig};

/// Render the grid onto a tiny-skia pixmap (RGBA8888).
pub fn render_grid(pixmap: &mut Pixmap, grid: &Grid, config: &GridConfig, font: &Font, font_size: f32) {
    let w = pixmap.width() as f32;
    let h = pixmap.height() as f32;

    fill_background(pixmap, w, h, config);

    let line_color = rgba_to_color(&config.line_color);
    draw_grid_lines(pixmap, w, h, grid, &line_color, config.line_width);

    let label_color = rgba_to_color(&config.label_color);
    for cell in &grid.cells {
        draw_label(pixmap, &cell.label, cell.center.0, cell.center.1, font, font_size, &label_color);
    }
}

fn rgba_to_color(rgba: &[u8; 4]) -> Color {
    Color::from_rgba8(rgba[0], rgba[1], rgba[2], rgba[3])
}

fn fill_background(pixmap: &mut Pixmap, w: f32, h: f32, config: &GridConfig) {
    let bg = rgba_to_color(&config.bg_color);
    let rect = tiny_skia::Rect::from_xywh(0.0, 0.0, w, h).unwrap();
    let path = PathBuilder::from_rect(rect);
    let paint = Paint {
        shader: Shader::SolidColor(bg),
        anti_alias: false,
        ..Default::default()
    };
    pixmap.fill_path(&path, &paint, tiny_skia::FillRule::Winding, Transform::identity(), None);
}

fn draw_grid_lines(pixmap: &mut Pixmap, w: f32, h: f32, grid: &Grid, color: &Color, line_width: f32) {
    let paint = Paint {
        shader: Shader::SolidColor(*color),
        anti_alias: true,
        ..Default::default()
    };

    let stroke = Stroke {
        width: line_width,
        ..Default::default()
    };

    for row in 1..grid.rows {
        let y = (row as f32 / grid.rows as f32) * h;
        let mut pb = PathBuilder::new();
        pb.move_to(0.0, y);
        pb.line_to(w, y);
        pixmap.stroke_path(&pb.finish().unwrap(), &paint, &stroke, Transform::identity(), None);
    }

    for col in 1..grid.cols {
        let x = (col as f32 / grid.cols as f32) * w;
        let mut pb = PathBuilder::new();
        pb.move_to(x, 0.0);
        pb.line_to(x, h);
        pixmap.stroke_path(&pb.finish().unwrap(), &paint, &stroke, Transform::identity(), None);
    }
}

/// Draw a multi-character label precisely centred at (cx, cy).
fn draw_label(pixmap: &mut Pixmap, text: &str, cx: f32, cy: f32, font: &Font, size: f32, color: &Color) {
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return;
    }

    let char_spacing = size * 0.12;

    // Pre-rasterise every character.
    struct Glyph {
        metrics: fontdue::Metrics,
        bitmap: Vec<u8>,
    }
    let mut glyphs: Vec<Glyph> = Vec::with_capacity(chars.len());
    for &ch in &chars {
        let (metrics, bitmap) = font.rasterize(ch, size);
        glyphs.push(Glyph { metrics, bitmap });
    }

    // Visual bounding box of the whole label (relative to the first glyph's
    // origin, NOT the pen position).
    let first = &glyphs.first().unwrap().metrics;
    let mut vis_left = first.xmin as f32;
    let mut vis_right = first.xmin as f32 + first.width as f32;
    let mut vis_top = first.ymin as f32;
    let mut vis_bottom = first.ymin as f32 + first.height as f32;

    let mut pen = 0.0_f32;
    for g in &glyphs {
        let left = pen + g.metrics.xmin as f32;
        let right = left + g.metrics.width as f32;
        let top = g.metrics.ymin as f32;
        let bottom = top + g.metrics.height as f32;
        vis_left = vis_left.min(left);
        vis_right = vis_right.max(right);
        vis_top = vis_top.min(top);
        vis_bottom = vis_bottom.max(bottom);
        pen += g.metrics.advance_width + char_spacing;
    }

    let label_mid_x = (vis_left + vis_right) * 0.5;
    let label_mid_y = (vis_top + vis_bottom) * 0.5;
    let base_x = cx - label_mid_x;
    let base_y = cy - label_mid_y;

    let fg_a = color.alpha();
    let pw = pixmap.width() as usize;
    let pixels = pixmap.pixels_mut();

    let mut pen_x = 0.0_f32;
    for g in &glyphs {
        let m = &g.metrics;
        let bmp = &g.bitmap;
        if bmp.is_empty() {
            pen_x += m.advance_width + char_spacing;
            continue;
        }

        let gx = base_x + pen_x + m.xmin as f32;
        let gy = base_y + m.ymin as f32;

        for row in 0..m.height {
            for col in 0..m.width {
                let coverage = bmp[row * m.width + col] as f32 / 255.0;
                if coverage < 1.0 / 255.0 {
                    continue;
                }

                let ix = (gx + col as f32) as i32;
                let iy = (gy + row as f32) as i32;
                if ix < 0 || iy < 0 || ix as usize >= pw {
                    continue;
                }
                let idx = iy as usize * pw + ix as usize;
                if idx >= pixels.len() {
                    continue;
                }
                let dst = &mut pixels[idx];

                let src_a = coverage * fg_a;
                let src_r = color.red() * src_a;
                let src_g = color.green() * src_a;
                let src_b = color.blue() * src_a;

                let dst_r = dst.red() as f32 / 255.0;
                let dst_g = dst.green() as f32 / 255.0;
                let dst_b = dst.blue() as f32 / 255.0;
                let dst_a = dst.alpha() as f32 / 255.0;

                let inv_alpha = 1.0 - src_a;
                let r = src_r + dst_r * inv_alpha;
                let g = src_g + dst_g * inv_alpha;
                let b = src_b + dst_b * inv_alpha;
                let a = src_a + dst_a * inv_alpha;

                *dst = PremultipliedColorU8::from_rgba(
                    (r * 255.0).round() as u8,
                    (g * 255.0).round() as u8,
                    (b * 255.0).round() as u8,
                    (a * 255.0).round() as u8,
                )
                .unwrap();
            }
        }
        pen_x += m.advance_width + char_spacing;
    }
}
