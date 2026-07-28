// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

// US-QWERTY keycode → ASCII byte mapping.

pub const KEY_ESC: u16 = 1;
pub const KEY_BACKSPACE: u16 = 14;
pub const KEY_Q: u16 = 16;
pub const KEY_W: u16 = 17;
pub const KEY_E: u16 = 18;
pub const KEY_R: u16 = 19;
pub const KEY_T: u16 = 20;
pub const KEY_Y: u16 = 21;
pub const KEY_U: u16 = 22;
pub const KEY_I: u16 = 23;
pub const KEY_O: u16 = 24;
pub const KEY_P: u16 = 25;
pub const KEY_ENTER: u16 = 28;
pub const KEY_A: u16 = 30;
pub const KEY_S: u16 = 31;
pub const KEY_D: u16 = 32;
pub const KEY_F: u16 = 33;
pub const KEY_G: u16 = 34;
pub const KEY_H: u16 = 35;
pub const KEY_J: u16 = 36;
pub const KEY_K: u16 = 37;
pub const KEY_L: u16 = 38;
pub const KEY_LEFTSHIFT: u16 = 42;
pub const KEY_Z: u16 = 44;
pub const KEY_X: u16 = 45;
pub const KEY_C: u16 = 46;
pub const KEY_V: u16 = 47;
pub const KEY_B: u16 = 48;
pub const KEY_N: u16 = 49;
pub const KEY_M: u16 = 50;
pub const KEY_RIGHTSHIFT: u16 = 54;
pub const KEY_SPACE: u16 = 57;
pub const KEY_NUMLOCK: u16 = 69;
pub const KEY_KP7: u16 = 71;
pub const KEY_KP8: u16 = 72;
pub const KEY_KP9: u16 = 73;
pub const KEY_KPMINUS: u16 = 74;
pub const KEY_KP4: u16 = 75;
pub const KEY_KP5: u16 = 76;
pub const KEY_KP6: u16 = 77;
pub const KEY_KPPLUS: u16 = 78;
pub const KEY_KP1: u16 = 79;
pub const KEY_KP2: u16 = 80;
pub const KEY_KP3: u16 = 81;
pub const KEY_KP0: u16 = 82;
pub const KEY_KPDOT: u16 = 83;
pub const KEY_KPASTERISK: u16 = 55;
pub const KEY_KPSLASH: u16 = 98;
pub const KEY_KPENTER: u16 = 96;

#[derive(Default)]
pub struct ModState {
    pub shift: bool,
}

impl ModState {
    pub fn update(&mut self, code: u16, pressed: bool) {
        match code {
            KEY_LEFTSHIFT | KEY_RIGHTSHIFT => self.shift = pressed,
            _ => {}
        }
    }
}

/// Map keycode + modifiers → ASCII byte (a-z/linefeed/space/backspace/esc).
/// Returns None for unsupported / modifier-only keys.
#[must_use]
pub fn map(code: u16, mods: &ModState) -> Option<u8> {
    const LUT: [(u16, u8); 26] = [
        (KEY_Q, b'q'),
        (KEY_W, b'w'),
        (KEY_E, b'e'),
        (KEY_R, b'r'),
        (KEY_T, b't'),
        (KEY_Y, b'y'),
        (KEY_U, b'u'),
        (KEY_I, b'i'),
        (KEY_O, b'o'),
        (KEY_P, b'p'),
        (KEY_A, b'a'),
        (KEY_S, b's'),
        (KEY_D, b'd'),
        (KEY_F, b'f'),
        (KEY_G, b'g'),
        (KEY_H, b'h'),
        (KEY_J, b'j'),
        (KEY_K, b'k'),
        (KEY_L, b'l'),
        (KEY_Z, b'z'),
        (KEY_X, b'x'),
        (KEY_C, b'c'),
        (KEY_V, b'v'),
        (KEY_B, b'b'),
        (KEY_N, b'n'),
        (KEY_M, b'm'),
    ];
    let upper = mods.shift;
    for &(k, v) in &LUT {
        if code == k {
            return Some(if upper { v - 32 } else { v });
        }
    }

    match code {
        KEY_ENTER => Some(b'\n'),
        KEY_SPACE => Some(b' '),
        KEY_BACKSPACE => Some(0x7f),
        KEY_ESC => Some(0x1b),

        2..=11 => {
            const SHIFT: &[u8] = b")!@#$%^&*(";
            let idx = ((code - 1) % 10) as usize;
            Some(if upper { SHIFT[idx] } else { b'0' + idx as u8 })
        }

        _ => None,
    }
}
