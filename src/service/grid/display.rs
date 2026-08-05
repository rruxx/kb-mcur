// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::Result;

use super::state::{DrawState, font};
use super::{GridConfig, GridFilter};
use crate::config::{L3_KEYS, l1_key_pos};
use crate::overlay::Overlay;
use crate::render::TextCache;

// ── Display update ──────────────────────────────────────────────────

pub(crate) fn display_update(
    overlay: &Overlay,
    states: &mut [DrawState],
    cfg: &GridConfig,
    cache: &TextCache,
    font_size: f32,
    filter: &GridFilter,
    l4_offset: Option<(i32, i32)>,
) -> Result<()> {
    let l2_rect = if filter.is_empty() {
        None
    } else {
        filter
            .input()
            .chars()
            .next()
            .and_then(l1_key_pos)
            .map(|(r, c)| {
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
        filter
            .input()
            .chars()
            .nth(2)
            .and_then(crate::config::l3_key_pos)
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
            render_l3_overlay(
                &mut ds.pixmap,
                (x, y, w, h),
                cfg,
                font_size * 0.75,
                l3_sel,
                &mut ds.l3_cache,
            );
            if let Some((dx, dy)) = l4_offset
                && let Some((r, c)) = l3_sel
                && (dx != 0 || dy != 0)
            {
                let sub_w = w / 5.0;
                let sub_h = h / 3.0;
                let cx = x + (c as f32 + 0.5) * sub_w + dx as f32 * sub_w / 7.0;
                let cy = y + (r as f32 + 0.5) * sub_h + dy as f32 * sub_h / 7.0;
                let r = font_size * 0.15;
                render_l4_dot(&mut ds.pixmap, (cx, cy, r));
            }
        }
        overlay.upload(idx, &ds.pixmap)?;
    }

    overlay.show_all()?;
    overlay.redraw_all()?;
    Ok(())
}

fn render_l4_dot(pixmap: &mut tiny_skia::Pixmap, (cx, cy, r): (f32, f32, f32)) {
    use tiny_skia::{Color, Paint, PathBuilder, Shader, Transform};
    let c = Color::from_rgba8(0, 255, 0, 200);
    let mut pb = PathBuilder::new();
    pb.push_circle(cx, cy, r);
    pixmap.fill_path(
        &pb.finish().unwrap(),
        &Paint {
            shader: Shader::SolidColor(c),
            anti_alias: true,
            ..Default::default()
        },
        tiny_skia::FillRule::Winding,
        Transform::identity(),
        None,
    );
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
    for col in 1..3 {
        let lx = x + col as f32 * w / 3.0;
        let mut pb = PathBuilder::new();
        pb.move_to(lx, y);
        pb.line_to(lx, y + h);
        pixmap.stroke_path(
            &pb.finish().unwrap(),
            &paint,
            &stroke,
            Transform::identity(),
            None,
        );
    }
    for row in 1..9 {
        let ly = y + row as f32 * h / 9.0;
        let mut pb = PathBuilder::new();
        pb.move_to(x, ly);
        pb.line_to(x + w, ly);
        pixmap.stroke_path(
            &pb.finish().unwrap(),
            &paint,
            &stroke,
            Transform::identity(),
            None,
        );
    }
}

fn render_l3_overlay(
    pixmap: &mut tiny_skia::Pixmap,
    rect: (f32, f32, f32, f32),
    cfg: &GridConfig,
    font_size: f32,
    sel: Option<(usize, usize)>,
    l3_cache: &mut Option<TextCache>,
) {
    use tiny_skia::{Color, Paint, PathBuilder, Shader, Stroke, Transform};

    let (x, y, w, h) = rect;

    let cache = l3_cache.get_or_insert_with(|| TextCache::new(font(), font_size));

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
                cache,
                font_size,
                label_color,
            );
        }
    }

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
