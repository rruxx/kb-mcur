// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

//! `kursor service` — the three-mode daemon.

use anyhow::Result;

pub fn run() -> Result<()> {
    crate::service::run_service()
}
