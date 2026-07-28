// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};
use key_mcursor::config::MOVE_WAIT_MS;
use key_mcursor::overlay::query_screen_size;
use key_mcursor::uinput::Mouse;

#[derive(Parser)]
#[command(
    name = env!("CARGO_PKG_NAME"),
    about = "Keyboard-driven mouse-cursor control.",
)]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Relative move: x>0 right, y>0 down
    #[command(after_help = concat!("Example:\n  ", env!("CARGO_PKG_NAME"), " move -- 10 -5"))]
    Move { x: i32, y: i32 },

    /// Absolute positioning to screen pixels (x, y)
    #[command(
        name = "moveto",
        after_help = concat!("Example:\n  ", env!("CARGO_PKG_NAME"), " moveto 500 300")
    )]
    MoveTo { x: i16, y: i16 },

    /// Mouse click: L|M|R, -r N for repeat
    #[command(after_help = concat!("Example:\n  ", env!("CARGO_PKG_NAME"), " click -r 3 M  Middle click 3 times"))]
    Click {
        #[arg(short = 'r', default_value = "1")]
        repeat: u32,
        btn: String,
    },

    /// Interactive grid: 26×26 → 4×2 → 2×2 progressive refinement
    #[command(after_help = "Keys:\n  a-z           26×26 grid (2 letters)\n  q/w/e/r/a/s/d/f  4×2 sub-grid (1 letter)\n  e/r/d/f       2×2 quadrant\n  Space/Enter    Warp + exit\n  j/k/l          Click left/middle/right\n  0-9 digit prefix  Repeat (e.g. 3j)\n  Esc            Reset grid")]
    Grid,

    /// NumPad mouse navigation service
    #[command(
        name = "kp-nav",
        after_help = "NumLock+KPEnter toggles mouse control on/off.\n\nKeys (mouse mode):\n  8/2/4/6        Move up/down/left/right\n  7/9/1/3        Diagonal move\n  5              Click (press=down, release=up)\n  0              Hold button down\n  .              Release button\n  +              Double-click\n  / * -          Switch btn5 to left/middle/right\n\nAll non-NumPad keys are forwarded to the compositor.\nHold direction keys to auto-accelerate (3→50 px)."
    )]
    KpNav,
}

fn btn_code(s: &str) -> Result<u8> {
    match s {
        "L" | "l" | "left" | "1" => Ok(1),
        "M" | "m" | "middle" | "2" => Ok(2),
        "R" | "r" | "right" | "3" => Ok(3),
        other => anyhow::bail!("unknown button: {other} (use L|M|R)"),
    }
}

fn main() -> Result<()> {
    env_logger::init();
    let cli = Cli::parse();

    match cli.cmd {
        Some(Cmd::Move { x, y }) => {
            let (sw, sh) = query_screen_size();
            let mut m = Mouse::new(sw, sh)?;
            m.move_rel(x, y)?;
            std::thread::sleep(std::time::Duration::from_millis(MOVE_WAIT_MS));
        }
        Some(Cmd::MoveTo { x, y }) => {
            let (sw, sh) = query_screen_size();
            let mut m = Mouse::new(sw, sh)?;
            m.warp(x, y)?;
            std::thread::sleep(std::time::Duration::from_millis(MOVE_WAIT_MS));
        }
        Some(Cmd::Click { repeat, btn }) => {
            let (sw, sh) = query_screen_size();
            let mut m = Mouse::new(sw, sh)?;
            m.click(btn_code(&btn)?, repeat)?;
        }
        Some(Cmd::Grid) => {
            key_mcursor::run()?;
        }
        None => {
            Cli::command().print_help()?;
        }
        Some(Cmd::KpNav) => {
            key_mcursor::kpnav::run()?;
        }
    }
    Ok(())
}
