// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later


pub mod config;
pub mod evdev;
pub mod grid;
pub mod keymap;
pub mod overlay;
pub mod render;
pub mod uinput;

use std::io::Write;
use std::os::fd::AsRawFd;

use anyhow::{Context, Result};
use fontdue::Font;
use tiny_skia::Pixmap;

use config::{action_key, is_quad_key, is_sub_key, quad_key_index, quad_shrink, sub_key_index};
use evdev::KeyboardDev;
use grid::{Grid, GridConfig, GridFilter};
use keymap::{ModState, map as key_map};
use overlay::X11Overlay;
use render::TextCache;
use uinput::Mouse;

const FONT_DATA: &[u8] = include_bytes!("../assets/font.ttf");

/// Per-monitor state: the 26×26 grid, its base-layer RGBA bytes, and a
/// persistent pixmap that is re-uploaded on every redraw.
struct DrawState {
    grid: Grid,
    base: Vec<u8>,
    pixmap: Pixmap,
}

// ── Entry point ─────────────────────────────────────────────────────

pub fn run() -> Result<()> {
    let font = Font::from_bytes(FONT_DATA, fontdue::FontSettings::default())
        .map_err(|e| anyhow::anyhow!("failed to parse embedded font: {e}"))?;

    let mut overlay = X11Overlay::connect()?;
    let monitors = overlay.monitors().context("failed to query monitors")?;
    if monitors.is_empty() { anyhow::bail!("no active monitors detected"); }

    // Physical cursor control via /dev/uinput — best-effort.
    let max_w = monitors.iter().map(|m| m.0 + m.2 as i32).max().unwrap_or(1920) as u16;
    let max_h = monitors.iter().map(|m| m.1 + m.3 as i32).max().unwrap_or(1080) as u16;
    let mut mouse = Mouse::new(max_w, max_h)
        .map_err(|e| { eprintln!("warn: uinput unavailable — {e}"); e }).ok();

    let (cfg, font_size, cache, mut draw_states) = init_overlay(&mut overlay, &font, &monitors)?;

    let stdin_fd = std::io::stdin().as_raw_fd();
    let orig_term = terminal_raw_on(stdin_fd);
    if orig_term.is_err() {
        // No TTY — try evdev, fall back to display-only
        match KeyboardDev::open_all() {
            Ok(kbd) => {
                run_input_loop_evdev(&mut overlay, &mut mouse, &cfg, &cache, font_size, &mut draw_states, kbd)?;
            }
            Err(e) => {
                eprintln!("evdev unavailable: {e} — showing grid for 5 s then exiting");
                overlay.wait_or_timeout(5)?;
            }
        }
        return Ok(());
    }

    run_input_loop(&mut overlay, &mut mouse, &cfg, &cache, font_size, &mut draw_states, stdin_fd)?;

    terminal_raw_off(stdin_fd, orig_term.unwrap())?;
    eprintln!("bye");
    Ok(())
}

// ── Overlay initialisation ──────────────────────────────────────────

fn init_overlay(
    overlay: &mut X11Overlay,
    font: &Font,
    monitors: &[(i32, i32, u16, u16)],
) -> Result<(GridConfig, f32, TextCache, Vec<DrawState>)> {
    let cfg = GridConfig::default();
    let font_size = (monitors.iter().map(|m| m.3).min().unwrap_or(1080) as f32 / cfg.rows as f32 / 1.8).min(14.0).max(6.0).round();
    let cache = TextCache::new(font, font_size);

    let mut draw_states = Vec::new();
    for (idx, &(x, y, w, h)) in monitors.iter().enumerate() {
        let grid = Grid::new(w as u32, h as u32, &cfg);
        let mut pixmap = Pixmap::new(w as u32, h as u32).context("pixmap")?;
        render::render_base(&mut pixmap, &grid, &cfg);
        let base = pixmap.data().to_vec();
        render::render_labels(&mut pixmap, &grid, &cfg, &cache, font_size, None);
        overlay.add_window(x, y, w, h)?;
        overlay.upload(idx, &pixmap)?;
        draw_states.push(DrawState { grid, base, pixmap });
    }
    overlay.show_all()?;
    overlay.redraw_all()?;

    Ok((cfg, font_size, cache, draw_states))
}

// ── Input loop ──────────────────────────────────────────────────────

