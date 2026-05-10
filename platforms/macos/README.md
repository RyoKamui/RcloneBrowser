# Rclone Browser for macOS

This is the macOS Rclone Browser: a SwiftUI/AppKit interface backed by a
Rust static library. It deliberately keeps rclone as the storage and credential
engine, so every protocol exposed by the installed rclone build is available.

## Architecture

- `Sources/RcloneBrowserNative`: native macOS windows, navigation, file panes,
  tabs, sheets, settings, tasks, and activity presentation
- `rust-core`: rclone discovery/configuration, browsing operations, persistent
  settings/tasks, transfers, mounts, streams, and process cancellation
- `Sources/CRcloneCore`: the two-function C ABI used by Swift

The Swift/Rust boundary carries typed JSON. Blocking rclone work runs away from
the main actor, while SwiftUI state changes remain on the main actor.

## Build and package

```sh
platforms/macos/scripts/package-macos.sh
```

The resulting bundle is `artifacts/releases/macos/Rclone Browser.app`. The packaging step
ad-hoc signs local builds and uses the same cloud icon as the in-app identity.

To use portable mode, place `Rclone Browser.ini` beside the app. Imported Qt
settings and tasks are read from that location, and native data is kept in the
adjacent `Rclone Browser Data` folder.

Requires macOS 14 or newer, Swift 6, Rust 1.85 or newer, and rclone installed at
the configured path (Homebrew paths are detected automatically).
