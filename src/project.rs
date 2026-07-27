// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

// All project-level identifiers — change only here when renaming.

pub const BIN: &str = "key-cursor";
pub const SERVICE: &str = "key-cursord";
pub const SOCKET: &str = "/run/key-cursord.sock";

pub const DEV_ABS: &str = "key-cursor";
pub const DEV_REL: &str = "key-cursor-rel";
pub const DEV_KBD: &str = "key-cursor-kbd";
pub const DEV_PTR: &str = "key-cursor-ptr";

pub const GRID_WINDOW: &str = "key-cursor-grid";
pub const WLR_NAME: &str = "key-cursor";
pub const SHM_PREFIX: &str = "key-cursor-shm";

/// Uinput devices created by this project start with this prefix.
pub const OWN_PREFIX: &[u8] = b"key-cursor";
