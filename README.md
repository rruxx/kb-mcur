# kursor — Keyboard-driven mouse-cursor control

[中文](README-zh.md)

Three-layer progressive grid, glide-num (NumPad), glide-alpha (main keyboard), and single-shot CLI commands (move / moveto / click / pos).
Linux (X11 / wlroots / KDE / GNOME) and Windows (CLI + service).

## Motivation

- Built-in keyboard-driven mouse features on each platform are too weak — actually going mouse-free remains impractical.
- No uniform keyboard-driven mouse workflow across X11 and Wayland.
- [wl-kbptr](https://sr.ht/~q3cpma/wl-kbptr/) only supports wlroots compositors (Sway, Hyprland), not KDE or GNOME.
- KDE 5 had a toggle shortcut similar to Windows; it was removed in KDE 6.

kursor is a single binary that runs everywhere.

## Install

### Linux

```sh
git clone https://github.com/rruxx/kursor.git   # or https://gitee.com/rruxx/kursor.git
cd kursor
cargo build --release
sudo install -m755 target/release/kursor /usr/bin/
```

### Windows

Download `kursor-v{VERSION}-x86_64_v3-pc-windows-gnu.7z` from Releases — a single `kursor.exe`, no install.
(Cross-build from source: see `AGENTS.md`.)

## Requirements

| | |
| --- | --- |
| CPU | x86-64-v3+ (Zen3+ / AVX2) |
| OS | Linux / Windows |
| Rust | ≥ 1.80 |
| Linux | ≥ 5.0 (`/dev/uinput`), `sudo usermod -aG input $USER` |
| Windows | 7+ (subsystem 6.1) |

## Platform support

| Desktop | Support |
| --- | --- |
| wlroots / X11 | Full (native); X11 grid transparency needs a compositor |
| KDE / GNOME | Via XWayland |

**Tested:**
- **wlroots (niri, Sway):** everything works.
- **X11 (openbox):** works, except the grid overlay is opaque without a compositor.
- **KDE:** works via XWayland.
- **GNOME:** untested — expected to work via XWayland, but `kursor pos` may not.

## Usage

### service — glide-num + glide-alpha + grid

All three modes run on Linux and Windows; `--help` prints full key maps.

**glide-num (NumPad):** NumLock+KPEnter toggles. Direction keys move (accelerated); `/ * -` switch the button, NumLock+`/ 8 7 9` scrolls, NumLock+`* -` back/forward.

**glide-alpha (Main keyboard):** meta+shift+capslock toggles. `capslock+h/j/k/l` moves, `capslock+w/a/s/d` scrolls, `capslock+u/i/o` left/middle/right, `capslock+n/m` back/forward.

**grid (meta+capslock):** 27×27 grid in three layers (L1: 9×3, L2: 3×9, L3: 5×3). `u/i/o` clicks, `;` warps, 0-9 repeats, `p` resets; multi-monitor via `a-z`/Tab.

#### Windows

Double-clicking `kursor.exe` starts the service in the background — no console
window, exit via the tray icon's right-click **Exit** menu (or Task Manager). Running
`kursor service` in a terminal keeps it in the console (Ctrl+C to quit); other
commands print to the console.

`service` uses `WH_KEYBOARD_LL`, which the OS may silently freeze (auto-reinstalled) and cannot see elevated (UIPI) or secure-desktop input — platform limitations. The grid overlay draws into transparent click-through layered windows.

#### Linux (systemd)

```sh
sudo setcap cap_sys_admin+ep /usr/bin/kursor
sudo cp contrib/systemd/kursord.service /lib/systemd/system/
sudo systemctl daemon-reload && sudo systemctl enable --now kursord
```

### CLI (Linux & Windows)

| Command | Description |
| --- | --- |
| `kursor move -- 10 -5` | Relative move |
| `kursor moveto 500 300` | Absolute warp |
| `kursor click -r 3 M` | Click with repeat |
| `kursor pos` | Print cursor position and screen |

`kursor pos` is native on Windows/X11; on KDE Wayland it queries via KWin
scripting (`workspace.cursorPos`), and on wlroots/niri (Sway, Hyprland, …) via
per-output layer surfaces + a virtual-pointer poke — global coordinates on any
monitor, no external tools. (GNOME, see *Platform support*.)

`--help` prints full key maps.

## Architecture

Three layers on top of a thin CLI shell: `device/` (virtual pointer), `overlay/`
(rendering + cursor/screen query), `service/` (cross-mode state machine + main loops).

```
src/
├── main.rs        CLI entry (parses, dispatches)
├── lib.rs         Module declarations + re-exports
├── cli/           Command enum + one file per command (move/moveto/click/pos/service)
├── config.rs      Identity, key layouts, grid config, constants
├── keymap.rs      evdev keycodes, ModState, key map
├── font.rs        Embedded font
├── render.rs      Overlay rendering + text cache
├── debug.rs       Debug helpers (multi-monitor simulation)
├── device/        Virtual pointer per platform (Pointer/KeyboardOut traits)
│   ├── linux/     Kernel input ABI, keyboard grab, uinput Mouse
│   └── windows/   SendInput/SetCursorPos Mouse, VK→evdev map
├── overlay/       OverlayBackend trait + per-platform overlay + pos/screen query
│   ├── x11.rs     X11 RandR + SHAPE overlay
│   ├── wlr.rs     wlr-layer-shell overlay + virtual-pointer
│   ├── kde.rs     KDE Wayland pos (KWin scripting)
│   └── windows/   Per-monitor layered windows
└── service/       Cross-mode state machine + platform main loops
    ├── linux.rs   evdev grab loop
    ├── windows.rs WH_KEYBOARD_LL loop + liveness probe
    ├── glide_num.rs / glide_alpha.rs / dir.rs
    └── grid/      Grid model, rendering, state
```

## License

AGPL-3.0-or-later (see `LICENSE`; full text in `COPYING`).

Bundled third-party components — see `THIRD_PARTY_LICENSES`:
- **Hack font** (MIT / Bitstream Vera License), subset embedded as `assets/font/font.ttf`; full text in `assets/LICENSE-Hack`.

## See also

- [wl-kbptr](https://sr.ht/~q3cpma/wl-kbptr/) — keyboard-driven pointer for wlroots
- [keynav](https://github.com/jordansissel/keynav) — X11 keyboard-driven pointer
- [warpd](https://github.com/rvaiya/warpd) — modal keyboard-driven mouse
- [mouseless](https://github.com/jbensmann/mouseless) — keyboard-driven mouse control
- [xdotool](https://github.com/jordansissel/xdotool) / [ydotool](https://github.com/ReimuNotMoe/ydotool) — X11/Wayland automation
