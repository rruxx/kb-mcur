use tiny_skia::{IntRect, Rect};

#[derive(Clone)]
pub struct GridConfig {
    pub rows: u32,
    pub cols: u32,
    /// RGBA grid line colour
    pub line_color: [u8; 4],
    /// RGBA label text colour
    pub label_color: [u8; 4],
    /// RGBA background fill
    pub bg_color: [u8; 4],
    /// Line stroke width in pixels
    pub line_width: f32,
}

impl Default for GridConfig {
    fn default() -> Self {
        Self {
            rows: 26,
            cols: 26,
            line_color: [255, 255, 255, 40],
            label_color: [192, 255, 192, 192],
            bg_color: [0, 0, 0, 144],
            line_width: 1.0,
        }
    }
}

// ── Filter ──────────────────────────────────────────────────────────

/// Tracks a prefix typed by the user to narrow down grid cells.
#[derive(Default)]
pub struct GridFilter {
    input: String,
}

impl GridFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn input(&self) -> &str {
        &self.input
    }

    pub fn len(&self) -> usize {
        self.input.len()
    }

    /// Append a character to the filter.
    pub fn push(&mut self, ch: char) {
        self.input.push(ch);
    }

    /// Remove the last character (Backspace).
    pub fn pop(&mut self) {
        self.input.pop();
    }

    pub fn clear(&mut self) {
        self.input.clear();
    }

    #[allow(dead_code)]
    pub fn set(&mut self, s: &str) {
        self.input.clear();
        self.input.push_str(s);
    }

    /// True when the cell label starts with the current input prefix.
    pub fn matches(&self, label: &str) -> bool {
        label.to_ascii_lowercase().starts_with(&self.input.to_ascii_lowercase())
    }
}

// ── Main 26×26 grid ─────────────────────────────────────────────────

pub struct Cell {
    #[allow(dead_code)]
    pub rect: IntRect,
    pub center: (f32, f32),
    /// Two-letter label: row_letter + col_letter, e.g. "aa" … "zz"
    pub label: String,
}

pub struct Grid {
    #[allow(dead_code)]
    pub x: i32,
    #[allow(dead_code)]
    pub y: i32,
    #[allow(dead_code)]
    pub width: u32,
    #[allow(dead_code)]
    pub height: u32,
    pub cols: u32,
    pub rows: u32,
    pub cells: Vec<Cell>,
}

impl Grid {
    pub fn new(x: i32, y: i32, width: u32, height: u32, config: &GridConfig) -> Self {
        let cell_w = width as f32 / config.cols as f32;
        let cell_h = height as f32 / config.rows as f32;
        let mut cells = Vec::with_capacity((config.rows * config.cols) as usize);

        for row in 0..config.rows {
            for col in 0..config.cols {
                let cx = col as f32 * cell_w + cell_w / 2.0;
                let cy = row as f32 * cell_h + cell_h / 2.0;
                let rx = (col as f32 * cell_w) as i32;
                let ry = (row as f32 * cell_h) as i32;
                let rw = if col + 1 == config.cols { width - rx as u32 } else { cell_w as u32 };
                let rh = if row + 1 == config.rows { height - ry as u32 } else { cell_h as u32 };
                let rect = IntRect::from_xywh(rx, ry, rw, rh).unwrap_or_else(|| {
                    Rect::from_xywh(rx as f32, ry as f32, rw as f32, rh as f32)
                        .unwrap()
                        .round_out()
                        .expect("round_out failed")
                });

                let row_ch = (b'a' + row as u8) as char;
                let col_ch = (b'a' + col as u8) as char;
                let mut label = String::with_capacity(2);
                label.push(row_ch);
                label.push(col_ch);

                cells.push(Cell {
                    rect,
                    center: (cx, cy),
                    label,
                });
            }
        }

        Self { x, y, width, height, cols: config.cols, rows: config.rows, cells }
    }

    /// Find cell by its 2-letter label.
    pub fn cell_by_label(&self, label: &str) -> Option<&Cell> {
        self.cells.iter().find(|c| c.label == label)
    }
}

// ── Level-3 sub-grid (4×2 within a selected cell) ────────────────────

/// Labels for the 4×2 sub-grid (row-major).
pub const SUBGRID_LABELS: [[char; 4]; 2] = [
    ['a', 's', 'd', 'f'],
    ['j', 'k', 'l', ';'],
];

/// A sub-cell inside the final 4×2 grid.  Coordinates are pixel-space
/// (relative to the monitor), computed from the parent cell rect.
pub struct SubCell {
    pub center: (f32, f32),
    #[allow(dead_code)]
    pub label: char,
}

/// Build the eight sub-cells that partition `parent` (the Cell selected
/// in the 26×26 grid).  Labels come from [`SUBGRID_LABELS`].
pub fn subgrid_cells(parent: &Cell) -> [SubCell; 8] {
    let r = &parent.rect;
    let w = r.width() as f32 / 4.0;
    let h = r.height() as f32 / 2.0;
    let mut i = 0;
    std::array::from_fn(|_| {
        let row = i / 4;
        let col = i % 4;
        let cell = SubCell {
            center: (
                r.x() as f32 + (col as f32 + 0.5) * w,
                r.y() as f32 + (row as f32 + 0.5) * h,
            ),
            label: SUBGRID_LABELS[row][col],
        };
        i += 1;
        cell
    })
}

