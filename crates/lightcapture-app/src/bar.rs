#![deny(unsafe_op_in_unsafe_fn)]

use std::sync::Mutex;

use lightcapture_core::{list_displays, list_windows, CaptureDisplay};
use windows::core::w;
use windows::Win32::Foundation::{
    GetLastError, COLORREF, ERROR_CLASS_ALREADY_EXISTS, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE,
    DWMWCP_ROUND, DWM_WINDOW_CORNER_PREFERENCE,
};
use windows::Win32::Graphics::Gdi::{
    CreateFontW, CreateSolidBrush, DeleteObject, DrawTextW, FillRect, GetStockObject,
    ScreenToClient, SetBkMode, SetTextColor, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, COLOR_WINDOW,
    DEFAULT_CHARSET, DEFAULT_PITCH, DT_CENTER, DT_SINGLELINE, DT_VCENTER, FF_DONTCARE, FW_SEMIBOLD,
    HBRUSH, HDC, HFONT, OUT_TT_PRECIS, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{DRAWITEMSTRUCT, ODS_SELECTED};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    ChildWindowFromPoint, CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect,
    GetSystemMetrics, GetWindowLongPtrW, GetWindowTextW, LoadCursorW, RegisterClassW, SendMessageW,
    SetWindowDisplayAffinity, SetWindowLongPtrW, SetWindowPos, SetWindowTextW, ShowWindow,
    SystemParametersInfoW, BM_GETCHECK, BM_SETCHECK, BN_CLICKED, BS_AUTOCHECKBOX, BS_OWNERDRAW,
    CBN_DROPDOWN, CBN_SELCHANGE, CBS_DROPDOWNLIST, CB_ADDSTRING, CB_GETCURSEL, CB_RESETCONTENT,
    CB_SETCURSEL, CB_SETITEMHEIGHT, CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, GWLP_USERDATA,
    HTCAPTION, HTCLIENT, HWND_TOPMOST, IDC_ARROW, SM_CXSCREEN, SPI_GETWORKAREA, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SW_HIDE, SW_SHOW, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    WDA_EXCLUDEFROMCAPTURE, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE, WM_COMMAND, WM_CREATE,
    WM_CTLCOLORSTATIC, WM_DESTROY, WM_DRAWITEM, WM_ERASEBKGND, WM_NCHITTEST, WM_SETFONT, WNDCLASSW,
    WS_BORDER, WS_CHILD, WS_CLIPCHILDREN, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_TABSTOP,
    WS_VISIBLE, WS_VSCROLL,
};

use crate::settings::{AudioSetting, Settings, SourceSetting};
use crate::tray::Command;

const CLASS_NAME: windows::core::PCWSTR = w!("LightCaptureRecorderBar");
const IDC_MODE: i32 = 100;
const IDC_TARGET: i32 = 101;
const IDC_SYSTEM: i32 = 102;
const IDC_MIC: i32 = 103;
const IDC_RECORD: i32 = 104;
const IDC_PAUSE: i32 = 105;
const IDC_STOP: i32 = 106;
const IDC_ELAPSED: i32 = 107;
const IDC_STATUS: i32 = 108;
const IDC_CLOSE: i32 = 109;
const MODE_SCREEN: i32 = 0;
const MODE_WINDOW: i32 = 1;
const COLOR_BG: COLORREF = COLORREF(0x001F_1B1B);
const COLOR_TEXT: COLORREF = COLORREF(0x00F4_F4F5);
const COLOR_MUTED: COLORREF = COLORREF(0x00A1_A1AA);
const COLOR_RECORD: COLORREF = COLORREF(0x0048_1DE1);
const COLOR_RECORD_DOWN: COLORREF = COLORREF(0x003A_17B4);
const COLOR_PAUSE: COLORREF = COLORREF(0x003F_3F46);
const COLOR_PAUSE_DOWN: COLORREF = COLORREF(0x0052_525A);
const COLOR_STOP: COLORREF = COLORREF(0x0027_272A);
const COLOR_STOP_DOWN: COLORREF = COLORREF(0x003F_3F46);
const COLOR_CLOSE: COLORREF = COLORREF(0x0027_272A);

pub struct BarView {
    pub recording: bool,
    pub paused: bool,
    pub elapsed: std::time::Duration,
    pub status: Option<String>,
}

