mod grid;
mod overlay;
mod render;
mod uinput;

use std::io::Write;
use std::os::fd::AsRawFd;

use anyhow::{Context, Result};
use fontdue::Font;
use tiny_skia::Pixmap;

use crate::grid::{quad_key_index, quad_shrink, sub_key_index, Grid, GridConfig, GridFilter};
use crate::overlay::X11Overlay;
use crate::render::TextCache;
use crate::uinput::Mouse;

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
    if monitors.is_empty() { anyhow::bail!("no active monitors detected"); }

    let max_w = monitors.iter().map(|m| m.0 + m.2 as i32).max().unwrap_or(1920) as u16;
    let max_h = monitors.iter().map(|m| m.1 + m.3 as i32).max().unwrap_or(1080) as u16;
    let mut mouse = Mouse::new(max_w, max_h)
        .map_err(|e| { eprintln!("warn: uinput unavailable — {e}"); e })
        .ok();

    let cfg = GridConfig::default();
    let min_h = monitors.iter().map(|m| m.3).min().unwrap_or(1080) as f32;
    let cell_h = min_h / cfg.rows as f32;
    let font_size = (cell_h / 1.8).min(14.0).max(6.0).round();
    let cache = TextCache::new(&font, font_size);

    let mut draw_states: Vec<DrawState> = Vec::new();
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

    let stdin_fd = std::io::stdin().as_raw_fd();
    let orig_term = raw_mode_on(stdin_fd);
    if orig_term.is_err() {
        eprintln!("no tty — showing grid for 5 s then exiting");
        overlay.wait_or_timeout(5)?;
        return Ok(());
    }
    let orig_term = orig_term.unwrap();
    let mut filter = GridFilter::new();
    let mut repeat: u32 = 0;
    prompt(&filter);

    loop {
        let mut byte = 0u8;
        if unsafe { libc::read(stdin_fd, &mut byte as *mut u8 as *mut libc::c_void, 1) } != 1 {
            break;
        }
        match byte {
            b'\r' | b'\n' | b' ' => {
                if let Some((cx, cy)) = final_pos(&filter, &draw_states) {
                    if let Some(ref mut m) = mouse { m.warp(cx as i16, cy as i16)?; }
                    eprintln!("\n=> ({cx:.0}, {cy:.0})");
                }
                break;
            }
            0x1b => {
                filter.clear();
                repeat = 0;
                redraw(&overlay, &mut draw_states, &cfg, &cache, font_size, &filter)?;
                prompt(&filter);
            }
            0x7f | b'\x08' => {
                filter.pop();
                repeat = 0;
                redraw(&overlay, &mut draw_states, &cfg, &cache, font_size, &filter)?;
                prompt(&filter);
            }
            0x04 | 0x03 => { eprintln!(); break; }
            ch => {
                let ch = ch as char;

                // digits → accumulate repeat count
                if filter.len() >= 2 && ch.is_ascii_digit() {
                    repeat = repeat.saturating_mul(10).saturating_add((ch as u8 - b'0') as u32);
                    continue;
                }

                // action keys (anytime after position defined)
                if filter.len() >= 2 {
                    match ch {
                        'u' => { warp_act(&mut mouse, &filter, &draw_states, 1, false, repeat)?; break; }
                        'i' => { warp_act(&mut mouse, &filter, &draw_states, 2, false, repeat)?; break; }
                        'o' => { warp_act(&mut mouse, &filter, &draw_states, 3, false, repeat)?; break; }
                        'j' => { warp_act(&mut mouse, &filter, &draw_states, 1, true, repeat)?; break; }
                        'k' => { warp_act(&mut mouse, &filter, &draw_states, 2, true, repeat)?; break; }
                        'l' => { warp_act(&mut mouse, &filter, &draw_states, 3, true, repeat)?; break; }
                        _ => {}
                    }
                }

                // grid-position keys
                let ok = match filter.len() {
                    0 | 1 if ch.is_ascii_lowercase() => true,
                    2 if sub_key_index(ch).is_some() => true,
                    3..=6 if quad_key_index(ch).is_some() => true,
                    _ => false,
                };
                if !ok { continue; }
                filter.push(ch);
                repeat = 0;

                if filter.len() >= 7 {
                    if let Some((cx, cy)) = final_pos(&filter, &draw_states) {
                        if let Some(ref mut m) = mouse { m.warp(cx as i16, cy as i16)?; }
                        eprintln!("\n=> ({cx:.0}, {cy:.0})");
                    }
                    // Still allow one more key for action
                    let mut ab = 0u8;
                    if unsafe { libc::read(stdin_fd, &mut ab as *mut u8 as *mut libc::c_void, 1) } == 1 {
                        let ac = ab as char;
                        if filter.len() >= 2 {
                            match ac {
                                'u' => { warp_act(&mut mouse, &filter, &draw_states, 1, false, 0)?; }
                                'i' => { warp_act(&mut mouse, &filter, &draw_states, 2, false, 0)?; }
                                'o' => { warp_act(&mut mouse, &filter, &draw_states, 3, false, 0)?; }
                                'j' => { warp_act(&mut mouse, &filter, &draw_states, 1, true, 0)?; }
                                'k' => { warp_act(&mut mouse, &filter, &draw_states, 2, true, 0)?; }
                                'l' => { warp_act(&mut mouse, &filter, &draw_states, 3, true, 0)?; }
                                _ => {}
                            }
                        }
                    }
                    break;
                }
                redraw(&overlay, &mut draw_states, &cfg, &cache, font_size, &filter)?;
                prompt(&filter);
            }
        }
    }

    raw_mode_off(stdin_fd, orig_term)?;
    eprintln!("bye");
    Ok(())
}

