#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
desktop_dir="$(cd -- "${script_dir}/.." && pwd)"
repository_dir="$(cd -- "${desktop_dir}/../.." && pwd)"

case "$(uname -s)" in
  Linux*) release_platform="linux" ;;
  MINGW*|MSYS*|CYGWIN*) release_platform="windows" ;;
  Darwin*)
    echo "Use platforms/macos/scripts/package-macos.sh for macOS releases." >&2
    exit 1
    ;;
  *)
    echo "Unsupported packaging platform: $(uname -s)" >&2
    exit 1
    ;;
esac

release_dir="${repository_dir}/artifacts/releases/${release_platform}"

cd "${desktop_dir}"
npm run tauri build

rm -rf "${release_dir}"
mkdir -p "${release_dir}"
cp -R "${desktop_dir}/src-tauri/target/release/bundle/." "${release_dir}/"

echo "${release_dir}"
