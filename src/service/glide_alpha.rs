// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

//! glide-alpha — main-keyboard glide mode.

use std::fs::File;

use anyhow::Result;

use crate::config::{BTN_EXTRA, BTN_LEFT, BTN_MIDDLE, BTN_RIGHT, BTN_SIDE};
use crate::keymap::{
    KEY_APOSTROPHE, KEY_H, KEY_I, KEY_J, KEY_K, KEY_L, KEY_LEFTCTRL, KEY_LEFTSHIFT, KEY_RIGHTCTRL,
    KEY_RIGHTSHIFT, KEY_SEMICOLON, KEY_SPACE, KEY_U,
};
use crate::service::dir::{Dir, direction_tick, update_dir};
use crate::uinput::{EV_KEY, EV_REL, EV_SYN, REL_HWHEEL, REL_WHEEL, SYN_REPORT, write_event};

// ── glide-alpha state ═══════════════════════════════════════════════

pub struct GlideAlpha {
    active: bool,
    ctrl_held: bool,
    shift_held: bool,
    btn_held: Option<u16>,
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

    /// Toggle glide-alpha on/off.
    pub fn toggle(&mut self) {
        self.active = !self.active;
    }

    /// One per-frame movement step while a direction is held.
    pub fn direction_tick(&mut self, ptr_out: &mut File) -> Result<()> {
        direction_tick(self.dir_held, self.dir_mask, &mut self.dir_count, ptr_out)
    }

    // ── Event handling ──

    /// Handle one key event. Returns `Ok(true)` if the key was consumed.
    pub fn handle_event(
        &mut self,
        ptr_out: &mut File,
        code: u16,
        value: i32,
        is_press: bool,
    ) -> Result<bool> {
        // Modifier tracking.
        if ctrl_code(code) {
            self.ctrl_held = is_press;
            return Ok(true);
        }
        if shift_code(code) {
            self.shift_held = is_press;
            return Ok(true);
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
                write_event(ptr_out, EV_REL, axis, dir)?;
                write_event(ptr_out, EV_SYN, SYN_REPORT, 0)?;
            }
            return Ok(true);
        }

        // ctrl + u/i = back/forward.
        if c && !s {
            match code {
                KEY_U => {
                    if is_press {
                        write_event(ptr_out, EV_KEY, BTN_SIDE, 1)?;
                        write_event(ptr_out, EV_SYN, SYN_REPORT, 0)?;
                        write_event(ptr_out, EV_KEY, BTN_SIDE, 0)?;
                        write_event(ptr_out, EV_SYN, SYN_REPORT, 0)?;
                    }
                    return Ok(true);
                }
                KEY_I => {
                    if is_press {
                        write_event(ptr_out, EV_KEY, BTN_EXTRA, 1)?;
                        write_event(ptr_out, EV_SYN, SYN_REPORT, 0)?;
                        write_event(ptr_out, EV_KEY, BTN_EXTRA, 0)?;
                        write_event(ptr_out, EV_SYN, SYN_REPORT, 0)?;
                    }
                    return Ok(true);
                }
                _ => {}
            }
        }

        // Space / ; / ' = left / right / middle (press→down, release→up).
        let btn: u16 = match code {
            KEY_SPACE => BTN_LEFT,
            KEY_SEMICOLON => BTN_RIGHT,
            KEY_APOSTROPHE => BTN_MIDDLE,
            _ => return Ok(false),
        };

        if value > 0 {
            if self.btn_held.is_none() {
                write_event(ptr_out, EV_KEY, btn, 1)?;
                write_event(ptr_out, EV_SYN, SYN_REPORT, 0)?;
                self.btn_held = Some(btn);
            }
        } else if self.btn_held == Some(btn) {
            write_event(ptr_out, EV_KEY, btn, 0)?;
            write_event(ptr_out, EV_SYN, SYN_REPORT, 0)?;
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

fn scroll_code(code: u16) -> Option<(u16, i32)> {
    match code {
        KEY_H => Some((REL_HWHEEL, -1)),
        KEY_J => Some((REL_WHEEL, -1)),
        KEY_K => Some((REL_WHEEL, 1)),
        KEY_L => Some((REL_HWHEEL, 1)),
        _ => None,
    }
}

impl Default for GlideAlpha {
    fn default() -> Self {
        Self::new()
    }
}
