// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

//! `kursor click` — mouse click with optional repeat.

use crate::cli;
use crate::config::MouseButton;
use crate::device::pointer::Pointer;
use anyhow::Result;

pub fn run(repeat: u32, btn: String) -> Result<()> {
    let mut m = cli::mouse()?;
    m.click(parse_button(&btn)?, repeat)?;
    Ok(())
}

fn parse_button(s: &str) -> Result<MouseButton> {
    match s {
        "L" | "l" | "left" | "1" => Ok(MouseButton::Left),
        "M" | "m" | "middle" | "2" => Ok(MouseButton::Middle),
        "R" | "r" | "right" | "3" => Ok(MouseButton::Right),
        other => anyhow::bail!("unknown button: {other} (use L|M|R)"),
    }
}
