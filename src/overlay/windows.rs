// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Windows overlay backend: one transparent layered window per monitor,
//! painted via `UpdateLayeredWindow` with per-pixel alpha.

use anyhow::Result;
use tiny_skia::Pixmap as SkiaPixmap;
use windows_sys::Win32::Foundation::{HWND, POINT};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DestroyWindow, GetCursorPos, SW_SHOW, SetCursorPos, ShowWindow,
};

use crate::overlay::{Monitor, OverlayBackend};

pub mod dib;
pub mod monitor;
pub mod window;

/// Screen dimensions for the primary display (for CLI use).
#[must_use]
pub fn query_screen_size() -> (u16, u16) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
    let w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let h = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    (w as u16, h as u16)
}

/// Current cursor position in screen coordinates.
pub fn cursor_pos() -> Result<(i32, i32)> {
    let mut pt = POINT { x: 0, y: 0 };
    if unsafe { GetCursorPos(&raw mut pt) } == 0 {
        anyhow::bail!("GetCursorPos failed");
    }
    Ok((pt.x, pt.y))
}

/// Per-window state: the layered HWND and its backing DIB.
struct WinWindow {
    hwnd: HWND,
    x: i32,
    y: i32,
    dib: dib::Dib,
}

/// Overlay backend painting grid content into transparent layered windows.
pub struct WinBackend {
    monitors: Vec<Monitor>,
    windows: Vec<WinWindow>,
}

impl WinBackend {
    pub fn connect() -> Result<Self> {
        window::register_class()?;
        let monitors = monitor::monitors()?;
        Ok(Self {
            monitors,
            windows: Vec::new(),
        })
    }
}

impl OverlayBackend for WinBackend {
    fn named_monitors(&self) -> Result<Vec<Monitor>> {
        Ok(self.monitors.clone())
    }

    fn refresh_monitors(&mut self) -> Result<Vec<Monitor>> {
        self.monitors = monitor::monitors()?;
        Ok(self.monitors.clone())
    }

    fn add_window(&mut self, x: i32, y: i32, w: u16, h: u16) -> Result<usize> {
        let hwnd = window::create_window(x, y, w, h)?;
        let dib = match dib::Dib::new(w, h) {
            Ok(ok) => ok,
            Err(err) => {
                unsafe { DestroyWindow(hwnd) };
                return Err(err);
            }
        };
        self.windows.push(WinWindow { hwnd, x, y, dib });
        Ok(self.windows.len() - 1)
    }

    fn upload(&self, idx: usize, skia: &SkiaPixmap) -> Result<()> {
        let win = &self.windows[idx];
        win.dib.upload(skia)?;
        window::update_window(win.hwnd, win.x, win.y, &win.dib)
    }

    fn show_all(&self) -> Result<()> {
        for win in &self.windows {
            unsafe { ShowWindow(win.hwnd, SW_SHOW) };
        }
        Ok(())
    }

    fn redraw_all(&self) -> Result<()> {
        for win in &self.windows {
            window::update_window(win.hwnd, win.x, win.y, &win.dib)?;
        }
        Ok(())
    }

    fn pointer_warp(&self, x: i32, y: i32) -> Result<()> {
        if unsafe { SetCursorPos(x, y) } == 0 {
            anyhow::bail!("SetCursorPos failed");
        }
        Ok(())
    }
}

impl Drop for WinBackend {
    fn drop(&mut self) {
        for win in &self.windows {
            unsafe { DestroyWindow(win.hwnd) };
        }
    }
}
