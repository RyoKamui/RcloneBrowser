#!/bin/bash
set -e

QTDIR="/opt/homebrew/opt/qt"
ROOT="${PWD}"
BUILD="${ROOT}/build"

cd "${ROOT}"
V=`cat VERSION`
C=`git rev-parse --short HEAD`
VERSION="${V}-${C}"

TARGET="rclone-browser-${VERSION}-macos"
DMG="rclone-browser-${VERSION}-macos"
APP_BUNDLE="Rclone Browser.app"
APP="${TARGET}/${APP_BUNDLE}"

if [ -d "${BUILD}" ]; then rm -rf "${BUILD}"; fi
if [ -d "${ROOT}/release/${TARGET}" ]; then rm -rf "${ROOT}/release/${TARGET}"*; fi
if [ -f "${ROOT}/release/${DMG}.dmg" ]; then rm "${ROOT}/release/${DMG}.dmg"; fi
if [ -d "${ROOT}/release/Rclone Browser.app" ]; then rm -rf "${ROOT}/release/Rclone Browser.app"; fi

mkdir -p "${BUILD}"
cd "${BUILD}"

cmake -G Ninja ..     -DCMAKE_PREFIX_PATH="${QTDIR}"     -DCMAKE_BUILD_TYPE=Release     -DCMAKE_OSX_DEPLOYMENT_TARGET=10.15     -DCMAKE_OSX_ARCHITECTURES=arm64

ninja
cd build

"${QTDIR}/bin/macdeployqt" "${APP_BUNDLE}" -no-strip -always-overwrite || true
printf "[Paths]\nPlugins = PlugIns\n" > "${APP_BUNDLE}/Contents/MacOS/qt.conf"

cd ../..
mkdir -p release
cd release
mkdir "${TARGET}"
cp -R "${BUILD}/build/${APP_BUNDLE}" "${APP}"
cp "${ROOT}/README.md" "${TARGET}/Readme.md"
cp "${ROOT}/CHANGELOG.md" "${TARGET}/Changelog.md"
cp "${ROOT}/LICENSE" "${TARGET}/License.txt"
codesign --force --deep --sign - "${APP}"
codesign --verify --deep --strict --verbose=4 "${APP}"

echo "Preparing zip file"
zip -r9 "${TARGET}.zip" "${TARGET}"

echo "Preparing dmg file"
hdiutil create -volname "Rclone Browser" -srcfolder "${APP}" -ov -format UDZO "${DMG}.dmg"
rm -rf "${TARGET}"
echo "Done! The DMG is located in the release directory."
