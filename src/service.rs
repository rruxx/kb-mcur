// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

pub mod dir;
pub mod glide_alpha;
pub mod glide_num;
#[cfg(target_os = "linux")]
pub mod grid;
#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "windows")]
pub mod windows;

use anyhow::Result;

use crate::device::pointer::{KeyboardOut, Pointer};
use crate::keymap::{
    KEY_CAPSLOCK, KEY_KPENTER, KEY_LEFTMETA, KEY_NUMLOCK, KEY_RIGHTMETA, ModState,
};
#[cfg(target_os = "linux")]
use crate::keymap::{KEY_TAB, key_map};
use crate::service::glide_alpha::GlideAlpha;
use crate::service::glide_num::GlideNum;
#[cfg(target_os = "linux")]
use crate::service::grid::GridEnv;

// ── Cross-mode service state ─────────────────────────────────────────

/// Shared service state driven by a platform main loop.
pub struct Service {
    glide_num: GlideNum,
    glide_alpha: GlideAlpha,
    #[cfg(target_os = "linux")]
    grid: GridEnv,
    mods: ModState,
    /// A held modifier (Meta/NumLock) whose key-down has not yet been forwarded.
    /// Consumed when a mode-toggle chord follows; replayed otherwise.
    pending: Option<u16>,
}

impl Service {
    #[must_use]
    pub fn new() -> Self {
        Self {
            glide_num: GlideNum::new(),
            glide_alpha: GlideAlpha::new(),
            #[cfg(target_os = "linux")]
            grid: GridEnv::new(),
            mods: ModState::default(),
            pending: None,
        }
    }

    /// Dispatch one `EV_KEY` event. Returns `Ok(true)` if consumed.
    pub fn dispatch(
        &mut self,
        code: u16,
        value: i32,
        ptr: &mut dyn Pointer,
        kbd: &mut dyn KeyboardOut,
    ) -> Result<bool> {
        let is_press = value > 0;
        self.mods.update(code, is_press);
        if code == KEY_NUMLOCK {
            self.glide_num.set_numlock(value != 0);
        }

        // Precisely match a mode-toggle chord so extra modifiers
        // (e.g. meta+ctrl+capslock) are not mistaken for one.
        let chord = is_press
            && match code {
                KEY_CAPSLOCK => self.mods.meta && !self.mods.ctrl && !self.mods.alt,
                KEY_KPENTER => {
                    self.glide_num.numlock_held()
                        && !self.mods.meta
                        && !self.mods.shift
                        && !self.mods.ctrl
                        && !self.mods.alt
                }
                _ => false,
            };
        if let Some(p) = self.pending.take()
            && !chord
        {
            kbd.key(p, 1)?;
            kbd.sync()?;
        }

        // Hold Meta/NumLock presses: forward only if no chord follows.
        if is_press && (code == KEY_LEFTMETA || code == KEY_RIGHTMETA || code == KEY_NUMLOCK) {
            self.pending = Some(code);
            return Ok(true);
        }

        if self
            .glide_num
            .toggle(code, value, is_press, &self.mods, kbd)?
        {
            return Ok(true);
        }
        if self
            .glide_alpha
            .toggle(code, value, is_press, &self.mods, kbd)?
        {
            return Ok(true);
        }
        #[cfg(target_os = "linux")]
        if self.grid.toggle(code, value, is_press, &self.mods, kbd)? {
            return Ok(true);
        }
        if glide_num_input(code, value, is_press, &mut self.glide_num, ptr)? {
            return Ok(true);
        }
        if glide_alpha_input(code, value, is_press, &mut self.glide_alpha, ptr)? {
            return Ok(true);
        }
        #[cfg(target_os = "linux")]
        if grid_input(code, value, &mut self.grid, &self.mods) {
            return Ok(true);
        }
        Ok(false)
    }

    /// One per-frame movement tick for both glide modes.
    pub fn direction_tick(&mut self, ptr: &mut dyn Pointer) -> Result<()> {
        self.glide_num.direction_tick(ptr)?;
        self.glide_alpha.direction_tick(ptr)
    }
}

impl Default for Service {
    fn default() -> Self {
        Self::new()
    }
}

// ── Input dispatch helpers ───────────────────────────────────────────

fn glide_num_input(
    code: u16,
    value: i32,
    is_press: bool,
    glide_num: &mut GlideNum,
    ptr: &mut dyn Pointer,
) -> Result<bool> {
    if !glide_num.active() {
        return Ok(false);
    }
    glide_num.handle_event(ptr, code, value, is_press)
}

fn glide_alpha_input(
    code: u16,
    value: i32,
    is_press: bool,
    glide_alpha: &mut GlideAlpha,
    ptr: &mut dyn Pointer,
) -> Result<bool> {
    if !glide_alpha.active() {
        return Ok(false);
    }
    glide_alpha.handle_event(ptr, code, value, is_press)
}

#[cfg(target_os = "linux")]
fn grid_input(code: u16, value: i32, grid: &mut GridEnv, mods: &ModState) -> bool {
    if !grid.active() || !is_grid_key(code) || value == 0 {
        return false;
    }
    grid.handle_input(code, value, mods)
}

#[cfg(target_os = "linux")]
fn is_grid_key(code: u16) -> bool {
    key_map(code, &ModState::default()).is_some() || code == KEY_TAB
}

// ── Platform entry point ─────────────────────────────────────────────

pub fn run_service() -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        linux::run()
    }
    #[cfg(target_os = "windows")]
    {
        windows::run()
    }
}
