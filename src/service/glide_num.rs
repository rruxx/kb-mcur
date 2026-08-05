// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fs::File;

use anyhow::Result;
use log::info;

use crate::config::{self, BTN_EXTRA, BTN_LEFT, BTN_MIDDLE, BTN_RIGHT, BTN_SIDE};
use crate::keymap::{
    KEY_KP0, KEY_KP5, KEY_KP7, KEY_KP8, KEY_KP9, KEY_KPASTERISK, KEY_KPDOT, KEY_KPMINUS,
    KEY_KPPLUS, KEY_KPSLASH,
};
use crate::service::dir::{Dir, direction_tick, update_dir};
use crate::uinput::{EV_KEY, EV_REL, EV_SYN, REL_HWHEEL, REL_WHEEL, SYN_REPORT, write_event};

// ── glide-num state ═════════════════════════════════════════════════

pub struct GlideNum {
    active: bool,
    numlock_held: bool,
    btn5_binding: u8,
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
            btn5_binding: 1,
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

    /// Toggle glide-num on/off.
    pub fn toggle(&mut self) {
        self.active = !self.active;
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
    pub fn direction_tick(&mut self, ptr_out: &mut File) -> Result<()> {
        direction_tick(self.dir_held, self.dir_mask, &mut self.dir_count, ptr_out)
    }

    fn btn_code(&self) -> u16 {
        match self.btn5_binding {
            2 => BTN_MIDDLE,
            3 => BTN_RIGHT,
            _ => BTN_LEFT,
        }
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
        if self.numlock_held {
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
                    write_event(ptr_out, EV_KEY, self.btn_code(), 1)?;
                    write_event(ptr_out, EV_SYN, SYN_REPORT, 0)?;
                    self.btn_held = true;
                } else if value == 0 && self.btn_held {
                    write_event(ptr_out, EV_KEY, self.btn_code(), 0)?;
                    write_event(ptr_out, EV_SYN, SYN_REPORT, 0)?;
                    self.btn_held = false;
                }
                Ok(true)
            }
            KEY_KPDOT => {
                if is_press {
                    write_event(ptr_out, EV_KEY, self.btn_code(), 0)?;
                    write_event(ptr_out, EV_SYN, SYN_REPORT, 0)?;
                    self.btn_held = false;
                    info!("[release]");
                }
                Ok(true)
            }
            KEY_KP0 => {
                if value == 1 && !self.btn_held {
                    write_event(ptr_out, EV_KEY, self.btn_code(), 1)?;
                    write_event(ptr_out, EV_SYN, SYN_REPORT, 0)?;
                    self.btn_held = true;
                    info!("[hold]");
                }
                Ok(true)
            }
            KEY_KPASTERISK => {
                if is_press {
                    self.btn5_binding = 2;
                    info!("[btn5=M]");
                }
                Ok(true)
            }
            KEY_KPSLASH => {
                if is_press {
                    self.btn5_binding = 1;
                    info!("[btn5=L]");
                }
                Ok(true)
            }
            KEY_KPMINUS => {
                if is_press {
                    self.btn5_binding = 3;
                    info!("[btn5=R]");
                }
                Ok(true)
            }
            KEY_KPPLUS => {
                if value == 1 {
                    let code = self.btn_code();
                    let half = std::time::Duration::from_millis(config::CLICK_INTERVAL_MS / 2);
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
}
