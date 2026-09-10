# Windows installer — design

Date: 2026-09-10  
Status: approved for implementation

## Goal

Ship LightCapture as a Windows installer instead of a portable exe. A tagged GitHub Release publishes only the setup file (plus SHA256). Users install once, then launch from the Start Menu or desktop shortcut. No login auto-start. No auto-updater.

## Decisions (approved)

- Tool: Inno Setup 6, producing `LightCapture-Setup-{version}.exe`.
- Wizard asks **Install for me / for everyone**, default **for me** (no admin). Everyone-install elevates and uses Program Files.
- Start Menu shortcut: always created.
- Desktop shortcut: task, checked by default.
- Finish page: **Launch LightCapture** checked.
- Payload: tray app only (`LightCapture.exe`). CLI is not installed.
- GitHub Release assets: installer + SHA256 only. Do not upload `LightCapture.exe`.
- Uninstall via Apps & features. Settings (`%APPDATA%\LightCapture`) and recordings stay.

## Architecture

Inno Setup is packaging only. It does not change `lightcapture-core` or recording behavior. `lightcapture-app` stays the tray process; the installer copies the already-built release exe and writes shortcuts.

```
cargo build --release -p lightcapture-app
        │
        ▼
 dist/LightCapture.exe          (CI staging only; not a release asset)
        │
        ▼
 ISCC installer/LightCapture.iss
        │
        ▼
 dist/LightCapture-Setup-{version}.exe   ← GitHub Release
 dist/LightCapture-Setup-{version}.exe.sha256
```

### Install locations

Inno `PrivilegesRequired=lowest` plus `PrivilegesRequiredOverridesAllowed=dialog`. `{autopf}` / `{autoprograms}` / `{autodesktop}` follow the chosen scope:

| Scope | App folder | Shortcuts |
| --- | --- | --- |
| For me (default) | `%LOCALAPPDATA%\Programs\LightCapture` | current user Start Menu + desktop |
| For everyone | `C:\Program Files\LightCapture` | all-users Start Menu + public desktop |

Installed files: `{app}\LightCapture.exe` only.

### Stable identity

These strings must not change across versions:

- Inno `AppId`: `{8F3A1C2E-6B47-4D91-9E20-5C7A4B1D8E63}`
- `AppMutex`: `LightCapture-tray` (same name as the app’s `SingleInstance` lock)
- Display name: `LightCapture`
- Uninstall registry: Inno’s default for that `AppId`

Installer `AppVersion` is the git tag without the `v` prefix (`v0.1.0` → `0.1.0`). CI passes `/DMyAppVersion=...`. The `.iss` defaults to `0.1.0` for local compiles. Keep `Cargo.toml` workspace version in sync by hand; this slice does not fail the release if they differ.

### Wizard pages

1. Welcome
2. License — repo `LICENSE` (MIT)
3. Privileges dialog — me vs everyone (Inno built-in)
4. Directory (pre-filled from `{autopf}\LightCapture`)
5. Tasks — desktop shortcut, checked
6. Install
7. Finish — launch exe, `nowait`, checked

No “run at startup” task. No custom Inno Pascal beyond defines.

### Upgrade and running app

Same `AppId` replaces the previous install. `CloseApplications=yes`: if `LightCapture.exe` is running, Inno asks to close it. Refusing leaves files locked and the install fails with Inno’s standard error. Settings JSON and MP4s are not part of the install set, so upgrades keep them.

Downgrade is allowed (Inno overwrite). No migration step.

### Uninstall

Removes `{app}\LightCapture.exe` and the shortcuts Inno created. Does not delete `%APPDATA%\LightCapture` or `Videos\LightCapture`.

## Files

| Path | Role |
| --- | --- |
| `installer/LightCapture.iss` | Inno script (x64, MIT license, autopf, shortcuts, launch) |
| `scripts/build-installer.ps1` | Stage exe if needed, run `ISCC`, write SHA256. Used by release CI and local builds. |
| `.github/workflows/release.yml` | Tag `v*` → test → build app → installer → publish setup + hash only |
| `README.md` | Download = run the setup, not the raw exe |
| `docs/product/prd.md`, `docs/product/architecture.md`, `AGENTS.md`, `.cursor/rules/lightcapture-product.mdc` | Unpark **installer**; **auto-updater** stays parked |

PR CI (`.github/workflows/ci.yml`) does not install Inno Setup. Release is the installer compile gate. Developers with Inno installed run `.\scripts\build-installer.ps1`.

## Release CI

On `push` tags `v*`, `windows-latest`:

1. Checkout, stable MSVC Rust, cargo cache (existing).
2. `cargo test --workspace --locked`
3. `cargo build --release --locked -p lightcapture-app`
4. Copy `target\release\lightcapture.exe` → `dist\LightCapture.exe`
5. Install Inno Setup 6 (`choco install innosetup --no-progress -y`)
6. `.\scripts\build-installer.ps1 -AppVersion <tag without v> -ExePath dist\LightCapture.exe`
7. Publish with `softprops/action-gh-release`:
   - `dist/LightCapture-Setup-{version}.exe`
   - `dist/LightCapture-Setup-{version}.exe.sha256`
8. Release body tells users to run the setup. Note: unsigned, SmartScreen may warn.

`fail_on_unmatched_files: true` stays. Job name reflects installer, not “exe”.

Local equivalent after a release build:

```powershell
cargo build --release --locked -p lightcapture-app
.\scripts\build-installer.ps1 -AppVersion 0.1.0
```

## Errors

- Missing `ISCC.exe`: script exits non-zero with the expected path (`C:\Program Files (x86)\Inno Setup 6\ISCC.exe`).
- Missing staged exe: script exits non-zero.
- Tag `v0.1.0` vs workspace `0.1.0` mismatch is not auto-bumped; the installer version is the tag. Cargo crate version may lag until someone bumps it — acceptable for this slice; do not add a version-sync check unless it already exists.
- SmartScreen: document in README; no code signing in this slice.

## Testing

Automated:

- `scripts/build-installer.ps1` argument/path checks are exercised by the release job.
- A small unit-style test is not required for the `.iss` (no Rust surface). Keep a checklist in the script comment or this spec.

Manual (before treating a tag as good):

1. Per-user install, no UAC → files under `%LOCALAPPDATA%\Programs\LightCapture`, Start Menu + desktop shortcuts, launch from both, tray + `Ctrl+Shift+R`.
2. Uninstall → exe and shortcuts gone; settings and recordings remain.
3. All-users install → UAC, `Program Files\LightCapture`, shortcuts visible to other users on the machine.
4. Upgrade while the tray app is running → close prompt, then replace succeeds.
5. Confirm the GitHub Release for the tag has no `LightCapture.exe` asset.

Hardware capture tests stay `#[ignore]` as today.

## Out of scope

- Auto-updater / in-app “check for updates”
- Authenticode / EV code signing
- CLI inside the installer
- Start with Windows
- Portable exe or zip on GitHub Releases
- Custom `.ico` / `winres` embedding (shortcuts use the exe’s default icon)
- MSI, WiX, NSIS, Tauri/Electron bundlers
- Publishing the installer from pull-request CI
