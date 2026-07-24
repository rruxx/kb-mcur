use tiny_skia::{IntRect, Rect};

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
            rows: 26,
            cols: 26,
            line_color: [255, 255, 255, 40],
            label_color: [192, 255, 192, 192],
            bg_color: [0, 0, 0, 144],
            line_width: 1.0,
        }
    }
}

#[derive(Default)]
pub struct GridFilter {
    input: String,
}

impl GridFilter {
    pub fn new() -> Self { Self::default() }
    pub fn input(&self) -> &str { &self.input }
    pub fn len(&self) -> usize { self.input.len() }
    pub fn push(&mut self, ch: char) { self.input.push(ch); }
    pub fn pop(&mut self) { self.input.pop(); }
    pub fn clear(&mut self) { self.input.clear(); }

    pub fn matches(&self, label: &str) -> bool {
        label.to_ascii_lowercase().starts_with(&self.input.to_ascii_lowercase())
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
                cells.push(Cell {
                    rect,
                    center: (cx, cy),
                    label: format!("{row_ch}{col_ch}"),
                });
            }
        }
        Self { cells, cols: config.cols, rows: config.rows }
    }

    pub fn cell_by_label(&self, label: &str) -> Option<&Cell> {
        self.cells.iter().find(|c| c.label == label)
    }
}

// ── Sub-grid labels (4×2) ──────────────────────────────────────────

pub const SUBGRID_LABELS: [[char; 4]; 2] = [
    ['q', 'w', 'e', 'r'],
    ['a', 's', 'd', 'f'],
];

// ── Key index helpers ──────────────────────────────────────────────

pub fn sub_key_index(ch: char) -> Option<usize> {
    match ch {
        'q' => Some(0), 'w' => Some(1), 'e' => Some(2), 'r' => Some(3),
        'a' => Some(4), 's' => Some(5), 'd' => Some(6), 'f' => Some(7),
        _ => None,
    }
}

pub fn quad_key_index(ch: char) -> Option<usize> {
    match ch {
        'e' => Some(0), 'r' => Some(1), 'd' => Some(2), 'f' => Some(3),
        _ => None,
    }
}

/// Shrink (x, y, w, h) to the selected quadrant.
/// idx: 0=TL, 1=TR, 2=BL, 3=BR.
pub fn quad_shrink((x, y, w, h): (f32, f32, f32, f32), idx: usize) -> (f32, f32, f32, f32) {
    let hw = w * 0.5;
    let hh = h * 0.5;
    match idx {
        0 => (x,       y,        hw, hh),
        1 => (x + hw,  y,        hw, hh),
        2 => (x,       y + hh,   hw, hh),
        3 => (x + hw,  y + hh,   hw, hh),
        _ => (x, y, w, h),
    }
}
