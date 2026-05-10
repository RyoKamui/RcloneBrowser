# Feature parity and verification

This document distinguishes implementation from verification. A feature is not
considered complete merely because a control or backend function exists.

Status meanings:

- **Automated**: exercised by a deterministic test.
- **Smoke**: exercised end to end with the installed rclone and temporary local
  files.
- **Build only**: present in code and successfully compiled.
- **Manual required**: needs credentials, FUSE, another application, or an OS
  interaction that cannot be safely asserted by the current suite.

| Area | Original Qt behavior | macOS implementation | Current evidence | Remaining verification |
| --- | --- | --- | --- | --- |
| rclone discovery/version | Required | Implemented | Smoke with rclone 1.74.4 | Missing/executable override cases |
| Custom config and password | Required | Implemented | Config workflow fixture | Encrypted config and session reset |
| Proxy and global arguments | Required | Implemented | Legacy import unit coverage | Command-environment integration test |
| List all remotes/providers | Required | Implemented | Real smoke and deterministic fixture | Authenticated provider sample |
| Add/reconfigure/delete remote | Required | Implemented | Create/update/delete fixture | Cancellation and real OAuth provider |
| Local and remote browsing | Required | Implemented | Local smoke, fixture, and authenticated Drive/crypt probe | Additional provider samples |
| Hidden files/icons/row options | Required | Implemented | Swift model tests and packaged light/dark launch | Detailed interaction pass |
| Tabs, navigation, lazy cache | Required | Implemented | Swift history/cache unit tests | UI keyboard/mouse pass |
| Dual-pane copy/move/sync | Enhancement | Implemented | Local smoke and remote CLI fixture | Directories, failures, shared drive |
| Create/rename/move/delete | Required | Implemented | Local smoke and remote CLI fixture | Authenticated shared-drive case |
| Upload/download/drag and drop | Required | Implemented | Core local transfer smoke | Native picker and drag/drop UI pass |
| Progress/logs/cancellation | Required | Implemented | Progress smoke and cancellation fixture | Failure and process-tree edge cases |
| Copy rclone command | Required | Implemented | Backend quoting unit test | Clipboard UI pass |
| Folder size/tree/export | Required | Implemented | Local and remote fixtures including CSV | Filters, TXT, and escaping |
| Public links | Required where supported | Implemented | Remote CLI fixture | Authenticated supported/unsupported providers |
| Saved tasks/full options | Required | Implemented | Migration and save/run/dry-run fixture | UI edit and every advanced option |
| Mount/unmount | Required | Implemented | Mount lifecycle fixture | macFUSE mount, unmount, and forced quit cleanup |
| External-player streaming | Required | Implemented | Command parsing and pipe fixture | mpv/VLC launch, stop, and failure paths |
| Tray/status item | Required options | Implemented | Runtime launch only | Visibility, close-to-tray, reopen, quit |
| Notifications | Required option | Implemented | Build only | Permission granted/denied and completion |
| Update checks | Required options | Implemented | Shared parser tests, bridge fixture, and native/Tauri sheet build | Mocked application-release response |
| Portable mode/migration | Required | Implemented for macOS | Settings/tasks import unit coverage | Packaged marker and path integration |
| Signed package structure | Required for release | Implemented ad hoc | Package and strict signature verification | Developer ID, notarization, clean-machine run |

## Baseline recorded on 2026-08-15

- Shared Rust core: three update parsing, official-link, and numeric version
  tests pass and are consumed by both maintained backends.
- Native Rust: 6 unit tests passed before audit; 9 now pass. Strict Clippy
  initially failed on one derivable implementation and was repaired. A deterministic bridge
  integration test was added for remote configuration, browsing, operations,
  transfers, tasks, streaming, exports, public links, and update/config paths.
- Native bridge smoke: passed provider discovery, browsing, local CRUD/move,
  transfer/progress, size, tree, CSV export, and cleanup.
- Swift: debug and release builds passed; the package initially contained no
  Swift test target. Eight model, update-contract, pane-state, history, sorting, and cache tests
  were added during the audit.
- macOS package: built, ad-hoc signed, plist-linted, and passed strict local
  signature verification.
- Tauri fallback: 16 Rust tests and the production TypeScript/Vite build passed;
  strict Clippy and the final macOS app/DMG bundle passed. Rust formatting was
  not clean at audit start and was normalized.
- Qt: retained as the behavioral reference; no current Qt build was claimed or
  run on macOS.

The configured Drive backend and crypt wrapper both passed a read-only network
listing probe. rclone warned that the configuration uses its retiring shared
Google Drive client ID and that the crypt root contains some plaintext folder
names; those are remote-configuration warnings rather than application errors.

macFUSE and mpv/VLC are not installed on the audit machine. Real filesystem
mount/unmount and external media playback therefore remain manual release-gate
items despite their deterministic process-lifecycle coverage.

## Windows/Linux port status

The Tauri presentation now follows the macOS reference structure instead of
the previous single-pane layout. It has independent left/right tabs, history,
filtering, sorting, cache, selection, local/remote locations, inter-pane copy
and move, the matching toolbar and sidebar wording, and the compact transfer
shelf. A live macOS-hosted Tauri smoke confirmed both panes against the
installed rclone at the 1200×760 minimum window. This is implementation
evidence, not a substitute for Windows and Linux UI verification.

The verification workflow now builds and retains real Tauri bundles on current
Windows and Ubuntu runners after frontend, backend, fixture, formatting, and
strict lint checks. Native file dialogs, tray behavior, notifications,
mount/unmount, process cancellation, and visual layout still require their
platform runner/manual gates before either port is called releasable.

## Release gate

A platform is releasable only when its production package builds on a clean CI
runner, backend tests pass, the relevant UI smoke suite passes, and every
platform-specific item above has either passed or is documented as an explicit
unsupported limitation. “No known issues” may be stated only after that gate;
“without issues” is not treated as a provable absolute.
