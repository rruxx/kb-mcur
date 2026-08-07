// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

//! glide-alpha — main-keyboard glide mode.

use anyhow::Result;
use log::info;

use crate::config::MouseButton;
use crate::device::pointer::{KeyboardOut, Pointer, ScrollAxis, SideButton};
use crate::keymap::{
    KEY_A, KEY_CAPSLOCK, KEY_D, KEY_I, KEY_LEFTMETA, KEY_LEFTSHIFT, KEY_M, KEY_N, KEY_O,
    KEY_RIGHTMETA, KEY_RIGHTSHIFT, KEY_S, KEY_U, KEY_W, ModState,
};
use crate::service::dir::{Dir, DirState};

// ── glide-alpha state ═══════════════════════════════════════════════

pub struct GlideAlpha {
    active: bool,
    caps_held: bool,
    shift_held: bool,
    btn_held: Option<MouseButton>,
    dir: DirState,
}

impl GlideAlpha {
    #[must_use]
    pub fn new() -> Self {
        Self {
            active: false,
            caps_held: false,
            shift_held: false,
            btn_held: None,
            dir: DirState::new(),
        }
    }

    #[must_use]
    pub fn active(&self) -> bool {
        self.active
    }

    /// Toggle glide-alpha on/off via Meta+Shift+CapsLock.
    /// Returns `Ok(true)` if the event was consumed.
    pub fn toggle(
        &mut self,
        code: u16,
        _value: i32,
        is_press: bool,
        mods: &ModState,
        kbd: &mut dyn KeyboardOut,
    ) -> Result<bool> {
        if code != KEY_CAPSLOCK || !is_press || !mods.meta || !mods.shift || mods.ctrl {
            return Ok(false);
        }
        self.active = !self.active;
        self.reset_input();
        info!(
            "{}",
            if self.active {
                "[glide-alpha ON]"
            } else {
                "[glide-alpha OFF]"
            }
        );
        for key in [
            KEY_LEFTMETA,
            KEY_RIGHTMETA,
            KEY_LEFTSHIFT,
            KEY_RIGHTSHIFT,
            KEY_CAPSLOCK,
        ] {
            kbd.key(key, 0)?;
        }
        kbd.sync()?;
        Ok(true)
    }

    /// One per-frame movement step while a direction is held.
    pub fn direction_tick(&mut self, ptr: &mut dyn Pointer) -> Result<()> {
        self.dir.tick(ptr)
    }

    /// Clear held directions/buttons/modifiers (e.g. on mode toggle or a stuck state).
    pub(crate) fn reset_input(&mut self) {
        self.dir = DirState::new();
        self.btn_held = None;
        self.caps_held = false;
        self.shift_held = false;
    }

    // ── Event handling ──

    /// Handle one key event. Returns `Ok(true)` if the key was consumed.
    pub fn handle_event(
        &mut self,
        ptr: &mut dyn Pointer,
        code: u16,
        value: i32,
        is_press: bool,
    ) -> Result<bool> {
        // CapsLock is the glide-alpha modifier. While active it is swallowed so
        // holding it for chords never flips the desktop's caps-lock state.
        if code == KEY_CAPSLOCK {
            self.caps_held = is_press;
            return Ok(true);
        }
        if shift_code(code) {
            self.shift_held = is_press;
            return Ok(false);
        }

        let caps = self.caps_held;
        let s = self.shift_held;

        // capslock + h/j/k/l = move.
        if caps
            && !s
            && let Some(flag) = Dir::from_alpha(code)
        {
            self.dir.update(flag, value);
            return Ok(true);
        }

        // capslock + w/a/s/d = scroll up/left/down/right.
        if caps
            && !s
            && let Some((axis, dir)) = scroll_code(code)
        {
            if is_press {
                ptr.scroll(axis, dir)?;
            }
            return Ok(true);
        }

        // capslock + u/i/o = left/middle/right buttons (press→down, release→up);
        // capslock + n/m = back/forward.
        if caps && !s {
            if let Some(btn) = match code {
                KEY_U => Some(MouseButton::Left),
                KEY_I => Some(MouseButton::Middle),
                KEY_O => Some(MouseButton::Right),
                _ => None,
            } {
                if value > 0 {
                    if self.btn_held.is_none() {
                        ptr.button(btn, true)?;
                        self.btn_held = Some(btn);
                    }
                } else if self.btn_held == Some(btn) {
                    ptr.button(btn, false)?;
                    self.btn_held = None;
                }
                return Ok(true);
            }
            if let Some(side) = match code {
                KEY_N => Some(SideButton::Back),
                KEY_M => Some(SideButton::Forward),
                _ => None,
            } {
                if is_press {
                    ptr.side(side)?;
                }
                return Ok(true);
            }
            return Ok(false);
        }

        Ok(false)
    }
}

// ── Helpers ═════════════════════════════════════════════════════════

fn shift_code(code: u16) -> bool {
    matches!(code, KEY_LEFTSHIFT | KEY_RIGHTSHIFT)
}

fn scroll_code(code: u16) -> Option<(ScrollAxis, i32)> {
    match code {
        KEY_W => Some((ScrollAxis::Vertical, 1)),
        KEY_A => Some((ScrollAxis::Horizontal, -1)),
        KEY_S => Some((ScrollAxis::Vertical, -1)),
        KEY_D => Some((ScrollAxis::Horizontal, 1)),
        _ => None,
    }
}

impl Default for GlideAlpha {
    fn default() -> Self {
        Self::new()
    }
}
