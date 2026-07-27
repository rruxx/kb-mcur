# key-cursor — 键盘驱动光标控制

[English](README-en.md)

渐进式网格光标定位、小键盘鼠标导航常驻服务、CLI 宏快捷键。
支持 X11 / wlroots / KDE / GNOME。

## 安装

```bash
git clone https://github.com/xxx/key-cursor.git && cd key-cursor
cargo build --release
sudo install -m755 target/release/key-cursor /usr/bin/
```

## 依赖

| 类别 | 要求 |
| --- | --- |
| 构建 | Rust 工具链 ≥ 1.80 |
| 内核 | Linux ≥ 5.0（`/dev/uinput`） |
| 权限 | `sudo usermod -aG input $USER` |
| X11 合成器 | picom / compton 以支持叠加层透明 —— Wayland 原生支持 |

## 用法

### grid — 交互式渐进网格

1. 26×26 网格（a–z，2 个字母）
2. 4×2 子格（q/w/e/r/a/s/d/f）
3. 多层次 2×2 象限（e/r/d/f）
4. j/k/l 单击，空格/回车 定位并退出

运行 `key-cursor grid --help` 查看完整键表。

### kp-nav — 小键盘鼠标导航

NumLock+KPEnter 切换开关。非小键盘按键转发至合成器。
Grid 模式通过 Unix socket（`/run/key-cursord.sock`）自动请求键盘接管。
热插拔：每秒扫描键盘设备，拔出释放、插入独占。

运行 `key-cursor kp-nav --help` 查看完整键表。

#### systemd

```bash
sudo cp contrib/systemd/key-cursord.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now key-cursord
```

### CLI

| 命令 | 说明 |
| --- | --- |
| `key-cursor move -- 10 -5` | 相对移动 |
| `key-cursor moveto 500 300` | 绝对定位 |
| `key-cursor click -r 3 M` | 连击 |

运行任意命令加 `--help` 查看详情。

## 架构

```
src/
├── main.rs      CLI 入口
├── lib.rs       交互式网格编排
├── kpnav.rs     小键盘鼠标导航常驻服务
├── project.rs   命名常量集中定义
├── config.rs    按键映射——改键只需改此
├── grid.rs      26×26 网格 + 区域计算
├── render.rs    叠加层渲染
├── overlay.rs   X11/Wayland 双后端
├── uinput.rs    /dev/uinput 虚拟键鼠
├── evdev.rs     EVIOCGRAB 键盘接管
└── keymap.rs    US-QWERTY 键码映射
```

## 许可证

AGPL-3.0-or-later
