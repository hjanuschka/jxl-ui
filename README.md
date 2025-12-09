# jxl-ui

A native JPEG XL image viewer built with [GPUI](https://gpui.rs) (Zed's GPU-accelerated UI framework).

## Features

- **Native macOS app** - GPU-accelerated rendering using Metal
- **Animation support** - Smooth playback of animated JXL files
- **Multi-tab interface** - Open multiple images with tab navigation
- **URL support** - Open images directly from URLs (Cmd+N)
- **Zoom & pan** - Mouse wheel zoom, click-and-drag panning
- **Image info** - Toggle metadata overlay with 'i' key
- **Keyboard shortcuts** - Full keyboard navigation

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

### From Source

```bash
# Clone the repository
git clone https://github.com/hjanuschka/jxl-ui.git
cd jxl-ui

# Build and run
cargo run --release -- path/to/image.jxl
```

## Requirements

- macOS 11.0+ (Big Sur or later)
- Rust 1.75+ (for building from source)

## Usage

```bash
# Open a single image
jxl-ui image.jxl

# Open multiple images in tabs
jxl-ui image1.jxl image2.jxl image3.jxl

# Open from URL (use Cmd+N in the app)
```

## Built With

- [jxl-rs](https://github.com/libjxl/jxl-rs) - Pure Rust JPEG XL decoder
- [GPUI](https://gpui.rs) - GPU-accelerated UI framework from Zed
- [image-rs](https://github.com/image-rs/image) - Image processing

## License

BSD-3-Clause License - see [LICENSE](LICENSE) for details.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.
