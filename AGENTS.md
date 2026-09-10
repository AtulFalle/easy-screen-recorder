# Agent notes — LightCapture

Windows-only Rust workspace. MSVC toolchain required (`x86_64-pc-windows-msvc`).

## Skills

Language skills live in `.agents/skills/` (Rust workspace, modules, Clippy, unsafe/FFI, testing, Cargo, API design, review, concurrency, performance, CLI). Use those when writing or reviewing Rust.

Product intent is **not** a skill. It is in `docs/product/` and `.cursor/rules/`. Those rules override a Rust skill if they conflict.

## Setup vs product

Foundation plus a Windows recording engine (WGC → hardware H.264 MP4), CLI, and tray app. **Do not add egui until the user asks.** Audio (WASAPI) is the next engine slice.

## Crates

- `lightcapture-core` — engine library. No UI.
- `lightcapture-cli` — headless front on core.
- `lightcapture-app` — tray front on core (`Ctrl+Shift+R`, options menu).

## Verify before claiming done

Run `.\scripts\check.ps1` (fmt check, clippy `-D warnings`, workspace tests). Hardware capture/encode tests that need a display or GPU encoder stay `#[ignore]` so CI on `windows-latest` stays green.

## Out of MVP

Homelab/MediaMTX, replay buffer, region capture, HEVC/AV1, cloud, accounts, Chromium/Electron/Tauri in the record path.
