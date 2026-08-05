// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Windows virtual pointer via `SendInput` / `SetCursorPos`.

use anyhow::Result;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_MOVE, MOUSEINPUT, SendInput,
};
use windows_sys::Win32::UI::WindowsAndMessaging::SetCursorPos;

/// Synthetic pointer that injects mouse input via Win32 APIs.
pub struct Mouse {
    #[allow(dead_code)]
    screen_w: u16,
    #[allow(dead_code)]
    screen_h: u16,
}

impl Mouse {
    #[must_use]
    pub fn new(screen_w: u16, screen_h: u16) -> Result<Self> {
        Ok(Self { screen_w, screen_h })
    }

    /// Warp the cursor to absolute screen coordinates.
    pub fn warp(&mut self, x: i16, y: i16) -> Result<()> {
        let ok = unsafe { SetCursorPos(i32::from(x), i32::from(y)) };
        if ok == 0 {
            anyhow::bail!("SetCursorPos failed");
        }
        Ok(())
    }

    /// Move the cursor by a relative delta.
    pub fn move_rel(&mut self, dx: i32, dy: i32) -> Result<()> {
        let input = mouse_input(MOUSEEVENTF_MOVE, dx, dy, 0);
        send(&[input])?;
        Ok(())
    }

    /// Press a mouse button.
    pub fn button_press(&mut self, button: u8) -> Result<()> {
        let input = mouse_input(button_flag(button, true), 0, 0, 0);
        send(&[input])?;
        Ok(())
    }

    /// Release a mouse button.
    pub fn button_release(&mut self, button: u8) -> Result<()> {
        let input = mouse_input(button_flag(button, false), 0, 0, 0);
        send(&[input])?;
        Ok(())
    }

    /// Press and release a button `count` times.
    pub fn click(&mut self, button: u8, count: u32) -> Result<()> {
        for _ in 0..count {
            let down = mouse_input(button_flag(button, true), 0, 0, 0);
            let up = mouse_input(button_flag(button, false), 0, 0, 0);
            send(&[down, up])?;
        }
        Ok(())
    }
}

impl Drop for Mouse {
    fn drop(&mut self) {}
}

/// Build a `MOUSEEVENTF` flag for a button press/release.
fn button_flag(button: u8, press: bool) -> u32 {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP,
        MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
    };
    match (button, press) {
        (1, true) => MOUSEEVENTF_LEFTDOWN,
        (1, false) => MOUSEEVENTF_LEFTUP,
        (2, true) => MOUSEEVENTF_MIDDLEDOWN,
        (2, false) => MOUSEEVENTF_MIDDLEUP,
        (3, true) => MOUSEEVENTF_RIGHTDOWN,
        (3, false) => MOUSEEVENTF_RIGHTUP,
        (_, _) => 0,
    }
}

/// Build an `INPUT` for a mouse event.
#[allow(clippy::unnecessary_wraps)]
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
    let sent = unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        )
    };
    if sent != inputs.len() as u32 {
        anyhow::bail!("SendInput failed");
    }
    Ok(())
}
