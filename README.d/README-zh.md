# key-mcursor — 键驱光标

[English](README-en.md)

渐进式网格光标定位、小键盘鼠标导航常驻服务、CLI 宏快捷键。
支持 X11 / wlroots / KDE / GNOME。

## 初衷

- Linux 缺乏跨 X11 和 Wayland 的统一键盘鼠标工作流。
- [wl-kbptr](https://sr.ht/~q3cpma/wl-kbptr/) 仅支持 wlroots 系合成器（Sway、Hyprland），KDE 和 GNOME 不可用。
- KDE 5 原本有类似 Windows 的开关快捷键，但 KDE 6 已不可用。
- GNOME 从未提供此功能。

key-mcursor 一个二进制适配全部。

## 安装

```bash
git clone https://github.com/rruxx/key-mcursor.git    # GitHub
git clone https://gitee.com/rruxx/key-mcursor.git     # Gitee
cd key-mcursor
cargo build --release
sudo install -m755 target/release/key-mcursor /usr/bin/
```

## 依赖

| 类别 | 要求 |
| --- | --- |
| 构建 | Rust 工具链 ≥ 1.80 |
| 内核 | Linux ≥ 5.0（`/dev/uinput`） |
| 权限 | `sudo usermod -aG input $USER` |
| 叠加层透明 | X11 搭配合成器；Wayland 原生支持 |

## 用法

### grid — 交互式渐进网格

1. 26×26 网格（a–z，2 个字母）
2. 4×2 子格（q/w/e/r/a/s/d/f）
3. 多层次 2×2 象限（e/r/d/f）
4. j/k/l 单击，空格/回车 定位并退出

### kp-nav — 小键盘鼠标导航

NumLock+KPEnter 切换开关。非小键盘按键转发至合成器。
Grid 模式通过 Unix socket 自动请求键盘接管，热插拔通过 inotify 事件驱动。

#### systemd

```bash
sudo setcap cap_sys_admin+ep /usr/bin/key-mcursor
sudo cp contrib/systemd/key-mcursord.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now key-mcursord
```

### CLI

| 命令 | 说明 |
| --- | --- |
| `key-mcursor move -- 10 -5` | 相对移动 |
| `key-mcursor moveto 500 300` | 绝对定位 |
| `key-mcursor click -r 3 M` | 连击 |

各命令加 `--help` 查看完整键表。

## 架构

```
src/
├── main.rs      CLI 入口
├── lib.rs       交互式网格编排 + 多屏选屏
├── kpnav.rs     小键盘鼠标导航常驻服务
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
