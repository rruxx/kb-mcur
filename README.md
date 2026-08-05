# kursor — Keyboard-driven mouse-cursor control

[中文](README.d/README-zh.md)

Three-layer progressive grid, glide-num (NumPad), glide-alpha (main keyboard), single-shot CLI commands (move / moveto / click).
X11 / wlroots / KDE / GNOME.

## Motivation

- Built-in keyboard-driven mouse features on each platform are too weak — actually going mouse-free remains impractical.
- No uniform keyboard-driven mouse workflow across X11 and Wayland.
- [wl-kbptr](https://sr.ht/~q3cpma/wl-kbptr/) only supports wlroots compositors (Sway, Hyprland), not KDE or GNOME.
- KDE 5 had a toggle shortcut similar to Windows; it was removed in KDE 6.

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
| CPU | x86-64-v3 (Zen3+) — release builds are tuned for it |
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
meta+shift+capslock toggle. ctrl+h/j/k/l = move,
shift+h/j/k/l = scroll, ctrl+u/i = back/forward,
Space/;/' = left/right/middle click.

**grid (meta+capslock):**
Three-layer progressive grid (L1: 9×3, L2: 3×9 clockwise‑90°, L3: 5×3 left-half keyboard).
L4: subdivide the selected L3 cell 7×7; alt+h/j/k/l nudge from center.
Multi-monitor: type a letter (a, b, …) to select display; tab switches monitors.
j/k/l click, Enter warp. 0-9 prefix for repeat (e.g. 3j).
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
├── service/
│   ├── glide_num.rs   NumPad glide-num (direction + acceleration)
│   ├── glide_alpha.rs Main-keyboard glide-alpha
│   ├── grid.rs        Grid data model + re-exports
│   └── grid/
│       ├── base.rs       Base-layer rendering (BG + L1 + labels)
│       ├── state.rs      Grid state + input handlers (GridStateMut)
│       ├── display.rs    Display update + L2/L3/L4 rendering
│       ├── process.rs    Cursor actions + region geometry
│       ├── init.rs       Grid service init + connection
│       ├── device_perm.rs Session detection + device permission fix
│       ├── selection.rs  Multi-monitor selection UI
│       └── env.rs        GridEnv state + toggle/input API
│   ├── dir.rs            Shared direction bitmask + glide ticks
 ├── config.rs      Project identity, key mappings, grid config
 ├── debug.rs       Debug helpers (multi-monitor simulation)
 ├── device.rs      Device layer: kernel ABI + physical/virtual clients
 │   ├── abi.rs     Kernel input ABI: InputEvent + evdev/uinput ioctls
 │   ├── input.rs   Physical input: EVIOCGRAB keyboard grab + hot-plug
 │   └── uinput.rs  /dev/uinput virtual pointer (Mouse)
 ├── render.rs      Overlay rendering + text drawing
 ├── overlay.rs     X11/Wayland overlay backends (enum dispatch)
 ├── overlay/
 │   ├── x11.rs     X11 RandR + SHAPE overlay
 │   └── wlr.rs     wlr-layer-shell Wayland overlay
 ├── keymap.rs      US-QWERTY keycode → ASCII map
```

## License

AGPL-3.0-or-later

## See also

- [wl-kbptr](https://sr.ht/~q3cpma/wl-kbptr/) — keyboard-driven pointer for wlroots
- [keynav](https://github.com/jordansissel/keynav) — X11 keyboard-driven pointer (retire your mouse)
- [warpd](https://github.com/rvaiya/warpd) — modal keyboard-driven mouse
- [mouseless](https://github.com/jbensmann/mouseless) — keyboard-driven mouse control
- [xdotool](https://github.com/jordansissel/xdotool) / [ydotool](https://github.com/ReimuNotMoe/ydotool) — X11/Wayland automation
