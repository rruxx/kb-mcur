// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::Result;
use log::{info, warn};

use super::GridConfig;
use super::init::{GridPhase, enter_grid, init_grid_monitor};
use super::selection::show_selection;
use super::state::{DrawState, GridCtx};
use crate::keymap::{KEY_CAPSLOCK, KEY_LEFTMETA, KEY_RIGHTMETA};
use crate::overlay::Overlay;
use crate::render::TextCache;
use crate::uinput::Mouse;
use crate::uinput::{EV_KEY, EV_SYN, SYN_REPORT, write_event};

// ── Grid state ───────────────────────────────────────────────────────

pub(crate) struct GridEnv {
    pub(crate) active: bool,
    pub(crate) phase: GridPhase,
    pub(crate) overlay: Option<Overlay>,
    pub(crate) mouse: Option<Mouse>,
    pub(crate) cfg: Option<GridConfig>,
    pub(crate) cache: Option<TextCache>,
    pub(crate) font_size: f32,
    pub(crate) states: Option<Vec<DrawState>>,
    pub(crate) ctx: Option<GridCtx>,
    pub(crate) monitors: super::init::MonitorList,
    pub(crate) monitor_idx: usize,
    pub(crate) select_hint: String,
}

impl GridEnv {
    pub(crate) fn new() -> Self {
        Self {
            active: false,
            phase: GridPhase::Navigating,
            overlay: None,
            mouse: None,
            cfg: None,
            cache: None,
            font_size: 0.0,
            states: None,
            ctx: None,
            monitors: Vec::new(),
            monitor_idx: 0,
            select_hint: String::new(),
        }
    }

    pub(crate) fn active(&self) -> bool {
        self.active
    }

    /// Toggle grid mode on/off via CapsLock+Meta.
    /// Returns `Ok(true)` if the key was consumed.
    pub(crate) fn toggle(
        &mut self,
        code: u16,
        is_press: bool,
        meta_held: bool,
        kbd_out: &mut std::fs::File,
    ) -> Result<bool> {
        if code != KEY_CAPSLOCK || !is_press || !meta_held {
            return Ok(false);
        }
        if self.active() {
            self.active = false;
            self.overlay = None;
            self.mouse = None;
            self.cfg = None;
            self.cache = None;
            self.states = None;
            self.ctx = None;
            warn!("[grid OFF]");
        } else {
            match enter_grid() {
                Ok((init_overlay_conn, monitors_list, init_mouse)) => {
                    self.monitors = monitors_list;
                    self.monitor_idx = 0;
                    self.active = true;

                    if self.monitors.len() > 1 {
                        self.overlay = None;
                        self.select_hint.clear();
                        if let Err(e) = show_selection(&mut self.overlay, &self.monitors) {
                            warn!("[grid] selection: {e}");
                            self.active = false;
                        } else {
                            self.phase = GridPhase::Selecting;
                            info!(
                                "[grid] select monitor (a-{})",
                                (b'a' + (self.monitors.len() - 1) as u8) as char
                            );
                        }
                    } else {
                        self.overlay = Some(init_overlay_conn);
                        self.mouse = init_mouse;
                        if let Ok(state) = init_grid_monitor(0, &self.monitors) {
                            self.overlay = Some(state.overlay);
                            self.mouse = state.mouse;
                            self.cfg = Some(state.cfg);
                            self.cache = Some(state.cache);
                            self.font_size = state.font_size;
                            self.states = Some(state.draw_states);
                        }
                        self.ctx = Some(GridCtx::new());
                        self.phase = GridPhase::Navigating;
                        warn!("[grid ON]");
                    }
                }
                Err(e) => warn!("[grid] init failed: {e}"),
            }
        }
        for key in [KEY_LEFTMETA, KEY_RIGHTMETA, KEY_CAPSLOCK] {
            write_event(kbd_out, EV_KEY, key, 0)?;
        }
        write_event(kbd_out, EV_SYN, SYN_REPORT, 0)?;
        Ok(true)
    }
}
