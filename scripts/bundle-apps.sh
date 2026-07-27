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

make_bundle() {
  local display_name="$1" bin_name="$2" bundle_id="$3" ui_element="$4"
  local app_dir="$BUNDLES_DIR/$display_name.app"
  local bin_src="$TARGET_DIR/$bin_name"

  if [ ! -f "$bin_src" ]; then
    echo "error: missing release binary: $bin_src" >&2
    exit 1
  fi

  rm -rf "$app_dir"
  mkdir -p "$app_dir/Contents/MacOS"
  cp "$bin_src" "$app_dir/Contents/MacOS/$bin_name"

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
    <key>LSUIElement</key>
    <$ui_element/>
</dict>
</plist>
PLIST
}

make_bundle "ADE Control Center" "ade-control-center" "com.ade-bootstrapper.control-center" "false"
make_bundle "ADE Status" "ade-status" "com.ade-bootstrapper.status" "true"

echo "==> bundles ready:"
ls -d "$BUNDLES_DIR"/*.app
