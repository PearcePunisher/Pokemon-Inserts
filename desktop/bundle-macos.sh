#!/usr/bin/env bash
# Builds "dist/Pokemon Inserts.app" — a double-clickable macOS app.
set -euo pipefail
cd "$(dirname "$0")"
cargo build --release
APP="dist/Pokemon Inserts.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
cp target/release/pokemon-inserts "$APP/Contents/MacOS/"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>Pokemon Inserts</string>
  <key>CFBundleDisplayName</key><string>Pokémon Inserts</string>
  <key>CFBundleIdentifier</key><string>com.rileypearce.pokemon-inserts</string>
  <key>CFBundleExecutable</key><string>pokemon-inserts</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleVersion</key><string>0.1.0</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
</dict></plist>
PLIST
echo "Built $APP"
