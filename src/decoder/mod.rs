pub mod worker;
pub mod rgb_conversion;

use std::time::Duration;

// Decoder output format settings
#[derive(Clone, PartialEq, Debug)]
pub enum OutputColorType {
    Auto,       // Use native format from image
    Rgb,
    Rgba,
    Bgr,
    Bgra,
    Grayscale,
    GrayscaleAlpha,
}

impl OutputColorType {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Auto => "Auto (Native)",
            Self::Rgb => "RGB",
            Self::Rgba => "RGBA",
            Self::Bgr => "BGR",
            Self::Bgra => "BGRA",
            Self::Grayscale => "Grayscale",
            Self::GrayscaleAlpha => "Grayscale + Alpha",
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum OutputDataType {
    F32,
    F16,
    U16,
    U8,
}

impl OutputDataType {
    pub fn label(&self) -> &'static str {
        match self {
            Self::F32 => "Float 32-bit",
            Self::F16 => "Float 16-bit",
            Self::U16 => "Unsigned 16-bit",
            Self::U8 => "Unsigned 8-bit",
        }
    }
}

#[derive(Clone, Debug)]
pub struct DecoderSettings {
    pub color_type: OutputColorType,
    pub data_type: OutputDataType,
    pub premultiply_alpha: bool,
    pub linear_output: bool,  // xyb_output_linear
    pub high_precision: bool,
    /// Simulate slow network loading to visualize progressive decoding.
    /// When enabled, chunks are fed at a controlled rate.
    pub simulate_slow: bool,
    /// Percentage of the file to feed per chunk (e.g. 1 = 1% per chunk)
    pub slow_chunk_pct: f32,
    /// Delay in ms between chunks
    pub slow_delay_ms: u64,
}

impl Default for DecoderSettings {
    fn default() -> Self {
        Self {
            color_type: OutputColorType::Auto,
            data_type: OutputDataType::F32,
            premultiply_alpha: true,
            linear_output: false,  // sRGB output by default
            high_precision: false,
            simulate_slow: false,
            slow_chunk_pct: 1.0,
            slow_delay_ms: 100,
        }
    }
}

/// Result of a frame decode operation
#[derive(Clone)]
pub struct DecodedFrame {
    pub rgba_data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub decode_time: Duration,
    pub duration_ms: u32, // Frame duration for animations
}

/// Metadata about the decoded image
#[derive(Clone)]
#[allow(dead_code)]
pub struct ImageMetadata {
    pub width: u32,
    pub height: u32,
    pub bit_depth: String,
    pub has_animation: bool,
    pub frame_count: usize,
    pub loop_count: u32,
}

/// Progressive update during streaming decode
#[derive(Clone)]
pub struct ProgressiveUpdate {
    /// Current RGBA pixel data (may be partially decoded)
    pub rgba_data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// Number of passes completed so far
    pub completed_passes: usize,
    /// Total number of passes (if known)
    #[allow(dead_code)]
    pub total_passes: Option<usize>,
    /// Whether this is the final (fully decoded) frame
    pub is_final: bool,
    /// Time elapsed since decode started
    pub elapsed: Duration,
}

/// Result of decoding an image (single or animated)
#[allow(dead_code)]
pub enum DecodeResult {
    SingleFrame {
        frame: DecodedFrame,
        metadata: ImageMetadata,
    },
    Animation {
        frames: Vec<DecodedFrame>,
        metadata: ImageMetadata,
    },
}
