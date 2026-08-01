// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::ffi::CString;

use anyhow::{Context, Result};
use log::{info, warn};
use tiny_skia::{Color, Paint, PathBuilder, Pixmap, PremultipliedColorU8, Shader, Transform};

use crate::debug;
use super::GridConfig;
use crate::keymap::{KEY_TAB, ModState, map as key_map};
use crate::overlay::Overlay;
use crate::render::TextCache;
use crate::uinput::Mouse;
use super::input::{DrawState, FONT_DATA, GridCtx, init_overlay, process_byte};

// ── Grid 状态阶段 ──────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GridPhase {
    Selecting,
    Navigating,
}

// ── Watchdog ─────────────────────────────────────────────────────

fn display_session_uid() -> Option<u32> {
    if let Ok(dir) = std::fs::read_dir("/run/user") {
        for entry in dir.flatten() {
            let uid_str = entry.file_name().to_string_lossy().into_owned();
            let uid: u32 = uid_str.parse().ok()?;
            for wn in ["wayland-0", "wayland-1"] {
                if entry.path().join(wn).exists() {
                    return Some(uid);
                }
            }
        }
    }
    let path = CString::new("/tmp/.X11-unix/X0").unwrap();
    if let Ok(st) = nix::sys::stat::stat(path.as_c_str())
        && st.st_uid != 0
    {
        return Some(st.st_uid);
    }
    None
}

fn setup_display_env(uid: u32) {
    let run_user = format!("/run/user/{uid}");

    for wn in ["wayland-1", "wayland-0"] {
        if std::path::Path::new(&format!("{run_user}/{wn}")).exists() {
            unsafe {
                std::env::set_var("WAYLAND_DISPLAY", wn);
                std::env::set_var("XDG_RUNTIME_DIR", &run_user);
            }
            return;
        }
    }

    let home = nix::unistd::User::from_uid(nix::unistd::Uid::from_raw(uid))
        .ok()
        .flatten()
        .map_or_else(
            || format!("/home/{uid}"),
            |u| u.dir.to_string_lossy().into_owned(),
        );
    unsafe {
        std::env::set_var("DISPLAY", ":0");
        std::env::set_var("HOME", &home);
    }
}

pub fn watchdog() {
    let Some(session_uid) = display_session_uid() else {
        return;
    };

    let Ok(dir) = std::fs::read_dir("/sys/class/input/") else {
        return;
    };
    for entry in dir.flatten() {
        let ev_name = entry.file_name().to_string_lossy().into_owned();
        if !ev_name.starts_with("event") {
            continue;
        }

        let name_path = entry.path().join("device/name");
        let Ok(dev_name) = std::fs::read_to_string(&name_path) else {
            continue;
        };
        if !dev_name.trim().starts_with(crate::config::UINPUT_NAME) {
            continue;
        }

        let dev_path = format!("/dev/input/{ev_name}");
        let Ok(path_c) = CString::new(dev_path) else {
            continue;
        };
        let Ok(st) = nix::sys::stat::stat(path_c.as_c_str()) else {
            continue;
        };
        if st.st_uid != session_uid {
            let _ = nix::unistd::chown(
                path_c.as_c_str(),
                Some(nix::unistd::Uid::from_raw(session_uid)),
                Some(nix::unistd::Gid::from_raw(st.st_gid)),
            );
        }
    }
}

// ── Grid 状态 ────────────────────────────────────────────────────

pub(crate) struct GridState {
    pub(crate) overlay: Overlay,
    pub(crate) mouse: Option<Mouse>,
    pub(crate) cfg: GridConfig,
    pub(crate) cache: TextCache,
    pub(crate) font_size: f32,
    pub(crate) draw_states: Vec<DrawState>,
}

pub type MonitorList = Vec<(i32, i32, u16, u16)>;

pub(crate) struct GridStateMut<'a> {
    pub(crate) overlay: &'a mut Option<Overlay>,
    pub(crate) cfg: &'a mut Option<GridConfig>,
    pub(crate) cache: &'a mut Option<TextCache>,
    pub(crate) font_size: &'a mut f32,
    pub(crate) states: &'a mut Option<Vec<DrawState>>,
    pub(crate) ctx: &'a mut Option<GridCtx>,
    pub(crate) mouse: &'a mut Option<Mouse>,
}

// ── Grid 初始化 ────────────────────────────────────────────────────

