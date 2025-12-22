#!/bin/bash
set -e

# jxl-ui release script
# Creates a git tag and pushes it to trigger GitHub Actions release

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

print_status "Preparing release for jxl-ui v${VERSION}"

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

# Check for uncommitted changes
if [ -n "$(git status --porcelain)" ]; then
    print_warning "You have uncommitted changes:"
    git status --short
    read -p "Continue anyway? (y/N) " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        exit 1
    fi
fi

# Check if tag already exists
if git rev-parse "v${VERSION}" >/dev/null 2>&1; then
    print_warning "Tag v${VERSION} already exists!"
    read -p "Delete and recreate? (y/N) " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        git tag -d "v${VERSION}"
        git push origin --delete "v${VERSION}" 2>/dev/null || true
    else
        print_error "Aborted. Update version in Cargo.toml first."
        exit 1
    fi
fi

# Build locally to verify it compiles
print_status "Building locally to verify..."
cargo build --release

print_status "Creating tag v${VERSION}..."
git tag -a "v${VERSION}" -m "Release v${VERSION}"

print_status "Pushing tag to trigger GitHub Actions build..."
git push origin "v${VERSION}"

print_status "Release v${VERSION} initiated!"
echo ""
echo "GitHub Actions will now build for:"
echo "  - macOS (x86_64 and ARM64)"
echo "  - Linux (x86_64)"
echo "  - Windows (x86_64)"
echo ""
echo "Monitor the build at:"
echo "  https://github.com/${REPO}/actions"
echo ""
echo "Once complete, the release will be at:"
echo "  https://github.com/${REPO}/releases/tag/v${VERSION}"
