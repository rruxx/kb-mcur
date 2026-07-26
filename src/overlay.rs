// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

pub mod wlr;
pub mod x11;

use anyhow::Result;
use tiny_skia::Pixmap as SkiaPixmap;

/// Runtime polypick between X11 and wlr-layer-shell backends.
pub enum Overlay {
    X11(x11::X11Backend),
    Wlr(wlr::WlrBackend),
}

macro_rules! delegate {
    ($self:expr, $method:ident $(, $arg:expr)*) => {
        match $self {
            Overlay::X11(b) => b.$method($($arg),*),
            Overlay::Wlr(b) => b.$method($($arg),*),
        }
    };
}

impl Overlay {
    pub fn connect() -> Result<Self> {
        if std::env::var("WAYLAND_DISPLAY").is_ok() {
            if let Ok(b) = wlr::WlrBackend::connect() {
                return Ok(Overlay::Wlr(b));
            }
        }
        if std::env::var("DISPLAY").is_ok() {
            return Ok(Overlay::X11(x11::X11Backend::connect()?));
        }
        anyhow::bail!(
            "no display server detected.\n\
             For wlroots compositors (Sway/Hyprland/niri): wlr-layer-shell backend not yet implemented.\n\
             Install XWayland as a workaround: ensure DISPLAY is set."
        )
    }

    pub fn monitors(&self) -> Result<Vec<(i32, i32, u16, u16)>> {
        delegate!(self, monitors)
    }

    pub fn named_monitors(&self) -> Result<Vec<(String, i32, i32, u16, u16)>> {
        delegate!(self, named_monitors)
    }

    pub fn add_window(&mut self, x: i32, y: i32, w: u16, h: u16) -> Result<usize> {
        delegate!(self, add_window, x, y, w, h)
    }

    pub fn upload(&self, idx: usize, skia: &SkiaPixmap) -> Result<()> {
        delegate!(self, upload, idx, skia)
    }

    pub fn show_all(&self) -> Result<()> {
        delegate!(self, show_all)
    }

    pub fn redraw_all(&self) -> Result<()> {
        delegate!(self, redraw_all)
    }

    pub fn wait_or_timeout(&self, seconds: u64) -> Result<()> {
        delegate!(self, wait_or_timeout, seconds)
    }

    pub fn poll_fd(&self) -> Option<i32> {
        match self {
            Overlay::X11(_) => None,
            Overlay::Wlr(b) => b.poll_fd(),
        }
    }

    pub fn dispatch_pending(&self) -> Result<()> {
        match self {
            Overlay::X11(_) => Ok(()),
            Overlay::Wlr(b) => b.dispatch_pending(),
        }
    }

    /// Cursor control via native protocol (Wlr: zwlr_virtual_pointer, X11: no-op, uses uinput)
    pub fn pointer_warp(&self, x: i16, y: i16) -> Result<()> {
        match self {
            Overlay::X11(_) => Ok(()),
            Overlay::Wlr(b) => b.pointer_warp(x, y),
        }
    }

    pub fn pointer_click(&self, button: u8, count: u32) -> Result<()> {
        match self {
            Overlay::X11(_) => Ok(()),
            Overlay::Wlr(b) => b.pointer_click(button, count),
        }
    }

    pub fn pointer_toggle(&self, button: u8) -> Result<()> {
        match self {
            Overlay::X11(_) => Ok(()),
            Overlay::Wlr(b) => b.pointer_toggle(button),
        }
    }
}

/// Screen dimensions via a temporary X11 connection (for CLI use).
pub fn query_screen_size() -> (u16, u16) {
    x11::query_screen_size()
}
