// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::{Context, Result};
use tiny_skia::{Color, Paint, PathBuilder, Pixmap, PremultipliedColorU8, Shader, Transform};

use super::init::connect_as_user;
use crate::font;
use crate::overlay::{Monitor, Overlay};
use crate::render::{TextCache, draw_text};

// ── Multi-monitor selection ──────────────────────────────────────────

pub fn show_selection(overlay: &mut Option<Overlay>, monitors: &[Monitor]) -> Result<()> {
    let (bbox_x, bbox_y, bbox_w, bbox_h) = Monitor::bbox(monitors);

    let mut new_overlay = connect_as_user()?;
    new_overlay.add_window(bbox_x, bbox_y, bbox_w, bbox_h)?;
    new_overlay.show_all()?;
    redraw_select_hint(&mut new_overlay, monitors, "")?;
    *overlay = Some(new_overlay);
    Ok(())
}

pub(crate) fn redraw_select_hint(
    overlay: &mut Overlay,
    monitors: &[Monitor],
    hint: &str,
) -> Result<()> {
    let (bbox_x, bbox_y, bbox_w, bbox_h) = Monitor::bbox(monitors);

    let mut pixmap = Pixmap::new(bbox_w as u32, bbox_h as u32).context("pixmap")?;
    pixmap
        .pixels_mut()
        .fill(PremultipliedColorU8::from_rgba(0, 0, 0, 0).unwrap());

    let font_size = 128.0;
    let cache = TextCache::new(font::font(), font_size);

    let bg = Color::from_rgba8(0, 0, 0, 144);
    let paint = Paint {
        shader: Shader::SolidColor(bg),
        anti_alias: true,
        ..Default::default()
    };
    let pw = font_size * 1.8;
    let ph = font_size * 1.8;

    for (i, m) in monitors.iter().enumerate() {
        let label = format!("{}", (b'a' + i as u8) as char);
        if !hint.is_empty() && !label.starts_with(hint) {
            continue;
        }
        let cx = (m.x - bbox_x) as f32 + m.w as f32 * 0.5;
        let cy = (m.y - bbox_y) as f32 + m.h as f32 * 0.5;
        let x = cx - pw * 0.5;
        let y = cy - ph * 0.5;

        let mut pb = PathBuilder::new();
        pb.push_oval(tiny_skia::Rect::from_xywh(x, y, pw, ph).unwrap());
        let oval = pb.finish().unwrap();
        pixmap.fill_path(
            &oval,
            &paint,
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );

        draw_text(
            &mut pixmap,
            &label,
            cx,
            cy,
            &cache,
            font_size,
            [192, 255, 192, 192],
        );
    }

    overlay.upload(0, &pixmap)?;
    overlay.redraw_all()?;
    Ok(())
}
