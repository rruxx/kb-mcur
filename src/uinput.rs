// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

//! uinput virtual device layer.

pub mod mouse;
pub mod raw;

pub use mouse::Mouse;
pub use raw::{
    ABS_X, ABS_Y, EV_ABS, EV_KEY, EV_REL, EV_SYN, InputAbsinfo, InputEvent, REL_HWHEEL, REL_WHEEL,
    REL_X, REL_Y, SYN_REPORT, UinputAbsSetup, UinputSetup, ZERO_TIMEVAL, create_virt_device,
    ui_abs_setup, ui_dev_create, ui_dev_destroy, ui_dev_setup, ui_set_absbit, ui_set_evbit,
    ui_set_keybit, ui_set_relbit, write_event, write_event_raw,
};