fn connect_as_user() -> Result<Overlay> {
    let Some(session_uid) = display_session_uid() else {
        anyhow::bail!("no display session detected");
    };
    setup_display_env(session_uid);

    let saved = nix::unistd::geteuid();
    nix::unistd::seteuid(nix::unistd::Uid::from_raw(session_uid)).context("seteuid")?;
    let result = Overlay::connect();
    let _ = nix::unistd::seteuid(saved);
    result
}

fn mouse_for_monitors(monitors: &[(i32, i32, u16, u16)]) -> Option<Mouse> {
    use crate::config::FALLBACK_WIDTH;
    let max_w = monitors
        .iter()
        .map(|m| m.0 + i32::from(m.2))
        .max()
        .unwrap_or(i32::from(FALLBACK_WIDTH)) as u16;
    let max_h = monitors
        .iter()
        .map(|m| m.1 + i32::from(m.3))
        .max()
        .unwrap_or(i32::from(crate::config::FALLBACK_HEIGHT)) as u16;
    Mouse::new(max_w, max_h).ok()
}

pub fn enter_grid() -> Result<(Overlay, MonitorList, Option<Mouse>)> {
    let overlay = connect_as_user()?;
    let named = overlay
        .named_monitors()
        .context("failed to query monitors")?;
    if named.is_empty() {
        anyhow::bail!("no active monitors detected");
    }
    let monitors: Vec<(i32, i32, u16, u16)> =
        debug::clone_monitors(named.iter().map(|n| (n.1, n.2, n.3, n.4)).collect());

    let m = mouse_for_monitors(&monitors);
    Ok((overlay, monitors, m))
}

pub(crate) fn init_grid_monitor(
    idx: usize,
    monitors: &[(i32, i32, u16, u16)],
) -> Result<GridState> {
    let font = fontdue::Font::from_bytes(FONT_DATA, fontdue::FontSettings::default())
        .map_err(|e| anyhow::anyhow!("failed to parse embedded font: {e}"))?;

    let single = vec![monitors[idx]];
    let mut overlay = connect_as_user()?;
    let (cfg, font_size, cache, draw_states) = init_overlay(&mut overlay, &font, &single)?;

    Ok(GridState {
        overlay,
        mouse: mouse_for_monitors(monitors),
        cfg,
        cache,
        font_size,
        draw_states,
    })
}

// ── 多屏选屏 ──────────────────────────────────────────────────────

pub fn show_selection(
    overlay: &mut Option<Overlay>,
    monitors: &[(i32, i32, u16, u16)],
) -> Result<()> {
    let bbox_x = monitors.iter().map(|m| m.0).min().unwrap_or(0);
    let bbox_y = monitors.iter().map(|m| m.1).min().unwrap_or(0);
    let bbox_w = monitors.iter().map(|m| m.0 + m.2 as i32).max().unwrap_or(0) - bbox_x;
    let bbox_h = monitors.iter().map(|m| m.1 + m.3 as i32).max().unwrap_or(0) - bbox_y;

    let mut new_overlay = connect_as_user()?;
    new_overlay.add_window(bbox_x, bbox_y, bbox_w as u16, bbox_h as u16)?;
    new_overlay.show_all()?;
    redraw_select_hint(&mut new_overlay, monitors, "")?;
    *overlay = Some(new_overlay);
    Ok(())
}

