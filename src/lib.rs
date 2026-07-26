// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

pub mod config;
pub mod evdev;
pub mod grid;
pub mod keymap;
pub mod overlay;
pub mod render;
pub mod uinput;

use std::sync::OnceLock;

use anyhow::{Context, Result};
use fontdue::Font;
use tiny_skia::Pixmap;

use config::{
    action_key, direction_delta, is_quad_key, is_sub_key, quad_key_index, quad_shrink,
    sub_key_index,
};
use evdev::KeyboardDev;
use grid::{Grid, GridConfig, GridFilter};
use keymap::{ModState, map as key_map};
use overlay::Overlay;
use render::TextCache;
use uinput::Mouse;

const FONT_DATA: &[u8] = include_bytes!("../assets/font.ttf");

static MONITOR_NAME: OnceLock<String> = OnceLock::new();

/// Per-monitor state: the 26×26 grid, its base-layer RGBA bytes, and a
/// persistent pixmap that is re-uploaded on every redraw.
struct DrawState {
    grid: Grid,
    pixmap: Pixmap,
}

// ── Entry point ─────────────────────────────────────────────────────

pub fn run() -> Result<()> {
    let font = Font::from_bytes(FONT_DATA, fontdue::FontSettings::default())
        .map_err(|e| anyhow::anyhow!("failed to parse embedded font: {e}"))?;

    let mut overlay = Overlay::connect()?;
    let named = overlay
        .named_monitors()
        .context("failed to query monitors")?;
    if named.is_empty() {
        anyhow::bail!("no active monitors detected");
    }
    let backend = if matches!(overlay, Overlay::Wlr(_)) { "wlr" } else { "x11" };
    eprintln!("[{backend}] {} monitor(s) detected", named.len());
    let monitors: Vec<(i32, i32, u16, u16)> = named.iter().map(|n| (n.1, n.2, n.3, n.4)).collect();

    let monitor_idx = if monitors.len() == 1 {
        0
    } else {
        show_display_ids(&mut overlay, &font, &monitors)?;
        let idx = select_display(&monitors)?;
        overlay = Overlay::connect()?;
        idx
    };
    let selected = &monitors[monitor_idx];
    let _ = MONITOR_NAME.set(named[monitor_idx].0.clone());

    let max_w = monitors
        .iter()
        .map(|m| m.0 + m.2 as i32)
        .max()
        .unwrap_or(1920) as u16;
    let max_h = monitors
        .iter()
        .map(|m| m.1 + m.3 as i32)
        .max()
        .unwrap_or(1080) as u16;
    let mut mouse = Mouse::new(max_w, max_h)
        .map_err(|e| {
            eprintln!("warn: uinput unavailable — {e}");
            e
        })
        .ok();

    let single_monitors = vec![*selected];
    let (cfg, font_size, cache, mut draw_states) =
        init_overlay(&mut overlay, &font, &single_monitors)?;

    // Grab all keyboards via evdev.
    let kbd = KeyboardDev::open_all()?;
    run_input_evdev(
        &mut overlay,
        &mut mouse,
        &cfg,
        &cache,
        font_size,
        &mut draw_states,
        kbd,
    )?;

    eprintln!("bye");
    Ok(())
}

// ── Mouse-only entry point (no grid) ────────────────────────────────

