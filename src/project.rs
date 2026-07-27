// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

// All project-level identifiers — change only here when renaming.

pub const BIN: &str = "key-mcursor";
pub const SERVICE: &str = "key-mcursord";
pub const SOCKET: &str = "/run/key-mcursord.sock";

pub const UINPUT_NAME: &str = "key-mcursor";

/// Kernel UINPUT_MAX_NAME_SIZE — struct layout must match this.
pub const UINPUT_NAME_MAXLEN: usize = 80;

pub const DEV_ABS: &str = "key-mcursor";
pub const DEV_REL: &str = "key-mcursor-rel";
pub const DEV_KBD: &str = "key-mcursor-kbd";
pub const DEV_PTR: &str = "key-mcursor-ptr";

pub const GRID_WINDOW: &str = "key-mcursor-grid";
pub const WLR_NAME: &str = "key-mcursor";
pub const SHM_PREFIX: &str = "key-mcursor-shm";

/// Uinput devices created by this project start with this prefix.
pub const OWN_PREFIX: &[u8] = b"key-mcursor";
