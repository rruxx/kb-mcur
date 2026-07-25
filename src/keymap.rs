// US-QWERTY keycode → ASCII byte mapping.
// Only the 30 keys used by grid mode: a-z, Enter, Space, Backspace, Escape.

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
pub const KEY_MINUS: u16 = 12;
pub const KEY_EQUAL: u16 = 13;
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
pub const KEY_LEFTBRACE: u16 = 26;
pub const KEY_RIGHTBRACE: u16 = 27;
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
pub const KEY_GRAVE: u16 = 41;
pub const KEY_LEFTSHIFT: u16 = 42;
pub const KEY_BACKSLASH: u16 = 43;
pub const KEY_Z: u16 = 44;
pub const KEY_X: u16 = 45;
pub const KEY_C: u16 = 46;
pub const KEY_V: u16 = 47;
pub const KEY_B: u16 = 48;
pub const KEY_N: u16 = 49;
pub const KEY_M: u16 = 50;
pub const KEY_COMMA: u16 = 51;
pub const KEY_DOT: u16 = 52;
pub const KEY_SLASH: u16 = 53;
pub const KEY_RIGHTSHIFT: u16 = 54;
pub const KEY_CAPSLOCK: u16 = 58;
pub const KEY_SPACE: u16 = 57;

#[derive(Default)]
pub struct ModState {
    shift: bool,
    caps: bool,
}

impl ModState {
    pub fn update(&mut self, code: u16, pressed: bool) {
        let on = pressed;
        match code {
            KEY_LEFTSHIFT | KEY_RIGHTSHIFT => self.shift = on,
            KEY_CAPSLOCK => if on { self.caps = !self.caps },
            _ => {}
        }
    }
}

/// Map keycode + modifiers → ASCII byte (a-z/linefeed/space/backspace/esc).
/// Returns None for unsupported / modifier-only keys.
pub fn map(code: u16, mods: &ModState) -> Option<u8> {
    let upper = mods.shift ^ mods.caps;
    match code {
        KEY_ENTER => Some(b'\n'),
        KEY_SPACE => Some(b' '),
        KEY_BACKSPACE => Some(0x7f),
        KEY_ESC => Some(0x1b),

        KEY_A => Some(if upper { b'A' } else { b'a' }),
        KEY_B => Some(if upper { b'B' } else { b'b' }),
        KEY_C => Some(if upper { b'C' } else { b'c' }),
        KEY_D => Some(if upper { b'D' } else { b'd' }),
        KEY_E => Some(if upper { b'E' } else { b'e' }),
        KEY_F => Some(if upper { b'F' } else { b'f' }),
        KEY_G => Some(if upper { b'G' } else { b'g' }),
        KEY_H => Some(if upper { b'H' } else { b'h' }),
        KEY_I => Some(if upper { b'I' } else { b'i' }),
        KEY_J => Some(if upper { b'J' } else { b'j' }),
        KEY_K => Some(if upper { b'K' } else { b'k' }),
        KEY_L => Some(if upper { b'L' } else { b'l' }),
        KEY_M => Some(if upper { b'M' } else { b'm' }),
        KEY_N => Some(if upper { b'N' } else { b'n' }),
        KEY_O => Some(if upper { b'O' } else { b'o' }),
        KEY_P => Some(if upper { b'P' } else { b'p' }),
        KEY_Q => Some(if upper { b'Q' } else { b'q' }),
        KEY_R => Some(if upper { b'R' } else { b'r' }),
        KEY_S => Some(if upper { b'S' } else { b's' }),
        KEY_T => Some(if upper { b'T' } else { b't' }),
        KEY_U => Some(if upper { b'U' } else { b'u' }),
        KEY_V => Some(if upper { b'V' } else { b'v' }),
        KEY_W => Some(if upper { b'W' } else { b'w' }),
        KEY_X => Some(if upper { b'X' } else { b'x' }),
        KEY_Y => Some(if upper { b'Y' } else { b'y' }),
        KEY_Z => Some(if upper { b'Z' } else { b'z' }),

        // Digits mapped for click-repeat count
        KEY_1 => Some(if upper { b'!' } else { b'1' }),
        KEY_2 => Some(if upper { b'@' } else { b'2' }),
        KEY_3 => Some(if upper { b'#' } else { b'3' }),
        KEY_4 => Some(if upper { b'$' } else { b'4' }),
        KEY_5 => Some(if upper { b'%' } else { b'5' }),
        KEY_6 => Some(if upper { b'^' } else { b'6' }),
        KEY_7 => Some(if upper { b'&' } else { b'7' }),
        KEY_8 => Some(if upper { b'*' } else { b'8' }),
        KEY_9 => Some(if upper { b'(' } else { b'9' }),
        KEY_0 => Some(if upper { b')' } else { b'0' }),

        _ => None,
    }
}
