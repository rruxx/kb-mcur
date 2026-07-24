mod grid;
mod overlay;
mod render;

use std::io::Write;
use std::os::fd::AsRawFd;

use anyhow::{Context, Result};
use fontdue::Font;
use tiny_skia::Pixmap;

use crate::grid::{quad_key_index, quad_shrink, sub_key_index, Grid, GridConfig, GridFilter};
use crate::overlay::X11Overlay;
use crate::render::TextCache;

const FONT_DATA: &[u8] = include_bytes!("../assets/font.ttf");

struct DrawState {
    grid: Grid,
    base: Vec<u8>,
    pixmap: Pixmap,
}

fn main() -> Result<()> {
    let font = Font::from_bytes(FONT_DATA, fontdue::FontSettings::default())
        .map_err(|e| anyhow::anyhow!("failed to parse embedded font: {e}"))?;

    let mut overlay = X11Overlay::connect()?;
    let monitors = overlay.monitors().context("failed to query monitors")?;

    if monitors.is_empty() {
        anyhow::bail!("no active monitors detected");
    }

    let cfg = GridConfig::default();
    let min_h = monitors.iter().map(|m| m.3).min().unwrap_or(1080) as f32;
    let min_w = monitors.iter().map(|m| m.2).min().unwrap_or(1920) as f32;
    let cell_w = min_w / cfg.cols as f32;
    let cell_h = min_h / cfg.rows as f32;
    let font_size = (cell_w / 4.2).min(cell_h / 1.8).min(14.0).max(6.0).round();

    let cache = TextCache::new(&font, font_size);

    let mut draw_states: Vec<DrawState> = Vec::new();

    for &(x, y, w, h) in monitors.iter() {
        let grid = Grid::new(x, y, w as u32, h as u32, &cfg);
        let mut pixmap = Pixmap::new(w as u32, h as u32)
            .context("failed to create skia pixmap")?;
        render::render_base(&mut pixmap, &grid, &cfg);
        let base = pixmap.data().to_vec();
        render::render_labels(&mut pixmap, &grid, &cfg, &cache, font_size, None);
        overlay.add_window(x, y, w, h)?;
        overlay.upload(monitors.iter().position(|&m| m == (x, y, w, h)).unwrap(), &pixmap)?;
        draw_states.push(DrawState { grid, base, pixmap });
    }

    overlay.show_all()?;
    overlay.redraw_all()?;

    // ── Interactive loop ────────────────────────────────────────────

    let stdin_fd = std::io::stdin().as_raw_fd();
    let orig_term = raw_mode_on(stdin_fd);
    let is_tty = orig_term.is_ok();

    if !is_tty {
        eprintln!("no tty — showing grid for 5 s then exiting");
        overlay.wait_or_timeout(5)?;
        return Ok(());
    }

    let orig_term = orig_term.unwrap();
    let mut filter = GridFilter::new();
    print_prompt(&filter);

    loop {
        let mut byte = 0u8;
        let n = unsafe { libc::read(stdin_fd, &mut byte as *mut u8 as *mut libc::c_void, 1) };
        if n != 1 {
            break;
        }

        match byte {
            b'\r' | b'\n' => {
                if let Some((cx, cy)) = final_position(&filter, &draw_states) {
                    eprintln!("\n=> target ({cx:.0}, {cy:.0})");
                } else {
                    eprintln!("\n=> {0}", filter.input());
                }
                break;
            }
            0x1b => {
                filter.clear();
                redraw_grids(&overlay, &mut draw_states, &cfg, &cache, font_size, Some(&filter))?;
                print_prompt(&filter);
            }
            0x7f | b'\x08' => {
                filter.pop();
                redraw_current(&overlay, &mut draw_states, &cfg, &cache, font_size, &filter)?;
                print_prompt(&filter);
            }
            0x04 | 0x03 => {
                eprintln!();
                break;
            }
            ch => {
                let ch = ch as char;
                if filter.len() < 2 && ch.is_ascii_lowercase() {
                    // Level 1 & 2: 26×26 grid
                    filter.push(ch);
                } else if filter.len() == 2 && is_sub_key(ch) {
                    // Level 3: 4×2 sub-grid
                    filter.push(ch);
                } else if (3..=6).contains(&filter.len()) && quad_key_index(ch).is_some() {
                    // Levels 4-7: 2×2 bisect
                    filter.push(ch);
                } else {
                    continue; // ignore
                }

                if filter.len() >= 7 {
                    if let Some((cx, cy)) = final_position(&filter, &draw_states) {
                        eprintln!("\n=> target ({cx:.0}, {cy:.0})");
                    }
                    break;
                }

                redraw_current(&overlay, &mut draw_states, &cfg, &cache, font_size, &filter)?;
                print_prompt(&filter);
            }
        }
    }

    raw_mode_off(stdin_fd, orig_term)?;
    eprintln!("bye");
    Ok(())
}

// ── Redraw dispatcher ───────────────────────────────────────────────

/// Re-render according to current filter depth.
fn redraw_current(
    overlay: &X11Overlay,
    states: &mut [DrawState],
    cfg: &GridConfig,
    cache: &TextCache,
    font_size: f32,
    filter: &GridFilter,
) -> Result<()> {
    if filter.len() >= 3 {
        if let Some(region) = compute_region(filter, states) {
            redraw_bisect_overlay(overlay, states, cfg, cache, region)?;
        }
    } else if filter.len() >= 2 {
        redraw_subgrid_overlay(overlay, states, cfg, cache, filter)?;
    } else {
        redraw_grids(overlay, states, cfg, cache, font_size, Some(filter))?;
    }
    Ok(())
}

// ── 26×26 grid ──────────────────────────────────────────────────────

