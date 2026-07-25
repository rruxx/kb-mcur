# kb-mcur — 键驱鼠标

**Linux 全桌面键盘工作流——扔掉鼠标。**

七级键盘网格渐进细化定位光标到屏幕任意像素。支持 CLI 子命令挂载合成器快捷键。

支持 X11 / wlroots / KDE / GNOME。已在 Openbox / Sway / niri / KDE 测试通过。

## 安装

```bash
git clone https://github.com/xxx/kb-mcur.git
cd kb-mcur
cargo build --release
```

### 权限

需要 `/dev/input/event*` 和 `/dev/uinput` 读写权限：

```bash
sudo usermod -aG input $USER
# 注销并重新登录
```

### 合成器（X11 透明效果）

遮罩层需要 X11 合成器支持半透明效果。若无合成器的纯窗口管理器，遮罩层背景将不透明（全黑）。

```bash
picom &    # Openbox/i3 先启动合成器（未测试）
```

Wayland 合成器（Sway/Hyprland/niri）原生支持透明度。

## 用法

### 交互式网格

```bash
kb-mcur     # 无参默认 grid 模式
```

| 输入 | 层级 | 操作 |
| --- | --- | --- |
| `a–z` | 1–2 | 26×26 网格 |
| `q/w/e/r/a/s/d/f` | 3 | 4×2 子格 |
| `e/r/d/f` | 4–7 | 2×2 孙格 |

| 键 | 行为 |
| --- | --- |
| 空格/回车 | 移动光标并退出 |
| `u`/`i`/`o` | 移动光标 + 切换左/中/右键按下状态 |
| `j`/`k`/`l` | 移动光标 + 单击左/中/右键 |
| `3j` | 移动光标 + 连击 3 次左键 |
| Esc | 重置网格 |

### CLI

```bash
kb-mcur move -- 10 -5       # 相对位移：右 10px，上 5px
kb-mcur moveto 500 300      # 绝对定位到 (500, 300)
kb-mcur click L             # 左键单击
kb-mcur click -r 3 M        # 中键连击 3 次
kb-mcur click R             # 右键单击
```

## 对比

| | kb-mcur | warpd | wl-kbptr | ydotool | xdotool |
| --- | --- | --- | --- | --- | --- |
| X11 | ✓ | ✓ | ✗ | ✓ | ✓ |
| wlroots | ✓ | ✓ | ✓ | ✓ | ✓ (XWayland) |
| KDE/GNOME | ✓ (XWayland) | ✗ | ✗ | ✓ | ✓ (XWayland) |
| 输出层 | /dev/uinput | XTest / wlr-pointer | wlr-pointer | /dev/uinput | XTest |
| 权限 | 仅需 input 组 | 无需 | 无需 | 需要 root | 无需 |
| 键盘接管 | EVIOCGRAB | 合成器绑定 | 合成器绑定 | 不适用 | 不适用 |
| CLI 鼠标 | ✓ | ✗ | ✗ | ✓ | ✓ |
| 语言 | Rust | C | C | C | C |

## 架构

```
src/
├── main.rs      CLI 入口
├── lib.rs       交互式网格编排
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
