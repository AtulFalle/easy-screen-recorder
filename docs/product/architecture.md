# LightCapture architecture

## Crates

```
lightcapture-app  ──►  lightcapture-core
lightcapture-cli  ──►  lightcapture-core
```

`core` owns capture, audio, encode, mux, settings types, and session stats. Frontends must not talk to Windows capture/encode APIs directly.

## Encode-once pipeline (product path, not implemented in foundation)

```
WGC frame  →  D3D11 pool (size 2)  →  Media Foundation H.264
                                          │
                                          ▼
                                    encoded packet
                                          │
                                    MP4 (disk)
```

A future stream sink may subscribe to the same encoded packets. Do not run a second video encoder.

## Memory

- Frame pool length is 2.
- If the encoder is behind, drop the frame and increment `dropped_frames`.
- Never `Map` capture textures to CPU on the encode path.
- Never accumulate raw frames in RAM.

## Encoder selection

At session start: NVIDIA MFT → Intel Quick Sync MFT → AMD AMF MFT → software. One Media Foundation backend covers vendors (`MFTEnumEx` + `MFT_ENUM_FLAG_HARDWARE`).

## Audio

WASAPI loopback + microphone. MVP may mix to one stereo track; keep sources separate in the engine so two tracks can land later. Shared clock; target drift under 50 ms.

## Reliability

Write MP4 incrementally. Disk-full or encoder-lost: stop cleanly and keep bytes already written. Finalize should be fast for normal recordings.

## UI (after the engine writes a file)

egui/eframe, tray, `Ctrl+Shift+R`. No live preview in MVP (preview costs GPU/VRAM and fights “stay out of the way”).

## Adaptive

Triggered by encoder backpressure, drop rate, and process load — after start/stop/file is trustworthy.
