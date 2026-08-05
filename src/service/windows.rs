// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Windows service main loop: `WH_KEYBOARD_LL` hook + message pump.
//!
//! Stage 1: glide-num + glide-alpha (mouse-only, no overlay). The hook
//! swallows consumed keys (returns non-zero) and lets the rest through.
//! Injected input (`LLKHF_INJECTED`) is passed through untouched to avoid
//! replay feedback loops.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use anyhow::{Result, bail};
use log::{error, info, warn};
use windows_sys::Win32::System::Console::SetConsoleCtrlHandler;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, SendInput,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetSystemMetrics, HC_ACTION, KBDLLHOOKSTRUCT, MSG, PM_REMOVE,
    PeekMessageW, SM_CXSCREEN, SM_CYSCREEN, SetWindowsHookExW, TranslateMessage,
    UnhookWindowsHookEx, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

use crate::device::pointer::KeyboardOut;
use crate::device::windows::keyboard::vk_to_evdev;
use crate::device::windows::mouse::Mouse;
use crate::keymap::{KEY_LEFTMETA, KEY_NUMLOCK, KEY_RIGHTMETA};
use crate::service::Service;

const TICK_MS: u64 = 20; // direction-tick period (~50 Hz)
const PROBE_INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);
const PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(100);

// LLKHF_INJECTED — the low-level hook was not injected by SendInput.
const LLKHF_INJECTED: u32 = 0x10;
// LLKHF_EXTENDED — extended key (numpad Enter, arrows, …).
const LLKHF_EXTENDED: u32 = 0x01;

// VK codes for replayed modifiers (pending passthrough).
const VK_LWIN: u16 = 0x5B;
const VK_RWIN: u16 = 0x5C;
const VK_NUMLOCK: u16 = 0x90;

thread_local! {
    static SVC: RefCell<Option<Service>> = const { RefCell::new(None) };
    static MOUSE: RefCell<Option<Mouse>> = const { RefCell::new(None) };
    static HOOK: RefCell<HookState> = const { RefCell::new(HookState::new()) };
}

static SHUTDOWN: AtomicBool = AtomicBool::new(false);

/// Wall-clock ms of the last hook call — proves the hook is alive. The main
/// loop probes it; if a probe is not answered, the OS has stalled the hook
/// (slow-callback timeout) and we reinstall it.
static LAST_HOOK_MS: AtomicU64 = AtomicU64::new(0);

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// Inject a no-function key (`VK_NONAME`) down+up as a liveness probe.
/// The probe passes through the hook (`LLKHF_INJECTED`), updating `LAST_HOOK_MS`.
fn inject_probe() -> Result<()> {
    const KEYEVENTF_KEYUP: u32 = 0x0002;
    const VK_NONAME: u16 = 0xFF;
    let down = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VK_NONAME,
                wScan: 0,
                dwFlags: 0,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let up = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VK_NONAME,
                wScan: 0,
                dwFlags: KEYEVENTF_KEYUP,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let inputs = [down, up];
    if unsafe { SendInput(2, &raw const inputs[0], std::mem::size_of::<INPUT>() as i32) } != 2 {
        bail!("SendInput probe failed");
    }
    Ok(())
}

// ── Hook callback ────────────────────────────────────────────────────

/// Per-VK pressed/swallowed tracking. Windows auto-repeats `WM_KEYDOWN` while a
/// key is held (unlike evdev, which emits one press); repeats must be suppressed
/// or `dir_held` overflows. A swallowed key keeps its repeats swallowed.
struct HookState {
    pressed: [bool; 256],
    swallowed: [bool; 256],
}

impl HookState {
    const fn new() -> Self {
        Self {
            pressed: [false; 256],
            swallowed: [false; 256],
        }
    }
}

/// `KeyboardOut` for the hook: swallowing is implicit (return 1 from the hook),
/// so `key(v, >0)` only replays a previously swallowed modifier.
struct HookKbd;

