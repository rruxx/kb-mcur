mod grid;
mod overlay;
mod render;

use anyhow::{Context, Result};
use fontdue::Font;
use tiny_skia::Pixmap;

use crate::grid::{Grid, GridConfig};
use crate::overlay::X11Overlay;

const FONT_DATA: &[u8] = include_bytes!("../assets/font.ttf");

fn main() -> Result<()> {
    let font = Font::from_bytes(FONT_DATA, fontdue::FontSettings::default())
        .map_err(|e| anyhow::anyhow!("failed to parse embedded font: {e}"))?;

    let mut overlay = X11Overlay::connect()?;
    let monitors = overlay.monitors().context("failed to query monitors")?;

    if monitors.is_empty() {
        anyhow::bail!("no active monitors detected");
    }

    let grid_cfg = GridConfig::default();

    // Pick the smallest monitor height as the base for font scaling
    let min_h = monitors.iter().map(|m| m.3).min().unwrap_or(1080) as f32;
    let font_size = (min_h / 24.0).round(); // ~45px on 1080p, ~60px on 1440p

    for (idx, &(x, y, w, h)) in monitors.iter().enumerate() {
        let grid = Grid::new(x, y, w as u32, h as u32, &grid_cfg);

        let mut pixmap = Pixmap::new(w as u32, h as u32)
            .context("failed to create skia pixmap")?;

        render::render_grid(&mut pixmap, &grid, &grid_cfg, &font, font_size);

        overlay.add_window(x, y, w, h)?;
        overlay.upload(idx, &pixmap)?;
    }

    overlay.show_all()?;
    eprintln!("grid overlay visible — press any key or wait 5s to exit");

    overlay.wait_or_timeout(5)?;
    Ok(())
}
