// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::{Context, Result};
use tiny_skia::Pixmap as SkiaPixmap;
use x11rb::connection::Connection;
use x11rb::protocol::randr;
use x11rb::protocol::shape;
use x11rb::protocol::xproto::*;
use x11rb::rust_connection::RustConnection;

/// A fullscreen transparent overlay window on one monitor.
struct WindowState {
    window: u32,
    pixmap: u32,
    gc: u32,
    width: u16,
    height: u16,
}

/// Manages X11 overlay windows across monitors.
pub struct X11Overlay {
    conn: RustConnection,
    screen_num: usize,
    depth: u8,
    visual_id: u32,
    colormap: u32,
    windows: Vec<WindowState>,
}

impl X11Overlay {
    /// Connect to the X server and pick a suitable visual.
    pub fn connect() -> Result<Self> {
        let (conn, screen_num) = x11rb::connect(None).context("cannot connect to X11 display")?;
        let screen = &conn.setup().roots[screen_num];

        let (depth, visual_id) = find_visual(screen);

        let colormap = conn.generate_id()?;
        conn.create_colormap(ColormapAlloc::NONE, colormap, screen.root, visual_id)?;

        Ok(Self {
            conn,
            screen_num,
            depth,
            visual_id,
            colormap,
            windows: Vec::new(),
        })
    }

    /// Query active monitor CRTCs via RandR.
    pub fn monitors(&self) -> Result<Vec<(i32, i32, u16, u16)>> {
        let screen = &self.conn.setup().roots[self.screen_num];
        let resources = randr::get_screen_resources(&self.conn, screen.root)?
            .reply()
            .context("RandR get_screen_resources failed")?;

        let mut out = Vec::new();
        for &crtc in &resources.crtcs {
            let info = randr::get_crtc_info(&self.conn, crtc, x11rb::CURRENT_TIME)?.reply()?;
            if info.width == 0 || info.height == 0 {
                continue;
            }
            out.push((info.x as i32, info.y as i32, info.width, info.height));
        }
        Ok(out)
    }

    /// Query monitors with RandR output names (e.g. "eDP-1", "HDMI-1").
    pub fn named_monitors(&self) -> Result<Vec<(String, i32, i32, u16, u16)>> {
        let screen = &self.conn.setup().roots[self.screen_num];
        let resources = randr::get_screen_resources(&self.conn, screen.root)?
            .reply()?;
        let mut out = Vec::new();
        for &output in &resources.outputs {
            let info = randr::get_output_info(&self.conn, output, x11rb::CURRENT_TIME)?
                .reply()?;
            if info.crtc == 0 || info.connection != randr::Connection::CONNECTED {
                continue;
            }
            let crtc = randr::get_crtc_info(&self.conn, info.crtc, x11rb::CURRENT_TIME)?
                .reply()?;
            if crtc.width == 0 || crtc.height == 0 {
                continue;
            }
            let name = String::from_utf8_lossy(&info.name).to_string();
            out.push((name, crtc.x as i32, crtc.y as i32, crtc.width, crtc.height));
        }
        Ok(out)
    }

    /// Create an overlay window + backing pixmap at (x, y) with given size.
    pub fn add_window(&mut self, x: i32, y: i32, w: u16, h: u16) -> Result<usize> {
        let screen = &self.conn.setup().roots[self.screen_num];

        let window = self.conn.generate_id()?;
        let pixmap = self.conn.generate_id()?;
        let gc = self.conn.generate_id()?;

        let aux = CreateWindowAux::new()
            .background_pixel(0)
            .border_pixel(0)
            .colormap(self.colormap)
            .override_redirect(1)
            .event_mask(EventMask::EXPOSURE | EventMask::KEY_PRESS);

        self.conn.create_window(
            self.depth,
            window,
            screen.root,
            x as i16,
            y as i16,
            w,
            h,
            0,
            WindowClass::INPUT_OUTPUT,
            self.visual_id,
            &aux,
        )?;

        self.conn
            .create_pixmap(self.depth, pixmap, screen.root, w, h)?;

        self.conn.create_gc(gc, pixmap, &CreateGCAux::default())?;

        self.set_always_on_top(window)?;
        self.set_window_title(window, b"kb-mcur-grid")?;
        self.set_input_shape(window)?;

        self.windows.push(WindowState {
            window,
            pixmap,
            gc,
            width: w,
            height: h,
        });
        Ok(self.windows.len() - 1)
    }

    /// Upload RGBA pixmap to the backing X11 pixmap.
    pub fn upload(&self, idx: usize, skia: &SkiaPixmap) -> Result<()> {
        let ws = &self.windows[idx];
        let pixels = rgba_to_x11_pixels(skia.data());
        let bytes = unsafe { u32_slice_as_bytes(&pixels) };

        self.conn.put_image(
            ImageFormat::Z_PIXMAP,
            ws.pixmap,
            ws.gc,
            ws.width,
            ws.height,
            0,
            0, // dst x, y
            0, // left_pad
            self.depth,
            bytes,
        )?;
        Ok(())
    }

    /// Show all overlay windows.
    pub fn show_all(&self) -> Result<()> {
        for ws in &self.windows {
            self.conn.map_window(ws.window)?;
        }
        self.conn.flush()?;
        Ok(())
    }

    /// Redraw all windows (copy pixmap → window, then flush).
    pub fn redraw_all(&self) -> Result<()> {
        for ws in &self.windows {
            self.conn
                .copy_area(ws.pixmap, ws.window, ws.gc, 0, 0, 0, 0, ws.width, ws.height)?;
        }
        self.conn.flush()?;
        Ok(())
    }

