#![deny(unsafe_op_in_unsafe_fn)]

use std::time::Duration;

use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Controls::{
    InitCommonControlsEx, ICC_STANDARD_CLASSES, ICC_WIN95_CLASSES, INITCOMMONCONTROLSEX,
};
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, PeekMessageW, PostQuitMessage, TranslateMessage, MSG, PM_REMOVE, WM_QUIT,
};

/// Initialize COM as STA on this thread for the tray icon and folder dialog.
pub fn init_com() {
    // SAFETY: Called once on the UI thread before creating COM-backed UI.
    // S_FALSE (already initialized) is treated as success by the windows crate.
    let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    // SAFETY: Best-effort; the embedded manifest already requests Per-Monitor V2.
    // A failure here means DPI was already set or the API is missing.
    let _ = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    let icc = INITCOMMONCONTROLSEX {
        dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
        dwICC: ICC_STANDARD_CLASSES | ICC_WIN95_CLASSES,
    };
    // SAFETY: `icc` is a valid INITCOMMONCONTROLSEX on this thread before any controls.
    let _ = unsafe { InitCommonControlsEx(&icc) };
}

pub fn request_quit() {
    // SAFETY: PostQuitMessage is valid on a thread that pumps messages; it
    // only posts WM_QUIT to this thread's queue.
    unsafe { PostQuitMessage(0) };
}

/// Drain the thread message queue. Returns `false` when `WM_QUIT` arrives.
pub fn pump_pending() -> bool {
    let mut msg = MSG::default();
    loop {
        // SAFETY: `msg` is a valid stack-allocated MSG. `hwnd = None` asks
        // PeekMessageW for every message on this thread, including tray and
        // hotkey window messages. PM_REMOVE dequeues the message.
        let got = unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) };
        if !got.as_bool() {
            return true;
        }
        if msg.message == WM_QUIT {
            return false;
        }
        // SAFETY: `msg` was filled by PeekMessageW for this thread.
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

pub fn wait_and_pump(timeout: Duration) -> bool {
    if !pump_pending() {
        return false;
    }
    std::thread::sleep(timeout);
    pump_pending()
}
