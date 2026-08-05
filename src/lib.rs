// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

#![warn(clippy::pedantic)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::similar_names)]
#![allow(clippy::too_many_lines)]

pub mod config;
pub mod debug;
pub mod font;
pub mod keyboard;
pub mod keymap;
pub mod overlay;
pub mod render;
pub mod service;
pub mod uinput;

pub use overlay::{Monitor, Overlay, query_screen_size};
pub use service::grid::state::{DrawState, GridCtx, GridPhase, GridStateMut, MonitorList};
pub use service::grid::{Grid, GridConfig, GridFilter};
pub use uinput::Mouse;
