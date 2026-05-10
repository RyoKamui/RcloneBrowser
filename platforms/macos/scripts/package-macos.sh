#!/bin/zsh
set -euo pipefail

script_dir="${0:A:h}"
native_dir="${script_dir:h}"
repository_dir="${native_dir:h:h}"
release_dir="${repository_dir}/artifacts/releases/macos"
bundle_path="${release_dir}/Rclone Browser.app"
contents_path="${bundle_path}/Contents"

cargo build --release --offline --manifest-path "${native_dir}/rust-core/Cargo.toml"

export CLANG_MODULE_CACHE_PATH="${native_dir}/.build/module-cache"
export XDG_CACHE_HOME="${native_dir}/.build/cache"
swift build --disable-sandbox --package-path "${native_dir}" -c release

rm -rf "${bundle_path}"
mkdir -p "${contents_path}/MacOS" "${contents_path}/Resources"
cp "${native_dir}/.build/release/RcloneBrowserNative" "${contents_path}/MacOS/RcloneBrowserNative"
cp "${native_dir}/Info.plist" "${contents_path}/Info.plist"
cp "${repository_dir}/assets/icons/app-icon.icns" "${contents_path}/Resources/AppIcon.icns"
chmod 755 "${contents_path}/MacOS/RcloneBrowserNative"

/usr/bin/codesign --force --deep --sign - "${bundle_path}"
/usr/bin/plutil -lint "${contents_path}/Info.plist"
echo "${bundle_path}"