fn redraw_grids(
    overlay: &X11Overlay,
    states: &mut [DrawState],
    cfg: &GridConfig,
    cache: &TextCache,
    font_size: f32,
    filter: Option<&GridFilter>,
) -> Result<()> {
    for (idx, ds) in states.iter_mut().enumerate() {
        restore_base(&mut ds.pixmap, &ds.base);
        render::render_labels(&mut ds.pixmap, &ds.grid, cfg, cache, font_size, filter);
        overlay.upload(idx, &ds.pixmap)?;
    }
    overlay.redraw_all()?;
    Ok(())
}

// ── Sub-grid (4×2, level 3) ────────────────────────────────────────

fn redraw_subgrid_overlay(
    overlay: &X11Overlay,
    states: &mut [DrawState],
    cfg: &GridConfig,
    cache: &TextCache,
    filter: &GridFilter,
) -> Result<()> {
    let label = filter.input();
    if label.len() < 2 {
        return Ok(());
    }

    for (idx, ds) in states.iter_mut().enumerate() {
        restore_base(&mut ds.pixmap, &ds.base);
        if let Some(parent) = ds.grid.cell_by_label(&label[..2]) {
            let cell_w = parent.rect.width() as f32 / 4.0;
            let cell_h = parent.rect.height() as f32 / 2.0;
            let sub_font = (cell_w / 3.0).min(cell_h / 1.8).max(6.0).round();
            render::render_subgrid(&mut ds.pixmap, parent, cfg, cache, sub_font);
        }
        overlay.upload(idx, &ds.pixmap)?;
    }
    overlay.redraw_all()?;
    Ok(())
}

// ── Bisect (2×2, levels 4-7) ───────────────────────────────────────

fn redraw_bisect_overlay(
    overlay: &X11Overlay,
    states: &mut [DrawState],
    cfg: &GridConfig,
    cache: &TextCache,
    region: (f32, f32, f32, f32),
) -> Result<()> {
    let font = (region.2.min(region.3) / 8.0).max(6.0).round();
    for (idx, ds) in states.iter_mut().enumerate() {
        restore_base(&mut ds.pixmap, &ds.base);
        render::render_bisect(&mut ds.pixmap, region, cfg, cache, font);
        overlay.upload(idx, &ds.pixmap)?;
    }
    overlay.redraw_all()?;
    Ok(())
}

// ── Region computation ──────────────────────────────────────────────

/// Replay the filter string to compute the current target region
/// as a (x, y, w, h) tuple in monitor pixel coordinates.
fn compute_region(filter: &GridFilter, states: &[DrawState]) -> Option<(f32, f32, f32, f32)> {
    let input = filter.input();
    if input.len() < 2 {
        return None;
    }

    let prefix = &input[..2];
    let parent = states
        .iter()
        .find_map(|ds| ds.grid.cell_by_label(prefix))?;

    let pw = parent.rect.width() as f32;
    let ph = parent.rect.height() as f32;

    let mut region = (parent.rect.x() as f32, parent.rect.y() as f32, pw, ph);

    // Level 3: sub-cell within 4×2
    if let Some(ch) = input.chars().nth(2) {
        let idx = sub_key_index(ch)?;
        let sub_row = (idx / 4) as f32;
        let sub_col = (idx % 4) as f32;
        region = (
            parent.rect.x() as f32 + sub_col * pw / 4.0,
            parent.rect.y() as f32 + sub_row * ph / 2.0,
            pw / 4.0,
            ph / 2.0,
        );
    }

    // Levels 4-7: quadrant bisection
    for ch in input.chars().skip(3) {
        let idx = quad_key_index(ch)?;
        region = quad_shrink(region, idx);
    }

    Some(region)
}

/// The centre of the current region, if enough levels have been entered.
fn final_position(filter: &GridFilter, states: &[DrawState]) -> Option<(f32, f32)> {
    let (x, y, w, h) = compute_region(filter, states)?;
    Some((x + w * 0.5, y + h * 0.5))
}

// ── Helpers ────────────────────────────────────────────────────────

fn is_sub_key(ch: char) -> bool {
    matches!(ch, 'a' | 's' | 'd' | 'f' | 'j' | 'k' | 'l' | ';')
}

fn restore_base(pixmap: &mut Pixmap, base_data: &[u8]) {
    let dst = pixmap.pixels_mut();
    let dst_bytes: &mut [u8] =
        unsafe { std::slice::from_raw_parts_mut(dst.as_mut_ptr() as *mut u8, dst.len() * 4) };
    dst_bytes.copy_from_slice(base_data);
}

fn print_prompt(filter: &GridFilter) {
    let prefix = filter.input();
    let pad = " ".repeat(7usize.saturating_sub(prefix.len()));
    eprint!("\r[{prefix}]{pad}");
    let _ = std::io::stderr().flush();
}

// ── Raw terminal ───────────────────────────────────────────────────

fn raw_mode_on(fd: i32) -> Result<libc::termios> {
    let mut orig: libc::termios = unsafe { std::mem::zeroed() };
    if unsafe { libc::tcgetattr(fd, &mut orig) } != 0 {
        anyhow::bail!("tcgetattr failed");
    }
    let mut raw = orig;
    raw.c_lflag &= !(libc::ICANON | libc::ECHO);
    raw.c_cc[libc::VMIN] = 1;
    raw.c_cc[libc::VTIME] = 0;
    if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &raw) } != 0 {
        anyhow::bail!("tcsetattr failed");
    }
    Ok(orig)
}

fn raw_mode_off(fd: i32, orig: libc::termios) -> Result<()> {
    if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &orig) } != 0 {
        anyhow::bail!("tcsetattr restore failed");
    }
    Ok(())
}
