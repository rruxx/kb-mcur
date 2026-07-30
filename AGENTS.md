# AGENTS.md

## Project

key-mcursor — keyboard-driven mouse cursor control for Linux (X11 + wlroots).

## Conventions

- Rust edition 2024, `#![warn(clippy::pedantic)]`, allowed categories in `src/lib.rs`.
- Use `nix` for syscalls. Never raw `libc::` for ioctl, poll, stat, read, close, mmap.
- Use `bytemuck` for `#[repr(C)]` struct byte casts. Never `std::mem::zeroed()`.
- Use `log` (`info!`/`warn!`/`error!`). Never `eprintln!`.
- Constants in `src/config.rs`. No magic numbers.
- `src/uio.rs` is the single source for uinput structs, ioctl defs, device creation.

## QA && Build

```sh
cargo check
cargo fmt && cargo clippy
cargo build --release
```

## PGO release

```sh
export PROJ="$(pwd)"  # path to this project

cargo clean
RUSTFLAGS="-Cprofile-generate=${PROJ}/tmp/pgo-data" cargo build --release

sudo cp ${PROJ}/target/release/key-mcursor           /usr/bin/
sudo cp ${PROJ}/contrib/systemd/key-mcursord.service /lib/systemd/system/
sudo systemctl daemon-reload

key-mcursor grid

systemctl start key-mcursord    # NumLock+KPEnter, do sth, NumLock+KPEnter
systemctl stop key-mcursord

llvm-profdata merge -o ${PROJ}/tmp/merged.profdata ${PROJ}/tmp/pgo-data

cargo clean
RUSTFLAGS="-Cprofile-use=${PROJ}/tmp/merged.profdata" cargo build --release
```

## Pack

```sh
export PROJ_V="$(cargo pkgid | cut -d\# -f2)"

tar -I zstd \
    -cf $PWD/target/key-mcursor-v${PROJ_V}-x86_64-unknown-linux-gnu.tar.zst \
    -C $PWD/target/release key-mcursor \
    -C $PWD/contrib/systemd key-mcursord.service
```
