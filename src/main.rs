// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};
use kursor::config::{MOVE_WAIT_MS, MouseButton};
use kursor::device::Mouse;
use kursor::query_screen_size;

#[derive(Parser)]
#[command(
    name = env!("CARGO_PKG_NAME"),
    version,
    about = "Keyboard-driven mouse-cursor control.",
)]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Relative move: x>0 right, y>0 down.
    #[command(after_help = include_str!("../assets/help-move.txt"))]
    Move { x: i32, y: i32 },

    /// Absolute warp to screen pixels (x, y).
    #[command(
        name = "moveto",
        after_help = include_str!("../assets/help-moveto.txt")
    )]
    MoveTo { x: i16, y: i16 },

    /// Mouse click: L(eft)|M(iddle)|R(ight), -r N for repeat.
    #[command(after_help = include_str!("../assets/help-click.txt"))]
    Click {
        #[arg(short = 'r', default_value = "1")]
        repeat: u32,
        btn: String,
    },

    /// Triple-mode daemon: glide-num (NumPad) + glide-alpha (Main keyboard) + grid.
    #[command(after_help = include_str!("../assets/service-help.txt"))]
    Service,
}

fn parse_button(s: &str) -> Result<MouseButton> {
    match s {
        "L" | "l" | "left" | "1" => Ok(MouseButton::Left),
        "M" | "m" | "middle" | "2" => Ok(MouseButton::Middle),
        "R" | "r" | "right" | "3" => Ok(MouseButton::Right),
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
            m.click(parse_button(&btn)?, repeat)?;
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
