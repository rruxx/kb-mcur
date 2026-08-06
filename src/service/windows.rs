// Copyright (C) 2026 明雅流风
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Windows service main loop: `WH_KEYBOARD_LL` hook + message pump.
//!
//! Stage 1: glide-num + glide-alpha (mouse-only, no overlay). The hook
//! swallows consumed keys (returns non-zero) and lets the rest through.
//! Injected input (`LLKHF_INJECTED`) is passed through untouched to avoid
//! replay feedback loops. The OS may silently freeze a slow hook; a liveness
//! probe detects that and reinstalls it (see `Probe`).

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use log::{error, info, warn};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::System::Console::SetConsoleCtrlHandler;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, SendInput,
};
use windows_sys::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CallNextHookEx, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu,
    DestroyWindow, DispatchMessageW, GetCursorPos, GetSystemMetrics, HC_ACTION, HHOOK,
    IDI_APPLICATION, KBDLLHOOKSTRUCT, MF_STRING, MSG, PM_REMOVE, PeekMessageW, RegisterClassExW,
    SM_CXSCREEN, SM_CYSCREEN, SetForegroundWindow, SetWindowsHookExW, TPM_LEFTALIGN, TPM_RETURNCMD,
    TPM_RIGHTBUTTON, TrackPopupMenu, TranslateMessage, UnhookWindowsHookEx, WH_KEYBOARD_LL, WM_APP,
    WM_CONTEXTMENU, WM_KEYDOWN, WM_KEYUP, WM_QUIT, WM_RBUTTONUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
    WNDCLASSEXW,
};

use crate::device::pointer::KeyboardOut;
use crate::device::windows::keyboard::vk_to_evdev;
use crate::device::windows::mouse::Mouse;
use crate::service::Service;

const TICK_MS: u64 = 20; // direction-tick period (~50 Hz)
const PROBE_INTERVAL: Duration = Duration::from_millis(100);
const PROBE_TIMEOUT: Duration = Duration::from_millis(100);

// LLKHF_INJECTED — the low-level hook was not injected by SendInput.
const LLKHF_INJECTED: u32 = 0x10;
// LLKHF_EXTENDED — extended key (numpad Enter, arrows, …).
const LLKHF_EXTENDED: u32 = 0x01;

thread_local! {
    static SVC: RefCell<Option<Service>> = const { RefCell::new(None) };
    static MOUSE: RefCell<Option<Mouse>> = const { RefCell::new(None) };
    static HOOK: RefCell<HookState> = const { RefCell::new(HookState::new()) };
}

static SHUTDOWN: AtomicBool = AtomicBool::new(false);

/// Set by any hook call — proves the hook is alive. The liveness probe clears it
/// before injecting, then checks it: an unanswered probe means the OS stalled
/// the hook (slow-callback timeout) and we reinstall it.
static PROBE_ANSWERED: AtomicBool = AtomicBool::new(true);
/// Diagnostics: how many probes were injected vs. received by the hook.
static PROBE_INJECTED: AtomicU64 = AtomicU64::new(0);
static PROBE_RECEIVED: AtomicU64 = AtomicU64::new(0);

// ── Shared state access ───────────────────────────────────────────────

/// Run `f` with the shared service state (main-loop path, no re-entry).
fn with_service<T>(f: impl FnOnce(&mut Service, &mut Mouse) -> T) -> Option<T> {
    SVC.with(|svc| {
        MOUSE.with(|m| {
            let mut svc = svc.borrow_mut();
            let mut m = m.borrow_mut();
            let (Some(svc), Some(m)) = (svc.as_mut(), m.as_mut()) else {
                return None;
            };
            Some(f(svc, m))
        })
    })
}

/// Drain pending window messages so the low-level hook fires on this thread.
/// `msg` is only read after `PeekMessageW` reports a message.
fn pump_messages(msg: *mut MSG) {
    while unsafe { PeekMessageW(msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) } != 0 {
        if unsafe { (*msg).message } == WM_QUIT {
            break;
        }
        unsafe {
            TranslateMessage(msg);
            DispatchMessageW(msg);
        }
    }
}

// ── Liveness probe ────────────────────────────────────────────────────

