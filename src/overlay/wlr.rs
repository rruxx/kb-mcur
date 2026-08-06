// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

//! wlr-layer-shell overlay backend for wlroots-based Wayland compositors.

use std::collections::HashMap;
use std::os::fd::{AsFd, OwnedFd};
use std::ptr::NonNull;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use log::info;
use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
use tiny_skia::Pixmap as SkiaPixmap;
use wayland_client::{
    Connection, Dispatch, EventQueue, Proxy, QueueHandle,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{
        wl_compositor::WlCompositor,
        wl_output::{self, WlOutput},
        wl_pointer::{self, WlPointer},
        wl_registry::WlRegistry,
        wl_seat::{self, WlSeat},
        wl_shm::{self, WlShm},
        wl_shm_pool::WlShmPool,
        wl_surface::WlSurface,
    },
};
use wayland_protocols::xdg::xdg_output::zv1::client::{
    zxdg_output_manager_v1::ZxdgOutputManagerV1,
    zxdg_output_v1::{self, ZxdgOutputV1},
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
    /// `wl_output` geometry, collected while connecting: (x, y, w, h).
    /// w/h are overwritten by the `xdg-output` logical size when available,
    /// otherwise they keep the physical `Mode` size.
    outputs: HashMap<WlOutput, (i32, i32, i32, i32)>,
    /// Output bind order, kept so monitor names stay stable across refreshes.
    output_list: Vec<WlOutput>,
    /// Maps each `xdg_output` proxy back to its `wl_output`.
    xdg_map: HashMap<ZxdgOutputV1, WlOutput>,
    /// The connection's event queue — held so `refresh_monitors` can re-dispatch
    /// `wl_output` / xdg-output events on the proxies bound at connect time.
    queue: Option<EventQueue<WlrBackend>>,
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
dispatch_stub!(WlCompositor);
dispatch_stub!(WlShm);
dispatch_stub!(ZwlrLayerShellV1);
dispatch_stub!(WlSurface);
dispatch_stub!(WlShmPool);
dispatch_stub!(wayland_client::protocol::wl_buffer::WlBuffer);
dispatch_stub!(wayland_client::protocol::wl_region::WlRegion);
dispatch_stub!(ZwlrVirtualPointerV1);
dispatch_stub!(ZwlrVirtualPointerManagerV1);
dispatch_stub!(ZxdgOutputManagerV1);

impl Dispatch<ZxdgOutputV1, ()> for WlrBackend {
    fn event(
        state: &mut Self,
        proxy: &ZxdgOutputV1,
        event: <ZxdgOutputV1 as Proxy>::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // The logical size is the exact desktop pixel size before scaling;
        // layer surfaces are sized in these logical coordinates.
        if let zxdg_output_v1::Event::LogicalSize { width, height } = event
            && let Some(out) = state.xdg_map.get(proxy)
        {
            let entry = state.outputs.entry(out.clone()).or_insert((0, 0, 0, 0));
            entry.2 = width;
            entry.3 = height;
        }
    }
}

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

impl Dispatch<WlOutput, ()> for WlrBackend {
    fn event(
        state: &mut Self,
        proxy: &WlOutput,
        event: <WlOutput as Proxy>::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            // Geometry carries the global origin (logical) and physical size in
            // millimetres — the pixel size arrives via `Mode`.
            wl_output::Event::Geometry { x, y, .. } => {
                let (ox, oy, _, _) = state.outputs.entry(proxy.clone()).or_insert((0, 0, 0, 0));
                *ox = x;
                *oy = y;
            }
            wl_output::Event::Mode {
                flags,
                width,
                height,
                ..
            } if flags
                .into_result()
                .is_ok_and(|f| f.contains(wl_output::Mode::Current)) =>
            {
                let (_, _, w, h) = state.outputs.entry(proxy.clone()).or_insert((0, 0, 0, 0));
                *w = width;
                *h = height;
            }
            _ => {}
        }
    }
}

