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
        draw_label(pixmap, cell.label, cell.center.0, cell.center.1, font, font_size, &label_color);
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

    // horizontal lines
    for row in 1..grid.rows {
        let y = (row as f32 / grid.rows as f32) * h;
        let mut pb = PathBuilder::new();
        pb.move_to(0.0, y);
        pb.line_to(w, y);
        pixmap.stroke_path(&pb.finish().unwrap(), &paint, &stroke, Transform::identity(), None);
    }

    // vertical lines
    for col in 1..grid.cols {
        let x = (col as f32 / grid.cols as f32) * w;
        let mut pb = PathBuilder::new();
        pb.move_to(x, 0.0);
        pb.line_to(x, h);
        pixmap.stroke_path(&pb.finish().unwrap(), &paint, &stroke, Transform::identity(), None);
    }
}

/// Draw a single character label centred at (cx, cy).
fn draw_label(pixmap: &mut Pixmap, ch: char, cx: f32, cy: f32, font: &Font, size: f32, color: &Color) {
    let (metrics, bitmap) = font.rasterize(ch, size);
    if bitmap.is_empty() {
        return;
    }

    let glyph_w = metrics.width as f32;
    let glyph_h = metrics.height as f32;
    let px = cx - glyph_w / 2.0;
    let py = cy - glyph_h / 2.0;

    let fg_a = color.alpha();
    let pw = pixmap.width() as usize;
    let pixels = pixmap.pixels_mut();

    for row in 0..metrics.height {
        for col in 0..metrics.width {
            let coverage = bitmap[row * metrics.width + col] as f32 / 255.0;
            if coverage < 1.0 / 255.0 {
                continue;
            }

            let x = (px + col as f32 + metrics.xmin as f32) as i32;
            let y = (py + row as f32 + metrics.ymin as f32) as i32;
            if x < 0 || y < 0 || x as usize >= pw || (y as usize) * pw + (x as usize) >= pixels.len() {
                continue;
            }

            let dst = &mut pixels[y as usize * pw + x as usize];

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
}