/// Inject a no-function key (`VK_NONAME`) down+up as a liveness probe.
/// The hook receives it flagged `LLKHF_INJECTED` and marks `PROBE_ANSWERED`.
fn inject_probe() -> Result<()> {
    const KEYEVENTF_KEYUP: u32 = 0x0002;
    const VK_NONAME: u16 = 0xFF;
    // Reset before injecting: SendInput invokes the hook synchronously, so it
    // must observe the cleared flag *before* the injected events arrive.
    PROBE_ANSWERED.store(false, Ordering::Relaxed);
    let inputs = [
        key_input(VK_NONAME, 0),
        key_input(VK_NONAME, KEYEVENTF_KEYUP),
    ];
    if unsafe { SendInput(2, &raw const inputs[0], std::mem::size_of::<INPUT>() as i32) } != 2 {
        bail!("SendInput probe failed");
    }
    PROBE_INJECTED.fetch_add(1, Ordering::Relaxed);
    Ok(())
}

/// Build a keyboard `INPUT` for a VK with the given flags.
fn key_input(vk: u16, flags: u32) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

/// Liveness-probe state machine: arm a probe on an interval, then declare the
/// hook stalled if `PROBE_ANSWERED` was not set within a timeout.
struct Probe {
    interval: Duration,
    timeout: Duration,
    last: Instant,
    pending: bool,
}

impl Probe {
    fn new() -> Self {
        Self {
            interval: PROBE_INTERVAL,
            timeout: PROBE_TIMEOUT,
            last: Instant::now(),
            pending: false,
        }
    }

    fn due(&self, now: Instant) -> bool {
        !self.pending && now.duration_since(self.last) >= self.interval
    }

    fn arm(&mut self, now: Instant) {
        self.last = now;
        self.pending = true;
    }

    /// Returns `true` when the probe went unanswered (hook stalled).
    fn check(&mut self, now: Instant) -> bool {
        if !self.pending || now.duration_since(self.last) < self.timeout {
            return false;
        }
        self.pending = false;
        !PROBE_ANSWERED.load(Ordering::Relaxed)
    }
}

