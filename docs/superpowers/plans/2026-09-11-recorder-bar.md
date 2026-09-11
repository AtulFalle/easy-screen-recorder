# Compact Recorder Bar Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a compact native Win32 recorder bar to `lightcapture-app` so the user can pick a screen or window, toggle system/mic audio, and Record / Pause / Stop, without capturing the bar in the MP4.

**Architecture:** The bar is another front on the existing `Option<Recording>` in `app.rs`. Same STA UI thread and `PeekMessage` pump as the tray. `lightcapture-core` is unchanged except as already exposed (`list_displays`, `list_windows`, `CaptureTarget::WindowTitle`). The HWND is marked `WDA_EXCLUDEFROMCAPTURE`. Close hides; tray Exit quits; left-click tray shows the bar again (needed so hide is not a dead end).

**Tech Stack:** Rust, `windows` 0.61 Win32 (`CreateWindowExW`, combo/button/static), existing `tray-icon` + `global-hotkey`, `settings.json` via serde.

## Global Constraints

- Local-first; engine stays in `lightcapture-core`; app is a front only.
- No Chromium, Electron, Tauri, or egui.
- Encode once; no preview; no second video encoder.
- `SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE)` on the bar HWND.
- Close (X) hides to tray; tray Exit quits.
- No new automated tests in this slice (user). Do not add `#[test]` modules for the bar. Update existing match arms so the crate still compiles. Run `.\scripts\check.ps1` before claiming done (fmt, clippy `-D warnings`, existing tests).
- Region capture, HWND window identity, bar position memory, second-instance focus, quality-on-bar: out of slice.
- Every `unsafe` block needs a `// SAFETY:` comment. `#![deny(unsafe_op_in_unsafe_fn)]` stays on the binary.
- `docs/` is gitignored; `git add -f` any product/spec/plan markdown under `docs/`.

---

## File map

- Create: `crates/lightcapture-app/src/bar.rs` — HWND, controls, exclude-from-capture, command queue, `sync`.
- Modify: `crates/lightcapture-app/src/main.rs` — `mod bar;`
- Modify: `crates/lightcapture-app/src/settings.rs` — `SourceSetting::Window { title }`, `resolve` / `source_slug` / `to_target`.
- Modify: `crates/lightcapture-app/src/tray.rs` — `Command::SetAudio`, `Command::ShowBar`, menu id `show-bar`.
- Modify: `crates/lightcapture-app/src/app.rs` — create bar, drain commands, elapsed Instant, wire handlers.
- Modify: `crates/lightcapture-app/Cargo.toml` — Win32 features listed in Task 3.
- Modify: `docs/product/prd.md`, `docs/product/architecture.md`, `AGENTS.md`, `.cursor/rules/lightcapture-product.mdc`, `README.md` — unpark compact bar, keep egui/settings parked.

---

### Task 1: Persist window source

**Files:**
- Modify: `crates/lightcapture-app/src/settings.rs`

**Interfaces:**
- Consumes: existing `SourceSetting`, `CaptureTarget`
- Produces: `SourceSetting::Window { title: String }` (`serde` tag `kind: "window"`), mapped by `resolve` → `CaptureTarget::WindowTitle(title)` and `source_slug` → `sanitize_source_slug(title)`

- [ ] **Step 1: Add the variant and match arms**

In `SourceSetting`, after `Foreground,`:

```rust
    Window {
        title: String,
    },
```

In `to_target` (test-only), add:

```rust
            Self::Window { title } => CaptureTarget::WindowTitle(title.clone()),
```

In `resolve`, add before the closing of the match:

```rust
            Self::Window { title } => CaptureTarget::WindowTitle(title.clone()),
```

In `source_slug`, add:

```rust
            Self::Window { title } => sanitize_source_slug(title),
```

Do not add new `#[test]` functions. Existing tests keep passing because old JSON has no `window` kind.

- [ ] **Step 2: Compile and run existing app tests**

Run:

```powershell
cargo test -p lightcapture-app
```

Expected: PASS (existing settings/tray tests only).

- [ ] **Step 3: Commit**

```bash
git add crates/lightcapture-app/src/settings.rs
git commit -m "Add window title source setting for the recorder bar."
```

---

### Task 2: Bar commands on the shared enum

**Files:**
- Modify: `crates/lightcapture-app/src/tray.rs`

**Interfaces:**
- Consumes: `AudioSetting` from `settings.rs`
- Produces: `Command::SetAudio(AudioSetting)` and `Command::ShowBar`; menu id `show-bar` parses to `ShowBar`. `SetAudio` is not a tray menu id (the bar posts it).

