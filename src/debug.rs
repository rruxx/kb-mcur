// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

// Debug helpers — controlled via environment variables.

use crate::overlay::Monitor;
use log::info;

/// When `KURSOR_DEBUG_MONITORS=N` (N > 1) and a single real monitor
/// is detected, clone it N times so multi-display logic can be tested.
///
/// Usage:
/// ```sh
/// KURSOR_DEBUG_MONITORS=3 cargo run -- service
/// ```
#[must_use]
pub fn clone_monitors(monitors: &[Monitor]) -> Vec<Monitor> {
    let debug_n: usize = debug_monitor_count();

    if debug_n > 1 && monitors.len() == 1 {
        info!("debug: cloning monitor to {debug_n} displays");
        let m = &monitors[0];
        (0..debug_n).map(|_i| Monitor { ..m.clone() }).collect()
    } else {
        monitors.to_vec()
    }
}

#[must_use]
pub fn debug_monitor_count() -> usize {
    std::env::var("KURSOR_DEBUG_MONITORS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}
