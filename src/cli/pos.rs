// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

//! `kursor pos` — print the current cursor position and the screen it is on.

use crate::cursor_pos;
use crate::overlay::connect;
use anyhow::Result;

pub fn run() -> Result<()> {
    let (cx, cy) = cursor_pos()?;
    let overlay = connect()?;
    let monitors = overlay.named_monitors()?;
    let screen = monitors
        .iter()
        .find(|m| cx >= m.x && cx < m.x + i32::from(m.w) && cy >= m.y && cy < m.y + i32::from(m.h));
    match screen {
        Some(m) => println!("screen: {} ({}, {}) {}x{}", m.name, m.x, m.y, m.w, m.h),
        None => println!("screen: unknown"),
    }
    println!("cursor: ({cx}, {cy})");
    Ok(())
}
