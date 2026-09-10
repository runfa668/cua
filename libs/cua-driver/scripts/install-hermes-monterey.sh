#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
RUST_ROOT="$(cd -- "$SCRIPT_DIR/../rust" && pwd)"
BIN_DIR="$HOME/.local/bin"
LIB_DIR="$HOME/.local/lib/cua-driver-monterey"
PERSISTENT_BIN="$LIB_DIR/cua-driver"
LINK_PATH="$BIN_DIR/cua-driver-monterey"
STANDARD_LINK_PATH="$BIN_DIR/cua-driver"
ENV_DIR="$HOME/.hermes"
ENV_FILE="$ENV_DIR/cua-driver-monterey.env"
LINKER_WRAPPER="$RUST_ROOT/scripts/clang-monterey-linker.sh"
APP_ROOT="$HOME/Applications/CuaDriver.app"
APP_MACOS="$APP_ROOT/Contents/MacOS"
APP_BIN="$APP_MACOS/cua-driver"
[[ "$(uname -s)" == "Darwin" ]] || { echo "error: macOS only" >&2; exit 1; }
command -v cargo >/dev/null || { echo "error: cargo not found" >&2; exit 1; }
command -v xcrun >/dev/null || { echo "error: Xcode Command Line Tools required" >&2; exit 1; }
chmod +x "$LINKER_WRAPPER"
mkdir -p "$BIN_DIR" "$LIB_DIR" "$ENV_DIR" "$HOME/Applications"
export PATH="$BIN_DIR:$HOME/.cargo/bin:$PATH"
echo "==> Building Monterey cua-driver"
cd "$RUST_ROOT"
MACOSX_DEPLOYMENT_TARGET=12.3 CARGO_TARGET_X86_64_APPLE_DARWIN_LINKER="$LINKER_WRAPPER" cargo build --release -p cua-driver
BUILD_BIN="$RUST_ROOT/target/release/cua-driver"
[[ -x "$BUILD_BIN" ]] || { echo "error: build output not found" >&2; exit 1; }
# Persist outside target/ so cargo clean cannot break the installed driver.
cp "$BUILD_BIN" "$PERSISTENT_BIN"
chmod +x "$PERSISTENT_BIN"
ln -sfn "$PERSISTENT_BIN" "$LINK_PATH"
ln -sfn "$PERSISTENT_BIN" "$STANDARD_LINK_PATH"
rm -rf "$APP_ROOT"; mkdir -p "$APP_MACOS"; cp "$PERSISTENT_BIN" "$APP_BIN"; chmod +x "$APP_BIN"
cat > "$APP_ROOT/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>CFBundleName</key><string>CuaDriver</string><key>CFBundleExecutable</key><string>cua-driver</string><key>CFBundleIdentifier</key><string>com.trycua.driver</string><key>CFBundlePackageType</key><string>APPL</string><key>CFBundleShortVersionString</key><string>0.25.0</string><key>CFBundleVersion</key><string>25</string><key>LSBackgroundOnly</key><true/></dict></plist>
PLIST
codesign --force --deep --sign - "$APP_ROOT" >/dev/null 2>&1 || true
LSREGISTER="/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister"; [[ -x "$LSREGISTER" ]] && "$LSREGISTER" -f "$APP_ROOT" >/dev/null 2>&1 || true
cat > "$ENV_FILE" <<EOF
export PATH="$BIN_DIR:$HOME/.cargo/bin:\$PATH"
export HERMES_CUA_DRIVER_CMD="$LINK_PATH"
export CUA_DRIVER_RS_TELEMETRY_ENABLED=0
EOF
export HERMES_CUA_DRIVER_CMD="$LINK_PATH" CUA_DRIVER_RS_TELEMETRY_ENABLED=0
echo "==> Persistent binary: $PERSISTENT_BIN"
"$LINK_PATH" --version
open -n -g -a CuaDriver --args serve >/dev/null 2>&1 || true
sleep 2
if command -v hermes >/dev/null 2>&1; then hermes computer-use doctor || true; fi
echo "Installation complete. cargo clean is now safe for the installed driver."
