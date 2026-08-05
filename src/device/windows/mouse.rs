// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Windows virtual pointer via `SendInput` / `SetCursorPos`.

use anyhow::Result;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN,
    MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL, MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, MOUSEINPUT,
    SendInput,
};
use windows_sys::Win32::UI::WindowsAndMessaging::SetCursorPos;

use crate::config::{CLICK_INTERVAL_MS, MouseButton};
use crate::device::pointer::{Pointer, ScrollAxis, SideButton};

/// `WHEEL_DELTA` — one wheel notch.
const WHEEL_DELTA: i32 = 120;

/// XBUTTON1 / XBUTTON2 `mouseData` values (back/forward).
const XBUTTON_BACK: u32 = 0x0001;
const XBUTTON_FORWARD: u32 = 0x0002;

/// Synthetic pointer that injects mouse input via Win32 APIs.
pub struct Mouse;

impl Mouse {
    pub fn new(_screen_w: u16, _screen_h: u16) -> Result<Self> {
        Ok(Self)
    }

    /// Warp the cursor to absolute screen coordinates.
    pub fn warp(&mut self, x: i16, y: i16) -> Result<()> {
        if unsafe { SetCursorPos(i32::from(x), i32::from(y)) } == 0 {
            anyhow::bail!("SetCursorPos failed");
        }
        Ok(())
    }

    /// Move the cursor by a relative delta.
    pub fn move_rel(&mut self, dx: i32, dy: i32) -> Result<()> {
        send(&[mouse_input(MOUSEEVENTF_MOVE, dx, dy, 0)])
    }

    /// Press a mouse button.
    pub fn button_press(&mut self, button: MouseButton) -> Result<()> {
        send(&[mouse_input(button_flag(button, true), 0, 0, 0)])
    }

    /// Release a mouse button.
    pub fn button_release(&mut self, button: MouseButton) -> Result<()> {
        send(&[mouse_input(button_flag(button, false), 0, 0, 0)])
    }

    /// Press and release a button `count` times.
    pub fn click(&mut self, button: MouseButton, count: u32) -> Result<()> {
        let half = std::time::Duration::from_millis(CLICK_INTERVAL_MS / 2);
        for _ in 0..count {
            send(&[
                mouse_input(button_flag(button, true), 0, 0, 0),
                mouse_input(button_flag(button, false), 0, 0, 0),
            ])?;
            std::thread::sleep(half);
        }
        Ok(())
    }
}

impl Pointer for Mouse {
    fn move_rel(&mut self, dx: i32, dy: i32) -> Result<()> {
        Mouse::move_rel(self, dx, dy)
    }

    fn button(&mut self, button: MouseButton, press: bool) -> Result<()> {
        if press {
            Mouse::button_press(self, button)
        } else {
            Mouse::button_release(self, button)
        }
    }

    fn click(&mut self, button: MouseButton, count: u32) -> Result<()> {
        Mouse::click(self, button, count)
    }

    fn scroll(&mut self, axis: ScrollAxis, dir: i32) -> Result<()> {
        let (flags, data) = match axis {
            ScrollAxis::Vertical => (MOUSEEVENTF_WHEEL, dir * WHEEL_DELTA),
            ScrollAxis::Horizontal => (MOUSEEVENTF_HWHEEL, dir * WHEEL_DELTA),
        };
        send(&[mouse_input(flags, 0, 0, data as u32)])
    }

    fn side(&mut self, button: SideButton) -> Result<()> {
        let data = match button {
            SideButton::Back => XBUTTON_BACK,
            SideButton::Forward => XBUTTON_FORWARD,
        };
        send(&[
            mouse_input(MOUSEEVENTF_XDOWN, 0, 0, data),
            mouse_input(MOUSEEVENTF_XUP, 0, 0, data),
        ])
    }

    fn warp(&mut self, x: i16, y: i16) -> Result<()> {
        Mouse::warp(self, x, y)
    }
}

/// Build a `MOUSEEVENTF` flag for a button press/release.
fn button_flag(button: MouseButton, press: bool) -> u32 {
    match (button, press) {
        (MouseButton::Left, true) => MOUSEEVENTF_LEFTDOWN,
        (MouseButton::Left, false) => MOUSEEVENTF_LEFTUP,
        (MouseButton::Middle, true) => MOUSEEVENTF_MIDDLEDOWN,
        (MouseButton::Middle, false) => MOUSEEVENTF_MIDDLEUP,
        (MouseButton::Right, true) => MOUSEEVENTF_RIGHTDOWN,
        (MouseButton::Right, false) => MOUSEEVENTF_RIGHTUP,
    }
}

/// Build an `INPUT` for a mouse event.
fn mouse_input(flags: u32, dx: i32, dy: i32, mouse_data: u32) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: mouse_data,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

/// Send one or more input events.
fn send(inputs: &[INPUT]) -> Result<()> {
    let n = inputs.len() as u32;
    if unsafe { SendInput(n, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32) } != n {
        anyhow::bail!("SendInput failed");
    }
    Ok(())
}
