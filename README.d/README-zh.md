# kursor — 键驱光标

[English](../README.md)

三层渐进网格、glide-num（小键盘）、glide-alpha（主键盘）与一次性 CLI 命令（move / moveto / click）。
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
| Rust | ≥ 1.80 |
| CPU | x86-64-v3（Zen3+ / AVX2） |
| Linux | ≥ 5.0（`/dev/uinput`），`sudo usermod -aG input $USER` |
| Windows | 7+（子系统 6.1） |
| 叠加层 | X11 搭配合成器；Wayland 原生 |

## 用法

### service — glide-num + glide-alpha + grid

Linux 与 Windows 均支持全部三模式；各命令加 `--help` 查看完整键表。

**glide-num（小键盘）：** NumLock+KPEnter 切换。方向键移动（加速）；`/ * -` 切换按钮，NumLock+`/ 8 7 9` 滚动，NumLock+`* -` 后退/前进。

**glide-alpha（主键盘）：** meta+shift+capslock 切换。`ctrl+h/j/k/l` 移动，`shift+h/j/k/l` 滚动，`ctrl+u/i` 后退/前进，`ctrl+Space/;/'` 点击。

**grid（meta+capslock）：** 27×27 三层网格（L1: 9×3，L2: 3×9，L3: 5×3）。`j/k/l` 点击，Enter 定位，0-9 连击，Backspace/Esc 复位；多屏用 `a-z`/Tab。

#### Windows

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

各命令加 `--help` 查看完整键表。

## 架构

```
src/
├── main.rs        CLI 入口
├── lib.rs         模块声明
├── config.rs      项目标识、按键映射、网格配置
├── keymap.rs      US-QWERTY 键码映射
├── font.rs        内嵌字体（assets/font.ttf）
├── render.rs      叠加层渲染 + 文字绘制
├── debug.rs       调试辅助（多屏模拟）
├── device.rs      设备层入口：平台分目录虚拟指针
├── device/
│   ├── linux.rs       Linux 指针 re-export
│   ├── linux/
│   │   ├── abi.rs     内核 input ABI：结构体 + ioctls
│   │   ├── input.rs   物理键盘接管 + 热插拔
│   │   └── uinput.rs  虚拟指针（Mouse）
│   ├── windows.rs     Windows 指针 re-export
│   └── windows/
│       └── mouse.rs   SendInput / SetCursorPos 虚拟指针
├── overlay.rs     OverlayBackend trait + 平台 connect()
├── overlay/
│   ├── x11.rs     X11 RandR + SHAPE 叠加层
│   ├── wlr.rs     wlr-layer-shell Wayland 叠加层
│   └── windows.rs 屏幕尺寸查询（阶段 1）
├── service.rs     主事件循环 + 派发（Linux）
└── service/       （Linux）
    ├── glide_num.rs   小键盘 glide-num
    ├── glide_alpha.rs 主键盘 glide-alpha
    ├── dir.rs         共享方向位掩码 + 渐动步进
    ├── grid.rs        网格数据模型 + re-export
    └── grid/
        ├── base.rs        基础层渲染（背景 + L1 + 标签）
        ├── device_perm.rs 会话检测 + 设备权限修复
        ├── display.rs     显示更新 + L2/L3/L4 渲染
        ├── env.rs         GridEnv 状态 + 开关/输入 API
        ├── init.rs        网格服务初始化 + 连接
        ├── process.rs     光标操作 + 区域几何
        ├── selection.rs   多屏选择 UI
        └── state.rs       网格状态 + 输入处理（GridStateMut）
```

## 许可证

AGPL-3.0-or-later

## 参考

- [wl-kbptr](https://sr.ht/~q3cpma/wl-kbptr/) — wlroots 键盘驱动指针
- [keynav](https://github.com/jordansissel/keynav) — X11 键盘驱动指针
- [warpd](https://github.com/rvaiya/warpd) — 模态键盘驱动鼠标
- [mouseless](https://github.com/jbensmann/mouseless) — 键盘驱动鼠标控制
- [xdotool](https://github.com/jordansissel/xdotool) / [ydotool](https://github.com/ReimuNotMoe/ydotool) — X11/Wayland 自动化工具