    /// Simple blocking event loop — returns on any key press, or after
    /// a timeout (duration in seconds).  Zero means wait forever.
    pub fn wait_or_timeout(&self, timeout_secs: u64) -> Result<()> {
        use std::time::Instant;

        let deadline = if timeout_secs > 0 {
            Some(Instant::now() + std::time::Duration::from_secs(timeout_secs))
        } else {
            None
        };

        loop {
            if let Some(dl) = deadline {
                if Instant::now() >= dl {
                    return Ok(());
                }
            }

            match self.conn.poll_for_event()? {
                Some(x11rb::protocol::Event::Expose(_)) => {
                    for ws in &self.windows {
                        self.conn.copy_area(
                            ws.pixmap, ws.window, ws.gc, 0, 0, 0, 0, ws.width, ws.height,
                        )?;
                    }
                    self.conn.flush()?;
                }
                Some(x11rb::protocol::Event::KeyPress(_)) => break,
                Some(_) => {}
                None => std::thread::sleep(std::time::Duration::from_millis(16)),
            }
        }
        Ok(())
    }

    fn set_window_title(&self, window: u32, title: &[u8]) -> Result<()> {
        let atom = self.conn.intern_atom(false, b"_NET_WM_NAME")?.reply()?.atom;
        let atom_utf8 = self.conn.intern_atom(false, b"UTF8_STRING")?.reply()?.atom;

        self.conn.change_property(
            PropMode::REPLACE,
            window,
            atom,
            atom_utf8,
            8,
            title.len() as u32,
            title,
        )?;
        Ok(())
    }

    fn set_input_shape(&self, window: u32) -> Result<()> {
        shape::rectangles(
            &self.conn,
            shape::SO::SET,
            shape::SK::INPUT,
            ClipOrdering::UNSORTED,
            window,
            0,
            0,
            &[],
        )?;
        Ok(())
    }

    fn set_always_on_top(&self, window: u32) -> Result<()> {
        let atom_wm_state = self
            .conn
            .intern_atom(false, b"_NET_WM_STATE")?
            .reply()?
            .atom;
        let atom_wm_state_above = self
            .conn
            .intern_atom(false, b"_NET_WM_STATE_ABOVE")?
            .reply()?
            .atom;

        self.conn.change_property(
            PropMode::REPLACE,
            window,
            atom_wm_state,
            AtomEnum::ATOM,
            32,
            1,
            &atom_wm_state_above.to_ne_bytes(),
        )?;

        // Hint that this is a notification/overlay type window
        let atom_wm_window_type = self
            .conn
            .intern_atom(false, b"_NET_WM_WINDOW_TYPE")?
            .reply()?
            .atom;
        let atom_wm_type_notif = self
            .conn
            .intern_atom(false, b"_NET_WM_WINDOW_TYPE_NOTIFICATION")?
            .reply()?
            .atom;

        self.conn.change_property(
            PropMode::REPLACE,
            window,
            atom_wm_window_type,
            AtomEnum::ATOM,
            32,
            1,
            &atom_wm_type_notif.to_ne_bytes(),
        )?;

        Ok(())
    }
}

/// Open a temporary X11 connection and query screen dimensions.
/// Falls back to (1920, 1080) if X11 is unavailable.
pub fn query_screen_size() -> (u16, u16) {
    let Ok((conn, screen_num)) = x11rb::connect(None) else {
        return (1920, 1080);
    };
    let mut monitors = Vec::new();
    let resources = randr::get_screen_resources(&conn, conn.setup().roots[screen_num].root);
    let Ok(cookie) = resources else {
        return (1920, 1080);
    };
    let reply = cookie.reply();
    let Ok(reply) = reply else {
        return (1920, 1080);
    };
    for &crtc in &reply.crtcs {
        let r = randr::get_crtc_info(&conn, crtc, x11rb::CURRENT_TIME);
        let Ok(cookie) = r else {
            continue;
        };
        let info = cookie.reply();
        let Ok(info) = info else {
            continue;
        };
        if info.width > 0 && info.height > 0 {
            monitors.push((info.x as i32, info.y as i32, info.width, info.height));
        }
    }
    if monitors.is_empty() {
        return (1920, 1080);
    }
    let max_w = monitors
        .iter()
        .map(|m| m.0 + m.2 as i32)
        .max()
        .unwrap_or(1920) as u16;
    let max_h = monitors
        .iter()
        .map(|m| m.1 + m.3 as i32)
        .max()
        .unwrap_or(1080) as u16;
    (max_w, max_h)
}
fn find_visual(screen: &Screen) -> (u8, u32) {
    for depth in &screen.allowed_depths {
        if depth.depth == 32 {
            if let Some(v) = depth
                .visuals
                .iter()
                .find(|v| v.class == VisualClass::TRUE_COLOR)
            {
                return (32, v.visual_id);
            }
        }
    }
    for depth in &screen.allowed_depths {
        if depth.depth == 24 {
            if let Some(v) = depth
                .visuals
                .iter()
                .find(|v| v.class == VisualClass::TRUE_COLOR)
            {
                return (24, v.visual_id);
            }
        }
    }
    // Fallback: use screen root visual
    (screen.root_depth, screen.root_visual)
}

/// Convert RGBA8888 bytes to X11-native ARGB u32 pixels.
/// Standard little-endian X11 ARGB visual layout: A<<24 | R<<16 | G<<8 | B.
fn rgba_to_x11_pixels(rgba: &[u8]) -> Vec<u32> {
    rgba.chunks_exact(4)
        .map(|c| {
            let r = c[0] as u32;
            let g = c[1] as u32;
            let b = c[2] as u32;
            let a = c[3] as u32;
            (a << 24) | (r << 16) | (g << 8) | b
        })
        .collect()
}

unsafe fn u32_slice_as_bytes(v: &[u32]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, v.len() * 4) }
}
