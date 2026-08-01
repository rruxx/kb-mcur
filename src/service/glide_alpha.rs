// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

//! glide-alpha — main-keyboard glide mode.

use std::fs::File;

use anyhow::Result;

use crate::config::{self, BTN_EXTRA, BTN_LEFT, BTN_MIDDLE, BTN_RIGHT, BTN_SIDE};
use crate::keymap::{
    KEY_APOSTROPHE, KEY_H, KEY_I, KEY_J, KEY_K, KEY_L, KEY_LEFTCTRL, KEY_LEFTSHIFT,
    KEY_RIGHTCTRL, KEY_RIGHTSHIFT, KEY_SEMICOLON, KEY_SPACE, KEY_U,
};
use crate::uinput::{
    EV_KEY, EV_REL, EV_SYN, REL_HWHEEL, REL_WHEEL, REL_X, REL_Y, SYN_REPORT, write_event,
};

// ── Direction ════════════════════════════════════════════════════════

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct Dir: u8 {
        const LEFT  = 0x01;
        const DOWN  = 0x02;
        const UP    = 0x04;
        const RIGHT = 0x08;
    }
}

impl Dir {
    fn from_alpha(code: u16) -> Option<Self> {
        match code {
            KEY_H => Some(Dir::LEFT),
            KEY_J => Some(Dir::DOWN),
            KEY_K => Some(Dir::UP),
            KEY_L => Some(Dir::RIGHT),
            _ => None,
        }
    }

    fn to_vector(self) -> (i32, i32) {
        match self {
            Dir::LEFT => (-1, 0),
            Dir::DOWN => (0, 1),
            Dir::UP => (0, -1),
            Dir::RIGHT => (1, 0),
            _ => (0, 0),
        }
    }
}

// ── glide-alpha state ═══════════════════════════════════════════════

pub(crate) struct GlideAlpha {
    pub(crate) active: bool,
    ctrl_held: bool,
    shift_held: bool,
    btn_held: Option<u16>,
    dir_held: u8,
    dir_mask: Dir,
    dir_count: u32,
}

impl GlideAlpha {
    pub(crate) fn new() -> Self {
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

    pub(crate) fn active(&self) -> bool {
        self.active
    }
}

// ── Event handling ══════════════════════════════════════════════════

pub(crate) fn handle_alpha_event(
    glide_alpha: &mut GlideAlpha,
    ptr_out: &mut File,
    code: u16,
    value: i32,
    is_press: bool,
) -> Result<bool> {
    // ── modifier tracking ──
    if ctrl_code(code) {
        glide_alpha.ctrl_held = is_press;
        return Ok(true);
    }
    if shift_code(code) {
        glide_alpha.shift_held = is_press;
        return Ok(true);
    }

    let c = glide_alpha.ctrl_held;
    let s = glide_alpha.shift_held;

    // ── ctrl + h/j/k/l = move ──
    if c && !s
        && let Some(flag) = Dir::from_alpha(code)
    {
        update_dir(&mut glide_alpha.dir_held, &mut glide_alpha.dir_mask, &mut glide_alpha.dir_count, flag, value);
        return Ok(true);
    }

    // ── shift + h/j/k/l = scroll ──
    if s && !c
        && let Some((axis, dir)) = scroll_code(code)
    {
        if is_press {
            write_event(ptr_out, EV_REL, axis, dir)?;
            write_event(ptr_out, EV_SYN, SYN_REPORT, 0)?;
        }
        return Ok(true);
    }

    // ── ctrl + u/i = back/forward ──
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

    // ── Space / ; / ' = left / right / middle (press→down, release→up) ──
    let btn_code: u16 = match code {
        KEY_SPACE => BTN_LEFT,
        KEY_SEMICOLON => BTN_RIGHT,
        KEY_APOSTROPHE => BTN_MIDDLE,
        _ => return Ok(false),
    };

    if value > 0 {
        if glide_alpha.btn_held.is_none() {
            write_event(ptr_out, EV_KEY, btn_code, 1)?;
            write_event(ptr_out, EV_SYN, SYN_REPORT, 0)?;
            glide_alpha.btn_held = Some(btn_code);
        }
    } else if glide_alpha.btn_held == Some(btn_code) {
        write_event(ptr_out, EV_KEY, btn_code, 0)?;
        write_event(ptr_out, EV_SYN, SYN_REPORT, 0)?;
        glide_alpha.btn_held = None;
    }
    Ok(true)
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

fn update_dir(held: &mut u8, mask: &mut Dir, count: &mut u32, flag: Dir, value: i32) {
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

// ── Per‑frame tick ══════════════════════════════════════════════════

pub(crate) fn do_direction_alpha_tick(
    glide_alpha: &mut GlideAlpha,
    ptr_out: &mut File,
) -> Result<()> {
    if glide_alpha.dir_held != 1 {
        return Ok(());
    }
    let (dx, dy) = glide_alpha.dir_mask.to_vector();
    glide_alpha.dir_count = glide_alpha.dir_count.saturating_add(1);
    let step = config::cursor_speed(glide_alpha.dir_count) as f32;
    let mx = (dx as f32 * step) as i32;
    let my = (dy as f32 * step) as i32;
    write_event(ptr_out, EV_REL, REL_X, mx)?;
    write_event(ptr_out, EV_REL, REL_Y, my)?;
    write_event(ptr_out, EV_SYN, SYN_REPORT, 0)?;
    Ok(())
}