- [ ] **Step 1: Extend `Command` and parse `ShowBar`**

Add constants next to the other `ID_*` values:

```rust
pub const ID_SHOW_BAR: &str = "show-bar";
```

Add variants to `Command` (keep `Debug, Clone, PartialEq, Eq`):

```rust
    SetAudio(AudioSetting),
    ShowBar,
```

`AudioSetting` is already imported via `settings::{..., Settings, SourceSetting}` — add `AudioSetting` to that import.

In `parse_menu_id`, add:

```rust
        ID_SHOW_BAR => Some(Command::ShowBar),
```

In `TrayUi::new`, create a menu item after `open` / before `exit` is assembled. Add a field `show_bar: MenuItem` on `TrayUi`:

```rust
        let show_bar = MenuItem::with_id(ID_SHOW_BAR, "Show recorder", true, None);
```

Store it on `Self`, pass `&self.show_bar` into `MenuBits` / `assemble_menu`, and insert it in `Menu::with_items` immediately after `bits.toggle` (or after the first separator — visible without hunting). Suggested order:

```rust
        bits.toggle,
        bits.pause,
        bits.show_bar,
        &PredefinedMenuItem::separator(),
```

`ShowBar` does not need to be disabled while recording.

Do not add new parse tests.

- [ ] **Step 2: Compile**

Run:

```powershell
cargo test -p lightcapture-app
```

`Command` is matched in `app.rs`. Add these arms in `handle_command` now so the crate compiles (Task 4 replaces `ShowBar`):

```rust
        Command::SetAudio(_) if is_recording => {}
        Command::SetAudio(audio) => {
            settings.audio = audio;
            persist(settings, tray);
        }
        Command::ShowBar => {}
```

Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/lightcapture-app/src/tray.rs crates/lightcapture-app/src/app.rs
git commit -m "Add Show recorder and audio commands for the bar."
```

---

### Task 3: Native recorder bar window

**Files:**
- Create: `crates/lightcapture-app/src/bar.rs`
- Modify: `crates/lightcapture-app/src/main.rs` (only `mod bar;` — do not call it from `app` yet)
- Modify: `crates/lightcapture-app/Cargo.toml`

**Interfaces:**
- Consumes: `Command`, `AudioSetting`, `Settings`, `SourceSetting`; `list_displays`, `list_windows`, `CaptureDisplay`
- Produces:

```rust
pub struct BarView {
    pub recording: bool,
    pub paused: bool,
    pub elapsed: std::time::Duration,
    pub status: Option<String>,
}

pub struct RecorderBar { /* hwnd */ }

impl RecorderBar {
    pub fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>>;
    pub fn show(&self);
    pub fn hide(&self);
    pub fn take_commands(&self) -> Vec<crate::tray::Command>;
    pub fn sync(&self, settings: &Settings, view: &BarView);
}
```

X / `WM_CLOSE` must `ShowWindow(SW_HIDE)` and **not** post `WM_QUIT`. Left-click show is Task 4.

- [ ] **Step 1: Add Win32 features**

In `crates/lightcapture-app/Cargo.toml` `windows` features, add:

```toml
    "Win32_Graphics_Gdi",
    "Win32_System_LibraryLoader",
    "Win32_UI_Input_KeyboardAndMouse",
```

Keep existing features.

- [ ] **Step 2: Write `bar.rs`**

Create `crates/lightcapture-app/src/bar.rs` with the following (fmt will tidy). This is the whole module for this slice.

```rust
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
    SetWindowPos, SetWindowTextW, ShowWindow, SystemParametersInfoW, CREATESTRUCTW,
    BM_GETCHECK, BM_SETCHECK, BN_CLICKED, BS_AUTOCHECKBOX, CBN_DROPDOWN, CBN_SELCHANGE,
    CBS_DROPDOWNLIST, CB_ADDSTRING, CB_GETCURSEL, CB_RESETCONTENT, CB_SETCURSEL,
    CS_HREDRAW, CS_VREDRAW, GWLP_USERDATA, HWND_TOPMOST, IDC_ARROW,
    SM_CXSCREEN, SPI_GETWORKAREA, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
    SW_HIDE, SW_SHOW,
    SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, WDA_EXCLUDEFROMCAPTURE, WINDOW_EX_STYLE,
    WINDOW_STYLE, WM_CLOSE, WM_COMMAND, WM_CREATE, WM_DESTROY, WM_SETFONT, WNDCLASSW,
    WS_CAPTION, WS_CHILD, WS_CLIPCHILDREN, WS_POPUP, WS_SYSMENU, WS_TABSTOP,
    WS_VISIBLE, WS_VSCROLL, WS_EX_TOPMOST,
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
        let instance = unsafe { GetModuleHandleW(None) }?;
        let (x, y) = bar_origin();
        let hwnd = unsafe {
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
        }?;
        let bar = Self { hwnd };
        bar.show();
        Ok(bar)
    }

    pub fn show(&self) {
        unsafe {
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

    pub fn hide(&self) {
        unsafe {
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
                let _ = DestroyWindow(self.hwnd);
            }
        }
    }
}