impl KeyboardOut for HookKbd {
    fn key(&mut self, code: u16, value: i32) -> Result<()> {
        if value > 0 {
            replay_key(code)?;
        }
        Ok(())
    }
    fn sync(&mut self) -> Result<()> {
        Ok(())
    }
}

fn replay_key(code: u16) -> Result<()> {
    let vk = match code {
        KEY_LEFTMETA => VK_LWIN,
        KEY_RIGHTMETA => VK_RWIN,
        KEY_NUMLOCK => VK_NUMLOCK,
        _ => return Ok(()),
    };
    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: 0,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    if unsafe { SendInput(1, &raw const input, std::mem::size_of::<INPUT>() as i32) } != 1 {
        bail!("SendInput replay failed");
    }
    Ok(())
}

unsafe extern "system" fn hook_proc(n_code: i32, wparam: usize, lparam: isize) -> isize {
    if n_code != HC_ACTION as i32 {
        return unsafe { CallNextHookEx(std::ptr::null_mut(), n_code, wparam, lparam) };
    }
    // Every hook call (including our own injected probe) proves liveness.
    LAST_HOOK_MS.store(now_ms(), Ordering::Relaxed);
    let kb = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
    if kb.flags & LLKHF_INJECTED != 0 {
        return unsafe { CallNextHookEx(std::ptr::null_mut(), n_code, wparam, lparam) };
    }

    let key_down = matches!(wparam as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
    let key_up = matches!(wparam as u32, WM_KEYUP | WM_SYSKEYUP);
    let vk = kb.vkCode as usize;
    if !(key_down || key_up) || vk >= 256 {
        return unsafe { CallNextHookEx(std::ptr::null_mut(), n_code, wparam, lparam) };
    }

    // Suppress auto-repeat: only the first press dispatches.
    if key_down {
        let repeat = HOOK.with(|s| {
            let mut s = s.borrow_mut();
            if s.pressed[vk] {
                true
            } else {
                s.pressed[vk] = true;
                false
            }
        });
        if repeat {
            let swallowed = HOOK.with(|s| s.borrow().swallowed[vk]);
            return if swallowed {
                1
            } else {
                unsafe { CallNextHookEx(std::ptr::null_mut(), n_code, wparam, lparam) }
            };
        }
    } else {
        HOOK.with(|s| {
            let mut s = s.borrow_mut();
            if !s.pressed[vk] {
                warn!("keyup without keydown (vk=0x{vk:x})");
            }
            s.pressed[vk] = false;
            s.swallowed[vk] = false;
        });
    }

    let Some(code) = vk_to_evdev(kb.vkCode, kb.scanCode, kb.flags & LLKHF_EXTENDED != 0) else {
        return unsafe { CallNextHookEx(std::ptr::null_mut(), n_code, wparam, lparam) };
    };
    let value = i32::from(key_down);

    // try_borrow_mut: a re-entrant callback (e.g. our own SendInput replay) must
    // not panic — just let the key through.
    let consumed = SVC.with(|svc| {
        MOUSE.with(|m| {
            let Ok(mut svc) = svc.try_borrow_mut() else {
                return false;
            };
            let Ok(mut m) = m.try_borrow_mut() else {
                return false;
            };
            let (Some(svc), Some(m)) = (svc.as_mut(), m.as_mut()) else {
                return false;
            };
            svc.dispatch(code, value, m, &mut HookKbd).unwrap_or(false)
        })
    });

    if key_down {
        HOOK.with(|s| s.borrow_mut().swallowed[vk] = consumed);
    }
    if consumed {
        1
    } else {
        unsafe { CallNextHookEx(std::ptr::null_mut(), n_code, wparam, lparam) }
    }
}

// ── Shutdown ─────────────────────────────────────────────────────────

extern "system" fn ctrl_handler(_ctrl_type: u32) -> i32 {
    SHUTDOWN.store(true, Ordering::Relaxed);
    1 // handled
}

// ── Main loop ────────────────────────────────────────────────────────

pub fn run() -> Result<()> {
    info!("service (windows) — glide-num + glide-alpha");

    unsafe {
        let _ = SetConsoleCtrlHandler(Some(ctrl_handler), 1);
    }

    let screen = unsafe {
        (
            GetSystemMetrics(SM_CXSCREEN) as u16,
            GetSystemMetrics(SM_CYSCREEN) as u16,
        )
    };
    let mouse = Mouse::new(screen.0, screen.1)?;

    SVC.with(|s| *s.borrow_mut() = Some(Service::new()));
    MOUSE.with(|m| *m.borrow_mut() = Some(mouse));

    let hook =
        unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), std::ptr::null_mut(), 0) };
    if hook.is_null() {
        bail!("SetWindowsHookExW failed");
    }
    info!("hook installed; Ctrl+C to quit");

    let mut msg = unsafe { std::mem::zeroed::<MSG>() };
    let tick = Duration::from_millis(TICK_MS);
    let mut result = Ok(());
    let mut last_report = std::time::Instant::now();
    let mut last_probe = std::time::Instant::now();
    let mut probe_pending = false;
    let mut probe_before = 0u64;
    let mut hook = hook;

    loop {
        if SHUTDOWN.load(Ordering::Relaxed) {
            break;
        }
        let now = std::time::Instant::now();
        while unsafe { PeekMessageW(&raw mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) } != 0 {
            if msg.message == WM_QUIT {
                break;
            }
            unsafe {
                TranslateMessage(&raw const msg);
                DispatchMessageW(&raw const msg);
            }
        }

        // Liveness probe: inject a no-function key every PROBE_INTERVAL; if the
        // hook does not answer within PROBE_TIMEOUT the OS has stalled it
        // (slow-callback timeout). Reinstall the hook and clear stuck state.
        if !probe_pending && now.duration_since(last_probe) >= PROBE_INTERVAL {
            probe_before = LAST_HOOK_MS.load(Ordering::Relaxed);
            if let Err(e) = inject_probe() {
                warn!("probe failed: {e}");
            }
            last_probe = now;
            probe_pending = true;
        }
        if probe_pending && now.duration_since(last_probe) >= PROBE_TIMEOUT {
            probe_pending = false;
            if LAST_HOOK_MS.load(Ordering::Relaxed) <= probe_before {
                warn!("hook stalled — reinstalling");
                unsafe { UnhookWindowsHookEx(hook) };
                SVC.with(|s| {
                    if let Some(svc) = s.borrow_mut().as_mut() {
                        svc.reset_direction();
                    }
                });
                hook = unsafe {
                    SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), std::ptr::null_mut(), 0)
                };
                if hook.is_null() {
                    error!("hook reinstall failed");
                    result = Err(anyhow::anyhow!("SetWindowsHookExW failed on reinstall"));
                    break;
                }
                warn!("hook reinstalled");
            }
        }

        if let Err(e) = SVC.with(|svc| {
            MOUSE.with(|m| {
                let mut svc = svc.borrow_mut();
                let mut m = m.borrow_mut();
                let (Some(svc), Some(m)) = (svc.as_mut(), m.as_mut()) else {
                    return Ok(());
                };
                svc.direction_tick(m).map(|_| ())
            })
        }) {
            result = Err(e);
            break;
        }

        // Diagnostics: print held direction masks once per second.
        if now.duration_since(last_report) >= Duration::from_secs(1) {
            let summary = SVC.with(|s| {
                s.borrow()
                    .as_ref()
                    .map_or(String::new(), Service::direction_summary)
            });
            info!("dir: {summary}");
            last_report = now;
        }

        std::thread::sleep(tick);
    }

    unsafe { UnhookWindowsHookEx(hook) };
    info!("service stopped");
    result
}
