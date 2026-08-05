// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Device layer: platform-specific input capture and virtual pointer.

pub mod pointer;

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "linux")]
pub use linux::Mouse;
#[cfg(target_os = "windows")]
pub use windows::Mouse;
