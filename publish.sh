#!/bin/bash
set -e

# jxl-ui release script
# Cross-compiles for macOS, Linux, and Windows, publishes to GitHub releases

REPO="hjanuschka/jxl-ui"
APP_NAME="jxl-ui"
BUNDLE_ID="com.hjanuschka.jxl-ui"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

print_status() {
    echo -e "${GREEN}==>${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}Warning:${NC} $1"
}

print_error() {
    echo -e "${RED}Error:${NC} $1"
}

# Get version from Cargo.toml
VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')

if [ -z "$VERSION" ]; then
    print_error "Could not extract version from Cargo.toml"
    exit 1
fi

print_status "Building jxl-ui v${VERSION}"

# Check for required tools
if ! command -v gh &> /dev/null; then
    print_error "GitHub CLI (gh) is not installed. Please install it: brew install gh"
    exit 1
fi

if ! command -v cargo &> /dev/null; then
    print_error "Cargo is not installed. Please install Rust: https://rustup.rs"
    exit 1
fi

# Check if gh is authenticated
if ! gh auth status &> /dev/null; then
    print_error "Not authenticated with GitHub. Run: gh auth login"
    exit 1
fi

# Create release directory
RELEASE_DIR="target/release-builds"
rm -rf "$RELEASE_DIR"
mkdir -p "$RELEASE_DIR"

# Function to create macOS .app bundle
create_app_bundle() {
    local ARCH=$1
    local BINARY_PATH=$2
    local OUTPUT_DIR=$3

    local APP_DIR="$OUTPUT_DIR/JXL-UI.app"
    local CONTENTS_DIR="$APP_DIR/Contents"
    local MACOS_DIR="$CONTENTS_DIR/MacOS"
    local RESOURCES_DIR="$CONTENTS_DIR/Resources"

    print_status "Creating app bundle for $ARCH..."

    mkdir -p "$MACOS_DIR"
    mkdir -p "$RESOURCES_DIR"

    # Copy binary
    cp "$BINARY_PATH" "$MACOS_DIR/jxl-ui"
    chmod +x "$MACOS_DIR/jxl-ui"

    # Create Info.plist
    cat > "$CONTENTS_DIR/Info.plist" << PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleExecutable</key>
    <string>jxl-ui</string>
    <key>CFBundleIconFile</key>
    <string>AppIcon</string>
    <key>CFBundleIdentifier</key>
    <string>${BUNDLE_ID}</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>JXL-UI</string>
    <key>CFBundleDisplayName</key>
    <string>JXL-UI</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>${VERSION}</string>
    <key>CFBundleVersion</key>
    <string>${VERSION}</string>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSSupportsAutomaticGraphicsSwitching</key>
    <true/>
    <key>CFBundleDocumentTypes</key>
    <array>
        <dict>
            <key>CFBundleTypeName</key>
            <string>JPEG XL Image</string>
            <key>CFBundleTypeRole</key>
            <string>Viewer</string>
            <key>LSHandlerRank</key>
            <string>Default</string>
            <key>CFBundleTypeExtensions</key>
            <array>
                <string>jxl</string>
            </array>
            <key>CFBundleTypeMIMETypes</key>
            <array>
                <string>image/jxl</string>
            </array>
        </dict>
    </array>
</dict>
</plist>
PLIST

    # Create icns from PNG if available
    if [ -f "assets/icon.png" ]; then
        print_status "Creating app icon..."
        local ICONSET_DIR=$(mktemp -d)/AppIcon.iconset
        mkdir -p "$ICONSET_DIR"

        # Generate all required icon sizes
        sips -z 16 16     assets/icon.png --out "$ICONSET_DIR/icon_16x16.png" 2>/dev/null
        sips -z 32 32     assets/icon.png --out "$ICONSET_DIR/icon_16x16@2x.png" 2>/dev/null
        sips -z 32 32     assets/icon.png --out "$ICONSET_DIR/icon_32x32.png" 2>/dev/null
        sips -z 64 64     assets/icon.png --out "$ICONSET_DIR/icon_32x32@2x.png" 2>/dev/null
        sips -z 128 128   assets/icon.png --out "$ICONSET_DIR/icon_128x128.png" 2>/dev/null
        sips -z 256 256   assets/icon.png --out "$ICONSET_DIR/icon_128x128@2x.png" 2>/dev/null
        sips -z 256 256   assets/icon.png --out "$ICONSET_DIR/icon_256x256.png" 2>/dev/null
        sips -z 512 512   assets/icon.png --out "$ICONSET_DIR/icon_256x256@2x.png" 2>/dev/null
        sips -z 512 512   assets/icon.png --out "$ICONSET_DIR/icon_512x512.png" 2>/dev/null
        sips -z 1024 1024 assets/icon.png --out "$ICONSET_DIR/icon_512x512@2x.png" 2>/dev/null

        # Convert to icns
        iconutil -c icns "$ICONSET_DIR" -o "$RESOURCES_DIR/AppIcon.icns" 2>/dev/null || true
        rm -rf "$(dirname $ICONSET_DIR)"
    else
        print_warning "No icon found at assets/icon.png - app will use default icon"
    fi
}

