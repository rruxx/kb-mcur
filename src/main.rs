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

    /// Triple-mode daemon: glide-num (NumPad) + glide-alpha (Main keyboard) + grid.
    #[command(after_help = "\
glide-num (NumPad):
  NumLock+KPEnter   Toggle glide-num
  8/2/4/6           Move up/down/left/right
  7/9/1/3           Diagonal move
  5                 Click
  0                 Hold
  .                 Release
  +                 Double-click
  / * -             Switch btn5 to L/M/R
  NumLock+/ 8 7 9   Scroll up/down/left/right
  NumLock+* -       Back/forward

glide-alpha (Main keyboard):
  meta+shift+capslock  Toggle glide-alpha
  ctrl+h/j/k/l         Move left/down/up/right
  shift+h/j/k/l        Scroll left/down/up/right
  ctrl+u/i             Back/forward
  Space                Left button (press=down, release=up)
  ;                    Right button (press=down, release=up)
  '                    Middle button (press=down, release=up)

grid (Main keyboard):
  meta+capslock     Toggle grid overlay
  a-z letter        Select monitor (multi-monitor)
  tab               Switch monitor
  layer1 key        9×3 region (main keyboard layout)
  layer2 key        3×9 sub-region (clockwise‑90°)
  layer3 key        5×3 fine (left-half keyboard, q-t/a-g/z-b)
  Backspace     L3 → L2, L2 → L1
  Enter              Warp & reset
  j/k/l              Click L/M/R & reset
  0-9 prefix         Repeat (e.g. 3j)
  Esc                Reset filter")]
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
