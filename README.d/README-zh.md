# kursor — 键驱光标

[English](README-en.md)

渐进式网格光标定位、小键盘鼠标导航常驻服务、CLI 宏快捷键。
支持 X11 / wlroots / KDE / GNOME。

## 初衷

- Linux 缺乏跨 X11 和 Wayland 的统一键盘鼠标工作流。
- [wl-kbptr](https://sr.ht/~q3cpma/wl-kbptr/) 仅支持 wlroots 系合成器（Sway、Hyprland），KDE 和 GNOME 不可用。
- KDE 5 原本有类似 Windows 的开关快捷键，但 KDE 6 已不可用。
- GNOME 从未提供此功能。

kursor 一个二进制适配全部。

## 安装

```bash
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
| 权限 | `sudo usermod -aG input $USER` |
| 叠加层透明 | X11 搭配合成器；Wayland 原生支持 |

## 用法

### service — 双模常驻服务（渐动 + 跳转）

通过 systemd 启动一次，两种正交策略：

**渐动（小键盘）：**
鼠标模拟 + 自动加速。NumLock+KPEnter 切换。
按住方向键自动加速（3→50 px）。
按住 NumLock 再按 / 8 7 9 为滚动；* - 为后退/前进。

**跳转（meta+capslock）：**
meta+capslock 开关网格叠加层。多屏时先输入字母（a, b, …）选屏，再进入 26×26 渐进网格。
tab 键切换显示屏。点击/定位后 filter 复位，网格不退出。

#### systemd

```bash
sudo setcap cap_sys_admin+ep /usr/bin/kursor
sudo cp contrib/systemd/kursord.service /etc/systemd/system/
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
├── main.rs      CLI 入口
├── lib.rs       交互式网格编排 + 多屏选屏
├── service.rs   双模服务（渐动 + 跳转）
├── config.rs    项目标识、按键映射、网格配置
├── debug.rs     调试辅助（多屏模拟）
├── grid.rs      26×26 网格 + 区域计算
├── render.rs    叠加层渲染 + 文字绘制
├── overlay.rs   X11/Wayland 双后端（枚举分发）
├── overlay/
│   ├── x11.rs   X11 RandR + SHAPE 叠加层
│   └── wlr.rs   wlr-layer-shell Wayland 叠加层
├── uio.rs       共享 uinput：结构体、ioctl 定义、设备创建
├── uinput.rs    /dev/uinput 虚拟键鼠（Mouse）
├── evdev.rs     EVIOCGRAB 键盘接管 + inotify 热插拔
└── keymap.rs    US-QWERTY 键码映射
```

## 许可证

AGPL-3.0-or-later

## 参考

- [wl-kbptr](https://sr.ht/~q3cpma/wl-kbptr/) — wlroots 键盘驱动指针
- [warpd](https://github.com/rvaiya/warpd) — 模态键盘驱动鼠标
- [mouseless](https://github.com/jbensmann/mouseless) — 键盘驱动鼠标控制
- [xdotool](https://github.com/jordansissel/xdotool) / [ydotool](https://github.com/ReimuNotMoe/ydotool) — X11/Wayland 自动化工具
