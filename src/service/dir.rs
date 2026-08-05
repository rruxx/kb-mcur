// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Shared direction handling for glide modes: a direction bitmask with
//! hold/release tracking and per-frame accelerated cursor movement.

use anyhow::Result;

use crate::config;
use crate::device::pointer::Pointer;
use crate::keymap::{
    KEY_H, KEY_J, KEY_K, KEY_KP1, KEY_KP2, KEY_KP3, KEY_KP4, KEY_KP6, KEY_KP7, KEY_KP8, KEY_KP9,
    KEY_L,
};

// ── Direction ════════════════════════════════════════════════════════

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Dir: u8 {
        const UP    = 0x01;
        const DOWN  = 0x02;
        const LEFT  = 0x04;
        const RIGHT = 0x08;
        const UP_LEFT    = 0x10;
        const UP_RIGHT   = 0x20;
        const DOWN_LEFT  = 0x40;
        const DOWN_RIGHT = 0x80;
    }
}

impl Dir {
    /// Map a numpad keycode to its direction.
    #[must_use]
    pub fn from_numpad(code: u16) -> Option<Self> {
        match code {
            KEY_KP8 => Some(Dir::UP),
            KEY_KP2 => Some(Dir::DOWN),
            KEY_KP4 => Some(Dir::LEFT),
            KEY_KP6 => Some(Dir::RIGHT),
            KEY_KP7 => Some(Dir::UP_LEFT),
            KEY_KP9 => Some(Dir::UP_RIGHT),
            KEY_KP1 => Some(Dir::DOWN_LEFT),
            KEY_KP3 => Some(Dir::DOWN_RIGHT),
            _ => None,
        }
    }

    /// Map an alpha (h/j/k/l) keycode to its direction.
    #[must_use]
    pub fn from_alpha(code: u16) -> Option<Self> {
        match code {
            KEY_H => Some(Dir::LEFT),
            KEY_J => Some(Dir::DOWN),
            KEY_K => Some(Dir::UP),
            KEY_L => Some(Dir::RIGHT),
            _ => None,
        }
    }

    /// Unit movement vector for this direction.
    #[must_use]
    pub fn to_vector(self) -> (i32, i32) {
        match self {
            Dir::UP => (0, -1),
            Dir::DOWN => (0, 1),
            Dir::LEFT => (-1, 0),
            Dir::RIGHT => (1, 0),
            Dir::UP_LEFT => (-1, -1),
            Dir::UP_RIGHT => (1, -1),
            Dir::DOWN_LEFT => (-1, 1),
            Dir::DOWN_RIGHT => (1, 1),
            _ => (0, 0),
        }
    }
}

/// Track a direction hold/release on a mask + counter.
pub fn update_dir(held: &mut u8, mask: &mut Dir, count: &mut u32, flag: Dir, value: i32) {
    if value == 0 {
        mask.remove(flag);
        *held = held.saturating_sub(1);
        if *held == 0 {
            *count = 0;
        }
    } else if value == 1 {
        mask.insert(flag);
        *held = held.saturating_add(1);
    }
}

/// One per-frame movement step while a direction is held (accelerated).
pub fn direction_tick(held: u8, mask: Dir, count: &mut u32, ptr: &mut dyn Pointer) -> Result<()> {
    if held != 1 {
        return Ok(());
    }
    let (dx, dy) = mask.to_vector();
    *count = count.saturating_add(1);
    let step = config::cursor_speed(*count) as f32;
    let mx = (dx as f32 * step) as i32;
    let my = (dy as f32 * step) as i32;
    ptr.move_rel(mx, my)
}
