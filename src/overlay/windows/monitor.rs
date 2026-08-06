// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Monitor enumeration: `EnumDisplayMonitors` → `Monitor` list in screen
//! coordinates, mirroring the `X11 RandR` logic used on Linux.

use anyhow::Result;
use windows_sys::Win32::Foundation::{LPARAM, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO,
};

use crate::overlay::Monitor;

/// Enumerate active monitors into screen coordinates.
pub fn monitors() -> Result<Vec<Monitor>> {
    let mut out: Vec<Monitor> = Vec::new();
    unsafe {
        EnumDisplayMonitors(
            std::ptr::null_mut(),
            std::ptr::null(),
            Some(monitor_enum_proc),
            &raw mut out as LPARAM,
        );
    }
    Ok(out)
}

unsafe extern "system" fn monitor_enum_proc(
    hmon: HMONITOR,
    _hdc: HDC,
    _rect: *mut RECT,
    data: LPARAM,
) -> windows_sys::core::BOOL {
    unsafe {
        let out = &mut *(data as *mut Vec<Monitor>);
        let mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            rcMonitor: RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
            rcWork: RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
            dwFlags: 0,
        };
        let mut mi = mi;
        if GetMonitorInfoW(hmon, &raw mut mi) != 0 {
            out.push(Monitor {
                name: format!("Monitor {}", out.len() + 1),
                x: mi.rcMonitor.left,
                y: mi.rcMonitor.top,
                w: (mi.rcMonitor.right - mi.rcMonitor.left) as u16,
                h: (mi.rcMonitor.bottom - mi.rcMonitor.top) as u16,
            });
        }
        1 // TRUE — continue enumeration
    }
}
