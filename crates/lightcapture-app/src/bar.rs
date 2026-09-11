#![deny(unsafe_op_in_unsafe_fn)]

use std::sync::Mutex;

use lightcapture_core::{list_displays, list_windows, CaptureDisplay};
use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    GetStockObject, COLOR_WINDOW, DEFAULT_GUI_FONT, HBRUSH, HFONT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetSystemMetrics, GetWindowLongPtrW,
    LoadCursorW, RegisterClassW, SendMessageW, SetWindowDisplayAffinity, SetWindowLongPtrW,
    SetWindowPos, SetWindowTextW, ShowWindow, SystemParametersInfoW, BM_GETCHECK, BM_SETCHECK,
    BN_CLICKED, BS_AUTOCHECKBOX, CBN_DROPDOWN, CBN_SELCHANGE, CBS_DROPDOWNLIST, CB_ADDSTRING,
    CB_GETCURSEL, CB_RESETCONTENT, CB_SETCURSEL, CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW,
    GWLP_USERDATA, HWND_TOPMOST, IDC_ARROW, SM_CXSCREEN, SPI_GETWORKAREA, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SW_HIDE, SW_SHOW, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    WDA_EXCLUDEFROMCAPTURE, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE, WM_COMMAND, WM_CREATE,
    WM_DESTROY, WM_SETFONT, WNDCLASSW, WS_CAPTION, WS_CHILD, WS_CLIPCHILDREN, WS_EX_TOPMOST,
    WS_POPUP, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE, WS_VSCROLL,
};

use crate::settings::{AudioSetting, Settings, SourceSetting};
use crate::tray::Command;

const CLASS_NAME: windows::core::PCWSTR = w!("LightCaptureRecorderBar");
const BAR_WIDTH: i32 = 780;
const BAR_HEIGHT: i32 = 92;
const IDC_MODE: i32 = 100;
const IDC_TARGET: i32 = 101;
const IDC_SYSTEM: i32 = 102;
const IDC_MIC: i32 = 103;
const IDC_RECORD: i32 = 104;
const IDC_PAUSE: i32 = 105;
const IDC_STOP: i32 = 106;
const IDC_ELAPSED: i32 = 107;
const IDC_STATUS: i32 = 108;
const MODE_SCREEN: i32 = 0;
const MODE_WINDOW: i32 = 1;

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
    pub fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        register_class()?;
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
            displays: list_displays().unwrap_or_default(),
            window_titles: Vec::new(),
            commands: Mutex::new(Vec::new()),
            exclude_failed: false,
            suppress: false,
        });
        let raw = Box::into_raw(inner);
        // SAFETY: None requests the handle of this process; the Windows API documents that as valid.
        let instance = unsafe { GetModuleHandleW(None) }?;
        let (x, y) = bar_origin();
        let hwnd = unsafe {
            // SAFETY: CLASS_NAME is a static class we just registered; `raw` is a valid BarInner
            // pointer passed as lpParam and stored in WM_CREATE on success.
            CreateWindowExW(
                WS_EX_TOPMOST,
                CLASS_NAME,
                w!("LightCapture"),
                WS_POPUP | WS_CAPTION | WS_SYSMENU | WS_CLIPCHILDREN,
                x,
                y,
                BAR_WIDTH,
                BAR_HEIGHT,
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
                    // SAFETY: CreateWindowExW failed before WM_CREATE; we still own the Box at `raw`.
                    Box::from_raw(raw)
                });
                return Err(err.into());
            }
        };
        let bar = Self { hwnd };
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
            fill_mode(state);
            select_source(state, &settings.source);
            set_check(state.system, settings.audio.system);
            set_check(state.mic, settings.audio.microphone);
            let idle = !view.recording;
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
            set_text(state.status, status.as_deref().unwrap_or(""));
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

fn register_class() -> windows::core::Result<()> {
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
    unsafe {
        // SAFETY: `class` is fully initialized with a valid instance, class name, and WndProc.
        RegisterClassW(&class);
    }
    Ok(())
}

