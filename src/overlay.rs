// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

#[cfg(target_os = "windows")]
pub mod windows;
#[cfg(target_os = "linux")]
pub mod wlr;
#[cfg(target_os = "linux")]
pub mod x11;

use anyhow::Result;
#[cfg(target_os = "linux")]
use log::warn;
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
}

/// Platform-specific overlay backend.
pub trait OverlayBackend {
    fn named_monitors(&self) -> Result<Vec<Monitor>>;
    fn add_window(&mut self, x: i32, y: i32, w: u16, h: u16) -> Result<usize>;
    fn upload(&self, idx: usize, skia: &SkiaPixmap) -> Result<()>;
    fn show_all(&self) -> Result<()>;
    fn redraw_all(&self) -> Result<()>;
    fn pointer_warp(&self, x: i16, y: i16) -> Result<()>;
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
                Ok(b) => return Ok(Box::new(b)),
                Err(e) => warn!("Wayland connection failed: {e:#}"),
            }
        }
        if std::env::var("DISPLAY").is_ok() {
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
#[must_use]
pub fn query_screen_size() -> (u16, u16) {
    #[cfg(target_os = "windows")]
    {
        windows::query_screen_size()
    }
    #[cfg(target_os = "linux")]
    {
        x11::query_screen_size()
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        (0, 0)
    }
}