fn register_class() -> windows::core::Result<()> {
    let instance = unsafe { GetModuleHandleW(None) }?;
    let class = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(wndproc),
        hInstance: instance.into(),
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW) }?,
        hbrBackground: HBRUSH((COLOR_WINDOW.0 as usize + 1) as *mut core::ffi::c_void),
        lpszClassName: CLASS_NAME,
        ..Default::default()
    };
    unsafe { RegisterClassW(&class) };
    Ok(())
}

fn bar_origin() -> (i32, i32) {
    let mut work = RECT::default();
    let ok = unsafe {
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
    let screen_w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    ((screen_w - BAR_WIDTH).max(0) / 2, 12)
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
            // SAFETY: lparam is CREATESTRUCTW from CreateWindowExW; lpCreateParams is BarInner.
            let create = unsafe { &*(lparam.0 as *const CREATESTRUCTW) };
            let state = create.lpCreateParams as *mut BarInner;
            unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, state as isize) };
            unsafe { (*state).hwnd = hwnd };
            if let Err(err) = create_children(unsafe { &mut *state }) {
                eprintln!("{err}");
            }
            let excluded = unsafe { SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE) };
            if excluded.is_err() {
                unsafe { (*state).exclude_failed = true };
                set_text(
                    unsafe { (*state).status },
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
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut BarInner;
            if !ptr.is_null() {
                unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) };
                drop(unsafe { Box::from_raw(ptr) });
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn create_children(state: &mut BarInner) -> windows::core::Result<()> {
    let instance = unsafe { GetModuleHandleW(None) }?;
    let parent = state.hwnd;
    let font = HFONT(unsafe { GetStockObject(DEFAULT_GUI_FONT) }.0);
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
            Some(windows::Win32::UI::WindowsAndMessaging::HMENU(id as *mut core::ffi::c_void)),
            Some(instance),
            None,
        )
    }?;
    unsafe {
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
            let pos = state.window_titles.iter().position(|t| t == title);
            if pos.is_none() {
                state.window_titles.insert(0, title.clone());
                combo_reset(state.target);
                combo_add(state.target, "Foreground window");
                for t in &state.window_titles {
                    combo_add(state.target, t);
                }
                combo_set(state.target, 1);
            } else {
                combo_set(state.target, pos.unwrap() as i32 + 1);
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
    let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut BarInner;
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
        let _ = SetWindowTextW(hwnd, windows::core::PCWSTR(w.as_ptr()));
    }
}

fn combo_reset(hwnd: HWND) {
    unsafe {
        let _ = SendMessageW(hwnd, CB_RESETCONTENT, None, None);
    }
}

fn combo_add(hwnd: HWND, text: &str) {
    let w = wide(text);
    unsafe {
        let _ = SendMessageW(
            hwnd,
            CB_ADDSTRING,
            None,
            Some(LPARAM(w.as_ptr() as isize)),
        );
    }
}

fn combo_set(hwnd: HWND, index: i32) {
    unsafe {
        let _ = SendMessageW(hwnd, CB_SETCURSEL, Some(WPARAM(index as usize)), None);
    }
}

fn current_sel(hwnd: HWND) -> i32 {
    unsafe { SendMessageW(hwnd, CB_GETCURSEL, None, None).0 as i32 }
}

fn set_check(hwnd: HWND, checked: bool) {
    unsafe {
        let _ = SendMessageW(
            hwnd,
            BM_SETCHECK,
            Some(WPARAM(if checked { 1 } else { 0 })),
            None,
        );
    }
}

fn is_checked(hwnd: HWND) -> bool {
    unsafe { SendMessageW(hwnd, BM_GETCHECK, None, None).0 == 1 }
}