fn bar_origin() -> (i32, i32) {
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
        let x = work.left + (work.right - work.left - BAR_WIDTH).max(0) / 2;
        return (x, work.top + 12);
    }
    // SAFETY: SM_CXSCREEN is a documented GetSystemMetrics index.
    let screen_w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    ((screen_w - BAR_WIDTH).max(0) / 2, 12)
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
            // SAFETY: lparam is CREATESTRUCTW from CreateWindowExW; lpCreateParams is BarInner.
            let create = unsafe { &*(lparam.0 as *const CREATESTRUCTW) };
            let state = create.lpCreateParams as *mut BarInner;
            unsafe {
                // SAFETY: hwnd is the window being created; storing the BarInner pointer for later messages.
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, state as isize);
            }
            unsafe {
                // SAFETY: `state` is the Box we passed as lpParam in CreateWindowExW.
                (*state).hwnd = hwnd;
            }
            if let Err(err) = create_children(unsafe {
                // SAFETY: `state` is a valid exclusive BarInner during WM_CREATE.
                &mut *state
            }) {
                eprintln!("{err}");
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
                        // SAFETY: status HWND may still be default if create_children failed.
                        (*state).status
                    },
                    "Toolbar may appear in the recording (exclude-from-capture failed)",
                );
            }
            LRESULT(0)
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
                drop(unsafe {
                    // SAFETY: ptr was stored in GWLP_USERDATA at WM_CREATE and is uniquely owned here.
                    Box::from_raw(ptr)
                });
            }
            LRESULT(0)
        }
        _ => unsafe {
            // SAFETY: default handling for unhandled messages on a valid HWND.
            DefWindowProcW(hwnd, msg, wparam, lparam)
        },
    }
}

fn create_children(state: &mut BarInner) -> windows::core::Result<()> {
    // SAFETY: None requests the handle of this process.
    let instance = unsafe { GetModuleHandleW(None) }?;
    let parent = state.hwnd;
    let font = HFONT(
        unsafe {
            // SAFETY: DEFAULT_GUI_FONT is a predefined stock object; the returned handle is not owned.
            GetStockObject(DEFAULT_GUI_FONT)
        }
        .0,
    );
    state.mode = child(
        parent,
        instance.into(),
        w!("COMBOBOX"),
        "",
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_VSCROLL | WINDOW_STYLE(CBS_DROPDOWNLIST as u32),
        8,
        8,
        96,
        200,
        IDC_MODE,
        font,
    )?;
    state.target = child(
        parent,
        instance.into(),
        w!("COMBOBOX"),
        "",
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_VSCROLL | WINDOW_STYLE(CBS_DROPDOWNLIST as u32),
        112,
        8,
        236,
        200,
        IDC_TARGET,
        font,
    )?;
    state.system = child(
        parent,
        instance.into(),
        w!("BUTTON"),
        "System",
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
        356,
        10,
        80,
        24,
        IDC_SYSTEM,
        font,
    )?;
    state.mic = child(
        parent,
        instance.into(),
        w!("BUTTON"),
        "Mic",
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
        440,
        10,
        64,
        24,
        IDC_MIC,
        font,
    )?;
    state.elapsed = child(
        parent,
        instance.into(),
        w!("STATIC"),
        "00:00",
        WS_CHILD,
        512,
        12,
        52,
        20,
        IDC_ELAPSED,
        font,
    )?;
    state.record = child(
        parent,
        instance.into(),
        w!("BUTTON"),
        "Record",
        WS_CHILD | WS_VISIBLE | WS_TABSTOP,
        572,
        8,
        80,
        28,
        IDC_RECORD,
        font,
    )?;
    state.pause = child(
        parent,
        instance.into(),
        w!("BUTTON"),
        "Pause",
        WS_CHILD | WS_TABSTOP,
        572,
        8,
        80,
        28,
        IDC_PAUSE,
        font,
    )?;
    state.stop = child(
        parent,
        instance.into(),
        w!("BUTTON"),
        "Stop",
        WS_CHILD | WS_TABSTOP,
        660,
        8,
        80,
        28,
        IDC_STOP,
        font,
    )?;
    state.status = child(
        parent,
        instance.into(),
        w!("STATIC"),
        "",
        WS_CHILD | WS_VISIBLE,
        8,
        42,
        760,
        18,
        IDC_STATUS,
        font,
    )?;
    fill_mode(state);
    fill_targets(state, true);
    Ok(())
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
        // SAFETY: hwnd is the child just created; font is a stock HFONT valid for WM_SETFONT.
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
