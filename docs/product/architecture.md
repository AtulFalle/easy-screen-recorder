# LightCapture architecture

## Crates

```
lightcapture-app  ──►  lightcapture-core
lightcapture-cli  ──►  lightcapture-core
```

`core` owns capture, audio, encode, mux, settings types, and session stats. Frontends must not talk to Windows capture/encode APIs directly.

## Roadmap shape

- **MVP0 (shipped):** WGC → D3D11 / `send_frame` → Media Foundation H.264 → MP4. Tray + CLI. Audio disabled. Quality presets change FPS only.
- **MVP1 (Daily Driver, in):** WASAPI audio + shared clock, tray pause, GPU scaler to preset resolution, tray telemetry/failure UX, named profiles, smart filenames, recent files.
- **MVP2 (Encode-once, in):** adaptive FPS on the one H.264 encoder; MediaMTX ingest remuxes the finished MP4 (`ffmpeg -c copy`). Tray URL + toggle; open the HLS page in the default browser. No second video encoder.

## Encode-once pipeline

### MVP0 / MVP1 (file path)

```
WGC frame  →  D3D11 (optional GPU scale)  →  Media Foundation H.264
                                                    │
WASAPI loopback + mic  →  mix / clock  ─────────────┤
                                                    ▼
                                              mux → MP4
```

Implemented today in `lightcapture-core` + `lightcapture-cli` (`probe`, `record`) + `lightcapture-app` (tray). A 2-slot `FrameGate` drops extra frames instead of queueing them. WASAPI system + mic mix into one AAC track. Quality presets GPU-scale to fit 720p / 1080p before encode (never upscale).

### MVP2 (packet bus)

```
WGC + WASAPI  →  single H.264 MFT  →  encoded packet bus
                                         │
                         ┌───────────────┼───────────────┐
                         ▼               ▼               ▼
                       MP4 file     replay ring     MediaMTX ingest
```

Subscribers consume **already-encoded** packets. Never start a second video encoder. Replay is a circular buffer of those packets; stream ingest is another subscriber. Bounded queues only — no raw-frame rings.

## Memory

- Frame pool length is 2.
- If the encoder is behind, drop the frame and increment `dropped_frames`.
- Never `Map` capture textures to CPU on the encode path.
- Never accumulate raw frames in RAM.
- MVP2 packet queues stay bounded; drop or shed subscribers under backpressure rather than growing unbounded.

## Encoder selection

At session start: NVIDIA MFT → Intel Quick Sync MFT → AMD AMF MFT → software. One Media Foundation backend covers vendors (`MFTEnumEx` + `MFT_ENUM_FLAG_HARDWARE`).

## Audio (MVP1)

- WASAPI loopback (system) + WASAPI capture (microphone).
- Mix to one stereo track in the MP4 for MVP1; keep sources separate inside the engine so two tracks can land later.
- Shared clock with video; target drift under 50 ms.
- Pause freezes both video and audio without corrupting the MP4 timeline (no silent gap that breaks A/V alignment for players).

## GPU scaler (MVP1)

Quality presets (`720p30`, `1080p30`, `1080p60`) GPU-scale to fit the preset box before encode. A 4K monitor at 1080p30 must not encode 4K. Source pixels are never upscaled. The scaler uses D3D11 Video Processor when the GPU accepts BGRA, otherwise a bilinear fullscreen-triangle blit. Scaled pixels stay on the GPU; `windows-capture` still sees encode-sized content in the frame’s top-left (it copies rather than scales).

## Pause (MVP1 UX)

`Recording::set_paused` is called from the tray **Pause recording** / **Resume recording** item and `Ctrl+Shift+P`. Capture may still arrive; paused frames and audio are not written. Resume continues a coherent timeline (paused wall time is omitted from the file). The pause item is disabled while idle. If `Ctrl+Shift+P` cannot be registered, the menu still works and the tooltip notes it.

## Reliability

Write MP4 incrementally. Disk-full or encoder-lost: stop cleanly and keep bytes already written. Finalize should be fast for normal recordings. MVP2 adds ingest retry without restarting capture when the local file path stays healthy.

## UI

Tray-only (`lightcapture-app`): notification-area icon, `Ctrl+Shift+R`, right-click menu for quality / Work·Game·Silent profiles / display / foreground window / output folder / cursor / recent files. Tooltip shows encoder, FPS, encode size, encoded/dropped, elapsed, and this-process CPU/RAM. Stop (and disk-full / encoder-lost) posts one Windows toast with the file path. No visible main window and no live preview (preview costs GPU/VRAM and fights “stay out of the way”).

Displays persist by GDI device id (`\\.\DISPLAYn`) and friendly name, not a shifting index. Default filenames are `LightCapture-YYYYMMDD-HHMMSS-source.mp4`. A settings window / egui stays parked. Stream controls are tray toggle + `stream_url` in settings.json; ingest is `ffmpeg -c copy` of the finished file; open MediaMTX playback in the default browser — no in-process WebRTC viewer.

## Adaptive (MVP2)

Triggered by encoder backpressure (2-slot pool full), not FPS throttling. Ladder: 60 → 30 → 24 FPS. Encode size stays at the session preset (one encoder; no mid-session restart). `720p24` is also a start preset.

## Explicitly out (architecture)

Homelab suite (MediaMTX is an ingest target only), region capture, HEVC/AV1, egui/settings window, installer/updater, second encoder, Chromium/Electron/Tauri in the recording path.
