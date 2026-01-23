pub mod worker;
pub mod rgb_conversion;

use std::time::Duration;

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
pub struct ImageMetadata {
    pub width: u32,
    pub height: u32,
    pub bit_depth: String,
    pub has_animation: bool,
    pub frame_count: usize,
    pub loop_count: u32,
}

/// Result of decoding an image (single or animated)
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
