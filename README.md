# LightCapture

Lightweight native Windows screen recorder. Local-first: shortcut, hardware H.264, file on disk. No account, no cloud, no Chromium in the recording path.

This repository can record the primary display (or a window) to H.264 MP4 via Windows Graphics Capture and Media Foundation. System audio and adaptive quality are not in this slice.

## Download (mvp0)

Get **LightCapture.exe** from [GitHub Releases](https://github.com/AtulFalle/easy-screen-recorder/releases). Double-click it. You do not need Rust, Visual Studio, or any project script.

Windows SmartScreen may warn on the first run because the exe is not code-signed. Choose **More info** → **Run anyway**.

Requires Windows 10 version 1903 or later (x64).

## Workspace

| Crate | Role |
| --- | --- |
| `crates/lightcapture-core` | Recording engine (capture, encode, mux) |
| `crates/lightcapture-cli` | Headless CLI |
| `crates/lightcapture-app` | Tray app (`Ctrl+Shift+R`, quality/source/folder) |

## Requirements

- Windows 10 1903+
- Rust stable MSVC (`rust-toolchain.toml`)
- Visual Studio Build Tools / MSVC

## Tray app

The app sits in the notification area (no main window). `Ctrl+Shift+R` starts and stops recording. Right-click the icon for quality, capture source, output folder, and cursor. Recordings default to `Videos\LightCapture`.

From source (one-time compile, then run the exe):

```powershell
cargo build --release -p lightcapture-app
.\target\release\lightcapture.exe
```

You can copy `target\release\lightcapture.exe` anywhere (Desktop, a folder) and launch it without Cargo. Tagged `v*` pushes also publish that binary as `LightCapture.exe` on GitHub Releases.

## CLI

```powershell
cargo run -p lightcapture-cli --release -- probe
cargo run -p lightcapture-cli --release -- record --duration 10
cargo run -p lightcapture-cli --release -- record --window "Figma" --quality 1080p30
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
