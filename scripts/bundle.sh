#!/usr/bin/env bash
# Builds gwm.app, unsigned, next to the release binary, and prints its path.
# Only what macOS ships is used past cargo: sips draws the icon from its SVG,
# iconutil packs it. sips reading SVG was checked on macOS 27 only: an older
# one that cannot stops the script with its own error.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
target=${CARGO_TARGET_DIR:-$root/target}
version=$(sed -n 's/^version = "\([^"]*\)".*/\1/p' "$root/Cargo.toml" | head -n 1)

cargo build --release --manifest-path "$root/Cargo.toml"

app=$target/release/gwm.app
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$target/release/gwm-gpui" "$app/Contents/MacOS/"

iconset=$(mktemp -d)/gwm.iconset
mkdir "$iconset"
sips -s format png "$root/assets/app-icon.svg" --out "$iconset/icon_512x512@2x.png" >/dev/null
for size in 16 32 128 256 512; do
  sips -z $size $size "$iconset/icon_512x512@2x.png" --out "$iconset/icon_${size}x${size}.png" >/dev/null
  double=$((size * 2))
  [ "$double" = 1024 ] ||
    sips -z $double $double "$iconset/icon_512x512@2x.png" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/gwm.icns"
rm -rf "$(dirname "$iconset")"

# LSMinimumSystemVersion: Rust's default deployment target on Apple silicon.
# Nothing older than the build machine's macOS has been tried.
cat >"$app/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>gwm</string>
  <key>CFBundleDisplayName</key><string>gwm</string>
  <key>CFBundleIdentifier</key><string>io.github.kbrdn1.gwm-gpui</string>
  <key>CFBundleExecutable</key><string>gwm-gpui</string>
  <key>CFBundleIconFile</key><string>gwm</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
EOF
plutil -lint "$app/Contents/Info.plist" >/dev/null

echo "$app"