# =============================================================================
# macOS Builds
# =============================================================================

# Build for macOS ARM64 (Apple Silicon)
print_status "Building for macOS ARM64 (Apple Silicon)..."
rustup target add aarch64-apple-darwin 2>/dev/null || true
cargo build --release --target aarch64-apple-darwin

# Build for macOS x86_64 (Intel)
print_status "Building for macOS x86_64 (Intel)..."
rustup target add x86_64-apple-darwin 2>/dev/null || true
cargo build --release --target x86_64-apple-darwin

# Create macOS app bundles
print_status "Creating macOS release packages..."

# macOS ARM64 App Bundle
ARM64_DIR="$RELEASE_DIR/${APP_NAME}-v${VERSION}-macos-arm64"
mkdir -p "$ARM64_DIR"
create_app_bundle "arm64" "target/aarch64-apple-darwin/release/${APP_NAME}" "$ARM64_DIR"
cp README.md "$ARM64_DIR/" 2>/dev/null || true
cp LICENSE "$ARM64_DIR/" 2>/dev/null || true

# macOS x86_64 App Bundle
X64_DIR="$RELEASE_DIR/${APP_NAME}-v${VERSION}-macos-x86_64"
mkdir -p "$X64_DIR"
create_app_bundle "x86_64" "target/x86_64-apple-darwin/release/${APP_NAME}" "$X64_DIR"
cp README.md "$X64_DIR/" 2>/dev/null || true
cp LICENSE "$X64_DIR/" 2>/dev/null || true

# Create universal binary and app bundle
if command -v lipo &> /dev/null; then
    print_status "Creating universal macOS binary (ARM64 + x86_64)..."
    UNIVERSAL_DIR="$RELEASE_DIR/${APP_NAME}-v${VERSION}-macos-universal"
    mkdir -p "$UNIVERSAL_DIR"

    UNIVERSAL_BINARY=$(mktemp)
    lipo -create \
        "target/aarch64-apple-darwin/release/${APP_NAME}" \
        "target/x86_64-apple-darwin/release/${APP_NAME}" \
        -output "$UNIVERSAL_BINARY"

    create_app_bundle "universal" "$UNIVERSAL_BINARY" "$UNIVERSAL_DIR"
    rm "$UNIVERSAL_BINARY"

    cp README.md "$UNIVERSAL_DIR/" 2>/dev/null || true
    cp LICENSE "$UNIVERSAL_DIR/" 2>/dev/null || true
fi

