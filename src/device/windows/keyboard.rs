// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Windows virtual-keycode → evdev keycode mapping.
//!
//! The low-level hook reports physical keys (VK codes are layout-independent,
//! matching evdev's physical-key semantics). The extended-key prefix (0xE0 in
//! `scanCode`) disambiguates keys that share a VK code — most importantly the
//! numpad Enter vs. the main Enter.

use crate::keymap::{
    KEY_0, KEY_A, KEY_APOSTROPHE, KEY_B, KEY_BACKSPACE, KEY_C, KEY_CAPSLOCK, KEY_COMMA, KEY_D,
    KEY_DOT, KEY_E, KEY_ENTER, KEY_ESC, KEY_F, KEY_G, KEY_H, KEY_I, KEY_J, KEY_K, KEY_KP0, KEY_KP1,
    KEY_KP2, KEY_KP3, KEY_KP4, KEY_KP5, KEY_KP6, KEY_KP7, KEY_KP8, KEY_KP9, KEY_KPASTERISK,
    KEY_KPDOT, KEY_KPENTER, KEY_KPMINUS, KEY_KPPLUS, KEY_KPSLASH, KEY_L, KEY_LEFTALT, KEY_LEFTCTRL,
    KEY_LEFTMETA, KEY_LEFTSHIFT, KEY_M, KEY_N, KEY_NUMLOCK, KEY_O, KEY_P, KEY_Q, KEY_R,
    KEY_RIGHTALT, KEY_RIGHTCTRL, KEY_RIGHTMETA, KEY_RIGHTSHIFT, KEY_S, KEY_SEMICOLON, KEY_SPACE,
    KEY_T, KEY_TAB, KEY_U, KEY_V, KEY_W, KEY_X, KEY_Y, KEY_Z,
};

/// Map a Windows virtual-key code + scan code to an evdev keycode.
/// `extended` is the low-level-hook `LLKHF_EXTENDED` flag (set for numpad
/// Enter, arrows, etc.) — combined with the 0xE0 scan-code prefix for safety.
#[must_use]
pub fn vk_to_evdev(vk: u32, scan: u32, extended: bool) -> Option<u16> {
    let ext = extended || scan >> 8 == 0xE0;
    match vk {
        // Backspace / Tab / Enter / Esc / Space
        0x08 => Some(KEY_BACKSPACE),
        0x09 => Some(KEY_TAB),
        0x0D => Some(if ext { KEY_KPENTER } else { KEY_ENTER }),
        0x1B => Some(KEY_ESC),
        0x20 => Some(KEY_SPACE),
        // Caps / Num lock
        0x14 => Some(KEY_CAPSLOCK),
        0x90 => Some(KEY_NUMLOCK),
        // Win (meta)
        0x5B => Some(KEY_LEFTMETA),
        0x5C => Some(KEY_RIGHTMETA),
        // Shift / Ctrl / Alt — explicit left/right VK codes
        0xA0 => Some(KEY_LEFTSHIFT),
        0xA1 => Some(KEY_RIGHTSHIFT),
        0xA2 => Some(KEY_LEFTCTRL),
        0xA3 => Some(KEY_RIGHTCTRL),
        0xA4 => Some(KEY_LEFTALT),
        0xA5 => Some(KEY_RIGHTALT),
        // Generic modifier VK codes — disambiguate via scan code
        0x10 => Some(if scan == 0x36 {
            KEY_RIGHTSHIFT
        } else {
            KEY_LEFTSHIFT
        }),
        0x11 => Some(if ext { KEY_RIGHTCTRL } else { KEY_LEFTCTRL }),
        0x12 => Some(if ext { KEY_RIGHTALT } else { KEY_LEFTALT }),
        // Letters
        0x41..=0x5A => letter_key(vk),
        // Main digit row
        0x30 => Some(KEY_0),
        0x31..=0x39 => Some((vk - 0x30 + 1) as u16), // KEY_1..=KEY_9
        // Numpad
        0x60 => Some(KEY_KP0),
        0x61 => Some(KEY_KP1),
        0x62 => Some(KEY_KP2),
        0x63 => Some(KEY_KP3),
        0x64 => Some(KEY_KP4),
        0x65 => Some(KEY_KP5),
        0x66 => Some(KEY_KP6),
        0x67 => Some(KEY_KP7),
        0x68 => Some(KEY_KP8),
        0x69 => Some(KEY_KP9),
        0x6A => Some(KEY_KPASTERISK),
        0x6B => Some(KEY_KPPLUS),
        0x6D => Some(KEY_KPMINUS),
        0x6E => Some(KEY_KPDOT),
        0x6F => Some(KEY_KPSLASH),
        // US punctuation
        0xBA => Some(KEY_SEMICOLON),  // ;
        0xBC => Some(KEY_COMMA),      // ,
        0xBE => Some(KEY_DOT),        // .
        0xDE => Some(KEY_APOSTROPHE), // '
        _ => None,
    }
}

/// `VK_A`..=`VK_Z` → evdev letter keycodes (evdev encodes by physical position).
fn letter_key(vk: u32) -> Option<u16> {
    const LETTERS: [u16; 26] = [
        KEY_A, KEY_B, KEY_C, KEY_D, KEY_E, KEY_F, KEY_G, KEY_H, KEY_I, KEY_J, KEY_K, KEY_L, KEY_M,
        KEY_N, KEY_O, KEY_P, KEY_Q, KEY_R, KEY_S, KEY_T, KEY_U, KEY_V, KEY_W, KEY_X, KEY_Y, KEY_Z,
    ];
    LETTERS.get((vk - 0x41) as usize).copied()
}
