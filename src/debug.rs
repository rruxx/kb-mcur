// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

// Debug helpers — controlled via environment variables.

use log::info;

/// When `KEY_MCURSOR_DEBUG_MONITORS=N` (N > 1) and a single real monitor
/// is detected, clone it N times so multi-display logic can be tested.
///
/// Usage:
/// ```sh
/// KEY_MCURSOR_DEBUG_MONITORS=3 cargo run -- grid
/// ```
pub fn clone_monitors(
    monitors: Vec<(i32, i32, u16, u16)>,
) -> Vec<(i32, i32, u16, u16)> {
    let debug_n: usize = debug_monitor_count();

    if debug_n > 1 && monitors.len() == 1 {
        info!("debug: cloning monitor to {debug_n} displays");
        let m = monitors[0];
        (0..debug_n).map(|_i| (m.0, m.1, m.2, m.3)).collect()
    } else {
        monitors
    }
}

/// Select a monitor name for debug vs. real.
pub fn monitor_name(
    debug_n: usize,
    monitor_idx: usize,
    real_name: &str,
) -> String {
    if debug_n > 1 {
        format!("debug-{}", monitor_idx + 1)
    } else {
        real_name.to_owned()
    }
}

pub fn debug_monitor_count() -> usize {
    std::env::var("KEY_MCURSOR_DEBUG_MONITORS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}
