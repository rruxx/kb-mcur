// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Platform-neutral virtual pointer output (`Pointer`).

use std::time::Duration;

use anyhow::Result;

use crate::config::{CLICK_INTERVAL_MS, MouseButton};

/// Scroll axis for mouse-wheel events.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollAxis {
    Vertical,
    Horizontal,
}

/// Side buttons (browser back/forward).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SideButton {
    Back,
    Forward,
}

/// Virtual pointer operations shared by glide/grid modes.
/// Implemented per platform: Linux via uinput, Windows via `SendInput`.
pub trait Pointer {
    fn move_rel(&mut self, dx: i32, dy: i32) -> Result<()>;
    fn button(&mut self, button: MouseButton, press: bool) -> Result<()>;

    /// Press and release a button `count` times. The default implementation
    /// uses the shared [`CLICK_INTERVAL_MS`]; platform implementations may
    /// override it (e.g. to batch down+up into one syscall).
    fn click(&mut self, button: MouseButton, count: u32) -> Result<()> {
        let half = Duration::from_millis(CLICK_INTERVAL_MS / 2);
        for _ in 0..count {
            self.button(button, true)?;
            std::thread::sleep(half);
            self.button(button, false)?;
            std::thread::sleep(half);
        }
        Ok(())
    }

    fn scroll(&mut self, axis: ScrollAxis, dir: i32) -> Result<()>;
    fn side(&mut self, button: SideButton) -> Result<()>;
    fn warp(&mut self, x: i32, y: i32) -> Result<()>;
}

/// Keyboard passthrough output for mode toggles.
/// Linux replays released keys to the virtual keyboard; Windows is a no-op
/// (the low-level hook simply never swallows those keys).
pub trait KeyboardOut {
    fn key(&mut self, code: u16, value: i32) -> Result<()>;
    fn sync(&mut self) -> Result<()>;
}