pub fn run_mouse() -> Result<()> {
    let (sw, sh) = overlay::query_screen_size();
    let mut mouse = Mouse::new(sw, sh).context("uinput")?;
    let mut kbd = KeyboardDev::open_all().context("keyboard grab")?;
    eprintln!("mouse mode — w/a/s/d move, j/k/l click, u/i/o toggle, Space exit");

    let mut ctx = MouseCtx::new();
    let mut mods = ModState::default();

    loop {
        match kbd.poll_key(32) {
            Ok(Some((code, value))) => {
                mods.update(code, value > 0);
                ctx.shift_held = mods.shift;

                if value == 0 {
                    if let Some(byte) = key_map(code, &mods) {
                        let ch = (byte as char).to_ascii_lowercase();
                        let bit = MouseCtx::dir_bit(ch);
                        if bit != 0 {
                            ctx.dir_mask &= !bit;
                            ctx.dir_held = ctx.dir_held.saturating_sub(1);
                            if ctx.dir_held == 0 {
                                ctx.direction_count = 0;
                            }
                        }
                    }
                    continue;
                }

                let Some(byte) = key_map(code, &mods) else {
                    continue;
                };
                let ch = (byte as char).to_ascii_lowercase();

                if value == 1 {
                    let bit = MouseCtx::dir_bit(ch);
                    if bit != 0 {
                        ctx.dir_mask |= bit;
                        ctx.dir_held = ctx.dir_held.saturating_add(1);
                    }
                }

                match byte {
                    b'\r' | b'\n' | b' ' | 0x1b | 0x04 | 0x03 => break,
                    _ => {}
                }

                if ch.is_ascii_digit() {
                    ctx.repeat = ctx
                        .repeat
                        .saturating_mul(10)
                        .saturating_add((ch as u8 - b'0') as u32);
                    continue;
                }

                if let Some((btn, is_click)) = action_key(ch) {
                    if is_click {
                        let n = if ctx.repeat == 0 { 1 } else { ctx.repeat };
                        mouse.click(btn, n)?;
                        eprintln!("click btn{btn} x{n}");
                    } else {
                        mouse.toggle(btn)?;
                        eprintln!("toggle btn{btn}");
                    }
                    break;
                }
            }
            Ok(None) => {
                if ctx.dir_held == 1 {
                    let dir_ch = MouseCtx::dir_char(ctx.dir_mask);
                    let (dx, dy) = direction_delta(dir_ch).unwrap_or((0, 0));
                    let step = if ctx.shift_held {
                        config::CTRL_BOOST_STEP
                    } else {
                        ctx.direction_count = ctx.direction_count.saturating_add(1);
                        config::cursor_speed(ctx.direction_count)
                    };
                    mouse.move_rel(dx * step, dy * step)?;
                }
            }
            Err(e) => return Err(e),
        }
    }

    std::thread::spawn(move || {
        kbd.release();
        drop(kbd);
    });
    eprintln!("bye");
    Ok(())
}

// ── Display selection (multi-monitor) ───────────────────────────────

fn show_display_ids(
    overlay: &mut Overlay,
    font: &Font,
    monitors: &[(i32, i32, u16, u16)],
) -> Result<()> {
    let cache = TextCache::new(font, 96.0);
    let cfg = GridConfig::default();
    for (i, &(x, y, w, h)) in monitors.iter().enumerate() {
        let mut pixmap = Pixmap::new(w as u32, h as u32).context("pixmap")?;
        render::render_base(
            &mut pixmap,
            &grid::Grid::new(w as u32, h as u32, &cfg),
            &cfg,
        );
        let digit = (b'1' + i as u8) as char;
        render::render_digit(
            &mut pixmap,
            digit,
            w as f32 * 0.5,
            h as f32 * 0.5,
            &cache,
            [192, 255, 192, 192],
        );
        overlay.add_window(x, y, w, h)?;
        overlay.upload(i, &pixmap)?;
    }
    overlay.show_all()?;
    overlay.redraw_all()?;
    Ok(())
}

fn select_display(monitors: &[(i32, i32, u16, u16)]) -> Result<usize> {
    let kbd = KeyboardDev::open_all().context("evdev for display select")?;
    let mut mods = ModState::default();
    let idx = loop {
        let (code, value) = kbd.next_keypress()?;
        mods.update(code, value > 0);
        if value == 0 {
            continue;
        }
        let Some(byte) = key_map(code, &mods) else {
            continue;
        };
        if (b'1'..=b'9').contains(&byte) {
            let i = (byte - b'1') as usize;
            if i < monitors.len() {
                std::thread::spawn(move || {
                    drop(kbd);
                });
                break i;
            }
        }
    };
    Ok(idx)
}

fn init_overlay(
    overlay: &mut Overlay,
    font: &Font,
    monitors: &[(i32, i32, u16, u16)],
) -> Result<(GridConfig, f32, TextCache, Vec<DrawState>)> {
    let cfg = GridConfig::default();
    let font_size =
        (monitors.iter().map(|m| m.3).min().unwrap_or(1080) as f32 / cfg.rows as f32 / 1.8)
            .min(14.0)
            .max(6.0)
            .round();
    let cache = TextCache::new(font, font_size);

    let mut draw_states = Vec::new();
    for (idx, &(x, y, w, h)) in monitors.iter().enumerate() {
        let grid = Grid::new(w as u32, h as u32, &cfg);
        let mut pixmap = Pixmap::new(w as u32, h as u32).context("pixmap")?;
        render::render_base(&mut pixmap, &grid, &cfg);
        render::render_labels(&mut pixmap, &grid, &cfg, &cache, font_size, None);
        overlay.add_window(x, y, w, h)?;
        overlay.upload(idx, &pixmap)?;
        draw_states.push(DrawState { grid, pixmap });
    }
    overlay.show_all()?;
    overlay.redraw_all()?;

    Ok((cfg, font_size, cache, draw_states))
}

// ── Input loop (grid mode) ──────────────────────────────────────────

struct GridCtx {
    filter: GridFilter,
    repeat: u32,
}

