// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

//! wlr-layer-shell overlay backend for wlroots-based Wayland compositors.

use std::os::fd::{AsFd, OwnedFd};
use std::ptr::NonNull;

use anyhow::{Context, Result};
use tiny_skia::Pixmap as SkiaPixmap;
use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{
        wl_compositor::WlCompositor,
        wl_output::WlOutput,
        wl_registry::WlRegistry,
        wl_shm::{self, WlShm},
        wl_shm_pool::WlShmPool,
        wl_surface::WlSurface,
    },
};
use wayland_protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1::{self, ZwlrLayerShellV1},
    zwlr_layer_surface_v1::{self, ZwlrLayerSurfaceV1},
};
use wayland_protocols_wlr::virtual_pointer::v1::client::{
    zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1,
    zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1,
};

use crate::overlay::{Monitor, OverlayBackend};

struct LayerWin {
    surface: WlSurface,
    w: i32,
    h: i32,
    pool_off: usize,
}

pub struct WlrBackend {
    conn: Connection,
    compositor: WlCompositor,
    shm: WlShm,
    layer_shell: ZwlrLayerShellV1,
    vptr: Option<ZwlrVirtualPointerV1>,
    windows: Vec<LayerWin>,
    monitors: Vec<Monitor>,
    shm_ptr: Option<NonNull<u8>>,
    shm_len: usize,
    shm_fd: Option<OwnedFd>,
    shm_pool: Option<WlShmPool>,
}

macro_rules! dispatch_stub {
    ($ty:ty) => {
        impl Dispatch<$ty, ()> for WlrBackend {
            fn event(
                _: &mut Self,
                _: &$ty,
                _: <$ty as Proxy>::Event,
                _: &(),
                _: &Connection,
                _: &QueueHandle<Self>,
            ) {
            }
        }
    };
}
impl Dispatch<WlRegistry, GlobalListContents> for WlrBackend {
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: <WlRegistry as Proxy>::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
dispatch_stub!(WlOutput);
dispatch_stub!(WlCompositor);
dispatch_stub!(WlShm);
dispatch_stub!(ZwlrLayerShellV1);
dispatch_stub!(WlSurface);
dispatch_stub!(WlShmPool);
dispatch_stub!(wayland_client::protocol::wl_buffer::WlBuffer);
dispatch_stub!(wayland_client::protocol::wl_region::WlRegion);
dispatch_stub!(ZwlrVirtualPointerV1);
dispatch_stub!(ZwlrVirtualPointerManagerV1);

impl Dispatch<ZwlrLayerSurfaceV1, ()> for WlrBackend {
    fn event(
        _: &mut Self,
        proxy: &ZwlrLayerSurfaceV1,
        event: <ZwlrLayerSurfaceV1 as Proxy>::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwlr_layer_surface_v1::Event::Configure { serial, .. } = event {
            proxy.ack_configure(serial);
        }
    }
}

impl WlrBackend {
    pub fn connect() -> Result<Self> {
        let conn = Connection::connect_to_env().context("Wayland connection")?;
        let (globals, evq) = registry_queue_init::<WlrBackend>(&conn)?;
        let qh = evq.handle();
        let compositor: WlCompositor = globals.bind(&qh, 4..=6, ()).context("wl_compositor")?;
        let shm: WlShm = globals.bind(&qh, 1..=1, ()).context("wl_shm")?;
        let layer_shell: ZwlrLayerShellV1 = globals
            .bind(&qh, 1..=5, ())
            .context("zwlr_layer_shell_v1")?;
        let vptr = globals
            .bind(&qh, 1..=2, ())
            .ok()
            .map(|mgr: ZwlrVirtualPointerManagerV1| mgr.create_virtual_pointer(None, &qh, ()));
        Ok(WlrBackend {
            conn,
            compositor,
            shm,
            layer_shell,
            vptr,
            windows: Vec::new(),
            monitors: vec![Monitor {
                name: "WL-1".into(),
                x: 0,
                y: 0,
                w: crate::config::FALLBACK_WIDTH,
                h: crate::config::FALLBACK_HEIGHT,
            }],
            shm_ptr: None,
            shm_len: 0,
            shm_fd: None,
            shm_pool: None,
        })
    }
}

impl OverlayBackend for WlrBackend {
    #[allow(clippy::unnecessary_wraps)]
    fn named_monitors(&self) -> Result<Vec<Monitor>> {
        Ok(self.monitors.clone())
    }

