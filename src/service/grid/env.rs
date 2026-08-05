// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::Result;
use log::{info, warn};

use super::GridConfig;
use super::init::enter_grid;
use super::selection::show_selection;
use super::state::DrawState;
use super::state::{GridCtx, GridPhase, GridStateMut, MonitorList, init_grid_monitor};
use crate::device::abi::{EV_KEY, EV_SYN, SYN_REPORT, write_event};
use crate::device::uinput::Mouse;
use crate::keymap::{KEY_CAPSLOCK, KEY_LEFTMETA, KEY_RIGHTMETA, ModState};
use crate::overlay::Overlay;
use crate::render::TextCache;

// ── Grid session state ──────────────────────────────────────────────

pub struct GridEnv {
    active: bool,
    phase: GridPhase,
    overlay: Option<Overlay>,
    mouse: Option<Mouse>,
    cfg: Option<GridConfig>,
    cache: Option<TextCache>,
    font_size: f32,
    states: Option<Vec<DrawState>>,
    ctx: Option<GridCtx>,
    monitors: MonitorList,
    monitor_idx: usize,
    select_hint: String,
}

impl Default for GridEnv {
    fn default() -> Self {
        Self::new()
    }
}

impl GridEnv {
    #[must_use]
    pub fn new() -> Self {
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

    #[must_use]
    pub fn active(&self) -> bool {
        self.active
    }

    /// Toggle grid mode on/off via CapsLock+Meta.
    /// Returns `Ok(true)` if the key was consumed.
    pub fn toggle(
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
                Ok((init_overlay_conn, monitors_list)) => {
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
                    } else if let Ok(state) =
                        init_grid_monitor(0, &self.monitors, Some(init_overlay_conn))
                    {
                        self.overlay = Some(state.overlay);
                        self.mouse = state.mouse;
                        self.cfg = Some(state.cfg);
                        self.cache = Some(state.cache);
                        self.font_size = state.font_size;
                        self.states = Some(state.draw_states);
                        self.ctx = Some(GridCtx::new());
                        self.phase = GridPhase::Navigating;
                        warn!("[grid ON]");
                    } else {
                        self.active = false;
                        warn!("[grid] init failed");
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

    /// Dispatch one grid key event. Returns `true` if it was consumed.
    pub fn handle_input(&mut self, code: u16, value: i32, mods: &ModState) -> bool {
        if !self.active || value == 0 {
            return false;
        }
        let mut state = GridStateMut::new(
            &mut self.overlay,
            &mut self.cfg,
            &mut self.cache,
            &mut self.font_size,
            &mut self.states,
            &mut self.ctx,
            &mut self.mouse,
        );
        if self.phase == GridPhase::Selecting {
            state.handle_selecting(
                code,
                &mut self.monitor_idx,
                &mut self.phase,
                &self.monitors,
                mods,
                &mut self.select_hint,
            );
        } else {
            state.handle_navigating(
                code,
                &self.monitors,
                &mut self.monitor_idx,
                mods,
                self.phase,
            );
        }
        true
    }
}
