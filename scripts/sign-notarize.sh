#!/usr/bin/env bash
# Signs, notarizes, staples and packages target/bundle.noindex/NatiKube.app into a DMG.
# Needs the developer's credentials; nothing here runs in CI by default.
#
# Required environment:
#   DEVELOPER_ID     codesign identity, e.g. "Developer ID Application: Name (TEAMID)"
#   NOTARY_PROFILE   notarytool keychain profile (xcrun notarytool store-credentials)
set -euo pipefail

: "${DEVELOPER_ID:?set DEVELOPER_ID to your Developer ID Application identity}"
: "${NOTARY_PROFILE:?set NOTARY_PROFILE to your notarytool keychain profile}"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="$ROOT/target/bundle.noindex/NatiKube.app"
VERSION="$(/usr/libexec/PlistBuddy -c 'Print CFBundleShortVersionString' "$APP/Contents/Info.plist")"
DMG="$ROOT/target/NatiKube-${VERSION}-arm64.dmg"
ZIP="$ROOT/target/NatiKube-notarize.zip"

[[ -d "$APP" ]] || { echo "missing $APP; run scripts/bundle.sh first" >&2; exit 1; }

codesign --force --timestamp --options runtime --sign "$DEVELOPER_ID" "$APP/Contents/MacOS/natikube"
codesign --force --timestamp --options runtime --sign "$DEVELOPER_ID" "$APP"
codesign --verify --deep --strict --verbose=2 "$APP"

ditto -c -k --keepParent "$APP" "$ZIP"
xcrun notarytool submit "$ZIP" --keychain-profile "$NOTARY_PROFILE" --wait
rm -f "$ZIP"
xcrun stapler staple "$APP"
xcrun stapler validate "$APP"

STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
rm -f "$DMG"
hdiutil create -volname "NatiKube" -srcfolder "$STAGE" -ov -format UDZO "$DMG"
codesign --force --timestamp --sign "$DEVELOPER_ID" "$DMG"
xcrun notarytool submit "$DMG" --keychain-profile "$NOTARY_PROFILE" --wait
xcrun stapler staple "$DMG"
shasum -a 256 "$DMG"
echo "dmg=$DMG"