fn run_input_loop(
    overlay: &mut X11Overlay,
    mouse: &mut Option<Mouse>,
    cfg: &GridConfig,
    cache: &TextCache,
    font_size: f32,
    draw_states: &mut [DrawState],
    stdin_fd: i32,
) -> Result<()> {
    let mut filter = GridFilter::new();
    let mut repeat: u32 = 0;
    prompt(&filter);

    loop {
        let mut byte = 0u8;
        if unsafe { libc::read(stdin_fd, &mut byte as *mut u8 as *mut libc::c_void, 1) } != 1 { break; }

        match byte {
            // Enter / Space — warp cursor and quit (no button change)
            b'\r' | b'\n' | b' ' => {
                cursor_warp(mouse, &filter, draw_states)?;
                break;
            }
            // Escape — reset filter to full 26×26 grid
            0x1b => {
                filter.clear(); repeat = 0;
                display_update(overlay, draw_states, cfg, cache, font_size, &filter)?;
                prompt(&filter);
            }
            // Backspace — undo last character, revert one level
            0x7f | b'\x08' => {
                filter.pop(); repeat = 0;
                display_update(overlay, draw_states, cfg, cache, font_size, &filter)?;
                prompt(&filter);
            }
            // Ctrl+D / Ctrl+C
            0x04 | 0x03 => { eprintln!(); break; }
            ch => {
                let ch = ch as char;

                // Digits after position is defined → click-repeat count (e.g. "3j")
                if filter.len() >= 2 && ch.is_ascii_digit() {
                    repeat = repeat.saturating_mul(10).saturating_add((ch as u8 - b'0') as u32);
                    continue;
                }

                // Action keys (u/i/o toggle, j/k/l click) — warp & execute
                if filter.len() >= 2 {
                    if let Some((btn, is_click)) = action_key(ch) {
                        cursor_action(mouse, &filter, draw_states, btn, is_click, repeat)?;
                        break;
                    }
                }

                // Grid-position keys — narrow down the selected region
                let ok = match filter.len() {
                    0 | 1 if ch.is_ascii_lowercase() => true,
                    2 if is_sub_key(ch) => true,
                    3..=6 if is_quad_key(ch) => true,
                    _ => false,
                };
                if !ok { continue; }
                filter.push(ch);
                repeat = 0;
                display_update(overlay, draw_states, cfg, cache, font_size, &filter)?;
                prompt(&filter);
            }
        }
    }
    Ok(())
}

fn run_input_loop_evdev(
    overlay: &mut X11Overlay,
    mouse: &mut Option<Mouse>,
    cfg: &GridConfig,
    cache: &TextCache,
    font_size: f32,
    draw_states: &mut [DrawState],
    kbd: KeyboardDev,
) -> Result<()> {
    let mut filter = GridFilter::new();
    let mut repeat: u32 = 0;
    let mut mods = ModState::default();
    prompt(&filter);

    loop {
        let (code, value) = kbd.next_keypress()?;
        mods.update(code, value > 0);
        if value == 0 { continue; } // key release

        let Some(byte) = key_map(code, &mods) else { continue; };

        match byte {
            b'\r' | b'\n' | b' ' => {
                cursor_warp(mouse, &filter, draw_states)?;
                break;
            }
            0x1b => {
                filter.clear(); repeat = 0;
                display_update(overlay, draw_states, cfg, cache, font_size, &filter)?;
                prompt(&filter);
            }
            0x7f => {
                filter.pop(); repeat = 0;
                display_update(overlay, draw_states, cfg, cache, font_size, &filter)?;
                prompt(&filter);
            }
            ch => {
                let ch = ch as char;

                if filter.len() >= 2 && ch.is_ascii_digit() {
                    repeat = repeat.saturating_mul(10).saturating_add((ch as u8 - b'0') as u32);
                    continue;
                }
                if filter.len() >= 2 {
                    if let Some((btn, is_click)) = action_key(ch) {
                        cursor_action(mouse, &filter, draw_states, btn, is_click, repeat)?;
                        break;
                    }
                }
                let ok = match filter.len() {
                    0 | 1 if ch.is_ascii_lowercase() => true,
                    2 if is_sub_key(ch) => true,
                    3..=6 if is_quad_key(ch) => true,
                    _ => false,
                };
                if !ok { continue; }
                filter.push(ch);
                repeat = 0;
                display_update(overlay, draw_states, cfg, cache, font_size, &filter)?;
                prompt(&filter);
            }
        }
    }
    // grab releases automatically when kbd is dropped
    Ok(())
}

// ── Cursor & button actions ────────────────────────────────────────

