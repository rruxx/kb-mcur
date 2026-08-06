// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::Result;
use log::{info, warn};

use super::init::enter_grid;
use super::selection::{redraw_select_hint, show_selection};
use super::state::{GridCtx, GridPhase, GridState, GridStateMut, MonitorList, init_grid_monitor};
use crate::device::pointer::KeyboardOut;
use crate::keymap::{KEY_CAPSLOCK, KEY_LEFTMETA, KEY_RIGHTMETA, ModState};
use crate::overlay::{Monitor, Overlay};

// ── Grid session state ──────────────────────────────────────────────

pub struct GridEnv {
    active: bool,
    phase: GridPhase,
    /// Multi-monitor selection hint (only during the `Selecting` phase).
    sel_overlay: Option<Overlay>,
    /// The active grid session, held whole.
    state: Option<GridState>,
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
            sel_overlay: None,
            state: None,
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
    /// Returns `Ok(true)` if the event was consumed.
    pub fn toggle(
        &mut self,
        code: u16,
        _value: i32,
        is_press: bool,
        mods: &ModState,
        kbd: &mut dyn KeyboardOut,
    ) -> Result<bool> {
        if code != KEY_CAPSLOCK || !is_press || !mods.meta || mods.shift || mods.ctrl {
            return Ok(false);
        }
        if self.active() {
            self.active = false;
            self.sel_overlay = None;
            self.state = None;
            self.ctx = None;
            warn!("[grid OFF]");
        } else {
            match enter_grid() {
                Ok((init_overlay_conn, monitors_list)) => {
                    self.monitors = monitors_list;
                    self.monitor_idx = 0;
                    self.active = true;

                    if self.monitors.len() > 1 {
                        self.sel_overlay = None;
                        self.select_hint.clear();
                        if let Err(e) = show_selection(&mut self.sel_overlay, &self.monitors) {
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
                        self.state = Some(state);
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
            kbd.key(key, 0)?;
        }
        kbd.sync()?;
        Ok(true)
    }

    /// Re-check the active monitor's logical size; re-render the grid or the
    /// selection hint if the resolution/scale changed.
    pub fn poll_resize(&mut self) -> Result<()> {
        if !self.active {
            return Ok(());
        }
        if self.phase == GridPhase::Selecting {
            self.poll_selection_resize()
        } else {
            self.poll_grid_resize()
        }
    }

    fn poll_selection_resize(&mut self) -> Result<()> {
        let cur = self
            .sel_overlay
            .as_ref()
            .and_then(|o| o.named_monitors().ok())
            .map_or((0, 0, 0, 0), |m| Monitor::bbox(&m));
        let monitors = {
            let Some(sel) = self.sel_overlay.as_mut() else {
                return Ok(());
            };
            sel.refresh_monitors()?
        };
        if Monitor::bbox(&monitors) == cur {
            return Ok(());
        }
        self.monitors = monitors;
        let hint = self.select_hint.clone();
        let mut new_sel = None;
        if show_selection(&mut new_sel, &self.monitors).is_ok() {
            self.sel_overlay = new_sel;
            if !hint.is_empty()
                && let Some(o) = self.sel_overlay.as_mut()
            {
                let _ = redraw_select_hint(o, &self.monitors, &hint);
            }
        }
        Ok(())
    }

    fn poll_grid_resize(&mut self) -> Result<()> {
        let cur = self
            .state
            .as_ref()
            .and_then(|s| s.draw_states.first())
            .map(|ds| (ds.pixmap.width(), ds.pixmap.height()));
        let monitors = {
            let Some(s) = self.state.as_mut() else {
                return Ok(());
            };
            s.overlay.refresh_monitors()?
        };
        let new = monitors
            .get(self.monitor_idx)
            .map(|m| (u32::from(m.w), u32::from(m.h)));
        if cur == new {
            return Ok(());
        }
        self.monitors = monitors;
        if let Ok(s) = init_grid_monitor(self.monitor_idx, &self.monitors, None) {
            self.state = Some(s);
            self.ctx = Some(GridCtx::new());
            info!("[grid] resized — re-rendered");
        }
        Ok(())
    }

    /// Dispatch one grid key event. Returns `true` if it was consumed.
    pub fn handle_input(&mut self, code: u16, value: i32, mods: &ModState) -> bool {
        if !self.active || value == 0 {
            return false;
        }
        let mut state = GridStateMut::new(&mut self.sel_overlay, &mut self.state, &mut self.ctx);
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
