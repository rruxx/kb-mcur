// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::Result;
use log::info;

use crate::config::MouseButton;
use crate::device::pointer::{KeyboardOut, Pointer, ScrollAxis, SideButton};
use crate::keymap::{
    KEY_KP0, KEY_KP5, KEY_KP7, KEY_KP8, KEY_KP9, KEY_KPASTERISK, KEY_KPDOT, KEY_KPENTER,
    KEY_KPMINUS, KEY_KPPLUS, KEY_KPSLASH, ModState,
};
use crate::service::dir::{Dir, direction_tick, update_dir};

// ── glide-num state ═════════════════════════════════════════════════

pub struct GlideNum {
    active: bool,
    numlock_held: bool,
    btn5_binding: MouseButton,
    btn_held: bool,
    dir_held: u8,
    dir_mask: Dir,
    dir_count: u32,
}

impl GlideNum {
    #[must_use]
    pub fn new() -> Self {
        Self {
            active: false,
            btn5_binding: MouseButton::Left,
            btn_held: false,
            numlock_held: false,
            dir_held: 0,
            dir_mask: Dir::empty(),
            dir_count: 0,
        }
    }

    #[must_use]
    pub fn active(&self) -> bool {
        self.active
    }

    /// Currently held direction mask (diagnostics).
    #[must_use]
    pub fn held_dir(&self) -> Dir {
        self.dir_mask
    }

    /// Toggle glide-num on/off via NumLock+KPEnter.
    /// Returns `Ok(true)` if the event was consumed.
    pub fn toggle(
        &mut self,
        code: u16,
        _value: i32,
        is_press: bool,
        mods: &ModState,
        _kbd: &mut dyn KeyboardOut,
    ) -> Result<bool> {
        if code == KEY_KPENTER
            && is_press
            && self.numlock_held
            && !mods.meta
            && !mods.shift
            && !mods.ctrl
            && !mods.alt
        {
            self.active = !self.active;
            self.reset_input();
            info!(
                "{}",
                if self.active {
                    "[glide-num ON]"
                } else {
                    "[glide-num OFF]"
                }
            );
            return Ok(true);
        }
        Ok(false)
    }

    /// Record whether `NumLock` is currently held.
    pub fn set_numlock(&mut self, held: bool) {
        self.numlock_held = held;
    }

    #[must_use]
    pub fn numlock_held(&self) -> bool {
        self.numlock_held
    }

    /// One per-frame movement step while a direction is held.
    pub fn direction_tick(&mut self, ptr: &mut dyn Pointer) -> Result<bool> {
        direction_tick(self.dir_held, self.dir_mask, &mut self.dir_count, ptr)
    }

    /// Clear held directions/buttons (e.g. on mode toggle or a stuck state).
    pub(crate) fn reset_input(&mut self) {
        self.dir_held = 0;
        self.dir_mask = Dir::empty();
        self.dir_count = 0;
        self.btn_held = false;
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
        if self.numlock_held {
            match code {
                KEY_KPSLASH => {
                    if is_press {
                        ptr.scroll(ScrollAxis::Vertical, 1)?;
                    }
                    return Ok(true);
                }
                KEY_KP8 => {
                    if is_press {
                        ptr.scroll(ScrollAxis::Vertical, -1)?;
                    }
                    return Ok(true);
                }
                KEY_KP7 => {
                    if is_press {
                        ptr.scroll(ScrollAxis::Horizontal, -1)?;
                    }
                    return Ok(true);
                }
                KEY_KP9 => {
                    if is_press {
                        ptr.scroll(ScrollAxis::Horizontal, 1)?;
                    }
                    return Ok(true);
                }
                KEY_KPASTERISK => {
                    if is_press {
                        ptr.side(SideButton::Back)?;
                    }
                    return Ok(true);
                }
                KEY_KPMINUS => {
                    if is_press {
                        ptr.side(SideButton::Forward)?;
                    }
                    return Ok(true);
                }
                _ => {}
            }
        }

        match code {
            c if let Some(flag) = Dir::from_numpad(c) => {
                update_dir(
                    &mut self.dir_held,
                    &mut self.dir_mask,
                    &mut self.dir_count,
                    flag,
                    value,
                );
                Ok(true)
            }
            KEY_KP5 => {
                if value > 0 {
                    ptr.button(self.btn5_binding, true)?;
                    self.btn_held = true;
                } else if value == 0 && self.btn_held {
                    ptr.button(self.btn5_binding, false)?;
                    self.btn_held = false;
                }
                Ok(true)
            }
            KEY_KPDOT => {
                if is_press {
                    ptr.button(self.btn5_binding, false)?;
                    self.btn_held = false;
                    info!("[release]");
                }
                Ok(true)
            }
            KEY_KP0 => {
                if value == 1 && !self.btn_held {
                    ptr.button(self.btn5_binding, true)?;
                    self.btn_held = true;
                    info!("[hold]");
                }
                Ok(true)
            }
            KEY_KPASTERISK => {
                if is_press {
                    self.btn5_binding = MouseButton::Middle;
                    info!("[btn5=M]");
                }
                Ok(true)
            }
            KEY_KPSLASH => {
                if is_press {
                    self.btn5_binding = MouseButton::Left;
                    info!("[btn5=L]");
                }
                Ok(true)
            }
            KEY_KPMINUS => {
                if is_press {
                    self.btn5_binding = MouseButton::Right;
                    info!("[btn5=R]");
                }
                Ok(true)
            }
            KEY_KPPLUS => {
                if value == 1 {
                    ptr.click(self.btn5_binding, 2)?;
                    info!("[dblclick]");
                }
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}

impl Default for GlideNum {
    fn default() -> Self {
        Self::new()
    }
}