impl WlrBackend {
    pub fn connect() -> Result<Self> {
        let conn = Connection::connect_to_env().context("Wayland connection")?;
        let (globals, mut eq) = registry_queue_init::<WlrBackend>(&conn)?;
        let qh = eq.handle();
        let compositor: WlCompositor = globals.bind(&qh, 4..=6, ()).context("wl_compositor")?;
        let shm: WlShm = globals.bind(&qh, 1..=1, ()).context("wl_shm")?;
        let layer_shell: ZwlrLayerShellV1 = globals
            .bind(&qh, 1..=5, ())
            .context("zwlr_layer_shell_v1")?;
        let vptr = globals
            .bind(&qh, 1..=2, ())
            .ok()
            .map(|mgr: ZwlrVirtualPointerManagerV1| mgr.create_virtual_pointer(None, &qh, ()));
        let outputs: Vec<WlOutput> = globals
            .contents()
            .clone_list()
            .into_iter()
            .filter(|g| g.interface == "wl_output")
            .map(|g| globals.registry().bind(g.name, g.version.min(4), &qh, ()))
            .collect();

        // xdg-output reports exact logical sizes (desktop pixels before scaling);
        // without it we fall back to the physical `Mode` size.
        let mut xdg_map = HashMap::new();
        if let Ok(mgr) = globals.bind::<ZxdgOutputManagerV1, _, _>(&qh, 1..=3, ()) {
            for out in &outputs {
                let xo = mgr.get_xdg_output(out, &qh, ());
                xdg_map.insert(xo, out.clone());
            }
        }

        let mut backend = WlrBackend {
            conn,
            compositor,
            shm,
            layer_shell,
            vptr,
            windows: Vec::new(),
            monitors: Vec::new(),
            shm_ptr: None,
            shm_len: 0,
            shm_fd: None,
            shm_pool: None,
            outputs: HashMap::new(),
            output_list: outputs.clone(),
            xdg_map,
            queue: None,
        };
        // Collect `wl_output` geometry + xdg-output logical sizes.
        eq.roundtrip(&mut backend)?;
        backend.queue = Some(eq);
        backend.monitors = backend.assemble_monitors();
        info!("[overlay] monitors: {:?}", backend.monitors);
        Ok(backend)
    }

    /// Build the monitor list from the collected output geometry/sizes.
    fn assemble_monitors(&self) -> Vec<Monitor> {
        self.output_list
            .iter()
            .enumerate()
            .filter_map(|(i, out)| {
                let (x, y, w, h) = *self.outputs.get(out)?;
                Some(Monitor {
                    name: format!("WL-{}", i + 1),
                    x,
                    y,
                    w: w as u16,
                    h: h as u16,
                })
            })
            .collect()
    }
}

/// Screen size (bounding box over all outputs) for CLI use.
/// Uses the `wl_output` geometry/mode collected during connect; `None` when
/// no display is reachable.
#[must_use]
pub fn screen_size() -> Option<(u16, u16)> {
    let backend = WlrBackend::connect().ok()?;
    let m = backend.named_monitors().ok()?;
    if m.is_empty() {
        return None;
    }
    let (_, _, w, h) = Monitor::bbox(&m);
    Some((w, h))
}

impl OverlayBackend for WlrBackend {
    #[allow(clippy::unnecessary_wraps)]
    fn named_monitors(&self) -> Result<Vec<Monitor>> {
        Ok(self.monitors.clone())
    }

