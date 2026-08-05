# kursor — 键驱光标

[English](../README.md)

三层渐进网格、glide-num（小键盘）、glide-alpha（主键盘）、一次性 CLI 命令（move / moveto / click）。
支持 X11 / wlroots / KDE / GNOME。

## 初衷

- 各平台自带的键盘驱动鼠标功能不够强大，实际使用难以真正脱离鼠标。
- Linux 缺乏跨 X11 和 Wayland 的统一键盘鼠标工作流。
- [wl-kbptr](https://sr.ht/~q3cpma/wl-kbptr/) 仅支持 wlroots 系合成器（Sway、Hyprland），KDE 和 GNOME 不可用。
- KDE 5 原本有类似 Windows 的开关快捷键，但 KDE 6 已不可用。

kursor 一个二进制适配全部。

## 安装

```sh
git clone https://github.com/rruxx/kursor.git    # GitHub
git clone https://gitee.com/rruxx/kursor.git     # Gitee
cd kursor
cargo build --release
sudo install -m755 target/release/kursor /usr/bin/
```

## 依赖

| 类别 | 要求 |
| --- | --- |
| 构建 | Rust 工具链 ≥ 1.80 |
| 内核 | Linux ≥ 5.0（`/dev/uinput`） |
| CPU | x86-64-v3（Zen3+）—— 发布构建已启用该指令集优化 |
| 权限 | `sudo usermod -aG input $USER` |
| 叠加层透明 | X11 搭配合成器；Wayland 原生支持 |

## 用法

### service — 三模常驻服务（glide-num + glide-alpha + grid）

通过 systemd 启动一次，三种正交模式：

**glide-num（小键盘）：**
NumLock+KPEnter 切换。非小键盘按键转发至合成器。
按住方向键自动加速（3→50 px）。/ * - 切换按键按钮（左/中/右键）。
按住 NumLock 再按 / 8 7 9 为滚动；* - 为后退/前进。

**glide-alpha（主键盘）：**
meta+shift+capslock 切换。ctrl+h/j/k/l = 移动，
shift+h/j/k/l = 滚动，ctrl+u/i = 后退/前进，
Space/;/' = 左/右/中键。

**grid（meta+capslock）：**
三层渐进网格（L1: 9×3，L2: 3×9 顺时针 90°，L3: 5×3 左半键区）。
L4：将所选 L3 格七等分；alt+h/j/k/l 从中心微移。
多屏时先输入字母（a, b, …）选屏；tab 键切换显示屏。
j/k/l 点击，回车定位。0-9 前缀连击（如 3j）。
Backspace：L3 → L2，L2 → L1。
点击/定位后 filter 复位，网格不退出。

#### systemd

```sh
sudo setcap cap_sys_admin+ep /usr/bin/kursor
sudo cp contrib/systemd/kursord.service /lib/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now kursord
```

### CLI

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
├── service.rs     主事件循环 + 派发
├── service/
│   ├── glide_num.rs   小键盘 glide-num（方向 + 加速）
│   ├── glide_alpha.rs 主键盘 glide-alpha
│   ├── grid.rs        网格数据模型 + re-export
│   └── grid/
│       ├── base.rs       基础层渲染（背景 + L1 + 标签）
│       ├── state.rs      网格状态 + 输入处理（GridStateMut）
│       ├── display.rs    显示更新 + L2/L3/L4 渲染
│       ├── process.rs    光标操作 + 区域几何
│       ├── init.rs       网格服务初始化 + 连接
│       ├── device_perm.rs 会话检测 + 设备权限修复
│       ├── selection.rs  多屏选择 UI
│       └── env.rs        GridEnv 状态 + 开关/输入 API
│   ├── dir.rs            共享方向位掩码 + 渐动步进
 ├── config.rs      项目标识、按键映射、网格配置
 ├── debug.rs       调试辅助（多屏模拟）
 ├── device.rs      设备层：内核 ABI + 物理/虚拟客户端
 │   ├── abi.rs     内核 input ABI：InputEvent + evdev/uinput ioctls
 │   ├── input.rs   物理输入：EVIOCGRAB 键盘接管 + 热插拔
 │   └── uinput.rs  /dev/uinput 虚拟指针（Mouse）
 ├── render.rs      叠加层渲染 + 文字绘制
 ├── overlay.rs     X11/Wayland 双后端（枚举分发）
 ├── overlay/
 │   ├── x11.rs     X11 RandR + SHAPE 叠加层
 │   └── wlr.rs     wlr-layer-shell Wayland 叠加层
 ├── keymap.rs      US-QWERTY 键码映射
```

## 许可证

AGPL-3.0-or-later

## 参考

- [wl-kbptr](https://sr.ht/~q3cpma/wl-kbptr/) — wlroots 键盘驱动指针
- [keynav](https://github.com/jordansissel/keynav) — X11 键盘驱动指针（retire your mouse）
- [warpd](https://github.com/rvaiya/warpd) — 模态键盘驱动鼠标
- [mouseless](https://github.com/jbensmann/mouseless) — 键盘驱动鼠标控制
- [xdotool](https://github.com/jordansissel/xdotool) / [ydotool](https://github.com/ReimuNotMoe/ydotool) — X11/Wayland 自动化工具
