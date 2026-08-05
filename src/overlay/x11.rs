// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::{Context, Result};
use tiny_skia::Pixmap as SkiaPixmap;
use x11rb::connection::Connection;
use x11rb::protocol::randr;
use x11rb::protocol::shape;
use x11rb::protocol::xproto::{
    AtomEnum, ClipOrdering, ColormapAlloc, ConnectionExt, CreateGCAux, CreateWindowAux, EventMask,
    ImageFormat, PropMode, Screen, VisualClass, WindowClass,
};
use x11rb::rust_connection::RustConnection;

use crate::overlay::Monitor;

struct WindowState {
    window: u32,
    pixmap: u32,
    gc: u32,
    width: u16,
    height: u16,
}

pub struct X11Backend {
    conn: RustConnection,
    screen_num: usize,
    depth: u8,
    visual_id: u32,
    colormap: u32,
    windows: Vec<WindowState>,
}

impl X11Backend {
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

    pub fn named_monitors(&self) -> Result<Vec<Monitor>> {
        let screen = &self.conn.setup().roots[self.screen_num];
        let resources = randr::get_screen_resources(&self.conn, screen.root)?.reply()?;
        let mut out = Vec::new();
        for &output in &resources.outputs {
            let info = randr::get_output_info(&self.conn, output, x11rb::CURRENT_TIME)?.reply()?;
            if info.crtc == 0 || info.connection != randr::Connection::CONNECTED {
                continue;
            }
            let crtc = randr::get_crtc_info(&self.conn, info.crtc, x11rb::CURRENT_TIME)?.reply()?;
            if crtc.width == 0 || crtc.height == 0 {
                continue;
            }
            let name = String::from_utf8_lossy(&info.name).into_owned();
            out.push(Monitor {
                name,
                x: i32::from(crtc.x),
                y: i32::from(crtc.y),
                w: crtc.width,
                h: crtc.height,
            });
        }
        Ok(out)
    }

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
        set_always_on_top(&self.conn, window)?;
        set_window_title(&self.conn, window, crate::config::GRID_WINDOW.as_bytes())?;
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
        self.windows.push(WindowState {
            window,
            pixmap,
            gc,
            width: w,
            height: h,
        });
        Ok(self.windows.len() - 1)
    }

    pub fn upload(&self, idx: usize, skia: &SkiaPixmap) -> Result<()> {
        let ws = &self.windows[idx];
        let pixels = rgba_to_x11_pixels(skia.data());
        self.conn.put_image(
            ImageFormat::Z_PIXMAP,
            ws.pixmap,
            ws.gc,
            ws.width,
            ws.height,
            0,
            0,
            0,
            self.depth,
            bytemuck::cast_slice(&pixels),
        )?;
        Ok(())
    }

    pub fn show_all(&self) -> Result<()> {
        for ws in &self.windows {
            self.conn.map_window(ws.window)?;
        }
        self.conn.flush()?;
        Ok(())
    }

    pub fn redraw_all(&self) -> Result<()> {
        for ws in &self.windows {
            self.conn
                .copy_area(ws.pixmap, ws.window, ws.gc, 0, 0, 0, 0, ws.width, ws.height)?;
        }
        self.conn.flush()?;
        Ok(())
    }
}

#[must_use]
pub fn query_screen_size() -> (u16, u16) {
    let fallback = (
        crate::config::FALLBACK_WIDTH,
        crate::config::FALLBACK_HEIGHT,
    );
    let Ok((conn, screen_num)) = x11rb::connect(None) else {
        return fallback;
    };
    let mut monitors = Vec::new();
    let resources = randr::get_screen_resources(&conn, conn.setup().roots[screen_num].root);
    let Ok(cookie) = resources else {
        return fallback;
    };
    let reply = cookie.reply();
    let Ok(reply) = reply else {
        return fallback;
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
            monitors.push(Monitor {
                name: String::new(),
                x: i32::from(info.x),
                y: i32::from(info.y),
                w: info.width,
                h: info.height,
            });
        }
    }
    if monitors.is_empty() {
        return fallback;
    }
    let max_w = monitors
        .iter()
        .map(|m| m.x + i32::from(m.w))
        .max()
        .unwrap_or(i32::from(crate::config::FALLBACK_WIDTH)) as u16;
    let max_h = monitors
        .iter()
        .map(|m| m.y + i32::from(m.h))
        .max()
        .unwrap_or(i32::from(crate::config::FALLBACK_HEIGHT)) as u16;
    (max_w, max_h)
}

fn find_visual(screen: &Screen) -> (u8, u32) {
    for &d in &[32u8, 24] {
        if let Some(depth) = screen.allowed_depths.iter().find(|dp| dp.depth == d)
            && let Some(v) = depth
                .visuals
                .iter()
                .find(|v| v.class == VisualClass::TRUE_COLOR)
        {
            return (d, v.visual_id);
        }
    }
    (screen.root_depth, screen.root_visual)
}

fn rgba_to_x11_pixels(rgba: &[u8]) -> Vec<u32> {
    rgba.chunks_exact(4)
        .map(|c| {
            let r = u32::from(c[0]);
            let g = u32::from(c[1]);
            let b = u32::from(c[2]);
            let a = u32::from(c[3]);
            (a << 24) | (r << 16) | (g << 8) | b
        })
        .collect()
}

fn set_window_title(conn: &RustConnection, window: u32, title: &[u8]) -> Result<()> {
    let atom = conn.intern_atom(false, b"_NET_WM_NAME")?.reply()?.atom;
    let atom_utf8 = conn.intern_atom(false, b"UTF8_STRING")?.reply()?.atom;
    conn.change_property(
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

fn set_always_on_top(conn: &RustConnection, window: u32) -> Result<()> {
    let atom_wm_state = conn.intern_atom(false, b"_NET_WM_STATE")?.reply()?.atom;
    let atom_wm_state_above = conn
        .intern_atom(false, b"_NET_WM_STATE_ABOVE")?
        .reply()?
        .atom;
    conn.change_property(
        PropMode::REPLACE,
        window,
        atom_wm_state,
        AtomEnum::ATOM,
        32,
        1,
        &atom_wm_state_above.to_ne_bytes(),
    )?;
    let atom_wm_window_type = conn
        .intern_atom(false, b"_NET_WM_WINDOW_TYPE")?
        .reply()?
        .atom;
    let atom_wm_type_desktop = conn
        .intern_atom(false, b"_NET_WM_WINDOW_TYPE_DESKTOP")?
        .reply()?
        .atom;
    conn.change_property(
        PropMode::REPLACE,
        window,
        atom_wm_window_type,
        AtomEnum::ATOM,
        32,
        1,
        &atom_wm_type_desktop.to_ne_bytes(),
    )?;
    Ok(())
}
