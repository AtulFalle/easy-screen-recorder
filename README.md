# LightCapture

Lightweight native Windows screen recorder. Local-first: shortcut, hardware H.264, file on disk. No account, no cloud, no Chromium in the recording path.

This repository is in **foundation setup**. The workspace compiles, CI is defined, and crate boundaries exist. Capture, encode, audio, and UI are not implemented yet.

## Workspace

| Crate | Role |
| --- | --- |
| `crates/lightcapture-core` | Recording engine (capture, encode, audio, mux) |
| `crates/lightcapture-cli` | Headless CLI on the same engine |
| `crates/lightcapture-app` | Desktop UI (tray, hotkeys) — no UI toolkit yet |

## Requirements

- Windows 10 1903+
- Rust stable via [`rust-toolchain.toml`](rust-toolchain.toml) (`x86_64-pc-windows-msvc`, rustfmt, clippy)
- Visual Studio Build Tools / MSVC

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

## Agent skills

Rust language skills (workspace, Clippy, unsafe/FFI, testing, Cargo, and related) are in [`.agents/skills/`](.agents/skills/). Product scope lives in docs and `.cursor/rules/`, not as skills.

## Docs

- [Product (condensed PRD)](docs/product/prd.md)
- [Architecture](docs/product/architecture.md)
- [Agent notes](AGENTS.md)
