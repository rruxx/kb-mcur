// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Windows device layer: virtual pointer via Win32 APIs.

pub mod mouse;

pub use mouse::Mouse;

/// Screen dimensions for the primary display (for CLI use).
#[must_use]
pub fn query_screen_size() -> (u16, u16) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
    let w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let h = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    (w as u16, h as u16)
}
