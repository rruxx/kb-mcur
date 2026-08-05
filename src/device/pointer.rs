// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Platform-neutral virtual pointer output (`Pointer`).

use anyhow::Result;

use crate::config::MouseButton;

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
    fn click(&mut self, button: MouseButton, count: u32) -> Result<()>;
    fn scroll(&mut self, axis: ScrollAxis, dir: i32) -> Result<()>;
    fn side(&mut self, button: SideButton) -> Result<()>;
    fn warp(&mut self, x: i16, y: i16) -> Result<()>;
}

/// Keyboard passthrough output for mode toggles.
/// Linux replays released keys to the virtual keyboard; Windows is a no-op
/// (the low-level hook simply never swallows those keys).
pub trait KeyboardOut {
    fn key(&mut self, code: u16, value: i32) -> Result<()>;
    fn sync(&mut self) -> Result<()>;
}
