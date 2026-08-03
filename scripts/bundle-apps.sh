#!/usr/bin/env bash
# Package the two Rust release binaries into macOS app bundles (ISC-188).
#   ADE Control Center.app  — regular window app (egui/eframe)
#   ADE Status.app          — LSUIElement menu-bar helper
# Executable names are space-free (Pulse lesson: spaces propagate into launchd
# ProgramArguments and pkill patterns); display names carry the spaces.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BUNDLES_DIR="$REPO_ROOT/bundles"
TARGET_DIR="$REPO_ROOT/target/release"

echo "==> cargo build --release"
cargo build --release --manifest-path "$REPO_ROOT/Cargo.toml"

# Build the .icns once from the committed 1024px source (regenerate that with
# `bun scripts/make-icon.ts`). sips + iconutil are stock macOS tools.
ICON_SRC="$REPO_ROOT/assets/icon-1024.png"
ICNS="$BUNDLES_DIR/ade.icns"
make_icns() {
  local iconset="$BUNDLES_DIR/ade.iconset"
  rm -rf "$iconset"
  mkdir -p "$iconset"
  local size
  for size in 16 32 128 256 512; do
    sips -z "$size" "$size" "$ICON_SRC" --out "$iconset/icon_${size}x${size}.png" >/dev/null
    local double=$((size * 2))
    sips -z "$double" "$double" "$ICON_SRC" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
  done
  iconutil -c icns "$iconset" -o "$ICNS"
  rm -rf "$iconset"
}

make_bundle() {
  local display_name="$1" bin_name="$2" bundle_id="$3" ui_element="$4"
  local app_dir="$BUNDLES_DIR/$display_name.app"
  local bin_src="$TARGET_DIR/$bin_name"

  if [ ! -f "$bin_src" ]; then
    echo "error: missing release binary: $bin_src" >&2
    exit 1
  fi

  rm -rf "$app_dir"
  mkdir -p "$app_dir/Contents/MacOS" "$app_dir/Contents/Resources"
  cp "$bin_src" "$app_dir/Contents/MacOS/$bin_name"
  cp "$ICNS" "$app_dir/Contents/Resources/ade.icns"

  cat > "$app_dir/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>$display_name</string>
    <key>CFBundleDisplayName</key>
    <string>$display_name</string>
    <key>CFBundleIdentifier</key>
    <string>$bundle_id</string>
    <key>CFBundleVersion</key>
    <string>0.2.0</string>
    <key>CFBundleShortVersionString</key>
    <string>0.2.0</string>
    <key>CFBundleExecutable</key>
    <string>$bin_name</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>LSMinimumSystemVersion</key>
    <string>13.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>CFBundleIconFile</key>
    <string>ade</string>
    <key>NSRequiresAquaSystemAppearance</key>
    <false/>
    <key>LSUIElement</key>
    <$ui_element/>
</dict>
</plist>
PLIST

  # Ad-hoc signature: a STABLE code identity, so TCC grants (screen recording,
  # accessibility) survive rebuilds instead of evaporating with every binary.
  codesign --force --sign - "$app_dir"
}

mkdir -p "$BUNDLES_DIR"
make_icns
make_bundle "ADE Control Center" "ade-control-center" "com.ade-bootstrapper.control-center" "false"
make_bundle "ADE Status" "ade-status" "com.ade-bootstrapper.status" "true"

echo "==> bundles ready (ad-hoc signed, iconned):"
ls -d "$BUNDLES_DIR"/*.app
codesign -dv "$BUNDLES_DIR/ADE Control Center.app" 2>&1 | grep -E 'Signature|Identifier' | head -2
