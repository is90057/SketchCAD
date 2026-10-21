#!/usr/bin/env bash
# Builds a SketchCAD.app bundle and packs it as .dmg and .zip in dist/.
#
# Usage: scripts/package-macos.sh <version> <binary> [<binary> ...]
#   Several binaries (e.g. arm64 + x86_64) are merged into one universal binary with lipo.
#
# Example (local, Apple Silicon only):
#   cargo build --release -p sketchcad-app
#   scripts/package-macos.sh 0.4.0 target/release/sketchcad
set -euo pipefail

VERSION="${1:?version required}"
shift
[ "$#" -ge 1 ] || { echo "at least one binary required" >&2; exit 1; }

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DIST="$ROOT/dist"
APP="$DIST/SketchCAD.app"
NAME="SketchCAD-v${VERSION}-macos-universal"
if [ "$#" -eq 1 ]; then
    NAME="SketchCAD-v${VERSION}-macos-$(lipo -archs "$1" | tr ' ' '-')"
fi

rm -rf "$APP" "$DIST/$NAME.dmg" "$DIST/$NAME.zip"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

if [ "$#" -gt 1 ]; then
    lipo -create -output "$APP/Contents/MacOS/sketchcad" "$@"
else
    cp "$1" "$APP/Contents/MacOS/sketchcad"
fi
chmod +x "$APP/Contents/MacOS/sketchcad"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>SketchCAD</string>
    <key>CFBundleDisplayName</key>
    <string>SketchCAD</string>
    <key>CFBundleIdentifier</key>
    <string>com.jameschang.sketchcad</string>
    <key>CFBundleExecutable</key>
    <string>sketchcad</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleVersion</key>
    <string>${VERSION}</string>
    <key>CFBundleShortVersionString</key>
    <string>${VERSION}</string>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSHumanReadableCopyright</key>
    <string>Developer: James Chang</string>
</dict>
</plist>
PLIST

# Ad-hoc signature: required for arm64 binaries to launch (no Apple Developer ID needed)
codesign --force --deep --sign - "$APP"

# Disk image with an /Applications shortcut for drag-and-drop install
STAGE="$(mktemp -d)"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
hdiutil create -volname "SketchCAD ${VERSION}" -srcfolder "$STAGE" -ov -format UDZO "$DIST/$NAME.dmg" >/dev/null
rm -rf "$STAGE"

(cd "$DIST" && ditto -c -k --keepParent SketchCAD.app "$NAME.zip")

echo "Created:"
echo "  $DIST/$NAME.dmg"
echo "  $DIST/$NAME.zip"
