// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

// All key mappings, grid config, and project-level identifiers in one place.

// ── Project identity ─────────────────────────────────────────────────

use const_format::concatcp;

pub const PROJECT_NAME: &str = "key-mcursor";
pub const SERVICE: &str = concatcp!(PROJECT_NAME, "d");
pub const SOCKET: &str = concatcp!("/run/", SERVICE, ".sock");

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

pub const GRID_ROWS: u32 = 26;
pub const GRID_COLS: u32 = 26;

// ── Colours ──────────────────────────────────────────────────────────

pub const LINE_COLOR: [u8; 4] = [255, 255, 255, 40];
pub const LABEL_COLOR: [u8; 4] = [192, 255, 192, 192];
pub const BG_COLOR: [u8; 4] = [0, 0, 0, 144];
pub const LINE_WIDTH: f32 = 1.0;

// ── Font sizing ──────────────────────────────────────────────────────

pub const FONT_SIZE_MIN: f32 = 6.0;
pub const FONT_SIZE_MAX: f32 = 14.0;
pub const FONT_ROW_DIVISOR: f32 = 1.8;

// ── Level 3  sub-grid keys (4×2) ─────────────────────────────────────

pub const SUBGRID_LABELS: [[char; 4]; 2] = [['q', 'w', 'e', 'r'], ['a', 's', 'd', 'f']];

#[must_use]
pub fn sub_key_index(ch: char) -> Option<usize> {
    for (row, cols) in SUBGRID_LABELS.iter().enumerate() {
        for (col, &label) in cols.iter().enumerate() {
            if label == ch {
                return Some(row * 4 + col);
            }
        }
    }
    None
}

#[must_use]
pub fn is_sub_key(ch: char) -> bool {
    sub_key_index(ch).is_some()
}

// ── Level 3+  cursor movement keys ───────────────────────────────────

pub const CURSOR_STEP: i32 = 3;
pub const MAX_CURSOR_STEP: i32 = 50;

/// Every N consecutive timer ticks, step doubles.
pub const CURSOR_ACCEL_INTERVAL: u32 = 5;

/// Maximum number of doubling shifts before the step cap takes over.
pub const MAX_ACCEL_SHIFTS: u32 = 10;

/// Accelerated step size: doubles every `CURSOR_ACCEL_INTERVAL` repeats,
/// bounded by `MAX_ACCEL_SHIFTS` shifts and then `MAX_CURSOR_STEP`.
#[must_use]
pub fn cursor_speed(repeat_count: u32) -> i32 {
    let shifts = (repeat_count / CURSOR_ACCEL_INTERVAL).min(MAX_ACCEL_SHIFTS);
    let scale = (1u32 << shifts) as i32;
    CURSOR_STEP.saturating_mul(scale).min(MAX_CURSOR_STEP)
}

// ── Action keys ──────────────────────────────────────────────────────

pub const CLICK_KEYS: [(char, u8); 3] = [('j', 1), ('k', 2), ('l', 3)];

/// Returns button number (1=left, 2=middle, 3=right) if `ch` is a click key.
#[must_use]
pub fn action_key(ch: char) -> Option<u8> {
    for &(k, btn) in &CLICK_KEYS {
        if k == ch {
            return Some(btn);
        }
    }
    None
}

// ── Levels 4-7  bisect keys (2×2) ────────────────────────────────────

pub const BISECT_LABELS: [[char; 2]; 2] = [['e', 'r'], ['d', 'f']];

#[must_use]
pub fn quad_key_index(ch: char) -> Option<usize> {
    for (row, cols) in BISECT_LABELS.iter().enumerate() {
        for (col, &label) in cols.iter().enumerate() {
            if label == ch {
                return Some(row * 2 + col);
            }
        }
    }
    None
}

#[must_use]
pub fn is_quad_key(ch: char) -> bool {
    quad_key_index(ch).is_some()
}

/// Shrink (x, y, w, h) to the selected quadrant.
/// idx: 0=TL, 1=TR, 2=BL, 3=BR.
#[must_use]
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

// ── Timing ───────────────────────────────────────────────────────────

pub const CLICK_INTERVAL_MS: u64 = 100;
pub const UINPUT_CREATE_WAIT_MS: u64 = 50;
pub const MOVE_WAIT_MS: u64 = 20;
pub const POLL_INTERVAL_MS: u64 = 16;

// ── USB HID buttons ───────────────────────────────────────────────────

pub const BTN_LEFT: u16 = 0x110;
pub const BTN_MIDDLE: u16 = 0x112;
pub const BTN_RIGHT: u16 = 0x111;

/// USB HID button code for mouse buttons (1=left, 2=middle, 3=right).
#[must_use]
pub const fn btn_code(button: u8) -> u16 {
    match button {
        1 => BTN_LEFT,
        2 => BTN_MIDDLE,
        _ => BTN_RIGHT,
    }
}
