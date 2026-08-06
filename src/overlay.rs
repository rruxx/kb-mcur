// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

#[cfg(target_os = "linux")]
pub mod kde;
#[cfg(target_os = "windows")]
pub mod windows;
#[cfg(target_os = "linux")]
pub mod wlr;
#[cfg(target_os = "linux")]
pub mod x11;

use anyhow::Result;
#[cfg(target_os = "linux")]
use log::{info, warn};
use tiny_skia::Pixmap as SkiaPixmap;

/// A display output: its name, origin and size in screen coordinates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Monitor {
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub w: u16,
    pub h: u16,
}

impl Monitor {
    /// Bounding box covering all monitors, in screen coordinates.
    #[must_use]
    pub fn bbox(monitors: &[Monitor]) -> (i32, i32, u16, u16) {
        let bx = monitors.iter().map(|m| m.x).min().unwrap_or(0);
        let by = monitors.iter().map(|m| m.y).min().unwrap_or(0);
        let bw = monitors
            .iter()
            .map(|m| m.x + i32::from(m.w))
            .max()
            .unwrap_or(0)
            - bx;
        let bh = monitors
            .iter()
            .map(|m| m.y + i32::from(m.h))
            .max()
            .unwrap_or(0)
            - by;
        (bx, by, bw as u16, bh as u16)
    }

    /// Font-scaling baseline: the limiting logical height across all monitors,
    /// with logical width folded in by the glyph aspect ratio so text density
    /// stays balanced on narrow/vertical layouts. Usually resolves to the height.
    #[must_use]
    pub fn font_scale_base(monitors: &[Monitor]) -> f32 {
        let min_w = monitors
            .iter()
            .map(|m| m.w)
            .min()
            .unwrap_or(crate::config::FALLBACK_WIDTH) as f32;
        let min_h = monitors
            .iter()
            .map(|m| m.h)
            .min()
            .unwrap_or(crate::config::FALLBACK_HEIGHT) as f32;
        (min_w * crate::config::FONT_ASPECT_RATIO).min(min_h)
    }
}

/// Platform-specific overlay backend.
pub trait OverlayBackend {
    fn named_monitors(&self) -> Result<Vec<Monitor>>;
    /// Re-query the current monitor set (resolution/scale may have changed
    /// since connect).
    fn refresh_monitors(&mut self) -> Result<Vec<Monitor>>;
    fn add_window(&mut self, x: i32, y: i32, w: u16, h: u16) -> Result<usize>;
    fn upload(&self, idx: usize, skia: &SkiaPixmap) -> Result<()>;
    fn show_all(&self) -> Result<()>;
    fn redraw_all(&self) -> Result<()>;
    fn pointer_warp(&self, x: i32, y: i32) -> Result<()>;
}

/// The active overlay backend for the current platform.
pub type Overlay = Box<dyn OverlayBackend>;

/// Connect to the display server and pick the overlay backend.
pub fn connect() -> Result<Overlay> {
    #[cfg(target_os = "windows")]
    {
        Ok(Box::new(windows::WinBackend::connect()?))
    }
    #[cfg(target_os = "linux")]
    {
        if std::env::var("WAYLAND_DISPLAY").is_ok() {
            match wlr::WlrBackend::connect() {
                Ok(b) => {
                    info!("[overlay] backend: wlr layer-shell");
                    return Ok(Box::new(b));
                }
                Err(e) => warn!("Wayland connection failed: {e:#}"),
            }
        }
        if std::env::var("DISPLAY").is_ok() {
            info!("[overlay] backend: x11");
            return Ok(Box::new(x11::X11Backend::connect()?));
        }
        anyhow::bail!(
            "no display server detected.\n\
             For wlroots compositors (Sway/Hyprland/niri) ensure zwlr-layer-shell is enabled.\n\
             X11: ensure DISPLAY is set."
        )
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        anyhow::bail!("unsupported platform")
    }
}

/// Screen dimensions (for CLI use).
pub fn query_screen_size() -> Result<(u16, u16)> {
    #[cfg(target_os = "windows")]
    {
        Ok(windows::query_screen_size())
    }
    #[cfg(target_os = "linux")]
    {
        if std::env::var("WAYLAND_DISPLAY").is_ok() {
            wlr::screen_size()
                .ok_or_else(|| anyhow::anyhow!("no Wayland display or active outputs"))
        } else {
            x11::query_screen_size()
        }
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        anyhow::bail!("unsupported platform")
    }
}

/// Current cursor position in screen coordinates (for CLI use).
pub fn cursor_pos() -> Result<(i32, i32)> {
    #[cfg(target_os = "windows")]
    {
        windows::cursor_pos()
    }
    #[cfg(target_os = "linux")]
    {
        if std::env::var("WAYLAND_DISPLAY").is_ok() {
            // KDE exposes the cursor position via KWin scripting. For other
            // Wayland compositors there is no global pointer query API; we use
            // per-output layer surfaces + a virtual-pointer poke to capture an
            // `enter` event and read the position back.
            if std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|d| d.contains("KDE")) {
                kde::cursor_pos()
            } else {
                wlr::cursor_pos()
            }
        } else {
            x11::cursor_pos()
        }
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        anyhow::bail!("unsupported platform")
    }
}
