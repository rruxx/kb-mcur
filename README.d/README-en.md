# key-mcursor — Keyboard-driven mouse-cursor control

[中文](README-zh.md)

Grid-based cursor targeting, NumPad mouse navigation service, CLI macro shortcuts.
X11 / wlroots / KDE / GNOME.

## Motivation

- No uniform keyboard-driven mouse workflow across X11 and Wayland.
- [wl-kbptr](https://sr.ht/~q3cpma/wl-kbptr/) only supports wlroots compositors (Sway, Hyprland), not KDE or GNOME.
- KDE 5 had a toggle shortcut similar to Windows; it was removed in KDE 6.
- GNOME has never offered one.

key-mcursor is a single binary that runs everywhere.

## Install

```bash
git clone https://github.com/rruxx/key-mcursor.git    # GitHub
git clone https://gitee.com/rruxx/key-mcursor.git     # Gitee
cd key-mcursor
cargo build --release
sudo install -m755 target/release/key-mcursor /usr/bin/
```

## Dependencies

| Category | Requirement |
| --- | --- |
| Build | Rust toolchain ≥ 1.80 |
| Kernel | Linux ≥ 5.0 (`/dev/uinput`) |
| Permissions | `sudo usermod -aG input $USER` |
| Overlay transparency | X11 with compositor; Wayland native |

## Usage

### grid — Interactive progressive grid

1. 26×26 cell (a–z, 2 letters)
2. 4×2 sub-grid (q/w/e/r/a/s/d/f)
3. Multi-level 2×2 quadrant (e/r/d/f)
4. j/k/l click, Space/Enter warp and exit

### kp-nav — NumPad mouse navigation

NumLock+KPEnter toggle. Non-NumPad keys forwarded to the compositor.
Grid mode auto-handoff via Unix socket. Hot-plug via inotify.

#### systemd

```bash
sudo setcap cap_sys_admin+ep /usr/bin/key-mcursor
sudo cp contrib/systemd/key-mcursord.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now key-mcursord
```

### CLI

| Command | Description |
| --- | --- |
| `key-mcursor move -- 10 -5` | Relative move |
| `key-mcursor moveto 500 300` | Absolute warp |
| `key-mcursor click -r 3 M` | Click with repeat |

`--help` prints full key maps for every command.

## Architecture

```
src/
├── main.rs      CLI entry
├── lib.rs       Grid orchestration + display selection
├── kpnav.rs     NumPad mouse navigation service
├── config.rs    Project identity, key mappings, grid config
├── debug.rs     Debug helpers (multi-monitor simulation)
├── grid.rs      26×26 grid + region math
├── render.rs    Overlay rendering + text drawing
├── overlay.rs   X11/Wayland overlay backends (enum dispatch)
├── overlay/
│   ├── x11.rs   X11 RandR + SHAPE overlay
│   └── wlr.rs   wlr-layer-shell Wayland overlay
├── uio.rs       Shared uinput: structs, ioctl definitions, device creation
├── uinput.rs    /dev/uinput virtual pointer (Mouse)
├── evdev.rs     EVIOCGRAB keyboard grab + inotify hot-plug
└── keymap.rs    US-QWERTY keycode → ASCII map
```

## License

AGPL-3.0-or-later

## See also

- [wl-kbptr](https://sr.ht/~q3cpma/wl-kbptr/) — keyboard-driven pointer for wlroots
- [warpd](https://github.com/rvaiya/warpd) — modal keyboard-driven mouse
- [mouseless](https://github.com/jbensmann/mouseless) — keyboard-driven mouse control
- [xdotool](https://github.com/jordansissel/xdotool) / [ydotool](https://github.com/ReimuNotMoe/ydotool) — X11/Wayland automation