impl GridCtx {
    fn new() -> Self {
        Self {
            filter: GridFilter::new(),
            repeat: 0,
        }
    }
}

fn run_input_evdev(
    overlay: &mut Overlay,
    mouse: &mut Option<Mouse>,
    cfg: &GridConfig,
    cache: &TextCache,
    font_size: f32,
    draw_states: &mut [DrawState],
    mut kbd: KeyboardDev,
) -> Result<()> {
    let mut ctx = GridCtx::new();
    let mut mods = ModState::default();
    loop {
        let (code, value) = kbd.next_keypress()?;
        mods.update(code, value > 0);
        if value == 0 {
            continue;
        }
        let Some(byte) = key_map(code, &mods) else {
            continue;
        };
        if process_byte(
            byte,
            overlay,
            mouse,
            cfg,
            cache,
            font_size,
            draw_states,
            &mut ctx,
        )? {
            break;
        }
    }

    // Release grab on a background thread — EVIOCGRAB,0 ioctl blocks
    // in the kernel for ~400ms while the compositor re-acquires the
    // device.  Doing this inline would keep the overlay visible.
    std::thread::spawn(move || {
        kbd.release();
        drop(kbd);
    });

    Ok(())
}

// ── Grid-mode byte handler ──────────────────────────────────────────

/// Single-byte input handler for interactive grid mode.
fn process_byte(
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
        b'\r' | b'\n' | b' ' => {
            cursor_warp(mouse, &ctx.filter, draw_states)?;
            if let Some((cx, cy)) = region_center(&ctx.filter, draw_states) {
                overlay.pointer_warp(cx as i16, cy as i16)?;
            }
            return Ok(true);
        }

        0x1b => {
            ctx.filter.clear();
            ctx.repeat = 0;
            display_update(overlay, draw_states, cfg, cache, font_size, &ctx.filter)?;
        }

        0x7f | b'\x08' => {
            ctx.filter.pop();
            ctx.repeat = 0;
            display_update(overlay, draw_states, cfg, cache, font_size, &ctx.filter)?;
        }

        0x04 | 0x03 => {
            eprintln!();
            return Ok(true);
        }

        ch => {
            let ch = ch as char;

            if ctx.filter.len() >= 2 && ch.is_ascii_digit() {
                ctx.repeat = ctx
                    .repeat
                    .saturating_mul(10)
                    .saturating_add((ch as u8 - b'0') as u32);
                return Ok(false);
            }

            if ctx.filter.len() >= 2 {
                if let Some((btn, is_click)) = action_key(ch) {
                    cursor_action(
                        mouse,
                        &ctx.filter,
                        draw_states,
                        btn,
                        is_click,
                        ctx.repeat,
                    )?;
                    if let Some((cx, cy)) = region_center(&ctx.filter, draw_states) {
                        overlay.pointer_warp(cx as i16, cy as i16)?;
                    }
                    return Ok(true);
                }
            }

            match ctx.filter.len() {
                0 | 1 if ch.is_ascii_lowercase() => {}
                2 if is_sub_key(ch) => {}
                3..=6 if is_quad_key(ch) => {}
                _ => return Ok(false),
            };

            ctx.filter.push(ch);
            ctx.repeat = 0;
            display_update(overlay, draw_states, cfg, cache, font_size, &ctx.filter)?;
        }
    }
    Ok(false)
}

// ── Cursor & button actions ────────────────────────────────────────

/// Move the cursor to the centre of the currently-selected region.
fn cursor_warp(
    mouse: &mut Option<Mouse>,
    filter: &GridFilter,
    states: &[DrawState],
) -> Result<()> {
    let Some(m) = mouse else {
        return Ok(());
    };
    if let Some((cx, cy)) = region_center(filter, states) {
        m.warp(cx as i16, cy as i16)?;
        eprintln!(
            "\n=> {} ({cx:.0}, {cy:.0})",
            MONITOR_NAME.get().map_or("?", |s| s.as_str())
        );
    }
    Ok(())
}

/// Warp the cursor, then either click N times or toggle a button.
fn cursor_action(
    mouse: &mut Option<Mouse>,
    filter: &GridFilter,
    states: &[DrawState],
    button: u8,
    is_click: bool,
    repeat: u32,
) -> Result<()> {
    let Some(m) = mouse else {
        return Ok(());
    };
    let center = region_center(filter, states);
    if let Some((cx, cy)) = center {
        m.warp(cx as i16, cy as i16)?;
    }
    let name = MONITOR_NAME.get().map_or("?", |s| s.as_str());
    if is_click {
        let n = if repeat == 0 { 1 } else { repeat };
        m.click(button, n)?;
        if let Some((cx, cy)) = center {
            eprintln!("click btn{button} x{n}  {name} ({cx:.0}, {cy:.0})");
        }
    } else {
        m.toggle(button)?;
        if let Some((cx, cy)) = center {
            eprintln!("toggle btn{button}  {name} ({cx:.0}, {cy:.0})");
        }
    }
    Ok(())
}

