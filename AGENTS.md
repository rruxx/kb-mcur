# AGENTS.md

## Project

kursor — keyboard-driven mouse cursor control for Linux (X11 + wlroots).

## Conventions

- Rust edition 2024, `#![warn(clippy::pedantic)]`, allowed categories in `src/lib.rs`.
- Use `nix` for syscalls. Never raw `libc::` for ioctl, poll, stat, read, close, mmap.
- Use `bytemuck` for `#[repr(C)]` struct byte casts. Never `std::mem::zeroed()`.
- Use `log` (`info!`/`warn!`/`error!`). Never `eprintln!`.
- Constants in `src/config.rs`. No magic numbers.
- `src/uinput/raw.rs` is the single source for uinput structs, ioctl defs, device creation.

## QA && Build

```sh
cargo check
cargo fmt && cargo clippy
cargo build --release
```

## PGO release

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

```sh
export PROJ_V="$(cargo pkgid | cut -d\# -f2)"

tar -I zstd \
    -cf $PWD/target/kursor-v${PROJ_V}-x86_64-unknown-linux-gnu.tar.zst \
    -C  $PWD/target/release kursor \
    -C  $PWD/contrib/systemd kursord.service
```
