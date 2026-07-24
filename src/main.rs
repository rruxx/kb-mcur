mod grid;
mod overlay;
mod render;

use std::io::Write;
use std::os::fd::AsRawFd;

use anyhow::{Context, Result};
use fontdue::Font;
use tiny_skia::Pixmap;

use crate::grid::{Grid, GridConfig, GridFilter};
use crate::overlay::X11Overlay;
use crate::render::TextCache;

const FONT_DATA: &[u8] = include_bytes!("../assets/font.ttf");

/// Per-monitor reusable draw state — avoids allocation & base re-render on every key.
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

        // Base layer — background + grid lines
        let mut pixmap = Pixmap::new(w as u32, h as u32)
            .context("failed to create skia pixmap")?;
        render::render_base(&mut pixmap, &grid, &cfg);
        let base = pixmap.data().to_vec();

        // Initial render with all labels
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
            0x1b => {
                filter.clear();
                redraw_grids(&overlay, &mut draw_states, &cfg, &cache, font_size, Some(&filter))?;
                print_prompt(&filter);
            }
            0x7f | b'\x08' => {
                filter.pop();
                redraw_grids(&overlay, &mut draw_states, &cfg, &cache, font_size, Some(&filter))?;
                print_prompt(&filter);
            }
            0x04 | 0x03 => {
                eprintln!();
                break;
            }
            b'a'..=b'z' => {
                filter.push(byte as char);
                redraw_grids(&overlay, &mut draw_states, &cfg, &cache, font_size, Some(&filter))?;
                print_prompt(&filter);
            }
            _ => {}
        }
    }

    raw_mode_off(stdin_fd, orig_term)?;
    eprintln!("bye");
    Ok(())
}

// ── Redraw ─────────────────────────────────────────────────────────

fn redraw_grids(
    overlay: &X11Overlay,
    states: &mut [DrawState],
    cfg: &GridConfig,
    cache: &TextCache,
    font_size: f32,
    filter: Option<&GridFilter>,
) -> Result<()> {
    for (idx, ds) in states.iter_mut().enumerate() {
        // Reset pixmap to base layer
        restore_base(&mut ds.pixmap, &ds.base);

        // Draw matching labels only
        render::render_labels(&mut ds.pixmap, &ds.grid, cfg, cache, font_size, filter);

        overlay.upload(idx, &ds.pixmap)?;
    }
    overlay.redraw_all()?;
    Ok(())
}

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
