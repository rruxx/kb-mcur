# kursor — 键驱光标

[English](README.md)

三层渐进网格、glide-num（小键盘）、glide-alpha（主键盘）与一次性 CLI 命令（move / moveto / click / pos）。
支持 Linux（X11 / wlroots / KDE / GNOME）与 Windows（CLI + service）。

## 初衷

- 各平台自带的键盘驱动鼠标功能不够强大，实际使用难以真正脱离鼠标。
- Linux 缺乏跨 X11 和 Wayland 的统一键盘鼠标工作流。
- [wl-kbptr](https://sr.ht/~q3cpma/wl-kbptr/) 仅支持 wlroots 系合成器（Sway、Hyprland），KDE 和 GNOME 不可用。
- KDE 5 原本有类似 Windows 的开关快捷键，但 KDE 6 已不可用。

kursor 一个二进制适配全部。

## 安装

### Linux

```sh
git clone https://github.com/rruxx/kursor.git   # 或 https://gitee.com/rruxx/kursor.git
cd kursor
cargo build --release
sudo install -m755 target/release/kursor /usr/bin/
```

### Windows

从 Releases 下载 `kursor-v{VERSION}-x86_64_v3-pc-windows-gnu.7z`——内含单个 `kursor.exe`，无需安装。
（源码交叉编译见 `AGENTS.md`。）

## 要求

| | |
| --- | --- |
| CPU | x86-64-v3+（Zen3+ / AVX2） |
| OS | Linux / Windows |
| Rust | ≥ 1.80 |
| Linux | ≥ 5.0（`/dev/uinput`），`sudo usermod -aG input $USER` |
| Windows | 7+（子系统 6.1） |

## 平台支持

| 桌面环境 | 支持 |
| --- | --- |
| wlroots / X11 | 完整（原生）；X11 grid 半透明需合成器 |
| KDE / GNOME | 借助 XWayland |

**测试情况：**
- **wlroots（niri、Sway）：** 一切正常。
- **X11（openbox）：** 正常，但 grid 遮罩无法半透明（需合成器）。
- **KDE：** 借助 XWayland，一切正常。
- **GNOME：** 未测试——理论上可借助 XWayland 运行，但 `kursor pos` 可能无法正常工作。

## 用法

### service — glide-num + glide-alpha + grid

Linux 与 Windows 均支持全部三模式；各命令加 `--help` 查看完整键表。

**glide-num（小键盘）：** NumLock+KPEnter 切换。方向键移动（加速）；`/ * -` 切换按钮，NumLock+`/ 8 7 9` 滚动，NumLock+`* -` 后退/前进。

**glide-alpha（主键盘）：** meta+shift+capslock 切换。`ctrl+h/j/k/l` 移动，`ctrl+w/a/s/d` 滚动，`ctrl+u/i/o` 左/中/右键，`ctrl+n/m` 后退/前进。

**grid（meta+capslock）：** 27×27 三层网格（L1: 9×3，L2: 3×9，L3: 5×3）。`j/k/l` 点击，`;` 定位，0-9 连击，`p` 复位；多屏用 `a-z`/Tab。

#### Windows

双击 `kursor.exe` 即后台运行 service——无控制台窗口，托盘图标右键菜单 `Exit` 退出（或任务管理器结束）。在终端运行 `kursor service` 则为控制台会话（Ctrl+C 退出）；其余命令在终端输出。

`service` 使用 `WH_KEYBOARD_LL`，系统可能静默冻结它（自动重装），且无法捕获提权（UIPI）与安全桌面输入——属平台限制。grid 叠加层绘制到透明点击穿透分层窗口。

#### Linux（systemd）

```sh
sudo setcap cap_sys_admin+ep /usr/bin/kursor
sudo cp contrib/systemd/kursord.service /lib/systemd/system/
sudo systemctl daemon-reload && sudo systemctl enable --now kursord
```

### CLI（Linux 与 Windows）

| 命令 | 说明 |
| --- | --- |
| `kursor move -- 10 -5` | 相对移动 |
| `kursor moveto 500 300` | 绝对定位 |
| `kursor click -r 3 M` | 连击 |
| `kursor pos` | 打印光标位置与所在屏幕 |

`kursor pos` 在 Windows / X11 原生支持；KDE Wayland 通过 KWin scripting
（`workspace.cursorPos`）查询；wlroots/niri（Sway、Hyprland 等）通过每输出
layer-surface + virtual-pointer 触发读取——任意屏全局坐标，无需外部工具。
（GNOME，见"平台支持"）

各命令加 `--help` 查看完整键表。

## 架构

三层结构 + 薄 CLI 壳：`device/`（虚拟指针）、`overlay/`（渲染 + 光标/屏幕查询）、`service/`（跨模式状态机 + 平台主循环）。

```
src/
├── main.rs        CLI 入口（解析、派发）
├── lib.rs         模块声明 + 重导出
├── cli/           命令枚举 + 每命令一文件（move/moveto/click/pos/service）
├── config.rs      项目标识、按键布局、网格配置、常量
├── keymap.rs      evdev 键码、ModState、键映射
├── font.rs        内嵌字体
├── render.rs      叠加层渲染 + 文字缓存
├── debug.rs       调试辅助（多屏模拟）
├── device/        每平台虚拟指针（Pointer/KeyboardOut trait）
│   ├── linux/     内核 input ABI、键盘接管、uinput Mouse
│   └── windows/   SendInput/SetCursorPos Mouse、VK→evdev 映射
├── overlay/       OverlayBackend trait + 每平台叠加层 + pos/screen 查询
│   ├── x11.rs     X11 RandR + SHAPE 叠加层
│   ├── wlr.rs     wlr-layer-shell 叠加层 + virtual-pointer
│   ├── kde.rs     KDE Wayland pos（KWin scripting）
│   └── windows/   每显示器分层窗口
└── service/       跨模式状态机 + 平台主循环
    ├── linux.rs   evdev 接管循环
    ├── windows.rs WH_KEYBOARD_LL 循环 + 活性检测
    ├── glide_num.rs / glide_alpha.rs / dir.rs
    └── grid/      网格模型、渲染、状态
```

## 许可证

AGPL-3.0-or-later（见 `LICENSE`；全文见 `COPYING`）。

内置第三方组件 —— 见 `THIRD_PARTY_LICENSES`：
- **Hack 字体**（MIT / Bitstream Vera License），裁剪版嵌入为 `assets/font/font.ttf`；完整文本见 `assets/LICENSE-Hack`。

## 参考

- [wl-kbptr](https://sr.ht/~q3cpma/wl-kbptr/) — wlroots 键盘驱动指针
- [keynav](https://github.com/jordansissel/keynav) — X11 键盘驱动指针
- [warpd](https://github.com/rvaiya/warpd) — 模态键盘驱动鼠标
- [mouseless](https://github.com/jbensmann/mouseless) — 键盘驱动鼠标控制
- [xdotool](https://github.com/jordansissel/xdotool) / [ydotool](https://github.com/ReimuNotMoe/ydotool) — X11/Wayland 自动化工具
