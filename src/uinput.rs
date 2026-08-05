// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

//! uinput virtual device layer.

pub mod mouse;
pub mod raw;

pub use mouse::Mouse;
pub use raw::{
    EV_KEY, EV_REL, EV_SYN, REL_HWHEEL, REL_WHEEL, REL_X, REL_Y, SYN_REPORT, create_virt_device,
    write_event, write_event_raw,
};
