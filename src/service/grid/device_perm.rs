// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::ffi::CString;

// ── Watchdog ─────────────────────────────────────────────────────

#[must_use]
pub fn display_session_uid() -> Option<u32> {
    if let Ok(dir) = std::fs::read_dir("/run/user") {
        for entry in dir.flatten() {
            let uid_str = entry.file_name().to_string_lossy().into_owned();
            let Ok(uid) = uid_str.parse::<u32>() else {
                continue;
            };
            for wn in ["wayland-0", "wayland-1"] {
                if entry.path().join(wn).exists() {
                    return Some(uid);
                }
            }
        }
    }
    let path = CString::new("/tmp/.X11-unix/X0").unwrap();
    if let Ok(st) = nix::sys::stat::stat(path.as_c_str())
        && st.st_uid != 0
    {
        return Some(st.st_uid);
    }
    None
}

pub fn setup_display_env(uid: u32) {
    let run_user = format!("/run/user/{uid}");

    for wn in ["wayland-0", "wayland-1"] {
        if std::path::Path::new(&format!("{run_user}/{wn}")).exists() {
            unsafe {
                std::env::set_var("WAYLAND_DISPLAY", wn);
                std::env::set_var("XDG_RUNTIME_DIR", &run_user);
            }
            return;
        }
    }

    let home = nix::unistd::User::from_uid(nix::unistd::Uid::from_raw(uid))
        .ok()
        .flatten()
        .map_or_else(
            || format!("/home/{uid}"),
            |u| u.dir.to_string_lossy().into_owned(),
        );
    unsafe {
        std::env::set_var("DISPLAY", ":0");
        std::env::set_var("HOME", &home);
    }
}

/// Fix ownership of the uinput devices this project created so the
/// current display session user can open them.
pub fn fix_device_permissions() {
    let Some(session_uid) = display_session_uid() else {
        return;
    };

    let Ok(dir) = std::fs::read_dir("/sys/class/input/") else {
        return;
    };
    for entry in dir.flatten() {
        let ev_name = entry.file_name().to_string_lossy().into_owned();
        if !ev_name.starts_with("event") {
            continue;
        }

        let name_path = entry.path().join("device/name");
        let Ok(dev_name) = std::fs::read_to_string(&name_path) else {
            continue;
        };
        if !dev_name.trim().starts_with(crate::config::UINPUT_NAME) {
            continue;
        }

        let dev_path = format!("/dev/input/{ev_name}");
        let Ok(path_c) = CString::new(dev_path) else {
            continue;
        };
        let Ok(st) = nix::sys::stat::stat(path_c.as_c_str()) else {
            continue;
        };
        if st.st_uid != session_uid {
            let _ = nix::unistd::chown(
                path_c.as_c_str(),
                Some(nix::unistd::Uid::from_raw(session_uid)),
                Some(nix::unistd::Gid::from_raw(st.st_gid)),
            );
        }
    }
}
