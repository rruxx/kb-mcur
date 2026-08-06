// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

// ── Standard 105-key USB HID → Linux evdev keycodes ─────────────────

/// Highest evdev keycode this project registers on the virtual keyboard.
/// 255 covers the standard keyset (letters, digits, numpad, modifiers).
pub const KEYCODE_MAX: u16 = 255;

// pub const KEY_RESERVED: u16 = 0;
pub const KEY_ESC: u16 = 1;
pub const KEY_1: u16 = 2;
pub const KEY_2: u16 = 3;
pub const KEY_3: u16 = 4;
pub const KEY_4: u16 = 5;
pub const KEY_5: u16 = 6;
pub const KEY_6: u16 = 7;
pub const KEY_7: u16 = 8;
pub const KEY_8: u16 = 9;
pub const KEY_9: u16 = 10;
pub const KEY_0: u16 = 11;
// pub const KEY_MINUS: u16 = 12;
// pub const KEY_EQUAL: u16 = 13;
pub const KEY_BACKSPACE: u16 = 14;
pub const KEY_TAB: u16 = 15;
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
// pub const KEY_LEFTBRACE: u16 = 26;
// pub const KEY_RIGHTBRACE: u16 = 27;
pub const KEY_ENTER: u16 = 28;
pub const KEY_LEFTCTRL: u16 = 29;
pub const KEY_A: u16 = 30;
pub const KEY_S: u16 = 31;
pub const KEY_D: u16 = 32;
pub const KEY_F: u16 = 33;
pub const KEY_G: u16 = 34;
pub const KEY_H: u16 = 35;
pub const KEY_J: u16 = 36;
pub const KEY_K: u16 = 37;
pub const KEY_L: u16 = 38;
pub const KEY_SEMICOLON: u16 = 39;
pub const KEY_APOSTROPHE: u16 = 40;
// pub const KEY_GRAVE: u16 = 41;
pub const KEY_LEFTSHIFT: u16 = 42;
// pub const KEY_BACKSLASH: u16 = 43;
pub const KEY_Z: u16 = 44;
pub const KEY_X: u16 = 45;
pub const KEY_C: u16 = 46;
pub const KEY_V: u16 = 47;
pub const KEY_B: u16 = 48;
pub const KEY_N: u16 = 49;
pub const KEY_M: u16 = 50;
pub const KEY_COMMA: u16 = 51;
pub const KEY_DOT: u16 = 52;
// pub const KEY_SLASH: u16 = 53;
pub const KEY_RIGHTSHIFT: u16 = 54;
pub const KEY_KPASTERISK: u16 = 55;
pub const KEY_LEFTALT: u16 = 56;
pub const KEY_SPACE: u16 = 57;
pub const KEY_CAPSLOCK: u16 = 58;
// pub const KEY_F1: u16 = 59;
// pub const KEY_F2: u16 = 60;
// pub const KEY_F3: u16 = 61;
// pub const KEY_F4: u16 = 62;
// pub const KEY_F5: u16 = 63;
// pub const KEY_F6: u16 = 64;
// pub const KEY_F7: u16 = 65;
// pub const KEY_F8: u16 = 66;
// pub const KEY_F9: u16 = 67;
// pub const KEY_F10: u16 = 68;
pub const KEY_NUMLOCK: u16 = 69;
// pub const KEY_SCROLLLOCK: u16 = 70;
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

// 84: gap (no standard keycode)

// pub const KEY_ZENKAKUHANKAKU: u16 = 85;
// pub const KEY_102ND: u16 = 86;
// pub const KEY_F11: u16 = 87;
// pub const KEY_F12: u16 = 88;
// pub const KEY_RO: u16 = 89;
// pub const KEY_KATAKANA: u16 = 90;
// pub const KEY_HIRAGANA: u16 = 91;
// pub const KEY_HENKAN: u16 = 92;
// pub const KEY_KATAKANAHIRAGANA: u16 = 93;
// pub const KEY_MUHENKAN: u16 = 94;
// pub const KEY_KPJPCOMMA: u16 = 95;

pub const KEY_KPENTER: u16 = 96;
pub const KEY_RIGHTCTRL: u16 = 97;
pub const KEY_KPSLASH: u16 = 98;
// pub const KEY_SYSRQ: u16 = 99;
pub const KEY_RIGHTALT: u16 = 100;

// pub const KEY_LINEFEED: u16 = 101;

// pub const KEY_HOME: u16 = 102;
// pub const KEY_UP: u16 = 103;
// pub const KEY_PAGEUP: u16 = 104;
// pub const KEY_LEFT: u16 = 105;
// pub const KEY_RIGHT: u16 = 106;
// pub const KEY_END: u16 = 107;
// pub const KEY_DOWN: u16 = 108;
// pub const KEY_PAGEDOWN: u16 = 109;
// pub const KEY_INSERT: u16 = 110;
// pub const KEY_DELETE: u16 = 111;

// pub const KEY_MACRO: u16 = 112;
// pub const KEY_MUTE: u16 = 113;
// pub const KEY_VOLUMEDOWN: u16 = 114;
// pub const KEY_VOLUMEUP: u16 = 115;
// pub const KEY_POWER: u16 = 116;
// pub const KEY_KPEQUAL: u16 = 117;
// pub const KEY_KPPLUSMINUS: u16 = 118;
// pub const KEY_PAUSE: u16 = 119;
// pub const KEY_SCALE: u16 = 120;
// pub const KEY_KPCOMMA: u16 = 121;
// pub const KEY_HANGEUL: u16 = 122;
// pub const KEY_HANJA: u16 = 123;
// pub const KEY_YEN: u16 = 124;

pub const KEY_LEFTMETA: u16 = 125;
pub const KEY_RIGHTMETA: u16 = 126;
// pub const KEY_COMPOSE: u16 = 127;

/// Modifier-key state (shift/ctrl/meta/alt), one bool per key.
#[derive(Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct ModState {
    pub shift: bool,
    pub ctrl: bool,
    pub meta: bool,
    pub alt: bool,
}

impl ModState {
    pub fn update(&mut self, code: u16, pressed: bool) {
        match code {
            KEY_LEFTSHIFT | KEY_RIGHTSHIFT => self.shift = pressed,
            KEY_LEFTCTRL | KEY_RIGHTCTRL => self.ctrl = pressed,
            KEY_LEFTMETA | KEY_RIGHTMETA => self.meta = pressed,
            KEY_LEFTALT | KEY_RIGHTALT => self.alt = pressed,
            _ => {}
        }
    }
}

/// Map keycode + modifiers → ASCII byte (a-z/linefeed/space/backspace/esc).
/// Returns None for unsupported / modifier-only keys.
#[must_use]
pub fn key_map(code: u16, mods: &ModState) -> Option<u8> {
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
            return Some(if upper { v.to_ascii_uppercase() } else { v });
        }
    }

    match code {
        KEY_ENTER => Some(b'\n'),
        KEY_BACKSPACE => Some(0x7f),
        KEY_ESC => Some(0x1b),
        KEY_COMMA => Some(b','),
        KEY_DOT => Some(b'.'),
        KEY_SEMICOLON => Some(b';'),

        // evdev keycodes KEY_1..=KEY_0 (wrapping).
        KEY_1..=KEY_0 => {
            const SHIFT: &[u8] = b")!@#$%^&*(";
            let idx = ((code - 1) % 10) as usize;
            Some(if upper { SHIFT[idx] } else { b'0' + idx as u8 })
        }

        _ => None,
    }
}
