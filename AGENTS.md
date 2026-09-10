# Agent notes — LightCapture

Windows-only Rust workspace. MSVC toolchain required (`x86_64-pc-windows-msvc`).

## Skills

Language skills live in `.agents/skills/` (Rust workspace, modules, Clippy, unsafe/FFI, testing, Cargo, API design, review, concurrency, performance, CLI). Use those when writing or reviewing Rust.

Product intent is **not** a skill. It is in `docs/product/` and `.cursor/rules/`. Those rules override a Rust skill if they conflict.

## Setup vs product

MVP0 shipped: WGC → hardware H.264 MP4, CLI, tray (`Ctrl+Shift+R`). **Do not add egui until the user asks.**

Roadmap (see `docs/product/prd.md`):

- **MVP1 Daily Driver** — audio + tray pause + GPU scaler + tray telemetry/failure + names/history/profiles are in.
- **MVP2 Encode-once (in)** — adaptive FPS on the existing encoder; MediaMTX ingest is `ffmpeg -c copy` of the MP4 (tray URL + toggle). No second video encoder. Last-N replay buffer is not in (stop toast presents the file instead).

## Crates

- `lightcapture-core` — engine library. No UI.
- `lightcapture-cli` — headless front on core.
- `lightcapture-app` — tray front on core (`Ctrl+Shift+R`, `Ctrl+Shift+P` pause, options menu).

## Verify before claiming done

Run `.\scripts\check.ps1` (fmt check, clippy `-D warnings`, workspace tests). Hardware capture/encode tests that need a display or GPU encoder stay `#[ignore]` so CI on `windows-latest` stays green.

## Out of current work (parked)

Region capture, HEVC/AV1, cloud, accounts, Chromium/Electron/Tauri in the record path, egui/settings window, installer/updater, in-process WebRTC viewer.

**MVP2-only (do not implement during MVP1):** encoded-packet fan-out inside Media Foundation, last-N replay ring.
