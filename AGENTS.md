# AGENTS.md

## Project

kursor — keyboard-driven mouse cursor control for Linux (X11 + wlroots) and Windows (CLI commands).

## Conventions

- Rust edition 2024, `#![warn(clippy::pedantic)]`, allowed categories in `src/lib.rs`.
- Use `nix` for syscalls (Linux only). Never raw `libc::` for ioctl, poll, stat, read, close, mmap.
- Use `bytemuck` for `#[repr(C)]` struct byte casts. Never `std::mem::zeroed()`.
- Use `log` (`info!`/`warn!`/`error!`). Never `eprintln!`.
- Constants in `src/config.rs`. No magic numbers.
- `src/device/linux/abi.rs` is the single source for Linux kernel input structs, ioctl defs, device creation.
- Platform split lives under `src/device/` (`linux/` vs `windows/`) and `src/overlay/`; core logic (grid/glide/render) is platform-neutral.
- Use the modern module layout (a `linux.rs` + `linux/` dir), never `mod.rs`.

## QA && Build

```sh
cargo check
cargo fmt && cargo clippy
cargo build --release
```

## Windows cross-build

Requires `zig` + `cargo-zigbuild` (Linux host):

```sh
rustup target add x86_64-pc-windows-gnu
cargo check --target x86_64-pc-windows-gnu
cargo zigbuild --release --target x86_64-pc-windows-gnu
contrib/patch-pe-version.sh target/x86_64-pc-windows-gnu/release/kursor.exe
# → target/x86_64-pc-windows-gnu/release/kursor.exe (subsystem 10.00)
```

Note: zig prints `ignoring deprecated linker optimization setting '1'` — harmless.
The Windows build uses `-C target-cpu=x86-64-v3` (like Linux, see `.cargo/config.toml`).
zig cc does not forward a subsystem version to its internal lld-link, so
`contrib/patch-pe-version.sh` must run after linking to declare Windows 10/11
(os + subsystem version 10.0).
Windows currently supports only the CLI commands (`move` / `moveto` / `click`);
`service` (three-mode daemon) is Linux-only (stage 2).

## PGO release (unused)

```sh
export PROJ="$(pwd)"  # path to this project

sudo rm -r ${PROJ}/tmp/pgo-data

cargo clean
RUSTFLAGS="-Cprofile-generate=${PROJ}/tmp/pgo-data" cargo build --release

sudo systemctl stop kursord
sudo cp ${PROJ}/target/release/kursor           /usr/bin/kursor
sudo cp ${PROJ}/contrib/systemd/kursord.service /lib/systemd/system/kursord.service
sudo systemctl daemon-reload

sudo systemctl start kursord
# glide-* + grid
sudo systemctl stop kursord

llvm-profdata merge -o ${PROJ}/tmp/merged.profdata ${PROJ}/tmp/pgo-data

cargo clean
RUSTFLAGS="-Cprofile-use=${PROJ}/tmp/merged.profdata" cargo build --release
```

## Pack

Release binaries are built with `-C target-cpu=x86-64-v3` (see `.cargo/config.toml`),
so the archive name carries the `x86_64_v3` micro-architecture level (Arch convention).

```sh
export PROJ_V="$(cargo pkgid | cut -d\@ -f2)"

tar -I zstd \
    -cf $PWD/target/kursor-v${PROJ_V}-x86_64_v3-unknown-linux-gnu.tar.zst \
    -C  $PWD/target/release kursor \
    -C  $PWD/contrib/systemd kursord.service
```
