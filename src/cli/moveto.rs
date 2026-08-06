// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

//! `kursor moveto` — absolute cursor warp.

use crate::cli;
use crate::config::MOVE_WAIT_MS;
use anyhow::Result;

pub fn run(x: i32, y: i32) -> Result<()> {
    let mut m = cli::mouse()?;
    m.warp(x, y)?;
    std::thread::sleep(std::time::Duration::from_millis(MOVE_WAIT_MS));
    Ok(())
}
