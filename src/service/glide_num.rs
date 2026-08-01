// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fs::File;

use anyhow::Result;
use log::info;

use crate::config::{self, BTN_EXTRA, BTN_LEFT, BTN_MIDDLE, BTN_RIGHT, BTN_SIDE};
use crate::keymap::{
    KEY_KP0, KEY_KP1, KEY_KP2, KEY_KP3, KEY_KP4, KEY_KP5, KEY_KP6, KEY_KP7, KEY_KP8, KEY_KP9,
    KEY_KPASTERISK, KEY_KPDOT, KEY_KPMINUS, KEY_KPPLUS, KEY_KPSLASH,
};
use crate::uinput::{
    EV_KEY, EV_REL, EV_SYN, REL_HWHEEL, REL_WHEEL, REL_X, REL_Y, SYN_REPORT, write_event,
};

// ── 方向映射 ────────────────────────────────────────────────────────

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct Dir: u8 {
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
    fn from_numpad(code: u16) -> Option<Self> {
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

    fn to_vector(self) -> (i32, i32) {
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

// ── glide-num 状态 ─────────────────────────────────────────────────

pub(crate) struct GlideNum {
    pub(crate) active: bool,
    pub(crate) numlock_held: bool,
    btn_5: u8,
    btn_held: bool,
    dir_held: u8,
    dir_mask: Dir,
    dir_count: u32,
}

impl GlideNum {
    pub(crate) fn new() -> Self {
        Self {
            active: false,
            btn_5: 1,
            btn_held: false,
            numlock_held: false,
            dir_held: 0,
            dir_mask: Dir::empty(),
            dir_count: 0,
        }
    }

    pub(crate) fn active(&self) -> bool {
        self.active
    }

    fn btn_code(&self) -> u16 {
        match self.btn_5 {
            2 => BTN_MIDDLE,
            3 => BTN_RIGHT,
            _ => BTN_LEFT,
        }
    }
}

// ── glide-num 事件处理 ─────────────────────────────────────────────────

pub(crate) fn handle_key_event(
    glide_num: &mut GlideNum,
    ptr_out: &mut File,
    code: u16,
    value: i32,
    is_press: bool,
) -> Result<bool> {
    if glide_num.numlock_held {
        match code {
            KEY_KPSLASH => {
                if is_press {
                    write_event(ptr_out, EV_REL, REL_WHEEL, 1)?;
                    write_event(ptr_out, EV_SYN, SYN_REPORT, 0)?;
                }
                return Ok(true);
            }
            KEY_KP8 => {
                if is_press {
                    write_event(ptr_out, EV_REL, REL_WHEEL, -1)?;
                    write_event(ptr_out, EV_SYN, SYN_REPORT, 0)?;
                }
                return Ok(true);
            }
            KEY_KP7 => {
                if is_press {
                    write_event(ptr_out, EV_REL, REL_HWHEEL, -1)?;
                    write_event(ptr_out, EV_SYN, SYN_REPORT, 0)?;
                }
                return Ok(true);
            }
            KEY_KP9 => {
                if is_press {
                    write_event(ptr_out, EV_REL, REL_HWHEEL, 1)?;
                    write_event(ptr_out, EV_SYN, SYN_REPORT, 0)?;
                }
                return Ok(true);
            }
            KEY_KPASTERISK => {
                if is_press {
                    write_event(ptr_out, EV_KEY, BTN_SIDE, 1)?;
                    write_event(ptr_out, EV_SYN, SYN_REPORT, 0)?;
                    write_event(ptr_out, EV_KEY, BTN_SIDE, 0)?;
                    write_event(ptr_out, EV_SYN, SYN_REPORT, 0)?;
                }
                return Ok(true);
            }
            KEY_KPMINUS => {
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

    match code {
        c if Dir::from_numpad(c).is_some() => {
            let flag = Dir::from_numpad(c).unwrap();
            if value == 0 {
                glide_num.dir_mask.remove(flag);
                glide_num.dir_held = glide_num.dir_held.saturating_sub(1);
                if glide_num.dir_held == 0 {
                    glide_num.dir_count = 0;
                }
            } else if value == 1 {
                glide_num.dir_mask.insert(flag);
                glide_num.dir_held = glide_num.dir_held.saturating_add(1);
            }
            Ok(true)
        }
        KEY_KP5 => {
            if value > 0 {
                write_event(ptr_out, EV_KEY, glide_num.btn_code(), 1)?;
                write_event(ptr_out, EV_SYN, SYN_REPORT, 0)?;
                glide_num.btn_held = true;
            } else if value == 0 && glide_num.btn_held {
                write_event(ptr_out, EV_KEY, glide_num.btn_code(), 0)?;
                write_event(ptr_out, EV_SYN, SYN_REPORT, 0)?;
                glide_num.btn_held = false;
            }
            Ok(true)
        }
        KEY_KPDOT => {
            if is_press {
                write_event(ptr_out, EV_KEY, glide_num.btn_code(), 0)?;
                write_event(ptr_out, EV_SYN, SYN_REPORT, 0)?;
                glide_num.btn_held = false;
                info!("[release]");
            }
            Ok(true)
        }
        KEY_KP0 => {
            if value == 1 && !glide_num.btn_held {
                write_event(ptr_out, EV_KEY, glide_num.btn_code(), 1)?;
                write_event(ptr_out, EV_SYN, SYN_REPORT, 0)?;
                glide_num.btn_held = true;
                info!("[hold]");
            }
            Ok(true)
        }
        KEY_KPASTERISK => {
            if is_press {
                glide_num.btn_5 = 2;
                info!("[btn5=M]");
            }
            Ok(true)
        }
        KEY_KPSLASH => {
            if is_press {
                glide_num.btn_5 = 1;
                info!("[btn5=L]");
            }
            Ok(true)
        }
        KEY_KPMINUS => {
            if is_press {
                glide_num.btn_5 = 3;
                info!("[btn5=R]");
            }
            Ok(true)
        }
        KEY_KPPLUS => {
            if value == 1 {
                let code = glide_num.btn_code();
                let half = std::time::Duration::from_millis(50);
                for _ in 0..2 {
                    write_event(ptr_out, EV_KEY, code, 1)?;
                    write_event(ptr_out, EV_SYN, SYN_REPORT, 0)?;
                    std::thread::sleep(half);
                    write_event(ptr_out, EV_KEY, code, 0)?;
                    write_event(ptr_out, EV_SYN, SYN_REPORT, 0)?;
                    std::thread::sleep(half);
                }
                info!("[dblclick]");
            }
            Ok(true)
        }
        _ => Ok(false),
    }
}

pub(crate) fn do_direction_num_tick(glide_num: &mut GlideNum, ptr_out: &mut File) -> Result<()> {
    if glide_num.dir_held != 1 {
        return Ok(());
    }
    let (dx, dy) = glide_num.dir_mask.to_vector();
    glide_num.dir_count = glide_num.dir_count.saturating_add(1);
    let step = config::cursor_speed(glide_num.dir_count) as f32;
    let mx = (dx as f32 * step) as i32;
    let my = (dy as f32 * step) as i32;
    write_event(ptr_out, EV_REL, REL_X, mx)?;
    write_event(ptr_out, EV_REL, REL_Y, my)?;
    write_event(ptr_out, EV_SYN, SYN_REPORT, 0)?;
    Ok(())
}
