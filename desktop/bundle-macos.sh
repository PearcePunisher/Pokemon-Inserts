#!/usr/bin/env bash
# Builds "dist/Pokemon Inserts.app" and a drag-to-Applications .dmg.
#   ./bundle-macos.sh              # this Mac's architecture only
#   ./bundle-macos.sh --universal  # Apple Silicon + Intel in one app
set -euo pipefail
cd "$(dirname "$0")"

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
BIN_NAME=pokemon-inserts

if [[ "${1:-}" == "--universal" ]]; then
  rustup target add aarch64-apple-darwin x86_64-apple-darwin
  cargo build --release --target aarch64-apple-darwin
  cargo build --release --target x86_64-apple-darwin
  mkdir -p target/universal
  lipo -create -output "target/universal/$BIN_NAME" \
    "target/aarch64-apple-darwin/release/$BIN_NAME" \
    "target/x86_64-apple-darwin/release/$BIN_NAME"
  BIN="target/universal/$BIN_NAME"
  ARCH=universal
else
  cargo build --release
  BIN="target/release/$BIN_NAME"
  ARCH=$(uname -m)
fi

APP="dist/Pokemon Inserts.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
cp "$BIN" "$APP/Contents/MacOS/"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>Pokemon Inserts</string>
  <key>CFBundleDisplayName</key><string>Pokémon Inserts</string>
  <key>CFBundleIdentifier</key><string>com.rileypearce.pokemon-inserts</string>
  <key>CFBundleExecutable</key><string>$BIN_NAME</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
</dict></plist>
PLIST
# Ad-hoc signature: not notarized, but required for Apple Silicon to launch it.
codesign --force --deep --sign - "$APP"

DMG="dist/Pokemon-Inserts-$VERSION-macos-$ARCH.dmg"
STAGE=$(mktemp -d)
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
hdiutil create -volname "Pokemon Inserts" -srcfolder "$STAGE" -ov -format UDZO "$DMG" >/dev/null
rm -rf "$STAGE"
echo "Built $APP and $DMG"
