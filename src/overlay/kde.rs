// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

//! KDE Wayland cursor position via `KWin` scripting (`org.kde.KWin`).
//!
//! Wayland has no global pointer-query API; `KWin` exposes `workspace.cursorPos`
//! to its scripting API. A small script is loaded and run over D-Bus, printing
//! the position to the journal, which we read back with `journalctl`.

use std::process::Command;

use anyhow::{Result, bail};

const SCRIPT_BODY: &str =
    r#"console.info('*{ "x":' + workspace.cursorPos.x + ', "y":' + workspace.cursorPos.y + ' }');"#;

/// Query the cursor position via `KWin` scripting.
pub fn cursor_pos() -> Result<(i32, i32)> {
    let script_path = std::env::temp_dir().join("kursor-cursorpos.js");
    std::fs::write(&script_path, SCRIPT_BODY)?;

    let script_id = load_script(&script_path)?;
    run_script(script_id)?;

    // Give kwin_wayland time to flush console.info to the journal.
    std::thread::sleep(std::time::Duration::from_millis(200));

    read_coords()
}

fn load_script(path: &std::path::Path) -> Result<i32> {
    let arg = format!("string:{}", path.display());
    let out = Command::new("dbus-send")
        .args([
            "--print-reply",
            "--dest=org.kde.KWin",
            "/Scripting",
            "org.kde.kwin.Scripting.loadScript",
            &arg,
        ])
        .output()?;
    if !out.status.success() {
        bail!(
            "dbus loadScript failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    parse_script_id(&String::from_utf8_lossy(&out.stdout))
}

fn parse_script_id(out: &str) -> Result<i32> {
    out.lines()
        .find(|l| l.contains("int32"))
        .and_then(|l| l.split_whitespace().find_map(|t| t.parse().ok()))
        .ok_or_else(|| anyhow::anyhow!("no script id in dbus reply"))
}

fn run_script(script_id: i32) -> Result<()> {
    let object = format!("/Scripting/Script{script_id}");
    let out = Command::new("dbus-send")
        .args([
            "--print-reply",
            "--dest=org.kde.KWin",
            &object,
            "org.kde.kwin.Script.run",
        ])
        .output()?;
    if !out.status.success() {
        bail!("dbus run failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    Ok(())
}

fn read_coords() -> Result<(i32, i32)> {
    let out = Command::new("journalctl")
        .args([
            "_COMM=kwin_wayland",
            "--since",
            "2 seconds ago",
            "-o",
            "cat",
        ])
        .output()?;
    if !out.status.success() {
        bail!(
            "journalctl failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    parse_coords(&String::from_utf8_lossy(&out.stdout))
}

fn parse_coords(out: &str) -> Result<(i32, i32)> {
    out.lines()
        .rev()
        .find_map(|l| {
            let x = l
                .split("\"x\":")
                .nth(1)?
                .split(',')
                .next()?
                .trim()
                .parse()
                .ok()?;
            let y = l
                .split("\"y\":")
                .nth(1)?
                .trim()
                .trim_end_matches('}')
                .trim()
                .parse()
                .ok()?;
            Some((x, y))
        })
        .ok_or_else(|| anyhow::anyhow!("no cursor coords in kwin journal"))
}
