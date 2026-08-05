// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

//! glide-alpha — main-keyboard glide mode.

use anyhow::Result;
use log::info;

use crate::config::MouseButton;
use crate::device::pointer::{KeyboardOut, Pointer, ScrollAxis, SideButton};
use crate::keymap::{
    KEY_APOSTROPHE, KEY_CAPSLOCK, KEY_H, KEY_I, KEY_J, KEY_K, KEY_L, KEY_LEFTCTRL, KEY_LEFTMETA,
    KEY_LEFTSHIFT, KEY_RIGHTCTRL, KEY_RIGHTMETA, KEY_RIGHTSHIFT, KEY_SEMICOLON, KEY_SPACE, KEY_U,
    ModState,
};
use crate::service::dir::{Dir, direction_tick, update_dir};

// ── glide-alpha state ═══════════════════════════════════════════════

pub struct GlideAlpha {
    active: bool,
    ctrl_held: bool,
    shift_held: bool,
    btn_held: Option<MouseButton>,
    dir_held: u8,
    dir_mask: Dir,
    dir_count: u32,
}

impl GlideAlpha {
    #[must_use]
    pub fn new() -> Self {
        Self {
            active: false,
            ctrl_held: false,
            shift_held: false,
            btn_held: None,
            dir_held: 0,
            dir_mask: Dir::empty(),
            dir_count: 0,
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
        if code != KEY_CAPSLOCK || !is_press || !mods.meta || !mods.shift || mods.ctrl || mods.alt {
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
        direction_tick(self.dir_held, self.dir_mask, &mut self.dir_count, ptr)
    }

    /// Clear held directions/buttons/modifiers (e.g. on mode toggle or a stuck state).
    pub(crate) fn reset_input(&mut self) {
        self.dir_held = 0;
        self.dir_mask = Dir::empty();
        self.dir_count = 0;
        self.btn_held = None;
        self.ctrl_held = false;
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
        // Modifier tracking — state only, forward the event to the desktop.
        if ctrl_code(code) {
            self.ctrl_held = is_press;
            return Ok(false);
        }
        if shift_code(code) {
            self.shift_held = is_press;
            return Ok(false);
        }

        let c = self.ctrl_held;
        let s = self.shift_held;

        // ctrl + h/j/k/l = move.
        if c && !s
            && let Some(flag) = Dir::from_alpha(code)
        {
            update_dir(
                &mut self.dir_held,
                &mut self.dir_mask,
                &mut self.dir_count,
                flag,
                value,
            );
            return Ok(true);
        }

        // shift + h/j/k/l = scroll.
        if s && !c
            && let Some((axis, dir)) = scroll_code(code)
        {
            if is_press {
                ptr.scroll(axis, dir)?;
            }
            return Ok(true);
        }

        // ctrl + u/i = back/forward.
        if c && !s {
            let btn = match code {
                KEY_U => SideButton::Back,
                KEY_I => SideButton::Forward,
                _ => return Ok(false),
            };
            if is_press {
                ptr.side(btn)?;
            }
            return Ok(true);
        }

        // Space / ; / ' = left / right / middle (press→down, release→up).
        let btn: MouseButton = match code {
            KEY_SPACE => MouseButton::Left,
            KEY_SEMICOLON => MouseButton::Right,
            KEY_APOSTROPHE => MouseButton::Middle,
            _ => return Ok(false),
        };

        if value > 0 {
            if self.btn_held.is_none() {
                ptr.button(btn, true)?;
                self.btn_held = Some(btn);
            }
        } else if self.btn_held == Some(btn) {
            ptr.button(btn, false)?;
            self.btn_held = None;
        }
        Ok(true)
    }
}

// ── Helpers ═════════════════════════════════════════════════════════

fn ctrl_code(code: u16) -> bool {
    matches!(code, KEY_LEFTCTRL | KEY_RIGHTCTRL)
}

fn shift_code(code: u16) -> bool {
    matches!(code, KEY_LEFTSHIFT | KEY_RIGHTSHIFT)
}

fn scroll_code(code: u16) -> Option<(ScrollAxis, i32)> {
    match code {
        KEY_H => Some((ScrollAxis::Horizontal, -1)),
        KEY_J => Some((ScrollAxis::Vertical, -1)),
        KEY_K => Some((ScrollAxis::Vertical, 1)),
        KEY_L => Some((ScrollAxis::Horizontal, 1)),
        _ => None,
    }
}

impl Default for GlideAlpha {
    fn default() -> Self {
        Self::new()
    }
}
