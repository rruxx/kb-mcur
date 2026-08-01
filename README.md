# kursor — Keyboard-driven mouse-cursor control

[中文](README.d/README-zh.md)

Three-layer progressive grid, glide-num (NumPad), glide-alpha (main keyboard), CLI macros.
X11 / wlroots / KDE / GNOME.

## Motivation

- No uniform keyboard-driven mouse workflow across X11 and Wayland.
- [wl-kbptr](https://sr.ht/~q3cpma/wl-kbptr/) only supports wlroots compositors (Sway, Hyprland), not KDE or GNOME.
- KDE 5 had a toggle shortcut similar to Windows; it was removed in KDE 6.
- GNOME has never offered one.

kursor is a single binary that runs everywhere.

## Install

```sh
git clone https://github.com/rruxx/kursor.git    # GitHub
git clone https://gitee.com/rruxx/kursor.git     # Gitee
cd kursor
cargo build --release
sudo install -m755 target/release/kursor /usr/bin/
```

## Dependencies

| Category | Requirement |
| --- | --- |
| Build | Rust toolchain ≥ 1.80 |
| Kernel | Linux ≥ 5.0 (`/dev/uinput`) |
| Permissions | `sudo usermod -aG input $USER` |
| Overlay transparency | X11 with compositor; Wayland native |

## Usage

### service — Triple-mode daemon (glide-num + glide-alpha + grid)

Start once as a systemd service. Three orthogonal modes:

**glide-num (NumPad):**
Mouse emulation with acceleration. NumLock+KPEnter toggle.
Hold direction keys to auto-accelerate (3→50 px).
/ * - = switch btn5 (L/M/R). NumLock + / 8 7 9 = scroll; * - = back/forward.

**glide-alpha (Main keyboard):**
meta+shift+capslock toggle. h/j/k/l = move, Space = left click,
; = right click, ' = middle click.

**grid (meta+capslock):**
Three-layer progressive grid (L1: 9×3, L2: 3×9 clockwise‑90°, L3: 5×3 left-half keyboard).
Multi-monitor: type a letter (a, b, …) to select display; tab switches monitors.
j/k/l click, Space/Enter warp. 0-9 prefix for repeat (e.g. 3j).
Backspace: L3 → L2, L2 → L1.
After each click/warp, the filter resets — grid stays open.

#### systemd

```sh
sudo setcap cap_sys_admin+ep /usr/bin/kursor
sudo cp contrib/systemd/kursord.service /lib/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now kursord
```

### CLI

| Command | Description |
| --- | --- |
| `kursor move -- 10 -5` | Relative move |
| `kursor moveto 500 300` | Absolute warp |
| `kursor click -r 3 M` | Click with repeat |

`--help` prints full key maps for every command.

## Architecture

```
src/
├── main.rs        CLI entry
├── lib.rs         Module declarations
├── service.rs     Main event loop + dispatch
├── glide_num.rs   NumPad glide-num (direction + acceleration)
├── glide_alpha.rs Main-keyboard glide-alpha
├── grid.rs        Grid data model + re-exports
├── grid/
│   ├── init.rs     Grid service init + watchdog
│   └── input.rs    Grid input processing + rendering
├── config.rs      Project identity, key mappings, grid config
├── debug.rs       Debug helpers (multi-monitor simulation)
├── render.rs      Overlay rendering + text drawing
├── overlay.rs     X11/Wayland overlay backends (enum dispatch)
├── overlay/
│   ├── x11.rs     X11 RandR + SHAPE overlay
│   └── wlr.rs     wlr-layer-shell Wayland overlay
├── uio.rs         Shared uinput: structs, ioctl definitions, device creation
├── uinput.rs      /dev/uinput virtual pointer (Mouse)
├── evdev.rs       EVIOCGRAB keyboard grab + inotify hot-plug
└── keymap.rs      US-QWERTY keycode → ASCII map
```

## License

AGPL-3.0-or-later

## See also

- [wl-kbptr](https://sr.ht/~q3cpma/wl-kbptr/) — keyboard-driven pointer for wlroots
- [warpd](https://github.com/rvaiya/warpd) — modal keyboard-driven mouse
- [mouseless](https://github.com/jbensmann/mouseless) — keyboard-driven mouse control
- [xdotool](https://github.com/jordansissel/xdotool) / [ydotool](https://github.com/ReimuNotMoe/ydotool) — X11/Wayland automation