struct BarInner {
    hwnd: HWND,
    mode: HWND,
    target: HWND,
    system: HWND,
    mic: HWND,
    record: HWND,
    pause: HWND,
    stop: HWND,
    elapsed: HWND,
    status: HWND,
    close: HWND,
    font: HFONT,
    bg: HBRUSH,
    dpi: u32,
    displays: Vec<CaptureDisplay>,
    window_titles: Vec<String>,
    commands: Mutex<Vec<Command>>,
    exclude_failed: bool,
    suppress: bool,
}

pub struct RecorderBar {
    hwnd: HWND,
}

impl RecorderBar {
    pub fn new(settings: &Settings) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let instance = register_class()?;
        let inner = Box::new(BarInner {
            hwnd: HWND::default(),
            mode: HWND::default(),
            target: HWND::default(),
            system: HWND::default(),
            mic: HWND::default(),
            record: HWND::default(),
            pause: HWND::default(),
            stop: HWND::default(),
            elapsed: HWND::default(),
            status: HWND::default(),
            close: HWND::default(),
            font: HFONT::default(),
            bg: HBRUSH::default(),
            dpi: 96,
            displays: list_displays().unwrap_or_default(),
            window_titles: Vec::new(),
            commands: Mutex::new(Vec::new()),
            exclude_failed: false,
            suppress: false,
        });
        let raw = Box::into_raw(inner);
        let (x, y) = bar_origin(96);
        let layout = Layout::new(96);
        let hwnd = unsafe {
            // SAFETY: CLASS_NAME is a static class we just registered; `raw` is a valid BarInner
            // pointer passed as lpParam and stored in WM_CREATE on success.
            CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                CLASS_NAME,
                w!("LightCapture"),
                WS_POPUP | WS_BORDER | WS_CLIPCHILDREN,
                x,
                y,
                layout.width,
                layout.height,
                None,
                None,
                Some(instance.into()),
                Some(raw.cast()),
            )
        };
        let hwnd = match hwnd {
            Ok(hwnd) => hwnd,
            Err(err) => {
                drop(unsafe {
                    // SAFETY: WM_CREATE either did not run, or returned -1 without storing
                    // USERDATA, so WM_DESTROY did not take the Box. We still own `raw`.
                    Box::from_raw(raw)
                });
                return Err(err.into());
            }
        };
        apply_chrome(hwnd);
        let bar = Self { hwnd };
        with_inner(hwnd, |state| {
            state.suppress = true;
            select_source(state, &settings.source);
            set_check_if_changed(state.system, settings.audio.system);
            set_check_if_changed(state.mic, settings.audio.microphone);
            state.suppress = false;
        });
        bar.show();
        Ok(bar)
    }

    pub fn show(&self) {
        unsafe {
            // SAFETY: `self.hwnd` is a window created in `new` and not yet destroyed.
            let _ = SetWindowPos(
                self.hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
            let _ = ShowWindow(self.hwnd, SW_SHOW);
        }
    }

    #[allow(dead_code)] // Public API; the bar hides via WM_CLOSE, not the session loop.
    pub fn hide(&self) {
        unsafe {
            // SAFETY: `self.hwnd` is a window created in `new` and not yet destroyed.
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
    }

    pub fn take_commands(&self) -> Vec<Command> {
        with_inner(self.hwnd, |state| {
            state
                .commands
                .lock()
                .map(|mut q| q.drain(..).collect())
                .unwrap_or_default()
        })
        .unwrap_or_default()
    }

    pub fn sync(&self, settings: &Settings, view: &BarView) {
        with_inner(self.hwnd, |state| {
            state.suppress = true;
            let idle = !view.recording;
            if idle {
                set_check_if_changed(state.system, settings.audio.system);
                set_check_if_changed(state.mic, settings.audio.microphone);
            }
            unsafe {
                // SAFETY: child HWNDs were created in WM_CREATE and remain valid until WM_DESTROY.
                let _ = EnableWindow(state.mode, idle);
                let _ = EnableWindow(state.target, idle);
                let _ = EnableWindow(state.system, idle);
                let _ = EnableWindow(state.mic, idle);
                let _ = ShowWindow(state.record, if idle { SW_SHOW } else { SW_HIDE });
                let _ = ShowWindow(state.pause, if idle { SW_HIDE } else { SW_SHOW });
                let _ = ShowWindow(state.stop, if idle { SW_HIDE } else { SW_SHOW });
                let _ = ShowWindow(state.elapsed, if idle { SW_HIDE } else { SW_SHOW });
            }
            if view.recording {
                set_text(state.pause, if view.paused { "Resume" } else { "Pause" });
                set_text(state.elapsed, &format_elapsed(view.elapsed));
            }
            let status = view.status.clone().or_else(|| {
                state.exclude_failed.then(|| {
                    "Toolbar may appear in the recording (exclude-from-capture failed)".into()
                })
            });
            set_text(state.status, status.as_deref().unwrap_or("Ready"));
            state.suppress = false;
        });
    }
}

impl Drop for RecorderBar {
    fn drop(&mut self) {
        if !self.hwnd.is_invalid() {
            unsafe {
                // SAFETY: hwnd is owned by this RecorderBar; DestroyWindow posts WM_DESTROY which
                // frees BarInner. After this, the HWND must not be used.
                let _ = DestroyWindow(self.hwnd);
            }
        }
    }
}

#[derive(Clone, Copy)]
struct Layout {
    width: i32,
    height: i32,
    pad: i32,
    row_y: i32,
    ctrl_h: i32,
    mode_x: i32,
    mode_w: i32,
    target_x: i32,
    target_w: i32,
    system_x: i32,
    system_w: i32,
    mic_x: i32,
    mic_w: i32,
    elapsed_x: i32,
    elapsed_w: i32,
    record_x: i32,
    record_w: i32,
    pause_x: i32,
    pause_w: i32,
    stop_x: i32,
    stop_w: i32,
    close_x: i32,
    close_w: i32,
    status_y: i32,
    status_h: i32,
    combo_list_h: i32,
}

impl Layout {
    fn new(dpi: u32) -> Self {
        let s = |px: i32| scale_px(dpi, px);
        let pad = s(12);
        let gap = s(8);
        let row_y = s(10);
        let ctrl_h = s(32);
        let mode_w = s(110);
        let target_w = s(236);
        let system_w = s(80);
        let mic_w = s(64);
        let elapsed_w = s(56);
        let record_w = s(100);
        let pause_w = s(86);
        let stop_w = s(86);
        let close_w = s(32);
        let status_h = s(18);
        let mode_x = pad;
        let target_x = mode_x + mode_w + gap;
        let system_x = target_x + target_w + gap;
        let mic_x = system_x + system_w + gap;
        let elapsed_x = mic_x + mic_w + gap;
        let pause_x = elapsed_x + elapsed_w + gap;
        let stop_x = pause_x + pause_w + gap;
        let record_x = stop_x;
        let close_x = stop_x + stop_w + gap;
        let width = close_x + close_w + pad;
        let status_y = row_y + ctrl_h + s(6);
        let height = status_y + status_h + s(8);
        Self {
            width,
            height,
            pad,
            row_y,
            ctrl_h,
            mode_x,
            mode_w,
            target_x,
            target_w,
            system_x,
            system_w,
            mic_x,
            mic_w,
            elapsed_x,
            elapsed_w,
            record_x,
            record_w,
            pause_x,
            pause_w,
            stop_x,
            stop_w,
            close_x,
            close_w,
            status_y,
            status_h,
            combo_list_h: s(220),
        }
    }
}

fn scale_px(dpi: u32, px: i32) -> i32 {
    let dpi = i64::from(dpi.max(96));
    (i64::from(px) * dpi / 96) as i32
}

fn register_class() -> windows::core::Result<windows::Win32::Foundation::HMODULE> {
    // SAFETY: None requests the handle of this process.
    let instance = unsafe { GetModuleHandleW(None) }?;
    let class = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(wndproc),
        hInstance: instance.into(),
        // SAFETY: IDC_ARROW is a predefined system cursor resource.
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW) }?,
        hbrBackground: HBRUSH((COLOR_WINDOW.0 as usize + 1) as *mut core::ffi::c_void),
        lpszClassName: CLASS_NAME,
        ..Default::default()
    };
    let atom = unsafe {
        // SAFETY: `class` is fully initialized with a valid instance, class name, and WndProc.
        RegisterClassW(&class)
    };
    if atom == 0 {
        // SAFETY: called immediately after RegisterClassW so the last-error is that call.
        let err = unsafe { GetLastError() };
        if err != ERROR_CLASS_ALREADY_EXISTS {
            return Err(windows::core::Error::from_win32());
        }
    }
    Ok(instance)
}

