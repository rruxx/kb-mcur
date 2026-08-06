// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::OnceLock;

use fontdue::Font;

const FONT_DATA: &[u8] = include_bytes!("../assets/font/font.ttf");

/// Parsed once per process; `Font` is `Send + Sync` (plain data).
static FONT: OnceLock<Font> = OnceLock::new();

/// Access the embedded font, parsed lazily on first use.
pub fn font() -> &'static Font {
    FONT.get_or_init(|| {
        Font::from_bytes(FONT_DATA, fontdue::FontSettings::default())
            .expect("embedded font data corrupted")
    })
}
