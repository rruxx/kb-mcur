// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

//! glide-alpha — main-keyboard glide mode (stub, WIP).

use anyhow::Result;
use std::fs::File;

pub(crate) struct GlideAlpha {
    pub(crate) toggle: bool,
}

impl GlideAlpha {
    pub(crate) fn new() -> Self {
        Self { toggle: false }
    }

    pub(crate) fn active(&self) -> bool {
        self.toggle
    }
}

#[allow(clippy::unnecessary_wraps)]
pub(crate) fn handle_alpha_event(
    _ga: &mut GlideAlpha,
    _ptr_out: &mut File,
    _code: u16,
    _value: i32,
    _is_press: bool,
) -> Result<bool> {
    Ok(false)
}

#[allow(clippy::unnecessary_wraps)]
pub(crate) fn do_direction_alpha_tick(
    _ga: &mut GlideAlpha,
    _ptr_out: &mut File,
) -> Result<()> {
    Ok(())
}
