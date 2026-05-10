# Rclone Browser for Windows and Linux

This is the Rust/Tauri implementation of Rclone Browser for Windows and Linux. It uses the
installed `rclone` CLI as its storage engine and keeps credentials in rclone's
own configuration. It currently serves as a verified fallback and as the
Windows/Linux application; the SwiftUI/AppKit application in `../macos` is the
reference interface.

## Feature parity

- Remote discovery, local filesystem browsing, encrypted remotes, and Google
  Drive “Shared with me”
- Multiple remote tabs with lazy per-folder caching, search, sorting, and
  breadcrumbs
- Upload, download, copy, move, sync, create, rename, move, delete, and public
  links
- Drag-and-drop upload and concurrent background transfers with JSON progress,
  logs, cancellation, dry runs, and command copying
- Full advanced transfer options from the Qt application
- Persistent saved tasks with run/edit/delete/dry-run actions
- Automatic one-time migration of Qt 5 `tasks.bin` records to `tasks.json`
- Folder size, directory tree, and TXT/CSV recursive exports with the legacy
  depth, age, size, filesystem, exclude, and extra-argument filters
- FUSE mount/unmount and external-player streaming
- In-app location setup generated from the installed rclone provider registry
  (all available protocols), full interactive `rclone config` fallback, and
  per-remote OAuth reconnect workflows
- Custom rclone/config paths, session-only config password, proxy variables,
  global/default arguments, portable mode, and update checks
- System/light/dark themes with a theme-aware sidebar, legacy icon and row
  appearance controls, native pickers, context actions, tray behavior, native
  notifications, safe job-aware quit, and single-instance focusing

The original Qt source remains in `../../legacy/qt` as historical/reference code. This
application is not yet considered layout- or wording-equivalent to the native
macOS 3.0 interface. See `../../docs/FEATURE_PARITY.md` and
`../../docs/ARCHITECTURE.md` for the consolidation and release gates.

## Requirements

- Rust 1.85 or newer
- Node.js 20 or newer
- The platform prerequisites for Tauri 2
- `rclone` on `PATH`, or an explicit executable path in Settings
- FUSE/WinFsp when using mount, and a media player such as `mpv` or VLC when
  using stream

## Develop and verify

```sh
cd platforms/desktop
npm install
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
npm run tauri dev
```

## Release build

```sh
platforms/desktop/scripts/package-release.sh
```

Final installers are collected in `artifacts/releases/windows/` or
`artifacts/releases/linux/`.

Standard-mode settings and tasks are stored in the operating system's
application configuration directory. Portable mode is enabled by placing an
`.ini` marker with the application name next to the executable or macOS app
bundle. A password for an encrypted rclone configuration is held in memory only
and must be entered after restarting the app.