fn apply_chrome(hwnd: HWND) {
    let dark: i32 = 1;
    unsafe {
        // SAFETY: hwnd is our window; dark is a BOOL living for this call.
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            std::ptr::from_ref(&dark).cast(),
            std::mem::size_of_val(&dark) as u32,
        );
        let corner = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            std::ptr::from_ref(&corner).cast(),
            std::mem::size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32,
        );
    }
}

fn bar_origin(dpi: u32) -> (i32, i32) {
    let layout = Layout::new(dpi);
    let mut work = RECT::default();
    let ok = unsafe {
        // SAFETY: `work` is a valid RECT; SPI_GETWORKAREA writes a RECT into that buffer.
        SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some((&mut work as *mut RECT).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    if ok.is_ok() {
        let x = work.left + (work.right - work.left - layout.width).max(0) / 2;
        return (x, work.top + scale_px(dpi, 12));
    }
    // SAFETY: SM_CXSCREEN is a documented GetSystemMetrics index.
    let screen_w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    ((screen_w - layout.width).max(0) / 2, scale_px(dpi, 12))
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
            // SAFETY: lparam is CREATESTRUCTW from CreateWindowExW; lpCreateParams is BarInner.
            let create = unsafe { &*(lparam.0 as *const CREATESTRUCTW) };
            let state = create.lpCreateParams as *mut BarInner;
            unsafe {
                // SAFETY: `state` is the Box we passed as lpParam in CreateWindowExW.
                (*state).hwnd = hwnd;
            }
            if let Err(err) = create_children(unsafe {
                // SAFETY: `state` is a valid exclusive BarInner during WM_CREATE.
                &mut *state
            }) {
                eprintln!("{err}");
                return LRESULT(-1);
            }
            unsafe {
                // SAFETY: hwnd is the window being created; children exist, so the window may own BarInner.
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, state as isize);
            }
            let excluded = unsafe {
                // SAFETY: hwnd is the window currently being created.
                SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE)
            };
            if excluded.is_err() {
                unsafe {
                    // SAFETY: `state` remains valid for the duration of WM_CREATE.
                    (*state).exclude_failed = true;
                }
                set_text(
                    unsafe {
                        // SAFETY: status HWND was created before this affinity attempt.
                        (*state).status
                    },
                    "Toolbar may appear in the recording (exclude-from-capture failed)",
                );
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => {
            paint_background(hwnd, HDC(wparam.0 as *mut core::ffi::c_void));
            LRESULT(1)
        }
        WM_CTLCOLORSTATIC => with_inner(hwnd, |state| {
            let hdc = HDC(wparam.0 as *mut core::ffi::c_void);
            unsafe {
                // SAFETY: hdc is the control DC for this color message; bg brush is owned by BarInner.
                let _ = SetBkMode(hdc, TRANSPARENT);
                let _ = SetTextColor(hdc, COLOR_MUTED);
            }
            LRESULT(state.bg.0 as isize)
        })
        .unwrap_or(LRESULT(0)),
        WM_DRAWITEM => {
            // SAFETY: lparam is DRAWITEMSTRUCT for an owner-drawn child we created.
            let draw = unsafe { &*(lparam.0 as *const DRAWITEMSTRUCT) };
            draw_action(draw);
            LRESULT(1)
        }
        WM_NCHITTEST => {
            let hit = unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
            if hit.0 == HTCLIENT as isize {
                let mut pt = POINT {
                    x: lparam.0 as i16 as i32,
                    y: (lparam.0 >> 16) as i16 as i32,
                };
                unsafe {
                    // SAFETY: hwnd is our bar; pt is screen coordinates from lParam.
                    let _ = ScreenToClient(hwnd, &mut pt);
                    let child = ChildWindowFromPoint(hwnd, pt);
                    if child.0.is_null() || child == hwnd {
                        return LRESULT(HTCAPTION as isize);
                    }
                }
            }
            hit
        }
        WM_COMMAND => {
            with_inner(hwnd, |state| {
                if !state.suppress {
                    handle_command_message(state, wparam);
                }
            });
            LRESULT(0)
        }
        WM_CLOSE => {
            unsafe {
                // SAFETY: hwnd is our bar window; hide instead of destroying so WM_QUIT is not posted.
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            let ptr = unsafe {
                // SAFETY: hwnd is being destroyed; USERDATA was set in WM_CREATE or is zero.
                GetWindowLongPtrW(hwnd, GWLP_USERDATA)
            } as *mut BarInner;
            if !ptr.is_null() {
                unsafe {
                    // SAFETY: clearing USERDATA so later messages cannot alias the Box we are about to drop.
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                }
                let state = unsafe {
                    // SAFETY: ptr was stored in GWLP_USERDATA at WM_CREATE and is uniquely owned here.
                    Box::from_raw(ptr)
                };
                drop_gdi(&state);
                drop(state);
            }
            LRESULT(0)
        }
        _ => unsafe {
            // SAFETY: default handling for unhandled messages on a valid HWND.
            DefWindowProcW(hwnd, msg, wparam, lparam)
        },
    }
}

fn paint_background(hwnd: HWND, hdc: HDC) {
    let mut rc = RECT::default();
    unsafe {
        // SAFETY: rc receives the client rect of our window.
        let _ = GetClientRect(hwnd, &mut rc);
    }
    let brush = with_inner(hwnd, |state| state.bg).unwrap_or_else(|| unsafe {
        // SAFETY: BLACK_BRUSH is a stock object.
        HBRUSH(GetStockObject(windows::Win32::Graphics::Gdi::BLACK_BRUSH).0)
    });
    unsafe {
        // SAFETY: hdc is the erase DC; brush is either ours or a stock object.
        let _ = FillRect(hdc, &rc, brush);
    }
}

fn draw_action(draw: &DRAWITEMSTRUCT) {
    let selected = (draw.itemState.0 & ODS_SELECTED.0) != 0;
    let (label, color) = match draw.CtlID as i32 {
        IDC_RECORD => (
            "Record",
            if selected {
                COLOR_RECORD_DOWN
            } else {
                COLOR_RECORD
            },
        ),
        IDC_PAUSE => (
            "",
            if selected {
                COLOR_PAUSE_DOWN
            } else {
                COLOR_PAUSE
            },
        ),
        IDC_STOP => (
            "Stop",
            if selected {
                COLOR_STOP_DOWN
            } else {
                COLOR_STOP
            },
        ),
        IDC_CLOSE => ("×", COLOR_CLOSE),
        _ => return,
    };
    let label = if draw.CtlID as i32 == IDC_PAUSE {
        let mut buf = [0u16; 16];
        let n = unsafe {
            // SAFETY: hwndItem is our pause button; buf is a writable UTF-16 buffer.
            GetWindowTextW(draw.hwndItem, &mut buf)
        };
        let text = String::from_utf16_lossy(&buf[..n.max(0) as usize]);
        draw_filled(draw.hDC, draw.rcItem, color, &text);
        return;
    } else {
        label
    };
    draw_filled(draw.hDC, draw.rcItem, color, label);
}

fn draw_filled(hdc: HDC, rc: RECT, color: COLORREF, label: &str) {
    unsafe {
        // SAFETY: hdc is the owner-draw DC; brush lives for FillRect then is deleted.
        let brush = CreateSolidBrush(color);
        let _ = FillRect(hdc, &rc, brush);
        let _ = DeleteObject(brush.into());
        let _ = SetBkMode(hdc, TRANSPARENT);
        let _ = SetTextColor(hdc, COLOR_TEXT);
        let mut text = wide(label);
        let mut rc = rc;
        let _ = DrawTextW(
            hdc,
            &mut text,
            &mut rc,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
    }
}

fn drop_gdi(state: &BarInner) {
    unsafe {
        // SAFETY: font/bg were created in WM_CREATE and are not stock objects.
        if !state.font.is_invalid() {
            let _ = DeleteObject(state.font.into());
        }
        if !state.bg.is_invalid() {
            let _ = DeleteObject(state.bg.into());
        }
    }
}

fn create_children(state: &mut BarInner) -> windows::core::Result<()> {
    // SAFETY: None requests the handle of this process.
    let instance = unsafe { GetModuleHandleW(None) }?;
    let parent = state.hwnd;
    state.dpi = unsafe { GetDpiForWindow(parent) }.max(96);
    state.bg = unsafe { CreateSolidBrush(COLOR_BG) };
    state.font = make_font(state.dpi)?;
    let layout = Layout::new(state.dpi);
    unsafe {
        // SAFETY: parent is our newly created window; size matches the DPI layout.
        let _ = SetWindowPos(
            parent,
            None,
            0,
            0,
            layout.width,
            layout.height,
            SWP_NOMOVE | SWP_NOACTIVATE,
        );
    }
    let font = state.font;
    state.mode = child(
        parent,
        instance.into(),
        w!("COMBOBOX"),
        "",
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_VSCROLL | WINDOW_STYLE(CBS_DROPDOWNLIST as u32),
        layout.mode_x,
        layout.row_y,
        layout.mode_w,
        layout.combo_list_h,
        IDC_MODE,
        font,
    )?;
    state.target = child(
        parent,
        instance.into(),
        w!("COMBOBOX"),
        "",
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_VSCROLL | WINDOW_STYLE(CBS_DROPDOWNLIST as u32),
        layout.target_x,
        layout.row_y,
        layout.target_w,
        layout.combo_list_h,
        IDC_TARGET,
        font,
    )?;
    state.system = child(
        parent,
        instance.into(),
        w!("BUTTON"),
        "System",
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
        layout.system_x,
        layout.row_y,
        layout.system_w,
        layout.ctrl_h,
        IDC_SYSTEM,
        font,
    )?;
    state.mic = child(
        parent,
        instance.into(),
        w!("BUTTON"),
        "Mic",
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
        layout.mic_x,
        layout.row_y,
        layout.mic_w,
        layout.ctrl_h,
        IDC_MIC,
        font,
    )?;
    state.elapsed = child(
        parent,
        instance.into(),
        w!("STATIC"),
        "00:00",
        WS_CHILD,
        layout.elapsed_x,
        layout.row_y,
        layout.elapsed_w,
        layout.ctrl_h,
        IDC_ELAPSED,
        font,
    )?;
    state.record = child(
        parent,
        instance.into(),
        w!("BUTTON"),
        "Record",
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_OWNERDRAW as u32),
        layout.record_x,
        layout.row_y,
        layout.record_w,
        layout.ctrl_h,
        IDC_RECORD,
        font,
    )?;
    state.pause = child(
        parent,
        instance.into(),
        w!("BUTTON"),
        "Pause",
        WS_CHILD | WS_TABSTOP | WINDOW_STYLE(BS_OWNERDRAW as u32),
        layout.pause_x,
        layout.row_y,
        layout.pause_w,
        layout.ctrl_h,
        IDC_PAUSE,
        font,
    )?;
    state.stop = child(
        parent,
        instance.into(),
        w!("BUTTON"),
        "Stop",
        WS_CHILD | WS_TABSTOP | WINDOW_STYLE(BS_OWNERDRAW as u32),
        layout.stop_x,
        layout.row_y,
        layout.stop_w,
        layout.ctrl_h,
        IDC_STOP,
        font,
    )?;
    state.close = child(
        parent,
        instance.into(),
        w!("BUTTON"),
        "×",
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_OWNERDRAW as u32),
        layout.close_x,
        layout.row_y,
        layout.close_w,
        layout.ctrl_h,
        IDC_CLOSE,
        font,
    )?;
    state.status = child(
        parent,
        instance.into(),
        w!("STATIC"),
        "Ready",
        WS_CHILD | WS_VISIBLE,
        layout.pad,
        layout.status_y,
        layout.width - layout.pad * 2,
        layout.status_h,
        IDC_STATUS,
        font,
    )?;
    set_combo_height(state.mode, layout.ctrl_h);
    set_combo_height(state.target, layout.ctrl_h);
    fill_mode(state);
    fill_targets(state, true);
    Ok(())
}

