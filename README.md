<p align="center">
  <img src="assets/icon.png" alt="JXL-UI Logo" width="200"/>
</p>

<h1 align="center">JXL-UI</h1>

<p align="center">
  A cross-platform JPEG XL image viewer built with <a href="https://github.com/emilk/egui">egui</a>.
</p>

<p align="center">
  <a href="https://github.com/hjanuschka/jxl-ui/releases">Download</a> •
  <a href="#features">Features</a> •
  <a href="#installation">Installation</a> •
  <a href="#keyboard-shortcuts">Shortcuts</a>
</p>

---

## Features

- **Cross-platform** - Native apps for macOS, Windows, and Linux
- **SIMD optimized** - Full SIMD support (SSE4.2, AVX, AVX512, NEON)
- **Animation support** - Smooth playback of animated JXL files
- **Multi-tab interface** - Open multiple images with tab navigation
- **URL support** - Open images directly from URLs
- **Zoom & pan** - Mouse wheel zoom, click-and-drag panning
- **Image info** - Toggle metadata overlay with 'i' key

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `O` | Open file |
| `Cmd+N` | Open URL |
| `Cmd+W` | Close tab |
| `Cmd+[` / `Cmd+]` | Previous/Next tab |
| `Cmd+1-9` | Switch to tab N |
| `Space` | Play/Pause animation |
| `Left` / `Right` | Previous/Next frame |
| `R` | Reset view |
| `+` / `-` | Zoom in/out |
| `I` | Toggle image info |
| `?` | About dialog |
| `Q` / `Cmd+Q` | Quit |

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

# Open multiple images in tabs
jxl-ui image1.jxl image2.jxl image3.jxl
```

## Built With

- [jxl-rs](https://github.com/libjxl/jxl-rs) - Pure Rust JPEG XL decoder
- [egui](https://github.com/emilk/egui) - Immediate mode GUI framework
- [eframe](https://github.com/emilk/egui/tree/master/crates/eframe) - egui framework for native apps

## License

BSD-3-Clause License - see [LICENSE](LICENSE) for details.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.
