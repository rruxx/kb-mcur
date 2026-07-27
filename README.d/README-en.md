# key-cursor — Keyboard-driven cursor control

[中文](README-zh.md)

Grid-based cursor targeting, NumPad mouse navigation service, CLI macro shortcuts.
X11 / wlroots / KDE / GNOME.

## Install

```bash
git clone https://github.com/xxx/key-cursor.git && cd key-cursor
cargo build --release
sudo install -m755 target/release/key-cursor /usr/bin/
```

## Dependencies

| Category | Requirement |
| --- | --- |
| Build | Rust toolchain ≥ 1.80 |
| Kernel | Linux ≥ 5.0 (`/dev/uinput`) |
| Permissions | `sudo usermod -aG input $USER` |
| X11 compositor | picom / compton for overlay transparency — native on Wayland |

## Usage

### grid — Interactive progressive grid

1. 26×26 cell (a–z, 2 letters)
2. 4×2 sub-grid (q/w/e/r/a/s/d/f)
3. Multi-level 2×2 quadrant (e/r/d/f)
4. j/k/l click, Space/Enter warp and exit

Run `key-cursor grid --help` for the full key map.

### kp-nav — NumPad mouse navigation

NumLock+KPEnter toggle. Non-NumPad keys forwarded to the compositor.
Grid mode auto-handoff via Unix socket (`/run/key-cursord.sock`).
Hot-plug: keyboards re-scanned every second.

Run `key-cursor kp-nav --help` for the full key map.

#### systemd

```bash
sudo cp contrib/systemd/key-cursord.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now key-cursord
```

### CLI

| Command | Description |
| --- | --- |
| `key-cursor move -- 10 -5` | Relative move |
| `key-cursor moveto 500 300` | Absolute warp |
| `key-cursor click -r 3 M` | Click with repeat |

Run any command with `--help` for details.

## Architecture

```
src/
├── main.rs      CLI entry
├── lib.rs       Grid orchestration
├── kpnav.rs     NumPad mouse navigation service
├── project.rs   Centralised naming constants
├── config.rs    Key mappings — edit here
├── grid.rs      26×26 grid + region math
├── render.rs    Overlay rendering
├── overlay.rs   X11/Wayland overlay backends
├── uinput.rs    /dev/uinput virtual pointer
├── evdev.rs     EVIOCGRAB keyboard grab
└── keymap.rs    US-QWERTY keycode → ASCII
```

## License

AGPL-3.0-or-later
