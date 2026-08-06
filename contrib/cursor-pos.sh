#!/bin/bash

get_cursor_x11() {
    if command -v xdotool >/dev/null 2>&1; then
        eval $(xdotool getmouselocation --shell 2>/dev/null)
        if [ -n "$X" ] && [ -n "$Y" ]; then
            echo "$X,$Y"
            return 0
        fi
    fi
    return 1
}

get_cursor_kde_wayland() {
    script_path="/tmp/getcursor.js"

    # 创建脚本
    cat > "$script_path" << 'EOF'
console.info('*{ "x":' + workspace.cursorPos.x + ', "y":' + workspace.cursorPos.y + ' }');
EOF

    # 加载脚本
    script_id=$(dbus-send --print-reply --dest=org.kde.KWin \
        /Scripting org.kde.kwin.Scripting.loadScript \
        string:"$script_path" 2>/dev/null | grep "int32" | awk '{print $2}')

    if [ -z "$script_id" ]; then
        return 1
    fi

    # 运行脚本
    dbus-send --print-reply --dest=org.kde.KWin \
        "/Scripting/Script$script_id" \
        org.kde.kwin.Script.run > /dev/null 2>&1

    # 等待日志写入
    sleep 0.2

    # 抓取日志（扩大时间窗口到 2 秒）
    coords=$(journalctl _COMM=kwin_wayland --since "2 seconds ago" -o cat 2>/dev/null | \
        grep -o '\*{ "x":[0-9]*, "y":[0-9]* }' | tail -1)

    if [ -n "$coords" ]; then
        x=$(echo "$coords" | sed 's/.*"x":\([0-9]*\).*/\1/')
        y=$(echo "$coords" | sed 's/.*"y":\([0-9]*\).*/\1/')
        echo "$x,$y"
        return 0
    fi

    return 1
}

get_cursor_wlroots() {
    if command -v wl-find-cursor >/dev/null 2>&1; then
        coords=$(wl-find-cursor -p 2>/dev/null | tr -d '()' | tr ',' ' ')
        if [ -n "$coords" ]; then
            echo "$coords"
            return 0
        fi
    fi
    return 1
}

# 检测环境（按优先级）
if [ "$XDG_SESSION_TYPE" = "wayland" ] && [ "$XDG_CURRENT_DESKTOP" = "KDE" ]; then
    # KDE Wayland 优先
    get_cursor_kde_wayland || get_cursor_x11
elif [ "$XDG_SESSION_TYPE" = "wayland" ]; then
    # 其他 Wayland
    get_cursor_wlroots || {
        echo "错误：不支持的 Wayland 合成器"
        exit 1
    }
else
    # X11 或未知环境
    get_cursor_x11 || {
        echo "错误：无法获取鼠标位置（需要安装 xdotool）"
        exit 1
    }
fi
