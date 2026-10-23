#!/usr/bin/env bash
# Builds the macOS arm64 release artifacts:
#   target/dist/NatiKube-<version>-macos-arm64.dmg
#   target/dist/NatiKube-<version>-macos-arm64.dmg.sha256
# Usage: scripts/package-macos.sh [--skip-build]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
"$ROOT/scripts/bundle.sh" "$@"

APP="$ROOT/target/bundle.noindex/NatiKube.app"
VERSION="$(/usr/libexec/PlistBuddy -c 'Print CFBundleShortVersionString' "$APP/Contents/Info.plist")"
DIST="$ROOT/target/dist"
NAME="NatiKube-${VERSION}-macos-arm64.dmg"
DMG="$DIST/$NAME"

mkdir -p "$DIST"
rm -f "$DMG" "$DMG.sha256"

STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
cp -R "$APP" "$STAGE/NatiKube.app"
ln -s /Applications "$STAGE/Applications"

hdiutil create -volname "NatiKube" -srcfolder "$STAGE" -ov -format UDZO "$DMG" >/dev/null

# The checksum file lists the bare file name so `shasum -c` works from the
# directory the user downloaded both files into.
(cd "$DIST" && shasum -a 256 "$NAME" > "$NAME.sha256")

echo "dmg=$DMG"
echo "sha256=$(cut -d' ' -f1 "$DMG.sha256")"