fn make_font(dpi: u32) -> windows::core::Result<HFONT> {
    let height = -scale_px(dpi, 12);
    let font = unsafe {
        // SAFETY: Segoe UI is a system font name; returned HFONT is owned by the caller.
        CreateFontW(
            height,
            0,
            0,
            0,
            FW_SEMIBOLD.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_TT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            DEFAULT_PITCH.0 as u32 | FF_DONTCARE.0 as u32,
            w!("Segoe UI"),
        )
    };
    if font.is_invalid() {
        return Err(windows::core::Error::from_win32());
    }
    Ok(font)
}

fn set_combo_height(hwnd: HWND, height: i32) {
    unsafe {
        // SAFETY: hwnd is a combobox; wParam -1 sets the closed-state height.
        let _ = SendMessageW(
            hwnd,
            CB_SETITEMHEIGHT,
            Some(WPARAM(usize::MAX)),
            Some(LPARAM(height as isize)),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn child(
    parent: HWND,
    instance: windows::Win32::Foundation::HINSTANCE,
    class: windows::core::PCWSTR,
    title: &str,
    style: WINDOW_STYLE,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    id: i32,
    font: HFONT,
) -> windows::core::Result<HWND> {
    let title_w = wide(title);
    let hwnd = unsafe {
        // SAFETY: parent is a live window; title_w is NUL-terminated and lives for this call;
        // child id is passed as HMENU per Win32 CreateWindowEx for child windows.
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class,
            windows::core::PCWSTR(title_w.as_ptr()),
            style,
            x,
            y,
            w,
            h,
            Some(parent),
            Some(windows::Win32::UI::WindowsAndMessaging::HMENU(
                id as *mut core::ffi::c_void,
            )),
            Some(instance),
            None,
        )
    }?;
    unsafe {
        // SAFETY: hwnd is the child just created; font is an HFONT we created.
        let _ = SendMessageW(
            hwnd,
            WM_SETFONT,
            Some(WPARAM(font.0 as usize)),
            Some(LPARAM(1)),
        );
    }
    Ok(hwnd)
}

fn handle_command_message(state: &mut BarInner, wparam: WPARAM) {
    let id = (wparam.0 as u32) & 0xffff;
    let code = (wparam.0 as u32) >> 16;
    match id as i32 {
        IDC_RECORD | IDC_STOP if code == BN_CLICKED => push(state, Command::Toggle),
        IDC_PAUSE if code == BN_CLICKED => push(state, Command::Pause),
        IDC_CLOSE if code == BN_CLICKED => unsafe {
            // SAFETY: hwnd is the bar window; hide matches the caption-X behavior.
            let _ = ShowWindow(state.hwnd, SW_HIDE);
        },
        IDC_SYSTEM | IDC_MIC if code == BN_CLICKED => {
            push(
                state,
                Command::SetAudio(AudioSetting {
                    system: is_checked(state.system),
                    microphone: is_checked(state.mic),
                }),
            );
        }
        IDC_MODE if code == CBN_SELCHANGE => {
            let screen = current_sel(state.mode) == MODE_SCREEN;
            fill_targets(state, screen);
            if let Some(source) = selected_source(state) {
                push(state, Command::Source(source));
            }
        }
        IDC_TARGET if code == CBN_DROPDOWN => {
            if current_sel(state.mode) == MODE_WINDOW {
                fill_targets(state, false);
            } else {
                state.displays = list_displays().unwrap_or_default();
                fill_targets(state, true);
            }
        }
        IDC_TARGET if code == CBN_SELCHANGE => {
            if let Some(source) = selected_source(state) {
                push(state, Command::Source(source));
            }
        }
        _ => {}
    }
}

fn fill_mode(state: &mut BarInner) {
    combo_reset(state.mode);
    combo_add(state.mode, "Screen");
    combo_add(state.mode, "Window");
}

fn fill_targets(state: &mut BarInner, screen: bool) {
    combo_reset(state.target);
    if screen {
        combo_add(state.target, "Primary");
        for display in &state.displays {
            combo_add(
                state.target,
                &format!("{} ({}×{})", display.name, display.width, display.height),
            );
        }
    } else {
        combo_add(state.target, "Foreground window");
        state.window_titles = list_windows()
            .unwrap_or_default()
            .into_iter()
            .map(|w| w.title)
            .filter(|t| !t.trim().is_empty())
            .collect();
        for title in &state.window_titles {
            combo_add(state.target, title);
        }
    }
    combo_set(state.target, 0);
}

fn select_source(state: &mut BarInner, source: &SourceSetting) {
    match source {
        SourceSetting::Primary => {
            combo_set(state.mode, MODE_SCREEN);
            fill_targets(state, true);
            combo_set(state.target, 0);
        }
        SourceSetting::Display { id, name, index } => {
            combo_set(state.mode, MODE_SCREEN);
            state.displays = list_displays().unwrap_or_default();
            fill_targets(state, true);
            let pos = state.displays.iter().position(|d| {
                (!id.is_empty() && d.device_id == *id)
                    || (!name.is_empty() && d.name == *name)
                    || d.index == *index
            });
            combo_set(state.target, pos.map(|p| p as i32 + 1).unwrap_or(0));
        }
        SourceSetting::Foreground => {
            combo_set(state.mode, MODE_WINDOW);
            fill_targets(state, false);
            combo_set(state.target, 0);
        }
        SourceSetting::Window { title } => {
            combo_set(state.mode, MODE_WINDOW);
            fill_targets(state, false);
            if let Some(pos) = state.window_titles.iter().position(|t| t == title) {
                combo_set(state.target, pos as i32 + 1);
            } else {
                state.window_titles.insert(0, title.clone());
                combo_reset(state.target);
                combo_add(state.target, "Foreground window");
                for t in &state.window_titles {
                    combo_add(state.target, t);
                }
                combo_set(state.target, 1);
            }
        }
    }
}

fn selected_source(state: &BarInner) -> Option<SourceSetting> {
    if current_sel(state.mode) == MODE_SCREEN {
        let sel = current_sel(state.target);
        if sel <= 0 {
            return Some(SourceSetting::Primary);
        }
        let display = state.displays.get((sel - 1) as usize)?;
        Some(SourceSetting::Display {
            index: display.index,
            id: display.device_id.clone(),
            name: display.name.clone(),
        })
    } else {
        let sel = current_sel(state.target);
        if sel <= 0 {
            return Some(SourceSetting::Foreground);
        }
        let title = state.window_titles.get((sel - 1) as usize)?.clone();
        Some(SourceSetting::Window { title })
    }
}

fn with_inner<R>(hwnd: HWND, f: impl FnOnce(&mut BarInner) -> R) -> Option<R> {
    let ptr = unsafe {
        // SAFETY: hwnd is a live bar window (or null USERDATA before create / after destroy).
        GetWindowLongPtrW(hwnd, GWLP_USERDATA)
    } as *mut BarInner;
    if ptr.is_null() {
        None
    } else {
        // SAFETY: ptr was stored in GWLP_USERDATA at WM_CREATE and is freed only on WM_DESTROY.
        Some(f(unsafe { &mut *ptr }))
    }
}

fn push(state: &BarInner, command: Command) {
    if let Ok(mut q) = state.commands.lock() {
        q.push(command);
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

fn set_text(hwnd: HWND, text: &str) {
    let w = wide(text);
    unsafe {
        // SAFETY: hwnd is a control we created; `w` is a NUL-terminated UTF-16 buffer for this call.
        let _ = SetWindowTextW(hwnd, windows::core::PCWSTR(w.as_ptr()));
    }
}

fn combo_reset(hwnd: HWND) {
    unsafe {
        // SAFETY: hwnd is a combobox we created.
        let _ = SendMessageW(hwnd, CB_RESETCONTENT, None, None);
    }
}

fn combo_add(hwnd: HWND, text: &str) {
    let w = wide(text);
    unsafe {
        // SAFETY: hwnd is a combobox; `w` lives for the duration of CB_ADDSTRING.
        let _ = SendMessageW(hwnd, CB_ADDSTRING, None, Some(LPARAM(w.as_ptr() as isize)));
    }
}

fn combo_set(hwnd: HWND, index: i32) {
    unsafe {
        // SAFETY: hwnd is a combobox we created.
        let _ = SendMessageW(hwnd, CB_SETCURSEL, Some(WPARAM(index as usize)), None);
    }
}

fn current_sel(hwnd: HWND) -> i32 {
    unsafe {
        // SAFETY: hwnd is a combobox we created.
        SendMessageW(hwnd, CB_GETCURSEL, None, None).0 as i32
    }
}

fn set_check(hwnd: HWND, checked: bool) {
    unsafe {
        // SAFETY: hwnd is a checkbox button we created.
        let _ = SendMessageW(
            hwnd,
            BM_SETCHECK,
            Some(WPARAM(if checked { 1 } else { 0 })),
            None,
        );
    }
}

fn set_check_if_changed(hwnd: HWND, checked: bool) {
    if is_checked(hwnd) != checked {
        set_check(hwnd, checked);
    }
}

fn is_checked(hwnd: HWND) -> bool {
    unsafe {
        // SAFETY: hwnd is a checkbox button we created.
        SendMessageW(hwnd, BM_GETCHECK, None, None).0 == 1
    }
}

fn format_elapsed(elapsed: std::time::Duration) -> String {
    let secs = elapsed.as_secs();
    format!("{:02}:{:02}", secs / 60, secs % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_is_identity_at_96_dpi() {
        assert_eq!(scale_px(96, 32), 32);
    }

    #[test]
    fn scale_is_150_percent_at_144_dpi() {
        assert_eq!(scale_px(144, 10), 15);
    }

    #[test]
    fn layout_fits_action_buttons() {
        let layout = Layout::new(96);
        assert!(layout.close_x + layout.close_w <= layout.width);
        assert!(layout.stop_x > layout.pause_x);
        assert_eq!(layout.record_x, layout.stop_x);
        assert!(layout.height >= 56);
    }

    #[test]
    fn format_elapsed_pads_minutes() {
        assert_eq!(format_elapsed(std::time::Duration::from_secs(75)), "01:15");
    }
}
