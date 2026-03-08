# JXL-UI Mobile

Native JPEG XL image viewers for Android and iOS, powered by [jxl-rs](https://github.com/libjxl/jxl-rs).

## Architecture

```
mobile/
├── jxl-core/          # Shared Rust library (C FFI + JNI)
│   ├── src/lib.rs     # jxl-rs wrapper with C and JNI bindings
│   └── jxl_mobile_core.h  # C header for iOS
├── android/           # Kotlin / Jetpack Compose app
│   ├── build-rust.sh  # Build native .so libraries
│   └── app/           # Android app source
├── ios/               # Swift / SwiftUI app
│   ├── build-rust.sh  # Build XCFramework
│   └── JxlUI/         # iOS app source
└── README.md
```

## Features

- **Pure Rust JXL decoding** via jxl-rs (no C/C++ dependencies)
- **Open JXL files** from file picker or share sheet
- **Pinch-to-zoom** and pan with smooth gestures
- **Double-tap to reset** zoom
- **Image info overlay** (dimensions, decode time, file size, MP/s)
- **Dark theme** matching the desktop JXL-UI

---

## Android

### Prerequisites

```bash
# Install Rust nightly
rustup install nightly
rustup default nightly

# Install Android targets
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android

# Install cargo-ndk
cargo install cargo-ndk

# Android SDK + NDK (via Android Studio or sdkmanager)
# Set ANDROID_HOME and ANDROID_NDK_HOME
```

### Build

```bash
# 1. Build the Rust native libraries
cd mobile/android
./build-rust.sh

# 2. Build the APK
./gradlew assembleDebug

# APK at: app/build/outputs/apk/debug/app-debug.apk
```

### Install on device

```bash
adb install app/build/outputs/apk/debug/app-debug.apk
```

---

## iOS

### Prerequisites

```bash
# Install Rust nightly
rustup install nightly
rustup default nightly

# Install iOS targets
rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios

# Xcode 15+ (from App Store)
# xcodegen (for project generation)
brew install xcodegen
```

### Build

```bash
# 1. Build the Rust XCFramework
cd mobile/ios
./build-rust.sh

# 2. Generate Xcode project
xcodegen generate

# 3. Open in Xcode
open JxlUI.xcodeproj

# 4. Select your device/simulator and hit Run
#    For a physical device, set your signing team in Xcode
```

### Build IPA (for distribution)

```bash
# In Xcode: Product > Archive
# Then: Distribute App > Development (for self-signing)
```

---

## Self-Signing

### Android
Debug APKs are automatically signed with a debug key. For release:
```bash
# Generate a keystore
keytool -genkey -v -keystore release.keystore -alias jxlui -keyalg RSA -keysize 2048

# Build release APK
./gradlew assembleRelease
```

### iOS
1. In Xcode, select your Apple ID under Signing & Capabilities
2. Use "Personal Team" for free provisioning (device-only, 7-day expiry)
3. Or use an Apple Developer account ($99/year) for longer profiles

---

## How It Works

The shared Rust library (`jxl-core`) wraps jxl-rs and exposes two interfaces:

**C FFI** (iOS):
```c
JxlImage *jxl_decode(const uint8_t *data, size_t data_len);
void jxl_image_free(JxlImage *img);
```

**JNI** (Android):
```kotlin
external fun nativeDecode(data: ByteArray): DecodedImage?
```

Both return RGBA8 pixel data which the native UI converts to platform images
(UIImage on iOS, Bitmap on Android).
