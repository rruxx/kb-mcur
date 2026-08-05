// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::{Context, Result};

use super::device_perm::{display_session_uid, setup_display_env};
use super::state::MonitorList;
use crate::debug;
use crate::overlay::Overlay;

// ── Grid initialization ─────────────────────────────────────────────

pub fn connect_as_user() -> Result<Overlay> {
    let Some(session_uid) = display_session_uid() else {
        anyhow::bail!("no display session detected");
    };
    setup_display_env(session_uid);

    let saved = nix::unistd::geteuid();
    nix::unistd::seteuid(nix::unistd::Uid::from_raw(session_uid)).context("seteuid")?;
    let result = Overlay::connect();
    let _ = nix::unistd::seteuid(saved);
    result
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