# Create DMG for universal build
if command -v hdiutil &> /dev/null && [ -d "$UNIVERSAL_DIR" ]; then
    print_status "Creating DMG installer..."
    DMG_NAME="${APP_NAME}-v${VERSION}-macos-universal.dmg"

    DMG_TEMP=$(mktemp -d)
    cp -r "$UNIVERSAL_DIR/JXL-UI.app" "$DMG_TEMP/"
    ln -s /Applications "$DMG_TEMP/Applications"

    hdiutil create -volname "JXL-UI" -srcfolder "$DMG_TEMP" -ov -format UDZO "$RELEASE_DIR/$DMG_NAME" 2>/dev/null || true
    rm -rf "$DMG_TEMP"
fi

# =============================================================================
# Linux/Windows Builds - Note on GPUI limitations
# =============================================================================
#
# GPUI requires platform-native shader compilation:
# - macOS: Metal shaders (works natively)
# - Windows: DirectX HLSL shaders (needs Windows SDK)
# - Linux: Vulkan/SPIR-V shaders (needs Linux tools)
#
# Cross-compilation from macOS is NOT possible for GPUI apps.
# For Linux/Windows builds, use GitHub Actions with native runners.
# See: .github/workflows/release.yml (if you create one)
#
print_status "Note: GPUI requires native shader compilation"
print_status "Linux/Windows builds require building ON those platforms"
print_status "Consider GitHub Actions for cross-platform releases"

# =============================================================================
# Create macOS zip archives
# =============================================================================

print_status "Creating zip archives..."
(cd "$RELEASE_DIR" && zip -r "${APP_NAME}-v${VERSION}-macos-arm64.zip" "${APP_NAME}-v${VERSION}-macos-arm64")
(cd "$RELEASE_DIR" && zip -r "${APP_NAME}-v${VERSION}-macos-x86_64.zip" "${APP_NAME}-v${VERSION}-macos-x86_64")
if [ -d "$UNIVERSAL_DIR" ]; then
    (cd "$RELEASE_DIR" && zip -r "${APP_NAME}-v${VERSION}-macos-universal.zip" "${APP_NAME}-v${VERSION}-macos-universal")
fi

# =============================================================================
# Generate checksums and create release
# =============================================================================

print_status "Generating checksums..."
(cd "$RELEASE_DIR" && shasum -a 256 *.zip *.tar.gz *.dmg 2>/dev/null > checksums.txt || shasum -a 256 *.zip > checksums.txt)