    fn refresh_monitors(&mut self) -> Result<Vec<Monitor>> {
        // Re-dispatch pending `wl_output` / xdg-output events on the queue the
        // proxies were bound to, so geometry/logical sizes reflect any
        // resolution/scale change.
        if let Some(mut q) = self.queue.take() {
            q.roundtrip(self)?;
            self.queue = Some(q);
        }
        self.monitors = self.assemble_monitors();
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
    fn pointer_warp(&self, x: i32, y: i32) -> Result<()> {
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

/// Query the global cursor position on any wlr-layer-shell compositor.
///
/// Wayland has no pointer-query request: map a full-screen layer surface on
/// every output, poke a zero-size `virtual_pointer` motion so the compositor
/// re-runs its hit test, and read the `enter` event's global position.
pub fn cursor_pos() -> Result<(i32, i32)> {
    let conn = Connection::connect_to_env().context("Wayland connection")?;
    let (globals, mut eq) = registry_queue_init::<CursorQuery>(&conn)?;
    let qh = eq.handle();

    let compositor: WlCompositor = globals.bind(&qh, 1..=4, ()).context("wl_compositor")?;
    let shm: WlShm = globals.bind(&qh, 1..=1, ()).context("wl_shm")?;
    let layer_shell: ZwlrLayerShellV1 = globals
        .bind(&qh, 1..=5, ())
        .context("zwlr_layer_shell_v1")?;
    let vpm: ZwlrVirtualPointerManagerV1 = globals
        .bind(&qh, 1..=2, ())
        .context("zwlr_virtual_pointer_manager_v1")?;
    let _seat: WlSeat = globals.bind(&qh, 1..=1, ()).context("wl_seat")?;
    let outputs: Vec<WlOutput> = globals
        .contents()
        .clone_list()
        .into_iter()
        .filter(|g| g.interface == "wl_output")
        .map(|g| globals.registry().bind(g.name, g.version.min(4), &qh, ()))
        .collect();
    if outputs.is_empty() {
        bail!("compositor exposes no wl_output");
    }

    let mut state = CursorQuery::new();
    eq.roundtrip(&mut state)?;
    if state.origins.is_empty() {
        bail!("no wl_output geometry received");
    }
    if state.pointer.is_none() {
        bail!("compositor seat has no pointer capability");
    }

    for (out, (ox, oy)) in state.origins.clone() {
        let surface = compositor.create_surface(&qh, ());
        let ls = layer_shell.get_layer_surface(
            &surface,
            Some(&out),
            zwlr_layer_shell_v1::Layer::Overlay,
            crate::config::WLR_NAME.into(),
            &qh,
            (),
        );
        ls.set_anchor(
            zwlr_layer_surface_v1::Anchor::Top
                | zwlr_layer_surface_v1::Anchor::Bottom
                | zwlr_layer_surface_v1::Anchor::Left
                | zwlr_layer_surface_v1::Anchor::Right,
        );
        ls.set_exclusive_zone(-1);
        ls.set_keyboard_interactivity(zwlr_layer_surface_v1::KeyboardInteractivity::None);
        surface.commit();
        state.surfaces.insert(
            surface,
            CursorSurface {
                origin: (ox, oy),
                w: 0,
                h: 0,
                layer: ls,
            },
        );
    }
    eq.roundtrip(&mut state)?;

    let vptr = vpm.create_virtual_pointer(None, &qh, ());
    let (_fd, pool) = shm_pool_for(&conn, &qh, &shm, &state)?;
    let mut off = 0;
    for (surface, cs) in &state.surfaces {
        let stride = cs.w * 4;
        let buf = pool.create_buffer(off, cs.w, cs.h, stride, wl_shm::Format::Argb8888, &qh, ());
        surface.attach(Some(&buf), 0, 0);
        surface.commit();
        off += cs.h * stride;
    }
    conn.flush()?;

    vptr.motion(0, 0.0, 0.0);
    vptr.frame();
    conn.flush()?;

    let start = Instant::now();
    loop {
        if let Some(pos) = state.result {
            return Ok(pos);
        }
        let Some(remaining) = CURSOR_TIMEOUT.checked_sub(start.elapsed()) else {
            bail!(
                "no pointer enter event (compositor may not route events to layer \
                 surfaces on every output)"
            );
        };
        if let Some(guard) = eq.prepare_read() {
            let backend = conn.backend();
            let mut fds = [PollFd::new(backend.poll_fd(), PollFlags::POLLIN)];
            poll(&mut fds, PollTimeout::from(remaining.as_millis() as u16))?;
            if fds[0]
                .revents()
                .is_some_and(|f| f.contains(PollFlags::POLLIN))
            {
                guard.read().context("wayland socket read")?;
            }
        }
        eq.dispatch_pending(&mut state)?;
    }
}

const CURSOR_TIMEOUT: Duration = Duration::from_secs(1);

struct CursorSurface {
    origin: (i32, i32),
    w: i32,
    h: i32,
    layer: ZwlrLayerSurfaceV1,
}

struct CursorQuery {
    origins: HashMap<WlOutput, (i32, i32)>,
    surfaces: HashMap<WlSurface, CursorSurface>,
    pointer: Option<WlPointer>,
    result: Option<(i32, i32)>,
}

impl CursorQuery {
    fn new() -> Self {
        Self {
            origins: HashMap::new(),
            surfaces: HashMap::new(),
            pointer: None,
            result: None,
        }
    }
}

macro_rules! cursor_dispatch_stub {
    ($ty:ty) => {
        impl Dispatch<$ty, ()> for CursorQuery {
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
impl Dispatch<WlRegistry, GlobalListContents> for CursorQuery {
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
cursor_dispatch_stub!(WlCompositor);
cursor_dispatch_stub!(WlShm);
cursor_dispatch_stub!(ZwlrLayerShellV1);
cursor_dispatch_stub!(WlSurface);
cursor_dispatch_stub!(WlShmPool);
cursor_dispatch_stub!(wayland_client::protocol::wl_buffer::WlBuffer);
cursor_dispatch_stub!(ZwlrVirtualPointerV1);
cursor_dispatch_stub!(ZwlrVirtualPointerManagerV1);

impl Dispatch<WlOutput, ()> for CursorQuery {
    fn event(
        state: &mut Self,
        proxy: &WlOutput,
        event: <WlOutput as Proxy>::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_output::Event::Geometry { x, y, .. } = event {
            state.origins.insert(proxy.clone(), (x, y));
        }
    }
}

impl Dispatch<WlSeat, ()> for CursorQuery {
    fn event(
        state: &mut Self,
        proxy: &WlSeat,
        event: <WlSeat as Proxy>::Event,
        (): &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities { capabilities } = event
            && capabilities
                .into_result()
                .is_ok_and(|c| c.contains(wl_seat::Capability::Pointer))
        {
            state.pointer = Some(proxy.get_pointer(qh, ()));
        }
    }
}

impl Dispatch<WlPointer, ()> for CursorQuery {
    fn event(
        state: &mut Self,
        _: &WlPointer,
        event: <WlPointer as Proxy>::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_pointer::Event::Enter {
            surface,
            surface_x,
            surface_y,
            ..
        } = event
            && let Some(cs) = state.surfaces.get(&surface)
        {
            state.result = Some((
                cs.origin.0 + surface_x.round() as i32,
                cs.origin.1 + surface_y.round() as i32,
            ));
        }
    }
}

impl Dispatch<ZwlrLayerSurfaceV1, ()> for CursorQuery {
    fn event(
        state: &mut Self,
        proxy: &ZwlrLayerSurfaceV1,
        event: <ZwlrLayerSurfaceV1 as Proxy>::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwlr_layer_surface_v1::Event::Configure {
            serial,
            width,
            height,
        } = event
        {
            proxy.ack_configure(serial);
            for cs in state.surfaces.values_mut() {
                if cs.layer == *proxy {
                    cs.w = width as i32;
                    cs.h = height as i32;
                }
            }
        }
    }
}

fn shm_pool_for(
    conn: &Connection,
    qh: &QueueHandle<CursorQuery>,
    shm: &WlShm,
    state: &CursorQuery,
) -> Result<(OwnedFd, WlShmPool)> {
    let size: u32 = state
        .surfaces
        .values()
        .map(|cs| (cs.w * cs.h * 4).max(1) as u32)
        .sum();
    let fd = shm_fd(size)?;
    let pool = shm.create_pool(fd.as_fd(), size as i32, qh, ());
    conn.flush()?;
    Ok((fd, pool))
}
