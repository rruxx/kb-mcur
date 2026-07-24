use tiny_skia::{IntRect, Rect};

#[derive(Clone)]
pub struct GridConfig {
    pub rows: u32,
    pub cols: u32,
    /// Row-major labels: labels[row][col]
    pub labels: Vec<Vec<char>>,
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
            rows: 3,
            cols: 3,
            labels: vec![
                vec!['q', 'w', 'e'],
                vec!['a', 's', 'd'],
                vec!['z', 'x', 'c'],
            ],
            line_color: [255, 255, 255, 200],
            label_color: [255, 255, 255, 255],
            bg_color: [0, 0, 0, 80],
            line_width: 2.0,
        }
    }
}

pub struct Cell {
    #[allow(dead_code)]
    pub rect: IntRect,
    pub center: (f32, f32),
    pub label: char,
}

pub struct Grid {
    #[allow(dead_code)]
    /// Top-left of the grid region
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
                let rw = (if col + 1 == config.cols { width - rx as u32 } else { cell_w as u32 }) as u32;
                let rh = (if row + 1 == config.rows { height - ry as u32 } else { cell_h as u32 }) as u32;
                let rect = IntRect::from_xywh(rx, ry, rw, rh).unwrap_or_else(|| {
                    Rect::from_xywh(rx as f32, ry as f32, rw as f32, rh as f32)
                        .unwrap()
                        .round_out()
                        .expect("round_out failed for grid cell")
                });
                cells.push(Cell {
                    rect,
                    center: (cx, cy),
                    label: config.labels[row as usize][col as usize],
                });
            }
        }

        Self {
            x,
            y,
            width,
            height,
            cols: config.cols,
            rows: config.rows,
            cells,
        }
    }
}