    fn add_window(&mut self, _x: i32, _y: i32, w: u16, h: u16) -> Result<usize> {
        let mut eq = self.conn.new_event_queue::<WlrBackend>();
        let qh = eq.handle();
        let surface = self.compositor.create_surface(&qh, ());
        let layer_surface = self.layer_shell.get_layer_surface(
            &surface,
            None,
            zwlr_layer_shell_v1::Layer::Overlay,
            crate::config::WLR_NAME.into(),
            &qh,
            (),
        );
        layer_surface.set_anchor(
            zwlr_layer_surface_v1::Anchor::Top
                | zwlr_layer_surface_v1::Anchor::Bottom
                | zwlr_layer_surface_v1::Anchor::Left
                | zwlr_layer_surface_v1::Anchor::Right,
        );
        layer_surface.set_size(u32::from(w), u32::from(h));
        layer_surface.set_exclusive_zone(-1);
        layer_surface
            .set_keyboard_interactivity(zwlr_layer_surface_v1::KeyboardInteractivity::None);
        // Click-through — empty input region
        let region = self.compositor.create_region(&qh, ());
        region.add(0, 0, 0, 0);
        surface.set_input_region(Some(&region));
        region.destroy();
        surface.commit();
        self.conn.flush()?;
        eq.roundtrip(self)?;
        surface.commit();
        self.conn.flush()?;
        let idx = self.windows.len();
        self.windows.push(LayerWin {
            surface,
            w: i32::from(w),
            h: i32::from(h),
            pool_off: 0,
        });
        self.ensure_shm_pool()?;
        Ok(idx)
    }

    fn upload(&self, idx: usize, skia: &SkiaPixmap) -> Result<()> {
        let win = &self.windows[idx];
        let stride = win.w as usize * 4;
        let src = skia.data();
        let dst = unsafe {
            std::slice::from_raw_parts_mut(
                self.shm_ptr.unwrap().as_ptr().add(win.pool_off),
                (win.h as usize) * stride,
            )
        };
        for row in 0..win.h as usize {
            let s = &src[row * stride..(row + 1) * stride];
            let d = &mut dst[row * stride..(row + 1) * stride];
            for i in (0..stride).step_by(4) {
                d[i] = s[i + 2];
                d[i + 1] = s[i + 1];
                d[i + 2] = s[i];
                d[i + 3] = s[i + 3];
            }
        }
        let pool = self.shm_pool.as_ref().context("no shm pool")?;
        let eq = self.conn.new_event_queue::<WlrBackend>();
        let qh = eq.handle();
        let buf = pool.create_buffer(
            win.pool_off as i32,
            win.w,
            win.h,
            stride as i32,
            wl_shm::Format::Argb8888,
            &qh,
            (),
        );
        win.surface.attach(Some(&buf), 0, 0);
        win.surface.damage_buffer(0, 0, win.w, win.h);
        win.surface.commit();
        self.conn.flush()?;
        Ok(())
    }

    fn show_all(&self) -> Result<()> {
        self.conn.flush()?;
        Ok(())
    }
    // Signatures must match `x11::X11Backend` for the `delegate!` dispatch.
    #[allow(clippy::unused_self, clippy::unnecessary_wraps)]
    fn redraw_all(&self) -> Result<()> {
        Ok(())
    }
    fn pointer_warp(&self, x: i16, y: i16) -> Result<()> {
        let Some(ref v) = self.vptr else {
            return Ok(());
        };
        let sx = u32::from(self.monitors[0].w);
        let sy = u32::from(self.monitors[0].h);
        v.motion_absolute(0, x as u32, y as u32, sx, sy);
        v.frame();
        self.conn.flush()?;
        Ok(())
    }
}

impl WlrBackend {
    fn ensure_shm_pool(&mut self) -> Result<()> {
        if self.shm_fd.is_some() {
            return Ok(());
        }
        let stride = self.windows.iter().map(|w| w.w).max().unwrap_or(1) * 4;
        let size = self
            .windows
            .iter()
            .map(|w| w.h * stride)
            .sum::<i32>()
            .max(1) as u32;
        let fd = shm_fd(size)?;
        let ptr = unsafe {
            nix::sys::mman::mmap(
                None,
                std::num::NonZeroUsize::new(size as usize).unwrap(),
                nix::sys::mman::ProtFlags::PROT_READ | nix::sys::mman::ProtFlags::PROT_WRITE,
                nix::sys::mman::MapFlags::MAP_SHARED,
                &fd,
                0,
            )
        }
        .context("mmap SHM failed")?;
        self.shm_ptr = Some(ptr.cast::<u8>());
        self.shm_len = size as usize;
        let eq = self.conn.new_event_queue::<WlrBackend>();
        let qh = eq.handle();
        let pool = self.shm.create_pool(fd.as_fd(), size as i32, &qh, ());
        let mut off = 0i32;
        for win in &mut self.windows {
            let s = win.w * 4;
            let buf = pool.create_buffer(off, win.w, win.h, s, wl_shm::Format::Argb8888, &qh, ());
            win.surface.attach(Some(&buf), 0, 0);
            win.pool_off = off as usize;
            off += win.h * s;
        }
        self.shm_fd = Some(fd);
        self.shm_pool = Some(pool);
        Ok(())
    }
}

impl Drop for WlrBackend {
    fn drop(&mut self) {
        if let Some(ptr) = self.shm_ptr {
            let _ = unsafe { nix::sys::mman::munmap(ptr.cast(), self.shm_len) };
        }
    }
}

fn shm_fd(size: u32) -> Result<OwnedFd> {
    use nix::sys::memfd::MFdFlags;
    let fd = nix::sys::memfd::memfd_create(crate::config::SHM_PREFIX, MFdFlags::MFD_CLOEXEC)
        .context("memfd_create")?;
    nix::unistd::ftruncate(&fd, i64::from(size)).context("ftruncate")?;
    Ok(fd)
}
