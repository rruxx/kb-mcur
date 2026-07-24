mod grid;
mod overlay;
mod render;

use std::io::Write;
use std::os::fd::AsRawFd;

use anyhow::{Context, Result};
use fontdue::Font;
use tiny_skia::Pixmap;

use crate::grid::{Grid, GridConfig, GridFilter, subgrid_cells};
use crate::overlay::X11Overlay;
use crate::render::TextCache;

const FONT_DATA: &[u8] = include_bytes!("../assets/font.ttf");

struct DrawState {
    grid: Grid,
    /// Background + grid lines, RGBA8888.
    base: Vec<u8>,
    /// Persistent pixmap (same dimensions as `base`).
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
                let sel = filter.input().to_string();
                eprintln!("\n=> {sel}");
                break;
            }
            // Escape — reset to full 26×26 grid
            0x1b => {
                filter.clear();
                redraw_grids(&overlay, &mut draw_states, &cfg, &cache, font_size, Some(&filter))?;
                print_prompt(&filter);
            }
            // Backspace — undo last char
            0x7f | b'\x08' => {
                filter.pop();
                if filter.len() >= 2 {
                    redraw_subgrid_overlay(&overlay, &mut draw_states, &cfg, &cache, &filter)?;
                } else {
                    redraw_grids(&overlay, &mut draw_states, &cfg, &cache, font_size, Some(&filter))?;
                }
                print_prompt(&filter);
            }
            // Ctrl+D / Ctrl+C — quit
            0x04 | 0x03 => {
                eprintln!();
                break;
            }
            // Printable chars
            ch => {
                let ch = ch as char;
                if filter.len() < 2 && ch.is_ascii_lowercase() {
                    // Level 1 & 2: 26×26 grid row/col filter
                    filter.push(ch);
                    if filter.len() >= 2 {
                        // Entered 2 chars → zoom into sub-grid
                        redraw_subgrid_overlay(&overlay, &mut draw_states, &cfg, &cache, &filter)?;
                    } else {
                        redraw_grids(&overlay, &mut draw_states, &cfg, &cache, font_size, Some(&filter))?;
                    }
                    print_prompt(&filter);
                } else if filter.len() >= 2 && is_sub_key(ch) {
                    // Level 3: sub-grid cell selection
                    match find_target(&draw_states, &filter, ch) {
                        Some((x, y)) => {
                            eprintln!("\n=> target ({x:.0}, {y:.0})");
                            break;
                        }
                        None => eprintln!("\n=> cell not found for '{}'", filter.input()),
                    }
                }
                // Other bytes: silently ignored
            }
        }
    }

    raw_mode_off(stdin_fd, orig_term)?;
    eprintln!("bye");
    Ok(())
}

// ── 26×26 grid redraw ──────────────────────────────────────────────

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

// ── Sub-grid (level 3) redraw ──────────────────────────────────────

/// Restore base 26×26 grid, then overlay the 4×2 sub-grid inside the
/// cell that matches the current 2-character filter.
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
    let prefix = &label[..2];

    for (idx, ds) in states.iter_mut().enumerate() {
        restore_base(&mut ds.pixmap, &ds.base);

        if let Some(parent) = ds.grid.cell_by_label(prefix) {
            // Use a smaller font for the sub-grid labels
            let cell_w = parent.rect.width() as f32 / 4.0;
            let cell_h = parent.rect.height() as f32 / 2.0;
            let sub_font = (cell_w / 3.0).min(cell_h / 1.8).max(6.0).round();

            // Build a temporary cache with the sub-grid font size if needed.
            // For now, reuse the main cache — labels are single chars, readable.
            render::render_subgrid(&mut ds.pixmap, parent, cfg, cache, sub_font);
        }

        overlay.upload(idx, &ds.pixmap)?;
    }
    overlay.redraw_all()?;
    Ok(())
}

// ── Target lookup ──────────────────────────────────────────────────

/// Given the 2-letter cell filter and a sub-grid key (a/s/d/f/j/k/l/;),
/// compute the final cursor position in pixel space.
fn find_target(states: &[DrawState], filter: &GridFilter, sub_key: char) -> Option<(f32, f32)> {
    let label = filter.input();
    if label.len() < 2 {
        return None;
    }

    let sub_idx = sub_key_index(sub_key)?;

    // The selected cell might be on any monitor grid.
    for ds in states {
        if let Some(parent) = ds.grid.cell_by_label(&label[..2]) {
            let subs = subgrid_cells(parent);
            return Some(subs[sub_idx].center);
        }
    }
    None
}

fn is_sub_key(ch: char) -> bool {
    matches!(ch, 'a' | 's' | 'd' | 'f' | 'j' | 'k' | 'l' | ';')
}

fn sub_key_index(ch: char) -> Option<usize> {
    match ch {
        'a' => Some(0),
        's' => Some(1),
        'd' => Some(2),
        'f' => Some(3),
        'j' => Some(4),
        'k' => Some(5),
        'l' => Some(6),
        ';' => Some(7),
        _ => None,
    }
}

// ── Helpers ────────────────────────────────────────────────────────

fn restore_base(pixmap: &mut Pixmap, base_data: &[u8]) {
    let dst = pixmap.pixels_mut();
    let dst_bytes: &mut [u8] =
        unsafe { std::slice::from_raw_parts_mut(dst.as_mut_ptr() as *mut u8, dst.len() * 4) };
    dst_bytes.copy_from_slice(base_data);
}

fn print_prompt(filter: &GridFilter) {
    let prefix = filter.input();
    let pad = " ".repeat(2usize.saturating_sub(prefix.len()));
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
