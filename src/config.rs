// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

// All key mappings, grid config, and project-level identifiers in one place.

// ── Project identity ─────────────────────────────────────────────────

use const_format::concatcp;

pub const PROJECT_NAME: &str = "kursor";

/// Kernel `UINPUT_MAX_NAME_SIZE` — struct layout must match this.
pub const UINPUT_NAME_MAXLEN: usize = 80;

pub const UINPUT_NAME: &str = PROJECT_NAME;
pub const DEV_ABS: &str = PROJECT_NAME;
pub const DEV_REL: &str = concatcp!(PROJECT_NAME, "-rel");
pub const DEV_KBD: &str = concatcp!(PROJECT_NAME, "-kbd");
pub const DEV_PTR: &str = concatcp!(PROJECT_NAME, "-ptr");
pub const GRID_WINDOW: &str = concatcp!(PROJECT_NAME, "-grid");
pub const WLR_NAME: &str = PROJECT_NAME;
pub const SHM_PREFIX: &str = concatcp!(PROJECT_NAME, "-shm");

/// Uinput devices created by this project start with this prefix.
pub const OWN_PREFIX: &[u8] = PROJECT_NAME.as_bytes();

// ── Screen fallback ──────────────────────────────────────────────────

pub const FALLBACK_WIDTH: u16 = 1920;
pub const FALLBACK_HEIGHT: u16 = 1080;

// ── Grid geometry ────────────────────────────────────────────────────

pub const GRID_ROWS: u32 = 27;
pub const GRID_COLS: u32 = 27;

// ── Colours ──────────────────────────────────────────────────────────

pub const LINE_COLOR: [u8; 4] = [255, 255, 255, 40];
pub const LABEL_COLOR: [u8; 4] = [192, 255, 192, 192];
pub const BG_COLOR: [u8; 4] = [0, 0, 0, 144];
pub const LINE_WIDTH: f32 = 1.0;

// ── Font sizing ──────────────────────────────────────────────────────

pub const FONT_SIZE_MIN: f32 = 6.0;
pub const FONT_SIZE_MAX: f32 = 14.0;
pub const FONT_ROW_DIVISOR: f32 = 1.8;

// ── Two-layer grid key layouts ────────────────────────────────────────

/// Layer 1: 9 columns × 3 rows (main keyboard physical layout).
pub const L1_KEYS: [[char; 9]; 3] = [
    ['q', 'w', 'e', 'r', 't', 'y', 'u', 'i', 'o'],
    ['a', 's', 'd', 'f', 'g', 'h', 'j', 'k', 'l'],
    ['z', 'x', 'c', 'v', 'b', 'n', 'm', ',', '.'],
];

/// Layer 2: 3 columns × 9 rows (clockwise‑90° rotation of L1).
pub const L2_KEYS: [[char; 3]; 9] = [
    ['z', 'a', 'q'],
    ['x', 's', 'w'],
    ['c', 'd', 'e'],
    ['v', 'f', 'r'],
    ['b', 'g', 't'],
    ['n', 'h', 'y'],
    ['m', 'j', 'u'],
    [',', 'k', 'i'],
    ['.', 'l', 'o'],
];

/// Layer 3: 5 columns × 3 rows (left half of main keypad).
pub const L3_KEYS: [[char; 5]; 3] = [
    ['q', 'w', 'e', 'r', 't'],
    ['a', 's', 'd', 'f', 'g'],
    ['z', 'x', 'c', 'v', 'b'],
];

#[must_use]
fn key_pos<const R: usize, const C: usize>(
    keys: &[[char; C]; R],
    ch: char,
) -> Option<(usize, usize)> {
    for (r, row) in keys.iter().enumerate() {
        for (c, &k) in row.iter().enumerate() {
            if k == ch {
                return Some((r, c));
            }
        }
    }
    None
}

#[must_use]
pub fn l3_key_pos(ch: char) -> Option<(usize, usize)> {
    key_pos(&L3_KEYS, ch)
}

#[must_use]
pub fn l1_key_pos(ch: char) -> Option<(usize, usize)> {
    key_pos(&L1_KEYS, ch)
}

#[must_use]
pub fn l2_key_pos(ch: char) -> Option<(usize, usize)> {
    key_pos(&L2_KEYS, ch)
}

/// Full label for cell at global (row, col) in the 27×27 grid.
///
/// L2 is a 9×3 clockwise rotation of L1 (3×9). Each L1 cell spans a 9×3 block
/// of the grid, indexed by its row/col position; within a block, the 9×3 L2
/// grid selects the sub-cell. All divisors come from the key layouts so the
/// mapping stays correct if a layout changes.
#[must_use]
pub fn cell_label(row: u32, col: u32) -> String {
    let (r, c) = (row as usize, col as usize);
    let l1 = &L1_KEYS[r / L2_KEYS.len()][c / L2_KEYS[0].len()];
    let l2 = &L2_KEYS[r % L2_KEYS.len()][c % L2_KEYS[0].len()];
    format!("{l1}{l2}")
}

// ── Cursor movement acceleration ─────────────────────────────────────

pub const CURSOR_STEP: i32 = 3;
pub const MAX_CURSOR_STEP: i32 = 50;

pub const CURSOR_ACCEL_INTERVAL: u32 = 5;
pub const MAX_ACCEL_SHIFTS: u32 = 10;

#[must_use]
pub fn cursor_speed(repeat_count: u32) -> i32 {
    let shifts = (repeat_count / CURSOR_ACCEL_INTERVAL).min(MAX_ACCEL_SHIFTS);
    let scale = (1u32 << shifts) as i32;
    CURSOR_STEP.saturating_mul(scale).min(MAX_CURSOR_STEP)
}

// ── Action keys ──────────────────────────────────────────────────────

/// Mouse buttons in the shared 1=left / 2=middle / 3=right order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
}

impl MouseButton {
    /// Shared 1/2/3 ordering used by [`CLICK_KEYS`] and grid logging
    /// (1=left, 2=middle, 3=right). Not a Linux `BTN_*` code — see
    /// [`hid_button_code`].
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        match self {
            Self::Left => 1,
            Self::Middle => 2,
            Self::Right => 3,
        }
    }
}

pub const CLICK_KEYS: [(char, MouseButton); 3] = [
    ('j', MouseButton::Left),
    ('k', MouseButton::Middle),
    ('l', MouseButton::Right),
];

#[must_use]
pub fn action_key(ch: char) -> Option<MouseButton> {
    for &(k, btn) in &CLICK_KEYS {
        if k == ch {
            return Some(btn);
        }
    }
    None
}

// ── Timing ───────────────────────────────────────────────────────────

pub const CLICK_INTERVAL_MS: u64 = 100;
pub const UINPUT_CREATE_WAIT_MS: u64 = 50;
pub const MOVE_WAIT_MS: u64 = 20;

// ── USB HID buttons ───────────────────────────────────────────────────

pub const BTN_LEFT: u16 = 0x110;
pub const BTN_RIGHT: u16 = 0x111;
pub const BTN_MIDDLE: u16 = 0x112;
pub const BTN_SIDE: u16 = 0x113;
pub const BTN_EXTRA: u16 = 0x114;

/// USB HID button code for mouse buttons.
#[must_use]
pub const fn hid_button_code(button: MouseButton) -> u16 {
    match button {
        MouseButton::Left => BTN_LEFT,
        MouseButton::Middle => BTN_MIDDLE,
        MouseButton::Right => BTN_RIGHT,
    }
}
