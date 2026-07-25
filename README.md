# kb-mcur — Keyboard Mouse Cursor

**扔掉鼠标的 Linux 全桌面键盘工作流。**

通过键盘逐级定位光标到屏幕任意像素，支持 CLI 子命令绑定到合成器快捷键。

支持 X11 和 Wayland (via XWayland)。已在 Sway / Hyprland / KDE / GNOME 测试通过。

## 安装

```bash
git clone https://github.com/xxx/kb-mcur.git
cd kb-mcur
cargo build --release
```

### 权限

需要 `/dev/input/event*` 和 `/dev/uinput` 读写权限（无需 root）：

```bash
sudo usermod -aG input $USER
# 注销并重新登录
```

## 用法

### 交互式网格

```bash
kb-mcur                        # 无参进入网格
# 或绑定到桌面快捷键:
# Sway:  bindsym Mod4+g exec kb-mcur
# KDE:   自定义快捷键 → kb-mcur
```

| 输入 | 层级 | 操作 |
|------|------|------|
| `a–z` | 1 | 选行 |
| `a–z` | 2 | 选列 → 放大到 4×2 子网格 |
| `q/w/e/r/a/s/d/f` | 3 | 选子格 → 放大到 2×2 象限 |
| `e/r/d/f` | 4–7 | 逐级二分选象限 → 定位 |

| 键 | 行为 |
|----|------|
| 空格 / 回车 | 移动光标并退出 |
| `u`/`i`/`o` | 移动光标 + 切换左/中/右键按下状态 |
| `j`/`k`/`l` | 移动光标 + 单击左/中/右键 |
| `3j` | 移动光标 + 连击 3 次左键 |
| Esc | 重置网格 |

### CLI

```bash
kb-mcur move -- 10 -5       # 相对位移: 右 10px, 上 5px
kb-mcur moveto 500 300      # 绝对定位到 (500, 300)
kb-mcur click L             # 左键单击
kb-mcur click -r 3 M        # 中键连击 3 次
kb-mcur click R             # 右键单击
```

## Comparison

| | kb-mcur | warpd | wl-kbptr | ydotool |
|----|---------|-------|----------|---------|
| 叠加层 | X11/XWayland | X11/XWayland | wlr-layer-shell | 无 (纯 CLI) |
| X11 | ✓ | ✓ | ✗ | ✓ |
| Wayland wlroots | ✓ (XWayland) | ✓ (原生) | ✓ (原生) | ✓ (内核) |
| Wayland KDE/GNOME | ✓ (XWayland) | ✗ / 受限 | ✗ / 受限 | ✓ (内核) |
| 输出层 | /dev/uinput | XTest / wlr-pointer | wlr-pointer | /dev/uinput |
| root | 仅需 input 组 | 无需 | 无需 | 需要 |
| 键盘接管 | EVIOCGRAB | 合成器绑定 | 合成器绑定 | 不适用 |
| CLI 鼠标操作 | ✓ | ✗ | ✗ | ✓ |
| 语言 | Rust | C | C | C |

## 架构

```
src/
├── main.rs      CLI 入口
├── lib.rs       交互式网格编排
├── config.rs    全部按键映射 — 改键只需改这里
├── grid.rs      26×26 网格 + 区域计算
├── render.rs    叠加层渲染
├── overlay.rs   X11/XWayland 透明窗口
├── uinput.rs    /dev/uinput 虚拟键鼠
├── evdev.rs     EVIOCGRAB 全局键盘接管
└── keymap.rs    US-QWERTY 键码 → ASCII
```

## License

AGPL-3.0-or-later
