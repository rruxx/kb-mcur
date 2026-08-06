# AGENTS.md

## Project

kursor — keyboard-driven mouse cursor control. Linux (X11 + wlroots), Windows (CLI + service, GUI-subsystem background run).

## Conventions

- Rust edition 2024, `#![warn(clippy::pedantic)]`, allowed categories in `src/lib.rs`.
- `nix` for syscalls (Linux only); never raw `libc::` for ioctl/poll/stat/read/close/mmap.
- `bytemuck` for `#[repr(C)]` casts; never `std::mem::zeroed()`.
- `log`, never `eprintln!`. Constants in `src/config.rs`; no magic numbers.
- `src/device/linux/abi.rs` — single source for kernel input structs/ioctls/device creation.
- `src/keymap.rs` — keycode truth table: unused keys stay commented (commented = not used); enable a key only when the code needs it. Main-keyboard grab needs ≥45 keys (26 letters + `,` `.` `/` `;` + 0-9 + Tab/Caps/Shift/Ctrl/Meta) — see `src/device/linux/input.rs`.
- Platform split under `src/device/{linux,windows}/` and `src/overlay/`; core (grid/glide/render) platform-neutral.
- Modern module layout (`linux.rs` + `linux/`), never `mod.rs`.
- Windows uses `windows-sys`. GUI entry in `src/main.rs` (double-click → background `service`, tray `Exit`; terminal → `AttachConsole`; background logs `%LOCALAPPDATA%\kursor\kursor.log`). Tray/UI strings English.

## QA

```sh
cargo check --all-targets && cargo fmt
cargo clippy --all-targets
cargo clippy --all-targets --target x86_64-pc-windows-gnu  # windows
```

## Build & Pack: Linux

`x86-64-v3` via `.cargo/config.toml` (dev and release alike); archives carry `x86_64_v3`.

```sh
export PROJ_V="$(cargo pkgid | cut -d\@ -f2)"
cargo build --release
tar -I zstd -cf $PWD/target/kursor-v${PROJ_V}-x86_64_v3-unknown-linux-gnu.tar.zst \
    -C $PWD/target/release kursor -C $PWD/contrib/systemd kursord.service
```

## Cross-build & Pack: Windows

Requires `zig` + `cargo-zigbuild`. Build → patch subsystem 6.1 → 7z:

```sh
export PROJ_V="$(cargo pkgid | cut -d\@ -f2)"
export WIN_OUT="$PWD/target/kursor-v${PROJ_V}-x86_64_v3-pc-windows-gnu.7z"
cargo zigbuild --release --target x86_64-pc-windows-gnu
contrib/patch-pe-version.sh target/x86_64-pc-windows-gnu/release/kursor.exe
(cd target/x86_64-pc-windows-gnu/release && 7z a -mx=9 -bso0 -bsp0 "$WIN_OUT" kursor.exe)
```

Notes:
- `zig` prints `ignoring deprecated linker optimization setting '1'` — harmless.
- `patch-pe-version.sh` sets Windows 7 (NT 6.1) minimum; Win32 calls are 2000-era, real constraint is x86-64-v3 (AVX2).
- Binary is GUI-subsystem (`#![windows_subsystem = "windows"]` in `src/main.rs`): double-click → background `service` (tray `Exit` / Task Manager to quit); terminal runs attach parent console for CLI output; background logs to `%LOCALAPPDATA%\kursor\kursor.log`.
- `service` runs all three modes on both platforms (Windows glide via `WH_KEYBOARD_LL`, grid via per-monitor `UpdateLayeredWindow` + DIB). CLI (`move`/`moveto`/`click`/`pos`) works on both.