fn format_elapsed(elapsed: std::time::Duration) -> String {
    let secs = elapsed.as_secs();
    format!("{:02}:{:02}", secs / 60, secs % 60)
}
```

Add `mod bar;` to `crates/lightcapture-app/src/main.rs` next to the other `mod` lines.

- [ ] **Step 3: Compile the crate**

Run:

```powershell
cargo clippy -p lightcapture-app --all-targets -- -D warnings
```

Expected: PASS. If clippy flags unused imports or `collapsible_if`, fix those only. Do not change exclude-from-capture, hide-on-close, or command mapping.

- [ ] **Step 4: Commit**

```bash
git add crates/lightcapture-app/src/bar.rs crates/lightcapture-app/src/main.rs crates/lightcapture-app/Cargo.toml Cargo.lock
git commit -m "Add Win32 recorder bar window excluded from capture."
```

---

### Task 4: Wire the bar into the app loop

**Files:**
- Modify: `crates/lightcapture-app/src/app.rs`

**Interfaces:**
- Consumes: `RecorderBar`, `BarView`, `Command::SetAudio`, `Command::ShowBar`
- Produces: one shared `Option<Recording>`; wall-clock elapsed via `Instant` stored in `run`; bar reflects tray/hotkey/bar equally

- [ ] **Step 1: Create the bar and drain its commands**

At the top of `app.rs`:

```rust
use std::time::{Duration, Instant};

use crate::bar::{BarView, RecorderBar};
use tray_icon::{MouseButton, MouseButtonState, TrayIconEvent};
```

(Keep existing `Duration` import by merging.)

In `run`, after `TrayUi::new`:

```rust
    let mut bar = RecorderBar::new()?;
    let mut recorded_at: Option<Instant> = None;
```

Inside the loop, after pumping messages, drain bar + tray click:

```rust
        for command in bar.take_commands() {
            handle_command(
                command,
                &mut settings,
                &mut recording,
                &mut error,
                &mut tray,
                &mut bar,
                &mut recorded_at,
                hotkeys,
            )?;
        }

        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                bar.show();
            }
        }
```

Pass `&mut bar` and `&mut recorded_at` into the existing `handle_command` calls for menu and hotkey as well.

At the end of the loop body (next to `refresh_tooltip`):

```rust
        sync_bar(&bar, &settings, &recording, &error, recorded_at);
