#!/bin/bash
set -e

# Build jxl-mobile-core for iOS targets
# Produces a universal (fat) static library

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CORE_DIR="$SCRIPT_DIR/../jxl-core"
OUTPUT_DIR="$SCRIPT_DIR/lib"

echo "Building jxl-mobile-core for iOS..."

# Ensure targets are installed
rustup target add aarch64-apple-ios 2>/dev/null || true
rustup target add aarch64-apple-ios-sim 2>/dev/null || true
rustup target add x86_64-apple-ios 2>/dev/null || true

cd "$CORE_DIR"

# Build for device (arm64)
echo "  Building for iOS device (aarch64)..."
cargo build --release --target aarch64-apple-ios

# Build for simulator (arm64 + x86_64)
echo "  Building for iOS simulator (aarch64)..."
cargo build --release --target aarch64-apple-ios-sim

echo "  Building for iOS simulator (x86_64)..."
cargo build --release --target x86_64-apple-ios

# Create output directory
mkdir -p "$OUTPUT_DIR"

# Copy device library
cp "target/aarch64-apple-ios/release/libjxl_mobile_core.a" "$OUTPUT_DIR/libjxl_mobile_core-ios.a"

# Create fat library for simulator (arm64 + x86_64)
lipo -create \
    "target/aarch64-apple-ios-sim/release/libjxl_mobile_core.a" \
    "target/x86_64-apple-ios/release/libjxl_mobile_core.a" \
    -output "$OUTPUT_DIR/libjxl_mobile_core-sim.a"

# Create XCFramework
echo "  Creating XCFramework..."
rm -rf "$OUTPUT_DIR/JxlMobileCore.xcframework"
xcodebuild -create-xcframework \
    -library "$OUTPUT_DIR/libjxl_mobile_core-ios.a" \
    -headers "$CORE_DIR/jxl_mobile_core.h" \
    -library "$OUTPUT_DIR/libjxl_mobile_core-sim.a" \
    -headers "$CORE_DIR/jxl_mobile_core.h" \
    -output "$OUTPUT_DIR/JxlMobileCore.xcframework"

echo ""
echo "Done! XCFramework at: $OUTPUT_DIR/JxlMobileCore.xcframework"
echo ""
echo "To use in Xcode:"
echo "  1. Drag JxlMobileCore.xcframework into your project"
echo "  2. Set bridging header to JxlUI/BridgingHeader.h"
echo "  3. Build & run"
