// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::{Context, Result};

#[cfg(target_os = "linux")]
use super::device_perm::{display_session_uid, setup_display_env};
use super::state::MonitorList;
use crate::debug;
use crate::overlay::Overlay;

// ── Grid initialization ─────────────────────────────────────────────

/// Connect to the display, switching to the session user on Linux (where the
/// service runs as root); Windows connects directly.
pub fn connect_as_user() -> Result<Overlay> {
    #[cfg(target_os = "linux")]
    {
        let Some(session_uid) = display_session_uid() else {
            anyhow::bail!("no display session detected");
        };
        setup_display_env(session_uid);

        let saved = nix::unistd::geteuid();
        nix::unistd::seteuid(nix::unistd::Uid::from_raw(session_uid)).context("seteuid")?;
        let result = crate::overlay::connect();
        let _ = nix::unistd::seteuid(saved);
        result
    }
    #[cfg(target_os = "windows")]
    {
        crate::overlay::connect()
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        anyhow::bail!("unsupported platform")
    }
}

pub fn enter_grid() -> Result<(Overlay, MonitorList)> {
    let overlay = connect_as_user()?;
    let named = overlay
        .named_monitors()
        .context("failed to query monitors")?;
    if named.is_empty() {
        anyhow::bail!("no active monitors detected");
    }
    let monitors: MonitorList = debug::clone_monitors(&named);

    Ok((overlay, monitors))
}