/// Reinstall the hook after a stall and clear any stuck direction state.
fn reinstall_hook(hook: &mut HHOOK) -> Result<()> {
    unsafe { UnhookWindowsHookEx(*hook) };
    with_service(|svc, _| svc.reset_direction());
    *hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), std::ptr::null_mut(), 0) };
    if hook.is_null() {
        bail!("SetWindowsHookExW failed on reinstall");
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
/// and modifiers pass through immediately, so replaying is a no-op.
struct HookKbd;

impl KeyboardOut for HookKbd {
    fn key(&mut self, _code: u16, _value: i32) -> Result<()> {
        Ok(())
    }
    fn sync(&mut self) -> Result<()> {
        Ok(())
    }
}

unsafe extern "system" fn hook_proc(n_code: i32, wparam: usize, lparam: isize) -> isize {
    if n_code != HC_ACTION as i32 {
        return unsafe { CallNextHookEx(std::ptr::null_mut(), n_code, wparam, lparam) };
    }
    // Any hook call (including our own injected probe) proves liveness.
    PROBE_ANSWERED.store(true, Ordering::Relaxed);
    let kb = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
    if kb.flags & LLKHF_INJECTED != 0 {
        PROBE_RECEIVED.fetch_add(1, Ordering::Relaxed);
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

// ── System tray ─────────────────────────────────────────────────────

/// Hidden message window hosting the tray icon (background-run exit path).
const TRAY_CLASS: &str = "kursor-tray";
const TRAY_ID: u32 = 1;
const WM_TRAY: u32 = WM_APP + 1;
const MENU_EXIT: usize = 1;

fn encode_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn module_handle() -> windows_sys::Win32::Foundation::HMODULE {
    unsafe { GetModuleHandleW(std::ptr::null()) }
}

unsafe extern "system" fn tray_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_TRAY {
        match lparam as u32 {
            WM_RBUTTONUP | WM_CONTEXTMENU => unsafe { show_tray_menu(hwnd) },
            _ => {}
        }
        return 0;
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

unsafe fn show_tray_menu(hwnd: HWND) {
    let menu = unsafe { CreatePopupMenu() };
    if menu.is_null() {
        return;
    }
    let exit_label = encode_wide("Exit");
    unsafe {
        AppendMenuW(menu, MF_STRING, MENU_EXIT, exit_label.as_ptr());
        let mut pt = POINT { x: 0, y: 0 };
        GetCursorPos(&raw mut pt);
        SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(
            menu,
            TPM_LEFTALIGN | TPM_RIGHTBUTTON | TPM_RETURNCMD,
            pt.x,
            pt.y,
            0,
            hwnd,
            std::ptr::null(),
        );
        DestroyMenu(menu);
        if cmd as usize == MENU_EXIT {
            SHUTDOWN.store(true, Ordering::Relaxed);
        }
    }
}

fn register_tray_class() {
    let class = encode_wide(TRAY_CLASS);
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: 0,
        lpfnWndProc: Some(tray_wnd_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: module_handle(),
        hIcon: std::ptr::null_mut(),
        hCursor: std::ptr::null_mut(),
        hbrBackground: std::ptr::null_mut(),
        lpszMenuName: std::ptr::null(),
        lpszClassName: class.as_ptr(),
        hIconSm: std::ptr::null_mut(),
    };
    unsafe { RegisterClassExW(&raw const wc) };
}

fn create_tray_window() -> Result<HWND> {
    let class = encode_wide(TRAY_CLASS);
    let hwnd = unsafe {
        CreateWindowExW(
            0,
            class.as_ptr(),
            class.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            module_handle(),
            std::ptr::null_mut(),
        )
    };
    if hwnd.is_null() {
        bail!("CreateWindowExW failed (tray)");
    }
    Ok(hwnd)
}

/// Show the tray icon; the right-click menu exits the service.
fn add_tray_icon(hwnd: HWND) -> Result<()> {
    unsafe {
        let mut nid = std::mem::MaybeUninit::<NOTIFYICONDATAW>::zeroed();
        let nid = nid.as_mut_ptr();
        (*nid).cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        (*nid).hWnd = hwnd;
        (*nid).uID = TRAY_ID;
        (*nid).uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        (*nid).uCallbackMessage = WM_TRAY;
        (*nid).hIcon = windows_sys::Win32::UI::WindowsAndMessaging::LoadIconW(
            std::ptr::null_mut(),
            IDI_APPLICATION,
        );
        let tip = encode_wide("kursor");
        let len = tip.len().min(128);
        let mut sz_tip = [0u16; 128];
        sz_tip[..len].copy_from_slice(&tip[..len]);
        (*nid).szTip = sz_tip;
        if Shell_NotifyIconW(NIM_ADD, nid) == 0 {
            bail!("Shell_NotifyIconW failed");
        }
        Ok(())
    }
}

fn remove_tray_icon(hwnd: HWND) {
    unsafe {
        let mut nid = std::mem::MaybeUninit::<NOTIFYICONDATAW>::zeroed();
        let nid = nid.as_mut_ptr();
        (*nid).cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        (*nid).hWnd = hwnd;
        (*nid).uID = TRAY_ID;
        Shell_NotifyIconW(NIM_DELETE, nid);
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

    // System tray: exit path for a background (double-click) run.
    register_tray_class();
    let tray = create_tray_window()?;
    if let Err(e) = add_tray_icon(tray) {
        warn!("tray icon unavailable: {e}");
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

    let mut hook =
        unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), std::ptr::null_mut(), 0) };
    if hook.is_null() {
        bail!("SetWindowsHookExW failed");
    }
    info!("hook installed; Ctrl+C to quit");

    let mut msg = std::mem::MaybeUninit::<MSG>::uninit();
    let tick = Duration::from_millis(TICK_MS);
    let mut probe = Probe::new();
    let mut last_resize = Instant::now();
    let mut result = Ok(());

    loop {
        if SHUTDOWN.load(Ordering::Relaxed) {
            break;
        }
        let now = Instant::now();
        pump_messages(msg.as_mut_ptr());

        if probe.due(now) {
            if let Err(e) = inject_probe() {
                warn!("probe failed: {e}");
            }
            probe.arm(now);
        }
        if probe.check(now) {
            warn!(
                "hook stalled — reinstalling (probes injected={} received={})",
                PROBE_INJECTED.load(Ordering::Relaxed),
                PROBE_RECEIVED.load(Ordering::Relaxed),
            );
            if let Err(e) = reinstall_hook(&mut hook) {
                error!("hook reinstall failed: {e}");
                result = Err(e);
                break;
            }
            warn!("hook reinstalled");
        }

        if let Some(Err(e)) = with_service(|svc, m| svc.direction_tick(m)) {
            result = Err(e);
            break;
        }

        if now.duration_since(last_resize)
            >= Duration::from_millis(crate::config::GRID_RESIZE_CHECK_MS)
        {
            let _ = with_service(|svc, _| {
                if let Err(e) = svc.poll_grid_resize() {
                    warn!("grid resize check: {e}");
                }
            });
            last_resize = now;
        }

        std::thread::sleep(tick);
    }

    unsafe { UnhookWindowsHookEx(hook) };
    remove_tray_icon(tray);
    unsafe { DestroyWindow(tray) };
    info!("service stopped");
    result
}
