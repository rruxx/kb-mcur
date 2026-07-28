// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

//! wlr-layer-shell overlay backend for wlroots-based Wayland compositors.

use std::os::fd::{AsFd, AsRawFd, FromRawFd, OwnedFd};
use std::ptr::NonNull;

use anyhow::{Context, Result};
use tiny_skia::Pixmap as SkiaPixmap;
use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{
        wl_compositor::WlCompositor,
        wl_output::WlOutput,
        wl_pointer,
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

type MonitorInfo = (String, i32, i32, u16, u16);

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
    monitors: Vec<MonitorInfo>,
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
        let vptr_mgr: ZwlrVirtualPointerManagerV1 = globals
            .bind(&qh, 1..=2, ())
            .context("zwlr_virtual_pointer_manager_v1")?;
        let vptr = vptr_mgr.create_virtual_pointer(None, &qh, ());
        Ok(WlrBackend {
            conn,
            compositor,
            shm,
            layer_shell,
            vptr: Some(vptr),
            windows: Vec::new(),
            monitors: vec![(
                "WL-1".into(),
                0,
                0,
                crate::config::FALLBACK_WIDTH,
                crate::config::FALLBACK_HEIGHT,
            )],
            shm_ptr: None,
            shm_len: 0,
            shm_fd: None,
            shm_pool: None,
        })
    }

    pub fn monitors(&self) -> Result<Vec<(i32, i32, u16, u16)>> {
        Ok(self.monitors.iter().map(|m| (m.1, m.2, m.3, m.4)).collect())
    }
    pub fn named_monitors(&self) -> Result<Vec<MonitorInfo>> {
        Ok(self.monitors.clone())
    }

    pub fn add_window(&mut self, _x: i32, _y: i32, w: u16, h: u16) -> Result<usize> {
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

    pub fn upload(&self, idx: usize, skia: &SkiaPixmap) -> Result<()> {
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

    pub fn show_all(&self) -> Result<()> {
        self.conn.flush()?;
        Ok(())
    }
    pub fn redraw_all(&self) -> Result<()> {
        Ok(())
    }
    pub fn wait_or_timeout(&self, s: u64) -> Result<()> {
        std::thread::sleep(std::time::Duration::from_secs(s));
        Ok(())
    }
    #[must_use]
    pub fn poll_fd(&self) -> Option<i32> {
        Some(self.conn.backend().poll_fd().as_raw_fd())
    }
    pub fn dispatch_pending(&self) -> Result<()> {
        Ok(())
    }

    pub fn pointer_warp(&self, x: i16, y: i16) -> Result<()> {
        let Some(ref v) = self.vptr else {
            return Ok(());
        };
        let sx = u32::from(self.monitors[0].3);
        let sy = u32::from(self.monitors[0].4);
        v.motion_absolute(0, x as u32, y as u32, sx, sy);
        v.frame();
        self.conn.flush()?;
        Ok(())
    }

    pub fn pointer_click(&self, button: u8, count: u32) -> Result<()> {
        let Some(ref v) = self.vptr else {
            return Ok(());
        };
        let code = crate::config::btn_code(button);
        for _ in 0..count {
            v.button(0, code.into(), wl_pointer::ButtonState::Pressed);
            v.frame();
            self.conn.flush()?;
            std::thread::sleep(std::time::Duration::from_millis(50));
            v.button(0, code.into(), wl_pointer::ButtonState::Released);
            v.frame();
            self.conn.flush()?;
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        Ok(())
    }

    pub fn pointer_toggle(&self, button: u8) -> Result<()> {
        let Some(ref v) = self.vptr else {
            return Ok(());
        };
        let code = crate::config::btn_code(button);
        v.button(0, code.into(), wl_pointer::ButtonState::Pressed);
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
    use std::fs::OpenOptions;
    use std::os::fd::IntoRawFd;
    use std::os::unix::fs::OpenOptionsExt;
    let path = std::env::temp_dir().join(format!(
        "{}-{}",
        crate::config::SHM_PREFIX,
        std::process::id()
    ));
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .custom_flags(libc::O_CLOEXEC | libc::O_RDWR)
        .open(&path)
        .context("shm temp file")?;
    file.set_len(u64::from(size))?;
    let raw = file.into_raw_fd();
    let _ = nix::unistd::unlink(&path);
    Ok(unsafe { OwnedFd::from_raw_fd(raw) })
}
