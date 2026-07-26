// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::Result;
use clap::{Parser, Subcommand};
use kb_mcur::overlay::query_screen_size;
use kb_mcur::uinput::Mouse;

#[derive(Parser)]
#[command(
    name = "kb-mcur",
    about = "Keyboard-driven mouse cursor control.",
    after_help = "Examples:\n  \
                   kb-mcur                            Start interactive grid\n  \
                   kb-mcur mouse                      Direct w/a/s/d cursor control\n  \
                  kb-mcur move -- 10 -5              Move right 10px, up 5px\n  \
                  kb-mcur moveto 500 300             Warp to (500, 300)\n  \
                  kb-mcur click L                    Left click\n  \
                  kb-mcur click -r 3 M               Middle click 3 times\n  \
                  kb-mcur click R                    Right click\n\n  \
                  Negative values require -- prefix.\n\n  \
                  kb-mcur needs /dev/input/event* and /dev/uinput access.\n  \
                  Add your user to the input group:\n  \
                  $ sudo usermod -aG input $USER\n  \
                  Then log out and back in."
)]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Relative move: x>0 right, y>0 down
    Move { x: i32, y: i32 },

    /// Absolute positioning to screen coordinates (x, y)
    #[command(name = "moveto")]
    MoveTo { x: i16, y: i16 },

    /// Mouse click: L|M|R, -r N for repeat
    Click {
        #[arg(short = 'r', default_value = "1")]
        repeat: u32,
        btn: String,
    },

    /// Interactive keyboard grid (default)
    Grid,

    /// Direct w/a/s/d cursor movement (no grid)
    Mouse,
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
    let cli = Cli::parse();
    let (sw, sh) = query_screen_size();

    match cli.cmd {
        Some(Cmd::Move { x, y }) => {
            let mut m = Mouse::new(sw, sh)?;
            m.move_rel(x, y)?;
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        Some(Cmd::MoveTo { x, y }) => {
            let mut m = Mouse::new(sw, sh)?;
            m.warp(x, y)?;
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        Some(Cmd::Click { repeat, btn }) => {
            let mut m = Mouse::new(sw, sh)?;
            m.click(btn_code(&btn)?, repeat)?;
        }
        Some(Cmd::Grid) | None => {
            kb_mcur::run()?;
        }
        Some(Cmd::Mouse) => {
            kb_mcur::run_mouse()?;
        }
    }
    Ok(())
}