```

- [ ] **Step 2: Handle new commands and Instant**

Change `handle_command` signature to:

```rust
fn handle_command(
    command: Command,
    settings: &mut Settings,
    recording: &mut Option<Recording>,
    error: &mut Option<String>,
    tray: &mut TrayUi,
    bar: &mut RecorderBar,
    recorded_at: &mut Option<Instant>,
    hotkeys: HotkeyStatus,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
```

Replace stub arms:

```rust
        Command::SetAudio(_) if is_recording => {}
        Command::SetAudio(audio) => {
            settings.audio = audio;
            persist(settings, tray);
        }
        Command::ShowBar => bar.show(),
```

In `toggle`, when `recording.take()` stops a session, set `*recorded_at = None`. When `start` succeeds, set `*recorded_at = Some(Instant::now())`. When `start` fails, leave `recorded_at` as `None`. Thread `recorded_at` into `toggle` as `&mut Option<Instant>`.

On `Command::Exit`, `bar.hide()` is unnecessary; process is quitting.

Add:

```rust
fn sync_bar(
    bar: &RecorderBar,
    settings: &Settings,
    recording: &Option<Recording>,
    error: &Option<String>,
    recorded_at: Option<Instant>,
) {
    bar.sync(
        settings,
        &BarView {
            recording: recording.is_some(),
            paused: recording.as_ref().is_some_and(Recording::is_paused),
            elapsed: recorded_at.map(|t| t.elapsed()).unwrap_or_default(),
            status: error.clone(),
        },
    );
}
```

`persist_and_refresh_menu` does not need the bar; `sync_bar` after `handle_command` covers it.

Source from the bar uses existing `Command::Source` → `resolve_source` / `persist`. Window titles skip `resolve_source`’s display branch (`other` arm) — correct.

- [ ] **Step 3: Compile and existing tests**

Run:

```powershell
cargo clippy -p lightcapture-app --all-targets -- -D warnings
cargo test -p lightcapture-app
```

Expected: PASS.

Manual check (not CI): `cargo run -p lightcapture-app --release` — bar appears, X hides, left-click tray or **Show recorder** shows it, Record/Pause/Stop match tray, bar absent from a display recording when exclude succeeds.

- [ ] **Step 4: Commit**

```bash
git add crates/lightcapture-app/src/app.rs
git commit -m "Wire the recorder bar into the tray session loop."
```

---

### Task 5: Product docs

**Files:**
- Modify: `docs/product/prd.md`
- Modify: `docs/product/architecture.md`
- Modify: `AGENTS.md`
- Modify: `.cursor/rules/lightcapture-product.mdc`
- Modify: `README.md`

- [ ] **Step 1: Unpark the compact bar; keep egui/settings parked**

`docs/product/architecture.md` **UI** section — replace the tray-only paragraph with:

```markdown
## UI

Tray + compact recorder bar (`lightcapture-app`): on launch, a small always-on-top Win32 toolbar lists Screen/Window, System/Mic, and Record. While recording it shows elapsed, Pause, and Stop. Close hides to the tray (recording continues). Left-click the tray icon or **Show recorder** shows the bar again. Tray Exit quits. The bar HWND uses `WDA_EXCLUDEFROMCAPTURE` so WGC display capture omits it. No live preview.

Hotkeys `Ctrl+Shift+R` / `Ctrl+Shift+P` and the tray menu still start/stop/pause and own quality, profiles, folder, cursor, recent files, and stream. Window titles persist as `SourceSetting::Window`. A settings window / egui stays parked.
```

In **Explicitly out**, keep `egui/settings window` parked. Do not list the compact bar as out.

`docs/product/prd.md`:

- Under **MVP0 — Shipped**, keep tray as shipped history.
- Add a bullet under a short **Desktop chrome** note after MVP1 (or at the top of parked clarification): compact recorder bar is in; **egui / settings window** remains parked.
- **Out of MVP1** / **Parked**: keep `egui / settings window / dashboard`. Do not put the recorder bar in Parked.
- **Out of MVP2**: same — settings window stays out; do not mention the bar as out.

`.cursor/rules/lightcapture-product.mdc` replace:

```markdown
- Desktop front is a tray app plus a compact Win32 recorder bar in `lightcapture-app`. Do not add egui until asked.
```

Parked line stays `settings window` (not “any window”).

`AGENTS.md`:

- Change “Do not add egui until the user asks.” to stay (still true).
- `lightcapture-app` bullet: tray + compact recorder bar (`Ctrl+Shift+R`, `Ctrl+Shift+P`, Screen/Window on the bar).

`README.md` **Tray app** / crate table: the app shows a compact bar on launch; tray remains for extra options and hide-to-tray.

- [ ] **Step 2: Commit**

```bash
git add -f docs/product/prd.md docs/product/architecture.md AGENTS.md .cursor/rules/lightcapture-product.mdc README.md
git commit -m "Document the compact recorder bar in product docs."
```

---

### Task 6: Workspace gate

**Files:** none new

- [ ] **Step 1: Run the CI script**

Run:

```powershell
.\scripts\check.ps1
```

Expected: fmt check PASS, clippy `-D warnings` PASS, `cargo test --workspace` PASS. No new tests required. Hardware tests remain `#[ignore]`.

- [ ] **Step 2: Fix any fmt/clippy issues and commit if needed**

```bash
git add -u
git commit -m "Fix clippy and fmt from the recorder bar."
```

Only create this commit if Step 1 produced diffs.

---

## Spec coverage

| Spec item | Task |
| --- | --- |
| Compact Win32 bar in `lightcapture-app` | 3 |
| Same pump / one `Recording` | 4 |
| `WDA_EXCLUDEFROMCAPTURE` + status on failure | 3 |
| Screen/Window combos, Primary/displays, Foreground + titles | 3 |
| System/Mic, Record/Pause/Stop, elapsed wall clock | 3–4 |
| X hides; launch shows; Exit quits | 3–4 |
| Left-click / Show recorder (unhide) | 2, 4 |
| `SourceSetting::Window` | 1 |
| Shared `handle_command` | 2, 4 |
| Disable source/audio while recording | 3 `sync` |
| No new tests | Global + all tasks |
| Product docs unpark bar, not egui | 5 |
| Core encode path unchanged | no core files |

## Notes for the implementer

- `docs/` is gitignored; product doc commits need `git add -f`.
- Title-based window capture can pick the wrong window if titles collide — accepted for this slice.
- Do not persist bar position. Do not add a preview HWND.
- If clippy complains about `unsafe` in `wndproc`, keep `unsafe extern "system"` and put each FFI call in an `unsafe { }` block with `// SAFETY:`.
