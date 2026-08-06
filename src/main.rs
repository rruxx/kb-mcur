// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::Result;
use clap::{CommandFactory, Parser};
use kursor::cli;

#[derive(Parser)]
#[command(
    name = env!("CARGO_PKG_NAME"),
    version,
    about = "Keyboard-driven mouse-cursor control.",
)]
struct Cli {
    #[command(subcommand)]
    cmd: Option<cli::Cmd>,
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let cli = Cli::parse();
    match cli.cmd {
        Some(cmd) => cli::dispatch(cmd),
        None => {
            Cli::command().print_help()?;
            Ok(())
        }
    }
}
