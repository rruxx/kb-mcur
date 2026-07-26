# kb-mcur — Keyboard Mouse Cursor

[中文](README-zh.md)

**Linux desktop keyboard workflow — throw away the mouse.**

Precision cursor control through 7-level keyboard-driven grid, plus a standalone w/a/s/d mouse mode. CLI subcommands for compositor shortcut integration.

Works on X11 / wlroots / KDE / GNOME. Tested on Openbox / Sway / niri / KDE.

## Install

```bash
git clone https://github.com/xxx/kb-mcur.git
cd kb-mcur
cargo build --release
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
kb-mcur     # No args defaults to grid mode
```

| Input | Level | Action |
| --- | --- | --- |
| `a–z` | 1-2 | 26×26 grid |
| `q/w/e/r/a/s/d/f` | 3 | 4×2 sub-grid |
| `e/r/d/f` | 4–7 | 2×2 quadrant |

| Key | Action |
| --- | --- |
| Space / Enter | Move cursor and exit |
| `u`/`i`/`o` | Move + toggle left/middle/right press |
| `j`/`k`/`l` | Move + click left/middle/right |
| `3j` | Move + 3× left click |
| Esc | Reset grid |

### CLI

```bash
kb-mcur move -- 10 -5       # Relative: right 10px, up 5px
kb-mcur moveto 500 300      # Absolute: warp to (500, 300)
kb-mcur click L             # Left click
kb-mcur click -r 3 M        # Middle click × 3
kb-mcur click R             # Right click
```

### Mouse Mode

```bash
kb-mcur mouse   # Direct w/a/s/d cursor control (no grid overlay)
```

| Key | Action |
| --- | --- |
| `w`/`a`/`s`/`d` | Move cursor up/left/down/right |
| Hold (long press) | Auto-accelerates from 3 px to 50 px per step |
| Shift + `w/a/s/d` | Fixed 80 px per step (no acceleration) |
| `j`/`k`/`l` | Click left/middle/right |
| `u`/`i`/`o` | Toggle left/middle/right press |
| `3j` | 3× left click |
| Space / Enter / Esc | Exit |

## Comparison

| | kb-mcur | warpd | wl-kbptr | ydotool | xdotool |
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
