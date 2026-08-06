# kursor — Keyboard-driven mouse-cursor control

[中文](README.d/README-zh.md)

Three-layer progressive grid, glide-num (NumPad), glide-alpha (main keyboard), and single-shot CLI commands (move / moveto / click).
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
| Rust | ≥ 1.80 |
| CPU | x86-64-v3 (Zen3+ / AVX2) |
| Linux | ≥ 5.0 (`/dev/uinput`), `sudo usermod -aG input $USER` |
| Windows | 7+ (subsystem 6.1) |
| Overlay | X11 with compositor; Wayland native |

## Usage

### service — glide-num + glide-alpha + grid

All three modes run on Linux and Windows; `--help` prints full key maps.

**glide-num (NumPad):** NumLock+KPEnter toggles. Direction keys move (accelerated); `/ * -` switch the button, NumLock+`/ 8 7 9` scrolls, NumLock+`* -` back/forward.

**glide-alpha (Main keyboard):** meta+shift+capslock toggles. `ctrl+h/j/k/l` moves, `shift+h/j/k/l` scrolls, `ctrl+u/i` back/forward, `ctrl+Space/;/'` clicks.

**grid (meta+capslock):** 27×27 grid in three layers (L1: 9×3, L2: 3×9, L3: 5×3). `j/k/l` clicks, Enter warps, 0-9 repeats, Backspace/Esc reset; multi-monitor via `a-z`/Tab.

#### Windows

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

`--help` prints full key maps.

## Architecture

```
src/
├── main.rs        CLI entry
├── lib.rs         Module declarations
├── config.rs      Project identity, key mappings, grid config
├── keymap.rs      US-QWERTY keycode → ASCII map
├── font.rs        Embedded font (assets/font.ttf)
├── render.rs      Overlay rendering + text drawing
├── debug.rs       Debug helpers (multi-monitor simulation)
├── device.rs      Device layer entry: platform-split pointer
├── device/
│   ├── linux.rs       Linux pointer re-exports
│   ├── linux/
│   │   ├── abi.rs     Kernel input ABI: structs + ioctls
│   │   ├── input.rs   Physical keyboard grab + hot-plug
│   │   └── uinput.rs  Virtual pointer (Mouse)
│   ├── windows.rs     Windows pointer re-exports
│   └── windows/
│       └── mouse.rs   SendInput / SetCursorPos virtual pointer
├── overlay.rs     OverlayBackend trait + platform connect()
├── overlay/
│   ├── x11.rs     X11 RandR + SHAPE overlay
│   ├── wlr.rs     wlr-layer-shell Wayland overlay
│   └── windows.rs Screen-size query (stage 1)
├── service.rs     Main event loop + dispatch (Linux)
└── service/       (Linux)
    ├── glide_num.rs   NumPad glide-num
    ├── glide_alpha.rs Main-keyboard glide-alpha
    ├── dir.rs         Shared direction bitmask + glide ticks
    ├── grid.rs        Grid data model + re-exports
    └── grid/
        ├── base.rs        Base-layer rendering (BG + L1 + labels)
        ├── device_perm.rs Session detection + device permission fix
        ├── display.rs     Display update + L2/L3/L4 rendering
        ├── env.rs         GridEnv state + toggle/input API
        ├── init.rs        Grid service init + connection
        ├── process.rs     Cursor actions + region geometry
        ├── selection.rs   Multi-monitor selection UI
        └── state.rs       Grid state + input handlers (GridStateMut)
```

## License

AGPL-3.0-or-later

## See also

- [wl-kbptr](https://sr.ht/~q3cpma/wl-kbptr/) — keyboard-driven pointer for wlroots
- [keynav](https://github.com/jordansissel/keynav) — X11 keyboard-driven pointer
- [warpd](https://github.com/rvaiya/warpd) — modal keyboard-driven mouse
- [mouseless](https://github.com/jbensmann/mouseless) — keyboard-driven mouse control
- [xdotool](https://github.com/jordansissel/xdotool) / [ydotool](https://github.com/ReimuNotMoe/ydotool) — X11/Wayland automation
