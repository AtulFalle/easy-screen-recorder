# LightCapture — condensed PRD

Status: Active. Type: lightweight native Windows screen recorder.

Roadmap grain: **MVP0 shipped** → **MVP1 Daily Driver** (local file you trust) → **MVP2 Encode-once** (replay + stream on one encoder). No five-phase feature dump.

## Job

Install → `Ctrl+Shift+R` → record while working (Chrome, Figma, IDEs) or clipping gameplay → stop → MP4 on disk. No account. No cloud. Streaming is optional and only after the local file path is trustworthy.

## Principles

1. Performance over features — if it makes recording heavy, it does not belong on the core path.
2. Native over browser — no Chromium/Electron/Tauri in the recording engine.
3. Hardware over software — NVENC / Quick Sync / AMF via Media Foundation, then software.
4. Encode once — never duplicate video encoding for extra outputs.
5. Bounded memory — 2-slot GPU frame pool; drop on backpressure; no raw-frame `Vec` growth.
6. Graceful degradation — lower resolution/FPS before lagging the user’s apps.
7. Local-first — recording works offline; nothing leaves the machine unless the user configures it later.
8. Simple UX — start recording in seconds.

## MVP0 — Shipped

Local video file recorder:

- Native WGC display and window capture
- Hardware H.264 encode (Media Foundation), software fallback
- Incremental MP4 on disk
- Tray app + `Ctrl+Shift+R`
- Quality / source / output folder / cursor in the tray menu
- 2-slot `FrameGate`; drop on backpressure

Known gaps left for later MVPs: adaptive quality, replay, streaming.

## MVP1 — Daily Driver

Job: `Ctrl+Shift+R` → talk over a Figma/IDE/Chrome session → pause → stop → playable MP4 with audio, on this machine.

### In scope

- System audio + microphone, A/V sync (WASAPI; mix to one stereo track in the file; keep sources separate in the engine)
- Pause / resume in the tray and a second hotkey (e.g. `Ctrl+Shift+P`); freeze video and audio without corrupting the MP4 timeline
- Quality presets that GPU-scale to 720p / 1080p before encode
- Tray telemetry tooltip: encoder name, target FPS, encoded/dropped, elapsed (optional this-process CPU/RAM; no live GPU meters)
- Named tray profiles (Work / Game / Silent) in `%APPDATA%\LightCapture\settings.json`
- Smart filenames: `LightCapture-YYYYMMDD-HHMMSS-source.mp4`
- Recording history: tray “Recent ►” last ~10 files (open / show in folder)
- Multi-monitor polish: persist by stable display id/name, not a shifting index
- Clean failure: disk-full or encoder-lost → stop, keep bytes written, tooltip/toast with path
- Stop notification: one Windows toast with the file path

### Build order (slices, not extra MVPs)

1. Audio + A/V sync
2. Pause in tray
3. GPU scaler
4. Telemetry / failure UX
5. Names / history / profiles

### Ship checklist

- [ ] 10-minute work session with mic + system audio plays in Movies & TV / VLC with lip-sync (drift under 50 ms)
- [ ] Pause twice mid-session; timeline stays correct
- [ ] 4K monitor at 1080p30 does not encode 4K
- [ ] Disk-full or encoder-lost leaves a playable partial file and a clear tooltip/toast
- [ ] Recent files and profiles work from the tray without a settings window

### Out of MVP1

Region capture, instant replay, adaptive quality, MediaMTX / LAN / WebRTC, native settings window / egui, installer / auto-updater, performance dashboard.

## MVP2 — Encode-once

Job: encode H.264 **once**. Extra outputs subscribe to those packets. Never start a second video encoder.

### In scope

- Bounded encoded-packet fan-out in `lightcapture-core` (not raw frames)
- Instant replay: circular buffer of encoded packets; hotkey dumps last N seconds to a new MP4
- Record + stream: same packets → local MP4 and MediaMTX ingest (RTSP/RTMP); tray URL + enable
- Adaptive quality on backpressure / drops / process load: 1080p60 → 1080p30 → 720p30 → 720p24
- Session recovery: keep the file on encoder-lost or ingest-drop; retry ingest without restarting capture when possible
- Thin stream UX: URL in settings.json + tray toggle (no egui)
- Playback via MediaMTX in the default browser (no in-process WebRTC viewer)

### Ship checklist

- [x] Adaptive FPS steps down under encoder backpressure without a second encode (encode size stays at the session preset)
- [x] Tray stream toggle + `stream_url` in settings.json; **Open stream viewer** uses the default browser
- [x] Ingest is `ffmpeg -c copy` of the saved MP4 (RTSP/RTMP); ingest failure does not delete the file
- [ ] Live packet bus / last-N replay dump (not in: stop toast presents the file; Media Foundation still muxes directly to MP4)

### Out of MVP2

In-process WebRTC viewer, region capture, native settings window / installer / updater, live performance dashboard, HEVC/AV1, cloud, accounts, overlays, virtual camera.

## Parked (do not sneak back in)

- Homelab as a product (MediaMTX is an ingest target, not a suite)
- Region capture, HEVC/AV1, macOS/Linux
- egui / settings window / dashboard
- Installer, code signing, auto-update (GitHub Releases exe stays the ship vehicle)
- Cloud, accounts, editing, AI
- Second encoder for any output
- Bundled Chromium / Electron / Tauri in the recording path

## Engineering targets (product goals, not a lab on one PC)

Typical overhead: low CPU/RAM/GPU, extra VRAM bounded, dropped frames rare, audio drift under 50 ms. Prefer a usable 720p30 on 2–4 cores / 8 GB RAM over a max-quality encode that stalls Figma.