/// Move the cursor to the centre of the currently-selected region.
fn cursor_warp(
    mouse: &mut Option<Mouse>,
    filter: &GridFilter,
    states: &[DrawState],
) -> Result<()> {
    let Some(m) = mouse else { return Ok(()); };
    if let Some((cx, cy)) = region_center(filter, states) {
        m.warp(cx as i16, cy as i16)?;
        eprintln!("\n=> ({cx:.0}, {cy:.0})");
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
    let Some(m) = mouse else { return Ok(()); };
    if let Some((cx, cy)) = region_center(filter, states) { m.warp(cx as i16, cy as i16)?; }
    if is_click {
        let n = if repeat == 0 { 1 } else { repeat };
        m.click(button, n)?;
        eprintln!("click btn{button} x{n}");
    } else {
        m.toggle(button)?;
        eprintln!("toggle btn{button}");
    }
    Ok(())
}

// ── Display update ──────────────────────────────────────────────────

/// Re-render all monitors according to the current filter depth.
fn display_update(
    overlay: &X11Overlay,
    states: &mut [DrawState],
    cfg: &GridConfig,
    cache: &TextCache,
    font_size: f32,
    filter: &GridFilter,
) -> Result<()> {
    let (region, parent_rect) = if filter.len() < 2 {
        (None, None)
    } else {
        let parent = states.iter().find_map(|ds| ds.grid.cell_by_label(&filter.input()[..2])).map(|c| c.rect);
        if filter.len() >= 3 { (region_rect(filter, states), parent) } else { (None, parent) }
    };

    for (idx, ds) in states.iter_mut().enumerate() {
        pixmap_restore_base(&mut ds.pixmap, &ds.base);
        if let Some(r) = region {
            render::render_bisect(&mut ds.pixmap, r, cfg, cache, (r.2.min(r.3) / 8.0).max(6.0).round());
        } else if let Some(rect) = parent_rect {
            let cw = rect.width() as f32 / 4.0; let ch = rect.height() as f32 / 2.0;
            render::render_subgrid(&mut ds.pixmap, rect, cfg, cache, (cw / 3.0).min(ch / 1.8).max(6.0).round());
        } else {
            render::render_labels(&mut ds.pixmap, &ds.grid, cfg, cache, font_size, Some(filter));
        }
        overlay.upload(idx, &ds.pixmap)?;
    }
    overlay.redraw_all()?;
    Ok(())
}

// ── Region geometry ─────────────────────────────────────────────────

/// Replay the entire filter string to compute the currently-selected
/// rectangle in monitor-pixel coordinates as (x, y, w, h).
fn region_rect(filter: &GridFilter, states: &[DrawState]) -> Option<(f32, f32, f32, f32)> {
    let input = filter.input();
    if input.len() < 2 { return None; }
    let parent = states.iter().find_map(|ds| ds.grid.cell_by_label(&input[..2]))?;
    let (px, py, pw, ph) = (parent.rect.x() as f32, parent.rect.y() as f32, parent.rect.width() as f32, parent.rect.height() as f32);
    let mut r = (px, py, pw, ph);
    // Level 3 — sub-cell inside the 4×2 partition
    if let Some(ch) = input.chars().nth(2) {
        let idx = sub_key_index(ch)?;
        r = (px + (idx % 4) as f32 * pw / 4.0, py + (idx / 4) as f32 * ph / 2.0, pw / 4.0, ph / 2.0);
    }
    // Levels 4-7 — successive 2×2 bisection
    for ch in input.chars().skip(3) { r = quad_shrink(r, quad_key_index(ch)?); }
    Some(r)
}

/// Centre pixel of the current region.
fn region_center(filter: &GridFilter, states: &[DrawState]) -> Option<(f32, f32)> {
    let (x, y, w, h) = region_rect(filter, states)?;
    Some((x + w * 0.5, y + h * 0.5))
}

// ── Pixmap helpers ──────────────────────────────────────────────────

fn pixmap_restore_base(pixmap: &mut Pixmap, data: &[u8]) {
    let dst = pixmap.pixels_mut();
    unsafe { std::slice::from_raw_parts_mut(dst.as_mut_ptr() as *mut u8, dst.len() * 4) }.copy_from_slice(data);
}

// ── Terminal I/O ────────────────────────────────────────────────────

fn prompt(f: &GridFilter) {
    let s = f.input();
    eprint!("\r[{s}]{}", " ".repeat(7usize.saturating_sub(s.len())));
    let _ = std::io::stderr().flush();
}

fn terminal_raw_on(fd: i32) -> Result<libc::termios> {
    let mut orig: libc::termios = unsafe { std::mem::zeroed() };
    if unsafe { libc::tcgetattr(fd, &mut orig) } != 0 { anyhow::bail!("tcgetattr"); }
    let mut raw = orig;
    raw.c_lflag &= !(libc::ICANON | libc::ECHO);
    raw.c_cc[libc::VMIN] = 1; raw.c_cc[libc::VTIME] = 0;
    if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &raw) } != 0 { anyhow::bail!("tcsetattr"); }
    Ok(orig)
}

fn terminal_raw_off(fd: i32, orig: libc::termios) -> Result<()> {
    if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &orig) } != 0 { anyhow::bail!("tcsetattr restore"); }
    Ok(())
}
