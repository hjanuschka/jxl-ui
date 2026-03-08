#!/bin/bash
set -e

# Build jxl-mobile-core for Android targets
# Requires: cargo-ndk, Android NDK

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CORE_DIR="$SCRIPT_DIR/../jxl-core"
JNILIBS="$SCRIPT_DIR/app/src/main/jniLibs"

# Targets
TARGETS=(
    "aarch64-linux-android:arm64-v8a"
    "armv7-linux-androideabi:armeabi-v7a"
    "x86_64-linux-android:x86_64"
    "i686-linux-android:x86"
)

echo "Building jxl-mobile-core for Android..."

# Ensure rust targets are installed
for target_pair in "${TARGETS[@]}"; do
    target="${target_pair%%:*}"
    rustup target add "$target" 2>/dev/null || true
done

cd "$CORE_DIR"

for target_pair in "${TARGETS[@]}"; do
    target="${target_pair%%:*}"
    abi="${target_pair##*:}"

    echo "  Building for $abi ($target)..."
    cargo ndk -t "$abi" build --release --features android

    mkdir -p "$JNILIBS/$abi"
    cp "target/$target/release/libjxl_mobile_core.so" "$JNILIBS/$abi/"
done

echo "Done! Native libraries in $JNILIBS"
ls -la "$JNILIBS"/*/libjxl_mobile_core.so
