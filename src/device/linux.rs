// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Linux device layer: kernel input ABI (`abi`) + physical read (`input`) + virtual write (`uinput`).

pub mod abi;
pub mod input;
pub mod uinput;

pub use input::KeyboardDev;
pub use uinput::Mouse;
