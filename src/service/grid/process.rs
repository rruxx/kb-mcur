// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::Result;
use log::info;

use super::display::display_update;
use super::state::{DrawState, GridCtx, MONITOR_NAME};
use super::{GridConfig, GridFilter};
use crate::config::{action_key, l1_key_pos, l2_key_pos, l3_key_pos};
use crate::overlay::Overlay;
use crate::render::TextCache;
use crate::uinput::Mouse;

// ── Grid-mode byte handler ──────────────────────────────────────────

/// Single-byte input handler for interactive grid mode.
#[allow(clippy::too_many_arguments)]
pub(crate) fn process_byte(
    byte: u8,
    overlay: &mut Overlay,
    mouse: &mut Option<Mouse>,
    cfg: &GridConfig,
    cache: &TextCache,
    font_size: f32,
    draw_states: &mut [DrawState],
    ctx: &mut GridCtx,
) -> Result<bool> {
    match byte {
        b'\r' | b'\n' => {
            cursor_warp(mouse, &ctx.filter, draw_states, ctx)?;
            if let Some((cx, cy)) = region_center(&ctx.filter, draw_states, ctx) {
                overlay.pointer_warp(cx as i16, cy as i16)?;
            }
            ctx.filter.clear();
            ctx.repeat = 0;
            display_update(
                overlay,
                draw_states,
                cfg,
                cache,
                font_size,
                &ctx.filter,
                Some((ctx.l4_dx, ctx.l4_dy)),
            )?;
            return Ok(false);
        }

        0x1b => {
            ctx.filter.clear();
            ctx.repeat = 0;
            display_update(
                overlay,
                draw_states,
                cfg,
                cache,
                font_size,
                &ctx.filter,
                Some((ctx.l4_dx, ctx.l4_dy)),
            )?;
        }

        0x7f | b'\x08' => {
            ctx.filter.pop();
            if ctx.filter.len() >= 2 {
                ctx.filter.pop();
            }
            ctx.repeat = 0;
            display_update(
                overlay,
                draw_states,
                cfg,
                cache,
                font_size,
                &ctx.filter,
                Some((ctx.l4_dx, ctx.l4_dy)),
            )?;
        }

        ch => {
            let c = ch as char;

            if !ctx.filter.is_empty() && c.is_ascii_digit() {
                ctx.repeat = ctx
                    .repeat
                    .saturating_mul(10)
                    .saturating_add(u32::from(c as u8 - b'0'));
                return Ok(false);
            }

            if ctx.filter.len() >= 2
                && let Some(btn) = action_key(c)
            {
                cursor_action(mouse, &ctx.filter, draw_states, btn, ctx.repeat, ctx)?;
                if let Some((cx, cy)) = region_center(&ctx.filter, draw_states, ctx) {
                    overlay.pointer_warp(cx as i16, cy as i16)?;
                }
                ctx.filter.clear();
                ctx.repeat = 0;
                display_update(
                    overlay,
                    draw_states,
                    cfg,
                    cache,
                    font_size,
                    &ctx.filter,
                    Some((ctx.l4_dx, ctx.l4_dy)),
                )?;
                return Ok(false);
            }

            let valid = match ctx.filter.len() {
                0 => l1_key_pos(c).is_some(),
                1 => l2_key_pos(c).is_some(),
                2 | 3 => l3_key_pos(c).is_some(),
                _ => return Ok(false),
            };
            if !valid {
                return Ok(false);
            }

            if ctx.filter.len() >= 3 {
                ctx.filter.pop();
            }
            ctx.filter.push(c);
            ctx.repeat = 0;
            display_update(
                overlay,
                draw_states,
                cfg,
                cache,
                font_size,
                &ctx.filter,
                Some((ctx.l4_dx, ctx.l4_dy)),
            )?;
        }
    }
    Ok(false)
}

// ── Cursor & button actions ────────────────────────────────────────

fn cursor_warp(
    mouse: &mut Option<Mouse>,
    filter: &GridFilter,
    states: &[DrawState],
    ctx: &GridCtx,
) -> Result<()> {
    let Some(m) = mouse else {
        return Ok(());
    };
    if let Some((cx, cy)) = region_center(filter, states, ctx) {
        m.warp(cx as i16, cy as i16)?;
        info!(
            "=> {} ({cx:.0}, {cy:.0})",
            MONITOR_NAME.get().map_or("?", |s| s.as_str())
        );
    }
    Ok(())
}

fn cursor_action(
    mouse: &mut Option<Mouse>,
    filter: &GridFilter,
    states: &[DrawState],
    button: u8,
    repeat: u32,
    ctx: &GridCtx,
) -> Result<()> {
    let Some(m) = mouse else {
        return Ok(());
    };
    let center = region_center(filter, states, ctx);
    if let Some((cx, cy)) = center {
        m.warp(cx as i16, cy as i16)?;
    }
    let name = MONITOR_NAME.get().map_or("?", |s| s.as_str());
    let n = if repeat == 0 { 1 } else { repeat };
    m.click(button, n)?;
    if let Some((cx, cy)) = center {
        info!("click btn{button} x{n}  {name} ({cx:.0}, {cy:.0})");
    }
    Ok(())
}

// ── Region geometry ─────────────────────────────────────────────────

fn region_rect(
    filter: &GridFilter,
    states: &[DrawState],
    ctx: &GridCtx,
) -> Option<(f32, f32, f32, f32)> {
    let input = filter.input();
    if input.len() < 2 {
        return None;
    }
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

fn region_center(filter: &GridFilter, states: &[DrawState], ctx: &GridCtx) -> Option<(f32, f32)> {
    let (x, y, w, h) = region_rect(filter, states, ctx)?;
    Some((x + w * 0.5, y + h * 0.5))
}
