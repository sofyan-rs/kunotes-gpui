#!/usr/bin/env bash
# Builds KuNotes.app, KuNotes-<version>-<arch>.dmg (to install by hand), and
# KuNotes-macos-<arm64|x64>.zip (what the in-app updater downloads) in
# target/release/bundle/macos/.
# Usage: packaging/macos/bundle.sh
# The app is signed ad hoc (runs on this Mac; other Macs need right-click → Open
# until it's signed with a Developer ID and notarized).
set -euo pipefail
cd "$(dirname "$0")/../.."

VERSION=$(cargo metadata --no-deps --format-version 1 \
  | python3 -c 'import json,sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"]=="kunotes"))')
OUT=target/release/bundle/macos
APP="$OUT/KuNotes.app"

cargo build --release -p kunotes

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/kunotes "$APP/Contents/MacOS/kunotes"
cp packaging/macos/KuNotes.icns "$APP/Contents/Resources/KuNotes.icns"
sed "s/@VERSION@/$VERSION/g" packaging/macos/Info.plist > "$APP/Contents/Info.plist"
plutil -lint "$APP/Contents/Info.plist" >/dev/null

codesign --force --deep --sign - "$APP"

DMG="$OUT/KuNotes-$VERSION-$(uname -m).dmg"   # e.g. arm64 for Apple Silicon
rm -f "$DMG"
STAGE=$(mktemp -d)
trap 'rm -rf "$STAGE"' EXIT
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"   # drag-to-install shortcut
hdiutil create -volname "KuNotes" -srcfolder "$STAGE" -ov -format UDZO "$DMG" >/dev/null

# The updater's package: the app alone. `ditto` keeps the bundle's symlinks and
# signature intact; the name has no version, so the app can find it in any release.
case "$(uname -m)" in
  arm64) ZIP_ARCH=arm64 ;;
  *) ZIP_ARCH=x64 ;;
esac
ZIP="$OUT/KuNotes-macos-$ZIP_ARCH.zip"
rm -f "$ZIP"
ditto -c -k --keepParent "$APP" "$ZIP"

echo "Built $APP"
echo "Built $DMG"
echo "Built $ZIP"
