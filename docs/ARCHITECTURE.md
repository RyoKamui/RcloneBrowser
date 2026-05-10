# Architecture direction

## Decision

Rclone Browser will use two presentation layers over one Rust application core:

- macOS: SwiftUI and AppKit, retained as the reference interface.
- Windows and Linux: Tauri 2, with platform-native menus, dialogs,
  notifications, tray behavior, paths, and packaging.

Creating separate WinUI and GTK interfaces would maximize native widgets, but
would leave three independent implementations of every screen and interaction.
That conflicts with the requirement that layout and wording remain consistent.
Tauri provides the best balance: a single Windows/Linux presentation layer,
native operating-system integration, and a shared Rust backend.

## Target repository layout

```text
core/                    Shared models, rclone commands, persistence and jobs
platforms/macos/         SwiftUI/AppKit application and C ABI adapter
platforms/desktop/       Tauri Windows/Linux application and command adapter
legacy/qt/               Read-only source and asset archive
docs/                    Architecture, parity and release verification
assets/                  Current canonical branding and package icons
artifacts/releases/      Generated packages grouped by operating system
```

This layout is now active. Platform names describe their target operating
systems instead of using ambiguous labels such as `native`.

The extraction is now underway in `core/`. Its first production boundary owns
the typed rclone release model, parsing of stable and beta channels, numeric
version comparison, and official download-page derivation. Both the Swift
bridge and Tauri commands consume that same implementation. Process execution,
persistence, and jobs remain duplicated until each boundary has deterministic
coverage in both adapters.

## Consistency contract

The macOS application defines information architecture, feature names, control
order, empty states, confirmation text, error wording, and default values.
Windows and Linux reproduce that contract. Native adaptation is limited to
platform conventions such as window chrome, menu placement, file pickers,
keyboard modifier names, tray/status items, and filesystem path presentation.

User-visible strings should move into a shared catalog before the Tauri UI is
rewritten. Platform-specific strings must be explicitly marked and tested.

## Shared-core boundary

The core must own:

- rclone discovery and command construction
- provider-driven remote configuration
- browsing and file operations
- transfers, cancellation, progress parsing, mounts, and streams
- saved tasks and legacy migration
- settings validation and persistence models
- portable-mode detection policy

Presentation layers must own:

- windows, panes, tables, menus, dialogs, and keyboard shortcuts
- native file/folder pickers and config-file reveal actions
- notifications and tray/status items
- update presentation and platform packaging

Platform behavior in the core must be behind explicit adapters for application
data paths, executable discovery, process-tree termination, and unmounting.

## Port sequence

1. Establish original Qt-to-macOS feature parity and close macOS verification
   gaps.
2. Extract one tested Rust core from the stronger portions of the existing
   native and Tauri backends.
3. Keep the Swift C ABI thin and adapt Tauri commands to that same core.
4. Replace the existing Tauri presentation with the macOS information
   architecture and shared string catalog. This rewrite has started with the
   sidebar, native-height workspace toolbar, independent dual panes, tabs,
   pane-local cache/filter/selection, inter-pane copy/move, transfer shelf, and
   the structured rclone update sheet.
5. Verify on actual Windows and Linux runners, including packaging and
   platform-only behavior.
6. Remove the read-only Qt archive only after the parity matrix has no open
   reference-only features and its removal is explicitly approved.