fn redraw_select_hint(
    overlay: &mut Overlay,
    monitors: &[(i32, i32, u16, u16)],
    hint: &str,
) -> Result<()> {
    let bbox_x = monitors.iter().map(|m| m.0).min().unwrap_or(0);
    let bbox_y = monitors.iter().map(|m| m.1).min().unwrap_or(0);
    let bbox_w = monitors.iter().map(|m| m.0 + m.2 as i32).max().unwrap_or(0) - bbox_x;
    let bbox_h = monitors.iter().map(|m| m.1 + m.3 as i32).max().unwrap_or(0) - bbox_y;

    let mut pixmap = Pixmap::new(bbox_w as u32, bbox_h as u32).context("pixmap")?;
    pixmap
        .pixels_mut()
        .fill(PremultipliedColorU8::from_rgba(0, 0, 0, 0).unwrap());

    let font = fontdue::Font::from_bytes(FONT_DATA, fontdue::FontSettings::default())
        .map_err(|e| anyhow::anyhow!("font: {e}"))?;
    let font_size = 128.0;
    let cache = TextCache::new(&font, font_size);

    let bg = Color::from_rgba8(0, 0, 0, 144);
    let paint = Paint {
        shader: Shader::SolidColor(bg),
        anti_alias: true,
        ..Default::default()
    };
    let pw = font_size * 1.8;
    let ph = font_size * 1.8;

    for (i, &(mx, my, mw, mh)) in monitors.iter().enumerate() {
        let label = format!("{}", (b'a' + i as u8) as char);
        if !hint.is_empty() && !label.starts_with(hint) {
            continue;
        }
        let cx = (mx - bbox_x) as f32 + mw as f32 * 0.5;
        let cy = (my - bbox_y) as f32 + mh as f32 * 0.5;
        let x = cx - pw * 0.5;
        let y = cy - ph * 0.5;

        let mut pb = PathBuilder::new();
        pb.push_oval(tiny_skia::Rect::from_xywh(x, y, pw, ph).unwrap());
        let oval = pb.finish().unwrap();
        pixmap.fill_path(
            &oval,
            &paint,
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );

        crate::render::draw_text(
            &mut pixmap,
            &label,
            cx,
            cy,
            &cache,
            font_size,
            [192, 255, 192, 192],
        );
    }

    overlay.upload(0, &pixmap)?;
    overlay.redraw_all()?;
    Ok(())
}

// ── Grid 事件处理 ─────────────────────────────────────────────────

pub(crate) fn handle_selecting(
    code: u16,
    state: GridStateMut<'_>,
    grid_monitor_idx: &mut usize,
    grid_phase: &mut GridPhase,
    monitors: &MonitorList,
    mods: &ModState,
    select_hint: &mut String,
) {
    let byte = key_map(code, mods);
    if let Some(b) = byte
        && b.is_ascii_lowercase()
    {
        let idx = (b - b'a') as usize;
        if idx < monitors.len() {
            *grid_monitor_idx = idx;
            *state.overlay = None;
            if let Ok(s) = init_grid_monitor(*grid_monitor_idx, monitors) {
                *state.overlay = Some(s.overlay);
                *state.mouse = s.mouse;
                *state.cfg = Some(s.cfg);
                *state.cache = Some(s.cache);
                *state.font_size = s.font_size;
                *state.states = Some(s.draw_states);
                *state.ctx = Some(GridCtx::new());
                *grid_phase = GridPhase::Navigating;
                info!("[grid] selected monitor {}", *grid_monitor_idx + 1);
            }
        } else {
            *select_hint = format!("{}", b as char);
            if let Some(o) = state.overlay.as_mut() {
                let _ = redraw_select_hint(o, monitors, select_hint);
            }
        }
    }
    if let Some(o) = state.overlay.as_mut()
        && b'\x1b' == byte.unwrap_or(0)
    {
        select_hint.clear();
        let _ = redraw_select_hint(o, monitors, "");
    }
}

pub(crate) fn handle_navigating(
    code: u16,
    state: GridStateMut<'_>,
    monitors: &MonitorList,
    grid_monitor_idx: &mut usize,
    mods: &ModState,
    grid_phase: GridPhase,
) {
    if code == KEY_TAB && monitors.len() > 1 {
        *grid_monitor_idx = (*grid_monitor_idx + 1) % monitors.len();
        *state.overlay = None;
        if let Ok(s) = init_grid_monitor(*grid_monitor_idx, monitors) {
            *state.overlay = Some(s.overlay);
            *state.mouse = s.mouse;
            *state.cfg = Some(s.cfg);
            *state.cache = Some(s.cache);
            *state.font_size = s.font_size;
            *state.states = Some(s.draw_states);
            *state.ctx = Some(GridCtx::new());
            info!(
                "[grid] monitor {}/{}",
                *grid_monitor_idx + 1,
                monitors.len()
            );
        }
        return;
    }

    if grid_phase == GridPhase::Navigating {
        let byte = key_map(code, mods);
        if let Some(b) = byte
            && let (Some(o), Some(gcfg), Some(gcache), Some(gstates), Some(gctx)) = (
                state.overlay.as_mut(),
                state.cfg.as_mut(),
                state.cache.as_mut(),
                state.states.as_mut(),
                state.ctx.as_mut(),
            )
            && let Err(e) = process_byte(
                b,
                o,
                state.mouse,
                gcfg,
                gcache,
                *state.font_size,
                gstates,
                gctx,
            )
        {
            warn!("[grid] error: {e}");
        }
    }
}
