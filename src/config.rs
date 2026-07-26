// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

// All key mappings and grid config in one place.

pub const GRID_ROWS: u32 = 26;
pub const GRID_COLS: u32 = 26;

pub const LINE_COLOR: [u8; 4] = [255, 255, 255, 40];
pub const LABEL_COLOR: [u8; 4] = [192, 255, 192, 192];
pub const BG_COLOR: [u8; 4] = [0, 0, 0, 144];
pub const LINE_WIDTH: f32 = 1.0;

// ── Level 3  sub-grid keys (4×2) ───────────────────────────────────

pub const SUBGRID_LABELS: [[char; 4]; 2] = [['q', 'w', 'e', 'r'], ['a', 's', 'd', 'f']];

pub fn sub_key_index(ch: char) -> Option<usize> {
    for row in 0..2 {
        for col in 0..4 {
            if SUBGRID_LABELS[row][col] == ch {
                return Some(row * 4 + col);
            }
        }
    }
    None
}

pub fn is_sub_key(ch: char) -> bool {
    sub_key_index(ch).is_some()
}

// ── Level 3+  cursor movement keys ──────────────────────────────────

pub const CURSOR_STEP: i32 = 3;
pub const MAX_CURSOR_STEP: i32 = 50;
pub const CTRL_BOOST_STEP: i32 = 80;

/// Every N consecutive timer ticks, step doubles.
pub const CURSOR_ACCEL_INTERVAL: u32 = 5;

/// Returns (dx, dy) unit direction for movement keys (w/a/s/d).
pub fn direction_delta(ch: char) -> Option<(i32, i32)> {
    match ch {
        'w' => Some((0, -1)),
        'a' => Some((-1, 0)),
        's' => Some((0, 1)),
        'd' => Some((1, 0)),
        _ => None,
    }
}

pub fn is_direction_key(ch: char) -> bool {
    direction_delta(ch).is_some()
}

/// Accelerated step size: doubles every `CURSOR_ACCEL_INTERVAL` repeats, capped at `MAX_CURSOR_STEP`.
pub fn cursor_speed(repeat_count: u32) -> i32 {
    let shifts = (repeat_count / CURSOR_ACCEL_INTERVAL).min(10);
    let scale = (1u32 << shifts) as i32;
    CURSOR_STEP.saturating_mul(scale).min(MAX_CURSOR_STEP)
}

// ── Action keys ────────────────────────────────────────────────────

pub const TOGGLE_KEYS: [(char, u8); 3] = [('u', 1), ('i', 2), ('o', 3)];
pub const CLICK_KEYS: [(char, u8); 3] = [('j', 1), ('k', 2), ('l', 3)];

/// Returns (button, is_click) if `ch` is an action key.
pub fn action_key(ch: char) -> Option<(u8, bool)> {
    for &(k, btn) in &TOGGLE_KEYS {
        if k == ch {
            return Some((btn, false));
        }
    }
    for &(k, btn) in &CLICK_KEYS {
        if k == ch {
            return Some((btn, true));
        }
    }
    None
}

// ── Levels 4-7  bisect keys (2×2) ──────────────────────────────────

pub const BISECT_LABELS: [[char; 2]; 2] = [['e', 'r'], ['d', 'f']];

pub fn quad_key_index(ch: char) -> Option<usize> {
    for row in 0..2 {
        for col in 0..2 {
            if BISECT_LABELS[row][col] == ch {
                return Some(row * 2 + col);
            }
        }
    }
    None
}

pub fn is_quad_key(ch: char) -> bool {
    quad_key_index(ch).is_some()
}

/// Shrink (x, y, w, h) to the selected quadrant.
/// idx: 0=TL, 1=TR, 2=BL, 3=BR.
pub fn quad_shrink((x, y, w, h): (f32, f32, f32, f32), idx: usize) -> (f32, f32, f32, f32) {
    let hw = w * 0.5;
    let hh = h * 0.5;
    match idx {
        0 => (x, y, hw, hh),
        1 => (x + hw, y, hw, hh),
        2 => (x, y + hh, hw, hh),
        3 => (x + hw, y + hh, hw, hh),
        _ => (x, y, w, h),
    }
}

pub const CLICK_INTERVAL_MS: u64 = 100;

/// USB HID button code for mouse buttons (1=left, 2=middle, 3=right).
pub const fn btn_code(button: u8) -> u16 {
    match button {
        1 => 0x110, // BTN_LEFT
        2 => 0x112, // BTN_MIDDLE
        _ => 0x111, // BTN_RIGHT
    }
}
