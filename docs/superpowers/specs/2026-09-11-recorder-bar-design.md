# Compact recorder bar

Date: 2026-09-11  
Status: Approved design (tests skipped for this slice)

A first visible UI for LightCapture: a compact Win32 toolbar to pick a screen or window, toggle system/mic audio, and Record / Pause / Stop. The tray, hotkeys, quality, folder, profiles, recent files, and stream controls stay as they are.

## Goal

Launch LightCapture → see a small bar → pick Screen or Window → Record. While recording, Pause and Stop are on that bar. The bar must not appear in the MP4.

This is not a settings window, not egui, not a live preview, and not region capture.

## Constraints

- Local-first; engine stays in `lightcapture-core`; app and CLI remain fronts only.
- No Chromium, Electron, or Tauri in the recording path.
- Encode once; no second video encoder; no in-bar preview.
- `WDA_EXCLUDEFROMCAPTURE` on the bar HWND so WGC display capture omits it.
- Close (X) hides to tray; tray Exit quits.
- No new automated tests in this slice (user). Existing workspace `check.ps1` (fmt, clippy `-D warnings`, current tests) still runs before calling implementation done.

## Architecture

The bar is a native Win32 window in `lightcapture-app`, same STA UI thread and `PeekMessage` pump as the tray (`pump.rs`). It does not own capture. One `Option<Recording>` in `app.rs` is shared by bar, tray, and hotkeys.

New module: `crates/lightcapture-app/src/bar.rs`. `main.rs` stays a thin index (`mod bar;` plus existing mods). Win32 `CreateWindowEx` / control messages stay in `bar`; session start/stop stay in `app`.

`lightcapture-core` does not grow a UI. Window capture uses existing `list_windows()` and `CaptureTarget::WindowTitle` / `ForegroundWindow`. HWND-stable window pick is later, not this slice. Title match can be wrong if two windows share a name.

On HWND create, call `SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE)`. If it fails, the bar still works; show a one-line status that the recording may include the toolbar. Do not move the window off-screen or use transparency as a substitute.

Always-on-top. First show: top-center of the primary monitor. Position is not persisted. `WS_SYSMENU` so X works. X → `ShowWindow(SW_HIDE)`; process keeps running. Start menu / installer launch shows the bar. Single-instance behavior unchanged: a second process exits; it does not focus the existing bar.

## Layout and controls

One short row. No live preview.

**Idle**

```
[ Screen ▼ ]  [ Display 1 (1920×1080) ▼ ]  [☑ System]  [☑ Mic]  [ Record ]
```

- First combo: **Screen** or **Window**.
- Second combo:
  - Screen: **Primary**, then each `list_displays()` entry (name and size).
  - Window: **Foreground window**, then titles from `list_windows()`. Refresh that list when switching to Window or when the combo drops down. No background polling.
- System and Mic map to `Settings.audio`. Both unchecked is allowed (silent).
- **Record** starts a session. Window mode always includes **Foreground window**, so Record stays enabled even if `list_windows()` is empty.

**Recording**

Same window. Source and audio combos/checkboxes disabled.

```
[ Screen ▼ ]  [ Display 1 … ▼ ]  [☑ System]  [☑ Mic]  [ 00:12 ]  [ Pause ]  [ Stop ]
```

- Elapsed is wall-clock since Record; it keeps ticking while paused. File timeline still omits paused time (existing engine behavior).
- **Pause** / **Resume** = tray item and `Ctrl+Shift+P`.
- **Stop** = existing finish path (MP4, recent, toast, optional publish).
- X hides the bar; recording continues until Stop, tray toggle, or `Ctrl+Shift+R`.

Quality, output folder, profiles, cursor, recent files, stream: tray only.

## Data flow

Bar and tray read/write the same `%APPDATA%\LightCapture\settings.json`.

Add `SourceSetting::Window { title: String }` (`kind: "window"`). Mapping:

| Bar | Settings | `CaptureTarget` |
| --- | --- | --- |
| Screen + Primary | `Primary` | `PrimaryDisplay` |
| Screen + a monitor | `Display { id, name, index }` | resolved as today |
| Window + Foreground window | `Foreground` | `ForegroundWindow` |
| Window + a title | `Window { title }` | `WindowTitle(title)` |

Old settings files without `window` keep working. A wholly unreadable JSON file still loads `Settings::default()`, as today.

Record builds the same `RecordConfig` as the tray: resolved target, quality, cursor, audio, smart filename/output dir, then `lightcapture_core::start`. On success, store that `Recording` and flip the bar to Pause/Stop.

Pause calls `Recording::set_paused`. Stop uses the existing `finish_recording` path. Tray and hotkeys call the same `handle_command` handlers; the bar is refreshed from session state each pump (elapsed, paused, failed).

Source and audio changes from the bar persist immediately when idle and are ignored while recording (controls disabled). Audio checkboxes are user intent, not a live device enumerator.

Extend `tray::Command` with audio updates the bar needs (`SetAudio` or equivalent). Do not add a second command path around `handle_command`.

## Errors

- Exclude-from-capture failed: record anyway; one-line status that the toolbar may appear in the file.
- Start failed (missing display/window, encoder, disk): stay idle, one-line status; no `Recording` stored. Tray tooltip/toast behavior unchanged.
- Window closed between pick and Record: `TargetNotFound` → idle + status; user picks again.
- Disk full / encoder lost mid-session: existing keep-partial + toast; bar returns idle with that status.
- Missing audio device: engine degrades as today; checkboxes unchanged.

## Out of this slice

- New unit/integration tests for the bar (skipped by request)
- HWND / process-id window identity
- Region capture, live preview, egui, settings window
- Remember bar position; second-instance focus
- Quality / folder / profiles on the bar
- Changing capture source or audio graph mid-session
- Visual polish beyond a plain Win32 toolbar

## Product docs (when implementing)

Unpark this compact recorder bar in `docs/product/prd.md` and `architecture.md`. Keep **egui / settings window / dashboard** parked. Region capture stays parked.

## Success

- Fresh launch shows the bar; X hides it; tray Exit quits.
- User can record a chosen display or window with System/Mic as set on the bar.
- Pause and Stop work from the bar, tray, and hotkeys on the same session.
- Display recordings do not contain the bar when `WDA_EXCLUDEFROMCAPTURE` succeeds.
- Core encode path unchanged: one H.264 encoder, 2-slot frame pool, no preview.
