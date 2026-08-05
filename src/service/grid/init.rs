// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::{Context, Result};

use super::GridConfig;
use super::state::{DrawState, init_overlay};
use super::watchdog::{display_session_uid, setup_display_env};
use crate::debug;
use crate::overlay::Overlay;
use crate::render::TextCache;
use crate::uinput::Mouse;

// ── Grid 状态阶段 ──────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GridPhase {
    Selecting,
    Navigating,
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
    pub(crate) ctx: &'a mut Option<super::state::GridCtx>,
    pub(crate) mouse: &'a mut Option<Mouse>,
}

// ── Grid 初始化 ────────────────────────────────────────────────────

pub(crate) fn connect_as_user() -> Result<Overlay> {
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
    let single = vec![monitors[idx]];
    let mut overlay = connect_as_user()?;
    let (cfg, font_size, cache, draw_states) = init_overlay(&mut overlay, &single)?;

    Ok(GridState {
        overlay,
        mouse: mouse_for_monitors(monitors),
        cfg,
        cache,
        font_size,
        draw_states,
    })
}