fn warp_act(
    mouse: &mut Option<Mouse>,
    filter: &GridFilter,
    states: &[DrawState],
    button: u8,
    click: bool,
    repeat: u32,
) -> Result<()> {
    let Some(m) = mouse else { return Ok(()); };
    if let Some((cx, cy)) = final_pos(filter, states) {
        m.warp(cx as i16, cy as i16)?;
    }
    if click {
        let n = if repeat == 0 { 1 } else { repeat };
        m.click(button, n)?;
        eprintln!("click btn{button} x{n}");
    } else {
        m.toggle(button)?;
        eprintln!("toggle btn{button}");
    }
    Ok(())
}

// ── Unified redraw ──────────────────────────────────────────────────

fn redraw(
    overlay: &X11Overlay,
    states: &mut [DrawState],
    cfg: &GridConfig,
    cache: &TextCache,
    font_size: f32,
    filter: &GridFilter,
) -> Result<()> {
    let region = (filter.len() >= 3).then(|| compute_region(filter, states)).flatten();

    // fetch parent cell rects before the mutable borrow on `states`
    let p2 = filter.input()
        .get(..2)
        .and_then(|prefix| states.iter().find_map(|ds| ds.grid.cell_by_label(prefix)));
    let parent_rect = p2.map(|c| c.rect);

    for (idx, ds) in states.iter_mut().enumerate() {
        restore_base(&mut ds.pixmap, &ds.base);
        match (region, parent_rect) {
            (Some(r), _) => {
                let f = (r.2.min(r.3) / 8.0).max(6.0).round();
                render::render_bisect(&mut ds.pixmap, r, cfg, cache, f);
            }
            (_, Some(rect)) => {
                let cw = rect.width() as f32 / 4.0;
                let ch = rect.height() as f32 / 2.0;
                render::render_subgrid(&mut ds.pixmap, rect, cfg, cache, (cw / 3.0).min(ch / 1.8).max(6.0).round());
            }
            _ => render::render_labels(&mut ds.pixmap, &ds.grid, cfg, cache, font_size, Some(filter)),
        }
        overlay.upload(idx, &ds.pixmap)?;
    }
    overlay.redraw_all()?;
    Ok(())
}

fn compute_region(filter: &GridFilter, states: &[DrawState]) -> Option<(f32, f32, f32, f32)> {
    let input = filter.input();
    let parent = states.iter().find_map(|ds| ds.grid.cell_by_label(&input[..2]))?;
    let pw = parent.rect.width() as f32;
    let ph = parent.rect.height() as f32;
    let mut region = (parent.rect.x() as f32, parent.rect.y() as f32, pw, ph);

    if let Some(ch) = input.chars().nth(2) {
        let idx = sub_key_index(ch)?;
        region = (
            parent.rect.x() as f32 + (idx % 4) as f32 * pw / 4.0,
            parent.rect.y() as f32 + (idx / 4) as f32 * ph / 2.0,
            pw / 4.0, ph / 2.0,
        );
    }
    for ch in input.chars().skip(3) {
        region = quad_shrink(region, quad_key_index(ch)?);
    }
    Some(region)
}

fn final_pos(filter: &GridFilter, states: &[DrawState]) -> Option<(f32, f32)> {
    let (x, y, w, h) = compute_region(filter, states)?;
    Some((x + w * 0.5, y + h * 0.5))
}

fn restore_base(pixmap: &mut Pixmap, data: &[u8]) {
    let dst = pixmap.pixels_mut();
    let bytes = unsafe { std::slice::from_raw_parts_mut(dst.as_mut_ptr() as *mut u8, dst.len() * 4) };
    bytes.copy_from_slice(data);
}

fn prompt(f: &GridFilter) {
    let s = f.input();
    eprint!("\r[{s}]{}", " ".repeat(7usize.saturating_sub(s.len())));
    let _ = std::io::stderr().flush();
}

// ── Raw terminal ───────────────────────────────────────────────────

fn raw_mode_on(fd: i32) -> Result<libc::termios> {
    let mut orig: libc::termios = unsafe { std::mem::zeroed() };
    if unsafe { libc::tcgetattr(fd, &mut orig) } != 0 { anyhow::bail!("tcgetattr"); }
    let mut raw = orig;
    raw.c_lflag &= !(libc::ICANON | libc::ECHO);
    raw.c_cc[libc::VMIN] = 1;
    raw.c_cc[libc::VTIME] = 0;
    if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &raw) } != 0 { anyhow::bail!("tcsetattr"); }
    Ok(orig)
}

fn raw_mode_off(fd: i32, orig: libc::termios) -> Result<()> {
    if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &orig) } != 0 { anyhow::bail!("tcsetattr restore"); }
    Ok(())
}
