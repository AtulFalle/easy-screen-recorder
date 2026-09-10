# Tray UI — design

Date: 2026-09-10  
Status: approved for implementation

## Goal

A tray-only LightCapture desktop front: `Ctrl+Shift+R` toggles recording, the notification-area icon shows idle vs recording, and the right-click menu exposes basic options. No visible main window, no live preview, no audio, no egui.

## Architecture

`lightcapture-app` is a thin front. It never calls WGC or Media Foundation. It uses `lightcapture-core` only (`list_displays`, `start`, `Recording::stats`, `Recording::stop`). `lightcapture-cli` is unchanged.

The process owns:

- a notification-area icon and context menu (`tray-icon`)
- `Ctrl+Shift+R` (`global-hotkey`)
- persisted settings (quality, source, output folder, cursor)
- the current `Recording` handle, if any

Windows requires a message pump for tray + hotkey. The app keeps a hidden message-only pump on the UI thread — not a visible window. A named mutex allows one instance; a second launch prints that LightCapture is already running and exits 0.

## Settings

Path: `%APPDATA%\LightCapture\settings.json`.

Defaults:

- quality: 1080p30
- source: primary display
- output folder: `%USERPROFILE%\Videos\LightCapture` (create on demand; fall back to the home directory if Videos is missing)
- include cursor: true

Invalid or missing JSON loads defaults. Each option change writes the file. Quality, source, output folder, and cursor are locked while a recording is in progress.

## Menu

```
Start recording          (becomes Stop recording while active)
────────────────
Quality ►
  720p30
  1080p30
  1080p60
Capture source ►
  Primary display
  [1] name (WxH)         (one item per display from list_displays)
  Foreground window
Output folder…
Show cursor
────────────────
Open recordings folder
────────────────
Exit
```

No window-title picker. Empty display names render as `Display N (WxH)`.

## Status

- Idle icon: gray circle. Recording icon: red circle. Generated as 32×32 RGBA; no image assets.
- Tooltip idle: `LightCapture — idle`
- Tooltip recording: `LightCapture — {secs}s  {w}x{h}  encoded {n}  dropped {n}`
- Tooltip error: `LightCapture — {message}` (no toasts)
- Tooltip refresh about 4 times per second while recording

## Data flow

1. UI thread pumps Win32 messages.
2. Menu and hotkey events map to commands (toggle, quality, source, folder, cursor, open, exit).
3. Toggle / Start calls `RecordConfig` + `start`. Toggle / Stop / Exit-while-recording calls `Recording::stop`.
4. Output path is `RecordConfig::default_output_in(output_dir)`.
5. Folder pick uses a native Windows dialog (`rfd`). Open folder launches Explorer.

## Errors

Start/stop failures go to the tooltip and stderr. A failed start leaves the app idle. Exit always attempts a clean stop first. A second instance does not steal the hotkey.

If `Ctrl+Shift+R` is already taken, the app still runs; the menu Start/Stop remains available and the tooltip notes that the hotkey could not be registered.

## Testing

Unit-test settings round-trip, defaults, invalid JSON, tooltip strings, capture-target mapping, and icon buffer size. Do not create a tray icon or register a hotkey in CI tests.

## Out of scope

egui window, tray left-click toggle, toasts, audio, pause, adaptive quality, region capture, window-title picker, live preview, Chromium/Electron/Tauri.
