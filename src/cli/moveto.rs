// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

//! `kursor moveto` — absolute cursor warp.

use crate::config::MOVE_WAIT_MS;
use crate::device::Mouse;
use crate::query_screen_size;
use anyhow::Result;

pub fn run(x: i16, y: i16) -> Result<()> {
    let (sw, sh) = query_screen_size();
    let mut m = Mouse::new(sw, sh)?;
    m.warp(x, y)?;
    std::thread::sleep(std::time::Duration::from_millis(MOVE_WAIT_MS));
    Ok(())
}
