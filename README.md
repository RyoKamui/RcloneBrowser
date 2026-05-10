# Rclone Browser

<img src="assets/branding/app-icon.svg" alt="Rclone Browser" width="96">

A desktop interface for browsing and managing rclone remotes.

The macOS application uses SwiftUI and AppKit. Windows and Linux share a Tauri
interface with native menus, dialogs, notifications, tray integration, paths,
and installers. Both presentations use Rust backends and the shared behavior in
`core/`.

## Repository layout

| Path | Purpose |
| --- | --- |
| `assets/` | Current canonical brand artwork and package icons |
| `core/` | Shared Rust models and rclone behavior |
| `platforms/macos/` | SwiftUI/AppKit application for macOS |
| `platforms/desktop/` | Tauri application for Windows and Linux |
| `artifacts/releases/` | Generated distributables, grouped by operating system |
| `docs/` | Architecture, feature parity, and verification notes |
| `legacy/qt/` | Read-only historical reference; excluded from current builds |

## Build and verify

Shared behavior:

```sh
cargo test --locked --manifest-path core/Cargo.toml
```

macOS:

```sh
cargo test --manifest-path platforms/macos/rust-core/Cargo.toml
swift test --package-path platforms/macos
platforms/macos/scripts/package-macos.sh
```

The macOS application is written to
`artifacts/releases/macos/Rclone Browser.app`.

Windows or Linux:

```sh
cd platforms/desktop
npm ci
npm run build
cd ../..
platforms/desktop/scripts/package-release.sh
```

The platform package script writes installers to `artifacts/releases/windows/`
or `artifacts/releases/linux/`. GitHub Actions is configured to run the same
checks and package step on actual Windows and Linux runners.

See `docs/FEATURE_PARITY.md` for the current parity status and
`docs/ARCHITECTURE.md` for the platform strategy.

## Requirements

- rclone on `PATH`, or a configured rclone executable
- macOS 14 or newer with Swift 6 for the macOS application
- Rust 1.85 or newer and Node.js 20 or newer
- Tauri 2 platform prerequisites on Windows and Linux
- FUSE or WinFsp for mounts, and a compatible external player for streaming

Rclone Browser is licensed under the MIT License.
