// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::io::Write;

use anyhow::Result;

use crate::grid::GridFilter;

pub fn prompt(f: &GridFilter) {
    let s = f.input();
    eprint!("\r[{s}]{}", " ".repeat(7usize.saturating_sub(s.len())));
    let _ = std::io::stderr().flush();
}

pub fn raw_on(fd: i32) -> Result<libc::termios> {
    let mut orig: libc::termios = unsafe { std::mem::zeroed() };
    if unsafe { libc::tcgetattr(fd, &mut orig) } != 0 {
        anyhow::bail!("tcgetattr");
    }
    let mut raw = orig;
    raw.c_lflag &= !(libc::ICANON | libc::ECHO);
    raw.c_cc[libc::VMIN] = 1;
    raw.c_cc[libc::VTIME] = 0;
    if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &raw) } != 0 {
        anyhow::bail!("tcsetattr");
    }
    Ok(orig)
}

pub fn raw_off(fd: i32, orig: libc::termios) -> Result<()> {
    if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &orig) } != 0 {
        anyhow::bail!("tcsetattr restore");
    }
    Ok(())
}
