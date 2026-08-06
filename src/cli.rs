// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

//! CLI commands — one module per command (`move`, `moveto`, `click`, `pos`, `service`).

pub mod click;
pub mod r#move;
pub mod moveto;
pub mod pos;
pub mod service;

use anyhow::Result;
use clap::Subcommand;

/// A CLI subcommand.
#[derive(Subcommand)]
pub enum Cmd {
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

    /// Print the current cursor position and the screen it is on.
    #[command(after_help = include_str!("../assets/help-pos.txt"))]
    Pos,

    /// Triple-mode daemon: glide-num (`NumPad`) + glide-alpha (main keyboard) + grid.
    #[command(after_help = include_str!("../assets/service-help.txt"))]
    Service,
}

/// Run a CLI subcommand.
pub fn dispatch(cmd: Cmd) -> Result<()> {
    match cmd {
        Cmd::Move { x, y } => r#move::run(x, y),
        Cmd::MoveTo { x, y } => moveto::run(x, y),
        Cmd::Click { repeat, btn } => click::run(repeat, btn),
        Cmd::Pos => pos::run(),
        Cmd::Service => service::run(),
    }
}
