# key-cursor — Keyboard Mouse Cursor

[中文](README-zh.md)

**Linux desktop keyboard workflow — throw away the mouse.**

Precision cursor control through 7-level keyboard-driven grid, plus a NumPad mouse navigation service. CLI subcommands for compositor shortcut integration.

Works on X11 / wlroots / KDE / GNOME. Tested on Openbox / Sway / niri / KDE.

## Install

```bash
git clone https://github.com/xxx/key-cursor.git
cd key-cursor
cargo build --release
sudo install -m755 target/release/key-cursor /usr/bin/
```

### Permissions

Requires read/write access to `/dev/input/event*` and `/dev/uinput`:

```bash
sudo usermod -aG input $USER
# Log out and back in
```

### Compositor (X11 transparency)

The overlay needs an X11 compositor for semi-transparency. Without one, the mask background renders opaque (solid black).

```bash
picom &    # Start compositor first (Openbox/i3) — untested
```

Wayland compositors (Sway/Hyprland/niri) support transparency natively.

## Usage

### Interactive Grid

```bash
key-cursor grid
```

| Input | Level | Action |
| --- | --- | --- |
| `a–z` | 1-2 | 26×26 grid |
| `q/w/e/r/a/s/d/f` | 3 | 4×2 sub-grid |
| `e/r/d/f` | 4–7 | 2×2 quadrant |

| Key | Action |
| --- | --- |
| Space / Enter | Move cursor and exit |
| `j`/`k`/`l` | Move + click left/middle/right |
| `3j` | Move + 3× left click |
| Esc | Reset grid |

### CLI

```bash
key-cursor move -- 10 -5       # Relative: right 10px, up 5px
key-cursor moveto 500 300      # Absolute: warp to (500, 300)
key-cursor click L             # Left click
key-cursor click -r 3 M        # Middle click × 3
```

### NumPad Navigation (Service)

```bash
key-cursor kp-nav
```

NumLock+KPEnter toggles mouse control on/off. All non-NumPad keys are forwarded.

Grid mode (`key-cursor grid`) automatically requests keyboard hand-off from the service via Unix socket at `/run/key-cursord.sock`. Hot-plug is detected every second — unplugged keyboards are released, newly plugged keyboards are grabbed.

| Key | Action |
| --- | --- |
| kp8/2/4/6 | Move up/down/left/right |
| kp7/9/1/3 | Diagonal move |
| kp5 | Click (press=down, release=up) |
| kp0 | Hold button down |
| kp. | Release button |
| kp+ | Double-click |
| kp/ \* - | Switch btn5 to left/middle/right |
| Hold | Auto-accelerates from 3 px to 50 px per step |

#### systemd Service

```bash
sudo cp contrib/systemd/key-cursord.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now key-cursord
```

## Comparison

| | key-cursor | warpd | wl-kbptr | ydotool | xdotool |
| --- | --- | --- | --- | --- | --- |
| X11 | ✓ | ✓ | ✗ | ✓ | ✓ |
| wlroots | ✓ | ✓ | ✓ | ✓ | ✓ (XWayland) |
| KDE/GNOME | ✓ (XWayland) | ✗ | ✗ | ✓ | ✓ (XWayland) |
| Output | /dev/uinput | XTest / wlr-pointer | wlr-pointer | /dev/uinput | XTest |
| Root | input group only | none | none | required | none |
| Input grab | EVIOCGRAB | compositor bind | compositor bind | N/A | N/A |
| CLI mouse ops | ✓ | ✗ | ✗ | ✓ | ✓ |
| Language | Rust | C | C | C | C |

## Architecture

```
src/
├── main.rs      CLI entry
├── lib.rs       Grid orchestration
├── kpnav.rs     NumPad mouse navigation service
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
