<p align="center">
  <img src="assets/icon.png" alt="JXL-UI Logo" width="200"/>
</p>

<h1 align="center">JXL-UI</h1>

<p align="center">
  A cross-platform JPEG XL image viewer built with <a href="https://github.com/emilk/egui">egui</a> and <a href="https://github.com/libjxl/jxl-rs">jxl-rs</a>.
</p>

<p align="center">
  <a href="https://github.com/hjanuschka/jxl-ui/releases">Download</a> &bull;
  <a href="#features">Features</a> &bull;
  <a href="#installation">Installation</a> &bull;
  <a href="#keyboard-shortcuts">Shortcuts</a>
</p>

---

## Features

- **Progressive decoding** -- See images appear tile-by-tile and sharpen pass-by-pass using jxl-rs `flush_pixels()`. LF preview shows a blurry version almost instantly, then detail fills in progressively.
- **Cross-platform** -- Native apps for macOS (Intel + Apple Silicon), Windows, and Linux
- **SIMD optimized** -- Full SIMD support (SSE4.2, AVX, AVX512, NEON) via jxl-rs
- **Animation support** -- Smooth playback of animated JXL files with play/pause controls
- **Multi-tab interface** -- Open multiple images with tab navigation
- **Decoder settings** -- Configure output color format (RGB, RGBA, BGR, Grayscale, ...), data type (F32, F16, U16, U8), premultiplied alpha, and high precision mode
- **Slow Loading Demo** -- Built-in option to simulate slow network loading so you can visualize the progressive rendering passes and tile fill-in
- **Drag & drop** -- Drop JXL files directly onto the window
- **Image info panel** -- Dimensions, decode time, speed (MP/s), animation frame info

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Cmd+O` | Open file |
| `Cmd+T` | New tab |
| `Cmd+W` | Close tab |
| `Space` | Play/Pause animation |
| `I` | Toggle image info panel |
| `S` | Toggle decoder settings panel |
| `?` | About dialog |
| `Escape` | Close dialogs/panels |

## Installation

### From GitHub Releases

Download the latest release for your platform from the [releases page](https://github.com/hjanuschka/jxl-ui/releases).

- **macOS**: Download the `.dmg` file (available for both Intel and Apple Silicon)
- **Windows**: Download the `.zip` file
- **Linux**: Download the `.tar.gz` file

### From Source

Requires Rust nightly (jxl-rs uses unstable features).

```bash
# Clone the repository
git clone https://github.com/hjanuschka/jxl-ui.git
cd jxl-ui

# Build and run
cargo +nightly run --release -- path/to/image.jxl
```

## Requirements

- **macOS**: 10.13+ (High Sierra or later)
- **Windows**: Windows 10+
- **Linux**: X11 or Wayland with OpenGL support
- **Rust nightly** (for building from source)

## Usage

```bash
# Open a single image
jxl-ui image.jxl
```

Or launch the app and use **Cmd+O** to open a file, or drag and drop a `.jxl` file onto the window.

### Progressive Decoding

JXL-UI uses the jxl-rs progressive decoding API to show images as they decode:

1. **LF preview** -- A blurry low-frequency preview appears almost immediately
2. **Tile fill-in** -- Image groups/tiles fill in progressively within each pass
3. **Pass sharpening** -- Each completed pass sharpens the entire image

To visualize this on fast local files, open **Settings** (`S`) and enable **Slow Loading Demo** with a delay of 5-10ms per chunk.

## Built With

- [jxl-rs](https://github.com/libjxl/jxl-rs) -- Pure Rust JPEG XL decoder with progressive decoding and SIMD support
- [egui](https://github.com/emilk/egui) -- Immediate mode GUI framework
- [eframe](https://github.com/emilk/egui/tree/master/crates/eframe) -- egui framework for native apps

## License

BSD-3-Clause License -- see [LICENSE](LICENSE) for details.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.
