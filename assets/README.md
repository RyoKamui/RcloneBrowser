# Application assets

This directory is the only source of current Rclone Browser artwork.

- `branding/app-icon.svg` is the editable source artwork.
- `icons/` contains the generated package formats used by SwiftUI and Tauri.

Platform projects reference these files directly. Do not copy icons into a
platform folder; replacing the brand should update this directory once and then
regenerate every icon size together.
