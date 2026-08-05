# AGENTS.md

## Project

kursor — keyboard-driven mouse cursor control for Linux (X11 + wlroots) and Windows (CLI commands).

## Conventions

- Rust edition 2024, `#![warn(clippy::pedantic)]`, allowed categories in `src/lib.rs`.
- Use `nix` for syscalls (Linux only). Never raw `libc::` for ioctl, poll, stat, read, close, mmap.
- Use `bytemuck` for `#[repr(C)]` struct byte casts. Never `std::mem::zeroed()`.
- Use `log`. Never `eprintln!`.
- Constants in `src/config.rs`. No magic numbers.
- `src/device/linux/abi.rs` is the single source for Linux kernel input structs, ioctls, device creation.
- Platform split lives under `src/device/{linux,windows}/` and `src/overlay/`; core logic (grid/glide/render) is platform-neutral.
- Modern module layout (`linux.rs` + `linux/`), never `mod.rs`.

## QA

```sh
cargo check --all-targets
cargo fmt && cargo clippy --all-targets
```

## Build & Pack: Linux

Binaries are built for `x86-64-v3` (`.cargo/config.toml` rustflags apply per-target, dev and release alike), so release archives carry the `x86_64_v3` micro-arch level.

```sh
export PROJ_V="$(cargo pkgid | cut -d\@ -f2)"

cargo build --release

tar -I zstd \
    -cf $PWD/target/kursor-v${PROJ_V}-x86_64_v3-unknown-linux-gnu.tar.zst \
    -C  $PWD/target/release kursor \
    -C  $PWD/contrib/systemd kursord.service
```

## Cross-build & Pack: Windows

Requires `zig` + `cargo-zigbuild` (Linux host). Build → patch subsystem 6.1 → pack 7z:

```sh
export PROJ_V="$(cargo pkgid | cut -d\@ -f2)"
export WIN_OUT="$PWD/target/kursor-v${PROJ_V}-x86_64_v3-pc-windows-gnu.7z"

cargo zigbuild --release --target x86_64-pc-windows-gnu
contrib/patch-pe-version.sh target/x86_64-pc-windows-gnu/release/kursor.exe
(cd target/x86_64-pc-windows-gnu/release && 7z a -mx=9 -bso0 -bsp0 "$WIN_OUT" kursor.exe)
```

Note: zig prints `ignoring deprecated linker optimization setting '1'` — harmless.
`patch-pe-version.sh` declares Windows 7 (NT 6.1) as the minimum — zig cc can't forward a
subsystem version to its internal lld-link. Win32 calls (`SendInput`/`SetCursorPos`/
`GetSystemMetrics`) are Windows 2000-era, so the real constraint is x86-64-v3 (AVX2).
CLI (`move`/`moveto`/`click`) works on both platforms. `service` runs on Linux (all three
modes) and on Windows (stage 1: glide-num + glide-alpha via `WH_KEYBOARD_LL`); the
Windows grid overlay (Direct2D) is stage 2.
