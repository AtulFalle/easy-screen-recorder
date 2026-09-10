# LightCapture — condensed PRD

Status: Draft. Type: lightweight native Windows screen recorder. Homelab streaming is out of MVP.

## Job

Install → `Ctrl+Shift+R` → record while working (Chrome, Figma, IDEs) → stop → MP4 on disk. No account. No cloud.

## Principles

1. Performance over features — if it makes recording heavy, it does not belong on the core path.
2. Native over browser — no Chromium/Electron/Tauri in the recording engine.
3. Hardware over software — NVENC / Quick Sync / AMF via Media Foundation, then software.
4. Encode once — never duplicate video encoding for extra outputs.
5. Bounded memory — 2-slot GPU frame pool; drop on backpressure; no raw-frame `Vec` growth.
6. Graceful degradation — lower resolution/FPS before lagging the user’s apps.
7. Local-first — recording works offline; nothing leaves the machine unless the user configures it later.
8. Simple UX — start recording in seconds.

## MVP

- Display and window capture
- 720p / 1080p, 30 FPS, 60 FPS when hardware allows
- H.264 hardware encode, software fallback
- System audio + microphone, A/V sync
- Start / stop / pause, recording folder, incremental MP4
- Tray + global hotkey
- Live CPU / RAM / GPU / FPS / drops in the UI
- Adaptive quality after a reliable file exists: 1080p60 → 1080p30 → 720p30 → 720p24

## Not MVP

Cloud, accounts, editing, AI, overlays, virtual camera, region capture, multi-display, HEVC/AV1, replay buffer, macOS/Linux desktop, plugin ecosystem, MediaMTX/homelab.

## Engineering targets (product goals, not a lab on one PC)

Typical overhead: low CPU/RAM/GPU, extra VRAM bounded, dropped frames rare, audio drift under 50 ms. Prefer a usable 720p30 on 2–4 cores / 8 GB RAM over a max-quality encode that stalls Figma.
