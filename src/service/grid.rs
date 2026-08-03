// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

use tiny_skia::{IntRect, Rect};

use crate::config;

#[derive(Clone)]
pub struct GridConfig {
    pub rows: u32,
    pub cols: u32,
    pub line_color: [u8; 4],
    pub label_color: [u8; 4],
    pub bg_color: [u8; 4],
    pub line_width: f32,
}

impl Default for GridConfig {
    fn default() -> Self {
        Self {
            rows: config::GRID_ROWS,
            cols: config::GRID_COLS,
            line_color: config::LINE_COLOR,
            label_color: config::LABEL_COLOR,
            bg_color: config::BG_COLOR,
            line_width: config::LINE_WIDTH,
        }
    }
}

#[derive(Default)]
pub struct GridFilter {
    input: String,
}

impl GridFilter {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
    #[must_use]
    pub fn input(&self) -> &str {
        &self.input
    }
    #[must_use]
    pub fn len(&self) -> usize {
        self.input.len()
    }
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.input.is_empty()
    }
    pub fn push(&mut self, ch: char) {
        self.input.push(ch);
    }
    pub fn pop(&mut self) {
        self.input.pop();
    }
    pub fn clear(&mut self) {
        self.input.clear();
    }

    #[must_use]
    pub fn matches(&self, label: &str) -> bool {
        label.starts_with(&self.input)
    }
}

pub struct Cell {
    pub rect: IntRect,
    pub center: (f32, f32),
    pub label: String,
}

pub struct Grid {
    pub cells: Vec<Cell>,
    pub cols: u32,
    pub rows: u32,
}

impl Grid {
    #[must_use]
    pub fn new(width: u32, height: u32, config: &GridConfig) -> Self {
        let cell_w = width as f32 / config.cols as f32;
        let cell_h = height as f32 / config.rows as f32;
        let mut cells = Vec::with_capacity((config.rows * config.cols) as usize);

        for row in 0..config.rows {
            for col in 0..config.cols {
                let cx = col as f32 * cell_w + cell_w / 2.0;
                let cy = row as f32 * cell_h + cell_h / 2.0;
                let rx = (col as f32 * cell_w) as i32;
                let ry = (row as f32 * cell_h) as i32;
                let rw = if col + 1 == config.cols {
                    width - rx as u32
                } else {
                    cell_w as u32
                };
                let rh = if row + 1 == config.rows {
                    height - ry as u32
                } else {
                    cell_h as u32
                };
                let rect = IntRect::from_xywh(rx, ry, rw, rh).unwrap_or_else(|| {
                    Rect::from_xywh(rx as f32, ry as f32, rw as f32, rh as f32)
                        .unwrap()
                        .round_out()
                        .expect("round_out")
                });
                cells.push(Cell {
                    rect,
                    center: (cx, cy),
                    label: config::cell_label(row, col),
                });
            }
        }
        Self {
            cells,
            cols: config.cols,
            rows: config.rows,
        }
    }

    #[must_use]
    pub fn cell_by_label(&self, label: &str) -> Option<&Cell> {
        self.cells.iter().find(|c| c.label == label)
    }
}

// ── Sub-modules ────────────────────────────────────────────────────

pub mod state;
pub mod init;
pub(crate) mod display;
pub(crate) mod env;
pub(crate) mod handle;
pub(crate) mod process;
pub(crate) mod selection;
pub(crate) mod watchdog;

pub(crate) use env::GridEnv;
pub(crate) use handle::{handle_navigating, handle_selecting};
pub(crate) use init::{GridPhase, GridStateMut};
pub(crate) use watchdog::watchdog;
pub use state::{DrawState, FONT_DATA, GridCtx, init_overlay};
pub use process::process_byte;
