// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::Result;
use log::info;

use super::GridFilter;
use super::state::{DrawState, GridCtx};
use crate::config::{MouseButton, l3_key_pos};
use crate::device::Mouse;
use crate::device::pointer::Pointer;

// ── Cursor & button actions ────────────────────────────────────────

pub fn cursor_warp(
    mouse: &mut Option<Mouse>,
    filter: &GridFilter,
    states: &[DrawState],
    ctx: &GridCtx,
) -> Result<()> {
    let Some(m) = mouse else {
        return Ok(());
    };
    if let Some((cx, cy)) = region_center(filter, states, ctx) {
        m.warp(cx as i32, cy as i32)?;
        info!(
            "=> {} ({cx:.0}, {cy:.0})",
            states.first().map_or("?", |ds| ds.name.as_str())
        );
    }
    Ok(())
}

pub fn cursor_action(
    mouse: &mut Option<Mouse>,
    filter: &GridFilter,
    states: &[DrawState],
    button: MouseButton,
    repeat: u32,
    ctx: &GridCtx,
) -> Result<()> {
    let Some(m) = mouse else {
        return Ok(());
    };
    let center = region_center(filter, states, ctx);
    if let Some((cx, cy)) = center {
        m.warp(cx as i32, cy as i32)?;
    }
    let name = states.first().map_or("?", |ds| ds.name.as_str());
    let n = if repeat == 0 { 1 } else { repeat };
    m.click(button, n)?;
    if let Some((cx, cy)) = center {
        info!(
            "click btn{} x{n}  {name} ({cx:.0}, {cy:.0})",
            button.as_u8()
        );
    }
    Ok(())
}

// ── Region geometry ─────────────────────────────────────────────────

#[must_use]
pub fn region_rect(
    filter: &GridFilter,
    states: &[DrawState],
    ctx: &GridCtx,
) -> Option<(f32, f32, f32, f32)> {
    let input = filter.input();
    if input.len() < 2 {
        return None;
    }
    // `GridFilter` only ever holds ASCII (its chars come from `key_map`), so a
    // 2-byte prefix slice is always a char boundary.
    let cell = states
        .iter()
        .find_map(|ds| ds.grid.cell_by_label(&input[..2]))?;
    let (px, py, pw, ph) = (
        cell.rect.x() as f32,
        cell.rect.y() as f32,
        cell.rect.width() as f32,
        cell.rect.height() as f32,
    );
    if let Some(ch) = input.chars().nth(2) {
        let (r, c) = l3_key_pos(ch)?;
        let sx = px + c as f32 * pw / 5.0;
        let sy = py + r as f32 * ph / 3.0;
        let sw = pw / 5.0;
        let sh = ph / 3.0;
        let dx = ctx.l4_dx as f32 * sw / 7.0;
        let dy = ctx.l4_dy as f32 * sh / 7.0;
        Some((sx + dx, sy + dy, sw, sh))
    } else {
        Some((px, py, pw, ph))
    }
}

#[must_use]
pub fn region_center(
    filter: &GridFilter,
    states: &[DrawState],
    ctx: &GridCtx,
) -> Option<(f32, f32)> {
    let (x, y, w, h) = region_rect(filter, states, ctx)?;
    Some((x + w * 0.5, y + h * 0.5))
}