# List built artifacts
print_status "Built artifacts:"
ls -la "$RELEASE_DIR"/*.zip "$RELEASE_DIR"/*.tar.gz "$RELEASE_DIR"/*.dmg 2>/dev/null || ls -la "$RELEASE_DIR"/*.zip

# Check if this version already exists
if gh release view "v${VERSION}" --repo "$REPO" &> /dev/null; then
    print_warning "Release v${VERSION} already exists!"
    read -p "Do you want to delete and recreate it? (y/N) " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        print_status "Deleting existing release..."
        gh release delete "v${VERSION}" --repo "$REPO" --yes
        git push origin --delete "v${VERSION}" 2>/dev/null || true
    else
        print_error "Aborted. Please update the version in Cargo.toml"
        exit 1
    fi
fi

# Create GitHub release
print_status "Creating GitHub release v${VERSION}..."

# Build download table dynamically based on what was built
DOWNLOAD_TABLE="| Platform | Architecture | Download |
|----------|--------------|----------|"

# macOS
DOWNLOAD_TABLE="$DOWNLOAD_TABLE
| macOS | Universal (recommended) | \`${APP_NAME}-v${VERSION}-macos-universal.zip\` |
| macOS | Apple Silicon (ARM64) | \`${APP_NAME}-v${VERSION}-macos-arm64.zip\` |
| macOS | Intel (x86_64) | \`${APP_NAME}-v${VERSION}-macos-x86_64.zip\` |"

# Linux (if built)
if [ -f "$RELEASE_DIR/${APP_NAME}-v${VERSION}-linux-x86_64.tar.gz" ]; then
    DOWNLOAD_TABLE="$DOWNLOAD_TABLE
| Linux | x86_64 | \`${APP_NAME}-v${VERSION}-linux-x86_64.tar.gz\` |"
fi
if [ -f "$RELEASE_DIR/${APP_NAME}-v${VERSION}-linux-arm64.tar.gz" ]; then
    DOWNLOAD_TABLE="$DOWNLOAD_TABLE
| Linux | ARM64 | \`${APP_NAME}-v${VERSION}-linux-arm64.tar.gz\` |"
fi

# Windows (if built)
if [ -f "$RELEASE_DIR/${APP_NAME}-v${VERSION}-windows-x86_64.zip" ]; then
    DOWNLOAD_TABLE="$DOWNLOAD_TABLE
| Windows | x86_64 | \`${APP_NAME}-v${VERSION}-windows-x86_64.zip\` |"
fi
if [ -f "$RELEASE_DIR/${APP_NAME}-v${VERSION}-windows-arm64.zip" ]; then
    DOWNLOAD_TABLE="$DOWNLOAD_TABLE
| Windows | ARM64 | \`${APP_NAME}-v${VERSION}-windows-arm64.zip\` |"
fi

RELEASE_NOTES="## JXL-UI v${VERSION}

A native JPEG XL image viewer built with GPUI.

![JXL-UI Icon](https://raw.githubusercontent.com/hjanuschka/jxl-ui/main/assets/icon.png)

### Downloads

${DOWNLOAD_TABLE}

### Installation

#### macOS
1. Download the appropriate zip for your Mac (universal recommended)
2. Extract the zip file
3. Drag **JXL-UI.app** to your Applications folder
4. On first launch, right-click and select \"Open\" to bypass Gatekeeper

#### Linux
1. Download the tar.gz for your architecture
2. Extract: \`tar -xzf ${APP_NAME}-v${VERSION}-linux-*.tar.gz\`
3. Run: \`./${APP_NAME}\`
4. Optional: Copy the .desktop file to ~/.local/share/applications/

#### Windows
1. Download the zip for your architecture
2. Extract the zip file
3. Run \`${APP_NAME}.exe\`

### Features

- GPU-accelerated rendering
- Animation support with smooth playback
- Multi-tab interface
- URL support (Cmd+N / Ctrl+N)
- Zoom & pan controls
"

# Collect all release assets
ASSETS=()
for f in "$RELEASE_DIR"/*.zip "$RELEASE_DIR"/*.tar.gz "$RELEASE_DIR"/*.dmg; do
    [ -f "$f" ] && ASSETS+=("$f")
done
ASSETS+=("$RELEASE_DIR/checksums.txt")

gh release create "v${VERSION}" \
    --repo "$REPO" \
    --title "JXL-UI v${VERSION}" \
    --notes "$RELEASE_NOTES" \
    "${ASSETS[@]}"

print_status "Release v${VERSION} published successfully!"
print_status "View at: https://github.com/${REPO}/releases/tag/v${VERSION}"

# Print summary
echo ""
echo "=============================================="
echo "                BUILD SUMMARY"
echo "=============================================="
echo ""
echo "Artifacts created in: $RELEASE_DIR"
ls -la "$RELEASE_DIR"/*.zip "$RELEASE_DIR"/*.tar.gz "$RELEASE_DIR"/*.dmg 2>/dev/null | awk '{print "  " $NF}'
echo ""
echo "Cross-compilation uses: rustup target add <target> + cargo build --target <target>"
echo ""
echo "To enable more platforms, install toolchains:"
echo "  Linux:   brew install FiloSottile/musl-cross/musl-cross  OR  cargo install cross"
echo "  Windows: brew install mingw-w64  OR  cargo install cargo-xwin"
echo ""
echo "=============================================="
