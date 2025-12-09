#!/bin/bash
set -e

# jxl-ui release script
# Cross-compiles for macOS (ARM64 and x86_64) and publishes to GitHub releases

REPO="hjanuschka/jxl-ui"
APP_NAME="jxl-ui"

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

# Build for macOS ARM64 (Apple Silicon)
print_status "Building for macOS ARM64 (Apple Silicon)..."
rustup target add aarch64-apple-darwin 2>/dev/null || true
cargo build --release --target aarch64-apple-darwin

# Build for macOS x86_64 (Intel)
print_status "Building for macOS x86_64 (Intel)..."
rustup target add x86_64-apple-darwin 2>/dev/null || true
cargo build --release --target x86_64-apple-darwin

# Create archives
print_status "Creating release archives..."

# macOS ARM64
ARM64_DIR="$RELEASE_DIR/${APP_NAME}-v${VERSION}-macos-arm64"
mkdir -p "$ARM64_DIR"
cp "target/aarch64-apple-darwin/release/${APP_NAME}" "$ARM64_DIR/"
cp README.md "$ARM64_DIR/" 2>/dev/null || true
cp LICENSE "$ARM64_DIR/" 2>/dev/null || true
(cd "$RELEASE_DIR" && tar -czvf "${APP_NAME}-v${VERSION}-macos-arm64.tar.gz" "${APP_NAME}-v${VERSION}-macos-arm64")

# macOS x86_64
X64_DIR="$RELEASE_DIR/${APP_NAME}-v${VERSION}-macos-x86_64"
mkdir -p "$X64_DIR"
cp "target/x86_64-apple-darwin/release/${APP_NAME}" "$X64_DIR/"
cp README.md "$X64_DIR/" 2>/dev/null || true
cp LICENSE "$X64_DIR/" 2>/dev/null || true
(cd "$RELEASE_DIR" && tar -czvf "${APP_NAME}-v${VERSION}-macos-x86_64.tar.gz" "${APP_NAME}-v${VERSION}-macos-x86_64")

# Create universal binary (if lipo available)
if command -v lipo &> /dev/null; then
    print_status "Creating universal binary (ARM64 + x86_64)..."
    UNIVERSAL_DIR="$RELEASE_DIR/${APP_NAME}-v${VERSION}-macos-universal"
    mkdir -p "$UNIVERSAL_DIR"
    lipo -create \
        "target/aarch64-apple-darwin/release/${APP_NAME}" \
        "target/x86_64-apple-darwin/release/${APP_NAME}" \
        -output "$UNIVERSAL_DIR/${APP_NAME}"
    cp README.md "$UNIVERSAL_DIR/" 2>/dev/null || true
    cp LICENSE "$UNIVERSAL_DIR/" 2>/dev/null || true
    (cd "$RELEASE_DIR" && tar -czvf "${APP_NAME}-v${VERSION}-macos-universal.tar.gz" "${APP_NAME}-v${VERSION}-macos-universal")
fi

# Generate checksums
print_status "Generating checksums..."
(cd "$RELEASE_DIR" && shasum -a 256 *.tar.gz > checksums.txt)

# List built artifacts
print_status "Built artifacts:"
ls -la "$RELEASE_DIR"/*.tar.gz

# Check if this version already exists
if gh release view "v${VERSION}" --repo "$REPO" &> /dev/null; then
    print_warning "Release v${VERSION} already exists!"
    read -p "Do you want to delete and recreate it? (y/N) " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        print_status "Deleting existing release..."
        gh release delete "v${VERSION}" --repo "$REPO" --yes
    else
        print_error "Aborted. Please update the version in Cargo.toml"
        exit 1
    fi
fi

# Create GitHub release
print_status "Creating GitHub release v${VERSION}..."

RELEASE_NOTES="## jxl-ui v${VERSION}

A native JPEG XL image viewer built with GPUI.

### Downloads

| Platform | Architecture | Download |
|----------|--------------|----------|
| macOS | Apple Silicon (ARM64) | \`${APP_NAME}-v${VERSION}-macos-arm64.tar.gz\` |
| macOS | Intel (x86_64) | \`${APP_NAME}-v${VERSION}-macos-x86_64.tar.gz\` |
| macOS | Universal | \`${APP_NAME}-v${VERSION}-macos-universal.tar.gz\` |

### Installation

1. Download the appropriate archive for your system
2. Extract: \`tar -xzf ${APP_NAME}-v${VERSION}-macos-*.tar.gz\`
3. Run: \`./${APP_NAME}-v${VERSION}-macos-*/jxl-ui image.jxl\`

### Changes

- See commit history for changes since the last release
"

# Create the release with assets
gh release create "v${VERSION}" \
    --repo "$REPO" \
    --title "jxl-ui v${VERSION}" \
    --notes "$RELEASE_NOTES" \
    "$RELEASE_DIR"/*.tar.gz \
    "$RELEASE_DIR"/checksums.txt

print_status "Release v${VERSION} published successfully!"
print_status "View at: https://github.com/${REPO}/releases/tag/v${VERSION}"
