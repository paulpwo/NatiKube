#!/usr/bin/env bash
# Builds target/bundle.noindex/NatiKube.app (arm64) from the release binary and
# ad-hoc signs it. The .noindex folder keeps Spotlight and Launchpad from
# listing the build copy next to the installed app.
# Usage: scripts/bundle.sh [--skip-build]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="$ROOT/target/bundle.noindex/NatiKube.app"
BIN="$ROOT/target/release/natikube"
# Version of the workspace's app package, straight from cargo metadata.
VERSION="$(cargo metadata --manifest-path "$ROOT/Cargo.toml" --no-deps --format-version 1 \
  | jq -r '.packages[] | select(.name == "natikube-app") | .version')"
[[ -n "$VERSION" ]] || { echo "could not read the natikube-app version" >&2; exit 1; }

if [[ "${1:-}" != "--skip-build" ]]; then
  cargo build --manifest-path "$ROOT/Cargo.toml" --release
fi

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/natikube"

# Icon: the checked-in icon set rendered from assets/icon/natikube.svg.
cp "$ROOT/assets/icon/NatiKube.icns" "$APP/Contents/Resources/AppIcon.icns"

# Licenses ship inside the bundle too (see scripts/third-party-notices.sh).
cp "$ROOT/LICENSE" "$APP/Contents/Resources/LICENSE"
cp "$ROOT/packaging/THIRD-PARTY-NOTICES.md" "$APP/Contents/Resources/THIRD-PARTY-NOTICES.md"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>NatiKube</string>
  <key>CFBundleDisplayName</key><string>NatiKube</string>
  <key>CFBundleIdentifier</key><string>dev.natikube.app</string>
  <key>CFBundleExecutable</key><string>natikube</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>CFBundleVersion</key><string>${VERSION}</string>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.developer-tools</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSPrincipalClass</key><string>NSApplication</string>
</dict>
</plist>
PLIST

# Ad-hoc signature (no Apple Developer ID): keeps the bundle internally
# consistent on Apple Silicon. Gatekeeper still treats the download as
# unsigned; see the release notes for the install steps.
codesign --force --deep --options runtime --sign - "$APP"
codesign --verify --deep --strict "$APP"

plutil -lint "$APP/Contents/Info.plist"
lipo -archs "$APP/Contents/MacOS/natikube"
echo "bundle=$APP"