// ── Display update ──────────────────────────────────────────────────

fn display_update(
    overlay: &Overlay,
    states: &mut [DrawState],
    cfg: &GridConfig,
    cache: &TextCache,
    font_size: f32,
    filter: &GridFilter,
) -> Result<()> {
    let (region, parent_rect) = resolve_render_target(filter, states);

    for (idx, ds) in states.iter_mut().enumerate() {
        render::render_base(&mut ds.pixmap, &ds.grid, cfg);

        if let Some(r) = region {
            let f = (r.2.min(r.3) / 8.0).max(6.0).round();
            render::render_bisect(&mut ds.pixmap, r, cfg, cache, f);
        } else if let Some(rect) = parent_rect {
            let cw = rect.width() as f32 / 4.0;
            let ch = rect.height() as f32 / 2.0;
            let f = (cw / 3.0).min(ch / 1.8).max(6.0).round();
            render::render_subgrid(&mut ds.pixmap, rect, cfg, cache, f);
        } else {
            render::render_labels(
                &mut ds.pixmap,
                &ds.grid,
                cfg,
                cache,
                font_size,
                Some(filter),
            );
        }

        overlay.upload(idx, &ds.pixmap)?;
    }

    overlay.show_all()?;
    overlay.redraw_all()?;
    Ok(())
}

fn resolve_render_target(
    filter: &GridFilter,
    states: &[DrawState],
) -> (Option<(f32, f32, f32, f32)>, Option<tiny_skia::IntRect>) {
    if filter.len() < 2 {
        return (None, None);
    }
    let parent = states
        .iter()
        .find_map(|ds| ds.grid.cell_by_label(&filter.input()[..2]))
        .map(|c| c.rect);
    if filter.len() >= 3 {
        (region_rect(filter, states), parent)
    } else {
        (None, parent)
    }
}

// ── Region geometry ─────────────────────────────────────────────────

/// Replay the entire filter string to compute the currently-selected
/// rectangle in monitor-pixel coordinates as (x, y, w, h).
fn region_rect(filter: &GridFilter, states: &[DrawState]) -> Option<(f32, f32, f32, f32)> {
    let input = filter.input();
    if input.len() < 2 {
        return None;
    }
    let parent = states
        .iter()
        .find_map(|ds| ds.grid.cell_by_label(&input[..2]))?;
    let (px, py, pw, ph) = (
        parent.rect.x() as f32,
        parent.rect.y() as f32,
        parent.rect.width() as f32,
        parent.rect.height() as f32,
    );
    let mut r = (px, py, pw, ph);
    // Level 3 — sub-cell inside the 4×2 partition
    if let Some(ch) = input.chars().nth(2) {
        let idx = sub_key_index(ch)?;
        r = (
            px + (idx % 4) as f32 * pw / 4.0,
            py + (idx / 4) as f32 * ph / 2.0,
            pw / 4.0,
            ph / 2.0,
        );
    }
    // Levels 4-7 — successive 2×2 bisection
    for ch in input.chars().skip(3) {
        r = quad_shrink(r, quad_key_index(ch)?);
    }
    Some(r)
}

/// Centre pixel of the current region.
fn region_center(filter: &GridFilter, states: &[DrawState]) -> Option<(f32, f32)> {
    let (x, y, w, h) = region_rect(filter, states)?;
    Some((x + w * 0.5, y + h * 0.5))
}

// ═══════════════════════════════════════════════════════════════════
// Mouse mode — standalone w/a/s/d cursor control
// ═══════════════════════════════════════════════════════════════════

struct MouseCtx {
    dir_held: u8,
    dir_mask: u8,
    direction_count: u32,
    shift_held: bool,
    repeat: u32,
}

impl MouseCtx {
    fn new() -> Self {
        Self {
            dir_held: 0,
            dir_mask: 0,
            direction_count: 0,
            shift_held: false,
            repeat: 0,
        }
    }

    fn dir_bit(ch: char) -> u8 {
        match ch.to_ascii_lowercase() {
            'w' => 0x01,
            'a' => 0x02,
            's' => 0x04,
            'd' => 0x08,
            _ => 0x00,
        }
    }

    fn dir_char(mask: u8) -> char {
        match mask {
            0x01 => 'w',
            0x02 => 'a',
            0x04 => 's',
            0x08 => 'd',
            _ => '\0',
        }
    }
}
