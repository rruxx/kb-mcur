// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Windows overlay backend (stage 1: screen-size query only).

use anyhow::Result;

use crate::overlay::{Monitor, OverlayBackend};
use tiny_skia::Pixmap as SkiaPixmap;

/// Screen dimensions for the primary display (for CLI use).
#[must_use]
pub fn query_screen_size() -> (u16, u16) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
    let w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let h = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    (w as u16, h as u16)
}

/// Windows overlay backend (not yet implemented — stage 2).
pub struct WinBackend;

impl WinBackend {
    pub fn connect() -> Result<Self> {
        anyhow::bail!("Windows overlay is not implemented yet (stage 2)")
    }
}

impl OverlayBackend for WinBackend {
    fn named_monitors(&self) -> Result<Vec<Monitor>> {
        anyhow::bail!("Windows overlay is not implemented yet (stage 2)")
    }
    fn add_window(&mut self, _x: i32, _y: i32, _w: u16, _h: u16) -> Result<usize> {
        anyhow::bail!("Windows overlay is not implemented yet (stage 2)")
    }
    fn upload(&self, _idx: usize, _skia: &SkiaPixmap) -> Result<()> {
        anyhow::bail!("Windows overlay is not implemented yet (stage 2)")
    }
    fn show_all(&self) -> Result<()> {
        anyhow::bail!("Windows overlay is not implemented yet (stage 2)")
    }
    fn redraw_all(&self) -> Result<()> {
        anyhow::bail!("Windows overlay is not implemented yet (stage 2)")
    }
    fn pointer_warp(&self, _x: i16, _y: i16) -> Result<()> {
        anyhow::bail!("Windows overlay is not implemented yet (stage 2)")
    }
}
