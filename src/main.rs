// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};
use kursor::config::MOVE_WAIT_MS;
use kursor::overlay::query_screen_size;
use kursor::uinput::Mouse;

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
    /// Relative move: x>0 right, y>0 down.
    #[command(after_help = "\
Example:
  kursor move -- 10 -5     right 10 px, up 5 px
  kursor move -- -20 0     left 20 px")]
    Move { x: i32, y: i32 },

    /// Absolute warp to screen pixels (x, y).
    #[command(
        name = "moveto",
        after_help = "\
Example:
  kursor moveto 500 300   warp to (500, 300)
  kursor moveto 0 0       top-left corner"
    )]
    MoveTo { x: i16, y: i16 },

    /// Mouse click: L(eft)|M(iddle)|R(ight), -r N for repeat.
    #[command(after_help = "\
Example:
  kursor click L          left click once
  kursor click -r 3 M     middle click 3 times
  kursor click R          right click once")]
    Click {
        #[arg(short = 'r', default_value = "1")]
        repeat: u32,
        btn: String,
    },

    /// Dual-mode daemon: mouse (NumPad) + grid (Main keypad).
    #[command(after_help = "\
mouse (NumPad):
  NumLock+KPEnter   Toggle mouse mode
  8/2/4/6           Move up/down/left/right
  7/9/1/3           Diagonal move
  5                 Click
  0                 Hold
  .                 Release
  +                 Double-click
  / * -             Switch btn5 to L/M/R
  NumLock+/ 8 7 9   Scroll up/down/left/right
  NumLock+* -       Back/forward

grid (Main keypad):
  meta+capslock     Toggle grid overlay
  a-z letter        Select monitor (multi-monitor)
  tab               Switch monitor
  a-z (2 letters)   26×26 grid cell
  q/w/e/r/a/s/d/f   4×2 sub-grid
  e/r/d/f           2×2 quadrant
  Space/Enter        Warp & reset
  j/k/l              Click L/M/R & reset
  0-9 prefix         Repeat (e.g. 3j)
  Esc               Reset filter")]
    Service,
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
        Some(Cmd::Service) => {
            kursor::service::run_service()?;
        }
        None => {
            Cli::command().print_help()?;
        }
    }
    Ok(())
}
