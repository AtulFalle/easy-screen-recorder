# LightCapture

Lightweight native Windows screen recorder. Local-first: shortcut, hardware H.264, file on disk. No account, no cloud, no Chromium in the recording path.

This repository can record the primary display (or a window) to H.264 MP4 via Windows Graphics Capture and Media Foundation. Quality presets GPU-scale to 720p or 1080p before encode. System audio and microphone are captured by default (WASAPI, mixed to one AAC track). Encoder backpressure steps FPS down (60 → 30 → 24) without starting a second encoder. Optional MediaMTX ingest remuxes the finished file (`ffmpeg -c copy`).

## Download

Get **LightCapture-Setup-*.exe** from [GitHub Releases](https://github.com/AtulFalle/easy-screen-recorder/releases). Run the installer. You do not need Rust, Visual Studio, or any project script.

The wizard defaults to a per-user install (no admin). You can choose all users (UAC, Program Files). It creates a Start Menu shortcut and a desktop shortcut. It does not start LightCapture at login.

Windows SmartScreen may warn because the setup is not code-signed. Choose **More info** → **Run anyway**.

Requires Windows 10 version 1903 or later (x64).

## Workspace

| Crate | Role |
| --- | --- |
| `crates/lightcapture-core` | Recording engine (capture, encode, mux) |
| `crates/lightcapture-cli` | Headless CLI |
| `crates/lightcapture-app` | Tray app + compact recorder bar on launch (`Ctrl+Shift+R`, `Ctrl+Shift+P` pause, Screen/Window on the bar; tray for quality/profile/recent and hide-to-tray) |

## Requirements

- Windows 10 1903+
- Rust stable MSVC (`rust-toolchain.toml`)
- Visual Studio Build Tools / MSVC

## Tray app

On launch the app shows a compact always-on-top recorder bar (Screen/Window, System/Mic, Record). Closing the bar hides it to the notification area; recording continues. Left-click the tray icon or **Show recorder** brings the bar back. The tray remains for quality, profiles, output folder, cursor, recent files, stream toggle, and other options. `Ctrl+Shift+R` starts and stops recording. `Ctrl+Shift+P` pauses and resumes (video and audio freeze; the MP4 timeline does not insert a gap). Right-click the icon for pause, quality, Work/Game/Silent profiles, capture source, output folder, cursor, recent files, and MediaMTX stream toggle. Recordings default to `Videos\LightCapture` as `LightCapture-YYYYMMDD-HHMMSS-source.mp4`. A stop toast shows Open / Show in folder. Stream URL lives in `%APPDATA%\LightCapture\settings.json` (`stream_url`, default `rtsp://127.0.0.1:8554/live`). Ingest needs `ffmpeg` on PATH; **Open stream viewer** opens the HLS page in the default browser.

From source (one-time compile, then run the exe):

```powershell
cargo build --release -p lightcapture-app
.\target\release\lightcapture.exe
```

You can still launch `target\release\lightcapture.exe` without installing. Tagged `v*` pushes publish an Inno installer on GitHub Releases, not the raw exe.

Optional local installer (needs Inno Setup 6):

```powershell
cargo build --release --locked -p lightcapture-app
.\scripts\build-installer.ps1 -AppVersion 0.1.0
```

Output: `dist\LightCapture-Setup-0.1.0.exe`

## CLI

```powershell
cargo run -p lightcapture-cli --release -- probe
cargo run -p lightcapture-cli --release -- record --duration 10
cargo run -p lightcapture-cli --release -- record --window "Figma" --quality 1080p30
cargo run -p lightcapture-cli --release -- record --duration 10 --no-mic
cargo run -p lightcapture-cli --release -- record --duration 10 --stream-url rtsp://127.0.0.1:8554/live
```

Stop with Ctrl+C if `--duration` is omitted.

## Checks

Same commands as GitHub Actions:

```powershell
.\scripts\check.ps1
```

Or:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Hardware capture tests stay ignored on CI:

```powershell
cargo test -p lightcapture-core -- --ignored
```

## Agent skills

Rust language skills (workspace, Clippy, unsafe/FFI, testing, Cargo, and related) are in [`.agents/skills/`](.agents/skills/). Product scope lives in docs and `.cursor/rules/`, not as skills.

## Docs

- [Product (condensed PRD)](docs/product/prd.md)
- [Architecture](docs/product/architecture.md)
- [Agent notes](AGENTS.md)
