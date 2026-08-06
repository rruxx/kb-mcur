// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Overlay window: a borderless, topmost, click-through layered window painted
//! per-pixel via `UpdateLayeredWindow`.

use anyhow::Result;
use windows_sys::Win32::Foundation::{HMODULE, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{AC_SRC_ALPHA, AC_SRC_OVER, BLENDFUNCTION};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, RegisterClassExW, SW_SHOW, ShowWindow,
    ULW_ALPHA, UpdateLayeredWindow, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOPMOST,
    WS_EX_TRANSPARENT, WS_POPUP,
};

use super::dib::Dib;

const CLASS_NAME: &str = "kursor-grid";

fn encode_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// Register the overlay window class. Re-registration is harmless.
pub fn register_class() -> Result<()> {
    let class_name = encode_wide(CLASS_NAME);
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(wnd_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: module_handle(),
        hIcon: std::ptr::null_mut(),
        hCursor: std::ptr::null_mut(),
        hbrBackground: std::ptr::null_mut(),
        lpszMenuName: std::ptr::null(),
        lpszClassName: class_name.as_ptr(),
        hIconSm: std::ptr::null_mut(),
    };
    unsafe { RegisterClassExW(&raw const wc) };
    Ok(())
}

/// Create a borderless topmost click-through layered window.
pub fn create_window(x: i32, y: i32, w: u16, h: u16) -> Result<HWND> {
    let class_name = encode_wide(CLASS_NAME);
    let title = encode_wide(CLASS_NAME);
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOPMOST,
            class_name.as_ptr(),
            title.as_ptr(),
            WS_POPUP,
            x,
            y,
            i32::from(w),
            i32::from(h),
            std::ptr::null_mut(), // hwndParent
            std::ptr::null_mut(), // hMenu
            module_handle(),
            std::ptr::null_mut(), // lpParam
        )
    };
    if hwnd.is_null() {
        anyhow::bail!("CreateWindowExW failed");
    }
    unsafe { ShowWindow(hwnd, SW_SHOW) };
    Ok(hwnd)
}

/// Composite the DIB onto the layered window with per-pixel alpha.
// SAFETY: `HWND` is a Win32 handle (raw pointer); validity is guaranteed by
// `create_window`/`WinBackend::add_window`. Dereferencing only passes it to Win32.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub fn update_window(hwnd: HWND, x: i32, y: i32, dib: &Dib) -> Result<()> {
    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: 255,
        AlphaFormat: AC_SRC_ALPHA as u8,
    };
    let (w, h) = dib.size();
    let pos = POINT { x, y };
    let size = SIZE { cx: w, cy: h };
    let origin = POINT { x: 0, y: 0 };
    let ok = unsafe {
        UpdateLayeredWindow(
            hwnd,
            std::ptr::null_mut(), // hdcDst — the system supplies a screen DC
            &raw const pos,
            &raw const size,
            dib.dc(),
            &raw const origin,
            0,
            &raw const blend,
            ULW_ALPHA,
        )
    };
    if ok == 0 {
        anyhow::bail!("UpdateLayeredWindow failed");
    }
    Ok(())
}

fn module_handle() -> HMODULE {
    unsafe { GetModuleHandleW(std::ptr::null()) }
}
