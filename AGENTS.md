# AGENTS.md

## 项目

kursor —— 键驱光标

- Linux（X11 + wlroots + KDE + GNOME）
- Windows（CLI + service）

## 约定

- Rust 2024 版次，`#![warn(clippy::pedantic)]`，豁免类别见 `src/lib.rs`。
- Linux 用 `nix` 做系统调用；ioctl/poll/stat/read/close/mmap 一律不用裸 `libc::`。
- `#[repr(C)]` 结构用 `bytemuck` 转换；禁用 `std::mem::zeroed()`。
- 用 `log`，不用 `eprintln!`。常量放 `src/config.rs` —— 仅限 主要 / 常改 / 多次出现 / 用户可调 的值；一次性小字面量就地写。
- `src/device/linux/abi.rs` —— 内核 input 结构体 / ioctl / 设备创建的唯一定义处。
- `src/keymap.rs` —— 键码真值表：未用的键保持注释（注释 = 不用），代码需要时才启用。主键盘接管需 ≥44 键（26 字母 + `,` `.` `/` `;` + 0-9 + Tab/Caps/Shift/Meta）—— 见 `src/device/linux/input.rs`。
- 滚动方向：滚轮 `+1`=上、`-1`=下（glide-alpha `capslock+w`=上=+1，glide-num NumLock+`/`=上=+1）。方向符号易误读 —— 改键位/方向时对照 `assets/help/help-service.txt`。
- 平台拆分在 `src/device/{linux,windows}/` 与 `src/overlay/`；核心（grid/glide/render）平台无关。
- 现代模块布局（`linux.rs` + `linux/`），不用 `mod.rs`。
- Windows 用 `windows-sys`。GUI 入口在 `src/main.rs`（双击 → 后台 `service`，托盘 `Exit` 退出；终端 → `AttachConsole`；后台日志 `%LOCALAPPDATA%\kursor\kursor.log`）。托盘与 UI 字符串用英文。
- 文档注释：保留 机制/安全 说明（hook 活性、托盘、超采样、`MaybeUninit`）；删冗余叙述。
- 许可文件：根 `LICENSE` = 项目许可声明（SPDX 标识）；根 `COPYING` = AGPL-3.0-or-later 全文（GNU 惯例文件名）；第三方许可 = 根 `THIRD_PARTY_LICENSES`；字体许可 = `assets/LICENSE-Hack`。发布归档含 `LICENSE` + `COPYING` + `THIRD_PARTY_LICENSES` + 中英 README。

## 发布说明（release notes）

新建 `tmp/release-vX.Y.Z.md`，开头固定用以下模板（含 CPU 警告、范围标注、中英分条按版本）：

```markdown
## kursor vX.Y.Z 发布说明 / Release Notes（prev → vX.Y.Z）

⚠️ **CPU 要求 / CPU requirement**：
- 支持 **x86-64-v3+**（Zen3+ / AVX2）；旧 CPU 无法运行。OS：**Linux / Windows**。
- Supports **x86-64-v3+** (Zen3+ / AVX2); older CPUs can't run the release binaries.

<details>
<summary> 更新内容 </summary>
- **要点**（vA.B.C）：中文描述……
</details>

---

<details>
<summary> What's new (prev → vX.Y.Z) </summary>
- **Headline** (vA.B.C): English description……
</details>
```

- 范围：从上一发布版本起（如 v4.3.9 → v4.3.13）；每个变更标所属版本号。
- 中英分条一一对应；简明扼要；旧版 release notes 文件删除替换。

## QA

```sh
cargo check --all-targets && cargo fmt
cargo clippy --all-targets
cargo clippy --all-targets --target x86_64-pc-windows-gnu  # windows 交叉检查
```

## 构建与打包：Linux

经 `.cargo/config.toml` 启用 `x86-64-v3`（dev/release 均生效）；发布归档带 `x86_64_v3` 标识。

```sh
export PROJ_V="$(cargo pkgid | cut -d\@ -f2)"

cargo build --release

tar -I zstd \
    -cf $PWD/target/kursor-v${PROJ_V}-x86_64_v3-unknown-linux-gnu.tar.zst \
    -C  $PWD/target/release  kursor \
    -C  $PWD/contrib/systemd kursord.service \
    -C  $PWD  README.md README-zh.md LICENSE COPYING THIRD_PARTY_LICENSES assets/LICENSE-Hack
```

## 交叉构建与打包：Windows

需 `zig` + `cargo-zigbuild`。构建 → 补子系统 6.1 → 打 7z：

```sh
export PROJ_V="$(cargo pkgid | cut -d\@ -f2)"
export WIN_OUT="$PWD/target/kursor-v${PROJ_V}-x86_64_v3-pc-windows-gnu.7z"

cargo zigbuild --release --target x86_64-pc-windows-gnu
contrib/patch-pe-version.sh target/x86_64-pc-windows-gnu/release/kursor.exe

(cd target/x86_64-pc-windows-gnu/release && \
 cp $PWD/../../../README.md $PWD/../../../README-zh.md $PWD/../../../LICENSE $PWD/../../../COPYING $PWD/../../../THIRD_PARTY_LICENSES $PWD/../../../assets/LICENSE-Hack . && \
 7z a -mx=9 -bso0 -bsp0 "$WIN_OUT" kursor.exe README.md README-zh.md LICENSE COPYING THIRD_PARTY_LICENSES LICENSE-Hack)
```

说明：
- `zig` 打印 `ignoring deprecated linker optimization setting '1'` —— 无害。
- `patch-pe-version.sh` 设 Windows 7（NT 6.1）为最低版本；Win32 调用都是 2000 年代 API，真正限制是 x86-64-v3（AVX2）。
- 二进制为 GUI 子系统（`#![windows_subsystem = "windows"]` 在 `src/main.rs`）：双击 → 后台 `service`（托盘 `Exit` / 任务管理器退出）；终端运行附加父控制台输出；后台日志 `%LOCALAPPDATA%\kursor\kursor.log`。
- `service` 两平台均支持三模式（Windows glide 用 `WH_KEYBOARD_LL`，grid 用每显示器 `UpdateLayeredWindow` + DIB）。CLI（`move`/`moveto`/`click`/`pos`）两平台通用。
