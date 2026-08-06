// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

//! `kursor move` — relative cursor movement.

use crate::cli;
use crate::config::MOVE_WAIT_MS;
use anyhow::Result;

pub fn run(x: i32, y: i32) -> Result<()> {
    let mut m = cli::mouse()?;
    m.move_rel(x, y)?;
    std::thread::sleep(std::time::Duration::from_millis(MOVE_WAIT_MS));
    Ok(())
}
