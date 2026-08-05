// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

pub mod wlr;
pub mod x11;

use anyhow::Result;
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

/// Runtime polypick between X11 and wlr-layer-shell backends.
pub enum Overlay {
    X11(Box<x11::X11Backend>),
    Wlr(Box<wlr::WlrBackend>),
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
            match wlr::WlrBackend::connect() {
                Ok(b) => return Ok(Overlay::Wlr(Box::new(b))),
                Err(e) => warn!("Wayland connection failed: {e:#}"),
            }
        }
        if std::env::var("DISPLAY").is_ok() {
            return Ok(Overlay::X11(Box::new(x11::X11Backend::connect()?)));
        }
        anyhow::bail!(
            "no display server detected.\n\
             For wlroots compositors (Sway/Hyprland/niri) ensure zwlr-layer-shell is enabled.\n\
             X11: ensure DISPLAY is set."
        )
    }

    pub fn named_monitors(&self) -> Result<Vec<Monitor>> {
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

    pub fn pointer_warp(&self, x: i16, y: i16) -> Result<()> {
        match self {
            Overlay::X11(_) => Ok(()),
            Overlay::Wlr(b) => b.pointer_warp(x, y),
        }
    }
}

/// Screen dimensions via a temporary X11 connection (for CLI use).
#[must_use]
pub fn query_screen_size() -> (u16, u16) {
    x11::query_screen_size()
}
