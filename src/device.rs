// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Device layer: kernel input ABI (`abi`) + physical read client (`input`) + virtual write client (`uinput`).

pub mod abi;
pub mod input;
pub mod uinput;

pub use uinput::Mouse;
