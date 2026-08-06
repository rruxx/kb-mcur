// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

//! `kursor click` — mouse click with optional repeat.

use crate::config::MouseButton;
use crate::device::Mouse;
use crate::query_screen_size;
use anyhow::Result;

pub fn run(repeat: u32, btn: String) -> Result<()> {
    let (sw, sh) = query_screen_size();
    let mut m = Mouse::new(sw, sh)?;
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
