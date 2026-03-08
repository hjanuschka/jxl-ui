//! JXL Mobile Core - Shared FFI library for decoding JPEG XL images.
//!
//! Provides both C FFI (for iOS/Swift) and JNI (for Android/Kotlin).
//! Supports progressive decoding with flush_pixels() for streaming display.
//! Supports animation decoding (multi-frame JXL).

use jxl::api::{
    JxlColorType, JxlDecoder, JxlDecoderOptions, JxlOutputBuffer, JxlPixelFormat, ProcessingResult,
};
use jxl::headers::extra_channels::ExtraChannel;
use jxl::image::{Image, Rect};
use std::io::BufReader;
use std::panic;

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

/// Decoded image result (single frame)
pub struct DecodedImage {
    pub pixels: Vec<u8>, // RGBA8
    pub width: u32,
    pub height: u32,
}

/// A single animation frame
pub struct AnimationFrame {
    pub pixels: Vec<u8>, // RGBA8
    pub width: u32,
    pub height: u32,
    pub duration_ms: u32,
}

/// Decoded animation result
pub struct DecodedAnimation {
    pub frames: Vec<AnimationFrame>,
    pub width: u32,
    pub height: u32,
    pub loop_count: u32,
}

/// Decoder settings matching desktop jxl-ui
#[derive(Clone, Debug)]
pub struct MobileSettings {
    /// 0=Auto, 1=Rgb, 2=Rgba, 3=Bgr, 4=Bgra, 5=Grayscale, 6=GrayscaleAlpha
    pub color_type: u8,
    /// 0=F32, 1=U8, 2=U16, 3=F16
    pub data_type: u8,
    pub premultiply_alpha: bool,
    pub linear_output: bool,
    pub high_precision: bool,
    // Progressive
    pub simulate_slow: bool,
    pub slow_chunk_pct: f32,
    pub slow_delay_ms: u64,
}

impl Default for MobileSettings {
    fn default() -> Self {
        Self {
            color_type: 0, // Auto
            data_type: 0,  // F32
            premultiply_alpha: true,
            linear_output: false,
            high_precision: false,
            simulate_slow: false,
            slow_chunk_pct: 1.0,
            slow_delay_ms: 50,
        }
    }
}

/// Progressive update callback data
pub struct ProgressiveUpdate {
    pub pixels: Vec<u8>, // RGBA8
    pub width: u32,
    pub height: u32,
    pub completed_passes: usize,
    pub progress_pct: usize,
    pub is_final: bool,
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Setup decoder pixel format with alpha auto-detection
struct PixelSetup {
    color_type: JxlColorType,
    extra_buf_count: usize,
}

fn setup_pixel_format(
    decoder_with_info: &mut jxl::api::JxlDecoder<jxl::api::states::WithImageInfo>,
    extra_channels: &[jxl::api::JxlExtraChannel],
    native_color_type: JxlColorType,
) -> PixelSetup {
    setup_pixel_format_with_settings(
        decoder_with_info,
        extra_channels,
        native_color_type,
        &MobileSettings::default(),
    )
}

fn setup_pixel_format_with_settings(
    decoder_with_info: &mut jxl::api::JxlDecoder<jxl::api::states::WithImageInfo>,
    extra_channels: &[jxl::api::JxlExtraChannel],
    native_color_type: JxlColorType,
    settings: &MobileSettings,
) -> PixelSetup {
    let has_alpha = extra_channels
        .iter()
        .any(|ec| ec.ec_type == ExtraChannel::Alpha);

    // Map settings color_type to JxlColorType
    let target_color_type = match settings.color_type {
        1 => JxlColorType::Rgb,
        2 => JxlColorType::Rgba,
        3 => JxlColorType::Bgr,
        4 => JxlColorType::Bgra,
        5 => JxlColorType::Grayscale,
        6 => JxlColorType::GrayscaleAlpha,
        _ => {
            // Auto: use native but upgrade to include alpha if present
            if has_alpha {
                match native_color_type {
                    JxlColorType::Rgb => JxlColorType::Rgba,
                    JxlColorType::Bgr => JxlColorType::Bgra,
                    JxlColorType::Grayscale => JxlColorType::GrayscaleAlpha,
                    other => other,
                }
            } else {
                native_color_type
            }
        }
    };

    let alpha_folded = has_alpha
        && matches!(
            target_color_type,
            JxlColorType::Rgba | JxlColorType::Bgra | JxlColorType::GrayscaleAlpha
        );

    let requested_data_format = match settings.data_type {
        1 => jxl::api::JxlDataFormat::U8 { bit_depth: 8 },
        2 => jxl::api::JxlDataFormat::U16 {
            endianness: jxl::api::Endianness::native(),
            bit_depth: 16,
        },
        3 => jxl::api::JxlDataFormat::F16 {
            endianness: jxl::api::Endianness::native(),
        },
        _ => jxl::api::JxlDataFormat::f32(),
    };

    let extra_channel_format: Vec<Option<jxl::api::JxlDataFormat>> = extra_channels
        .iter()
        .map(|ec| {
            if alpha_folded && ec.ec_type == ExtraChannel::Alpha {
                None
            } else {
                Some(requested_data_format)
            }
        })
        .collect();

    let extra_buf_count = if alpha_folded {
        extra_channels
            .iter()
            .filter(|ec| ec.ec_type != ExtraChannel::Alpha)
            .count()
    } else {
        extra_channels.len()
    };

    decoder_with_info.set_pixel_format(JxlPixelFormat {
        color_type: target_color_type,
        color_data_format: Some(requested_data_format),
        extra_channel_format,
    });

    PixelSetup {
        color_type: target_color_type,
        extra_buf_count,
    }
}

/// Allocate main + extra buffers for a frame
fn alloc_buffers(
    width: usize,
    height: usize,
    samples_per_pixel: usize,
    extra_buf_count: usize,
) -> Result<(Image<f32>, Vec<Image<f32>>), String> {
    let main_buffer =
        Image::<f32>::new((width * samples_per_pixel, height)).map_err(|e| e.to_string())?;
    let extra_bufs: Vec<Image<f32>> = (0..extra_buf_count)
        .map(|_| Image::<f32>::new((width, height)))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok((main_buffer, extra_bufs))
}

/// Convert f32 interleaved buffer to RGBA8
fn f32_buffer_to_rgba8(
    main_buffer: &Image<f32>,
    width: usize,
    height: usize,
    color_type: JxlColorType,
) -> Vec<u8> {
    let samples_per_pixel = color_type.samples_per_pixel();
    let mut rgba = vec![0u8; width * height * 4];
    for y in 0..height {
        let row = main_buffer.row(y);
        for x in 0..width {
            let src = x * samples_per_pixel;
            let dst = (y * width + x) * 4;
            match color_type {
                JxlColorType::Grayscale => {
                    let g = (row[src].clamp(0.0, 1.0) * 255.0) as u8;
                    rgba[dst] = g;
                    rgba[dst + 1] = g;
                    rgba[dst + 2] = g;
                    rgba[dst + 3] = 255;
                }
                JxlColorType::GrayscaleAlpha => {
                    let g = (row[src].clamp(0.0, 1.0) * 255.0) as u8;
                    rgba[dst] = g;
                    rgba[dst + 1] = g;
                    rgba[dst + 2] = g;
                    rgba[dst + 3] = (row[src + 1].clamp(0.0, 1.0) * 255.0) as u8;
                }
                JxlColorType::Rgb => {
                    rgba[dst] = (row[src].clamp(0.0, 1.0) * 255.0) as u8;
                    rgba[dst + 1] = (row[src + 1].clamp(0.0, 1.0) * 255.0) as u8;
                    rgba[dst + 2] = (row[src + 2].clamp(0.0, 1.0) * 255.0) as u8;
                    rgba[dst + 3] = 255;
                }
                JxlColorType::Rgba => {
                    rgba[dst] = (row[src].clamp(0.0, 1.0) * 255.0) as u8;
                    rgba[dst + 1] = (row[src + 1].clamp(0.0, 1.0) * 255.0) as u8;
                    rgba[dst + 2] = (row[src + 2].clamp(0.0, 1.0) * 255.0) as u8;
                    rgba[dst + 3] = (row[src + 3].clamp(0.0, 1.0) * 255.0) as u8;
                }
                JxlColorType::Bgr => {
                    rgba[dst] = (row[src + 2].clamp(0.0, 1.0) * 255.0) as u8;
                    rgba[dst + 1] = (row[src + 1].clamp(0.0, 1.0) * 255.0) as u8;
                    rgba[dst + 2] = (row[src].clamp(0.0, 1.0) * 255.0) as u8;
                    rgba[dst + 3] = 255;
                }
                JxlColorType::Bgra => {
                    rgba[dst] = (row[src + 2].clamp(0.0, 1.0) * 255.0) as u8;
                    rgba[dst + 1] = (row[src + 1].clamp(0.0, 1.0) * 255.0) as u8;
                    rgba[dst + 2] = (row[src].clamp(0.0, 1.0) * 255.0) as u8;
                    rgba[dst + 3] = (row[src + 3].clamp(0.0, 1.0) * 255.0) as u8;
                }
            }
        }
    }
    rgba
}

fn u8_buffer_to_rgba8(
    main_buffer: &Image<u8>,
    width: usize,
    height: usize,
    color_type: JxlColorType,
) -> Vec<u8> {
    let samples_per_pixel = color_type.samples_per_pixel();
    let mut rgba = vec![0u8; width * height * 4];
    for y in 0..height {
        let row = main_buffer.row(y);
        for x in 0..width {
            let src = x * samples_per_pixel;
            let dst = (y * width + x) * 4;
            match color_type {
                JxlColorType::Grayscale => {
                    let g = row[src];
                    rgba[dst] = g;
                    rgba[dst + 1] = g;
                    rgba[dst + 2] = g;
                    rgba[dst + 3] = 255;
                }
                JxlColorType::GrayscaleAlpha => {
                    let g = row[src];
                    rgba[dst] = g;
                    rgba[dst + 1] = g;
                    rgba[dst + 2] = g;
                    rgba[dst + 3] = row[src + 1];
                }
                JxlColorType::Rgb => {
                    rgba[dst] = row[src];
                    rgba[dst + 1] = row[src + 1];
                    rgba[dst + 2] = row[src + 2];
                    rgba[dst + 3] = 255;
                }
                JxlColorType::Rgba => {
                    rgba[dst] = row[src];
                    rgba[dst + 1] = row[src + 1];
                    rgba[dst + 2] = row[src + 2];
                    rgba[dst + 3] = row[src + 3];
                }
                JxlColorType::Bgr => {
                    rgba[dst] = row[src + 2];
                    rgba[dst + 1] = row[src + 1];
                    rgba[dst + 2] = row[src];
                    rgba[dst + 3] = 255;
                }
                JxlColorType::Bgra => {
                    rgba[dst] = row[src + 2];
                    rgba[dst + 1] = row[src + 1];
                    rgba[dst + 2] = row[src];
                    rgba[dst + 3] = row[src + 3];
                }
            }
        }
    }
    rgba
}

fn u16_buffer_to_rgba8(
    main_buffer: &Image<u16>,
    width: usize,
    height: usize,
    color_type: JxlColorType,
) -> Vec<u8> {
    let samples_per_pixel = color_type.samples_per_pixel();
    let mut rgba = vec![0u8; width * height * 4];
    let to_u8 = |v: u16| (v >> 8) as u8;

    for y in 0..height {
        let row = main_buffer.row(y);
        for x in 0..width {
            let src = x * samples_per_pixel;
            let dst = (y * width + x) * 4;
            match color_type {
                JxlColorType::Grayscale => {
                    let g = to_u8(row[src]);
                    rgba[dst] = g;
                    rgba[dst + 1] = g;
                    rgba[dst + 2] = g;
                    rgba[dst + 3] = 255;
                }
                JxlColorType::GrayscaleAlpha => {
                    let g = to_u8(row[src]);
                    rgba[dst] = g;
                    rgba[dst + 1] = g;
                    rgba[dst + 2] = g;
                    rgba[dst + 3] = to_u8(row[src + 1]);
                }
                JxlColorType::Rgb => {
                    rgba[dst] = to_u8(row[src]);
                    rgba[dst + 1] = to_u8(row[src + 1]);
                    rgba[dst + 2] = to_u8(row[src + 2]);
                    rgba[dst + 3] = 255;
                }
                JxlColorType::Rgba => {
                    rgba[dst] = to_u8(row[src]);
                    rgba[dst + 1] = to_u8(row[src + 1]);
                    rgba[dst + 2] = to_u8(row[src + 2]);
                    rgba[dst + 3] = to_u8(row[src + 3]);
                }
                JxlColorType::Bgr => {
                    rgba[dst] = to_u8(row[src + 2]);
                    rgba[dst + 1] = to_u8(row[src + 1]);
                    rgba[dst + 2] = to_u8(row[src]);
                    rgba[dst + 3] = 255;
                }
                JxlColorType::Bgra => {
                    rgba[dst] = to_u8(row[src + 2]);
                    rgba[dst + 1] = to_u8(row[src + 1]);
                    rgba[dst + 2] = to_u8(row[src]);
                    rgba[dst + 3] = to_u8(row[src + 3]);
                }
            }
        }
    }
    rgba
}

fn f16_buffer_to_rgba8(
    main_buffer: &Image<u16>,
    width: usize,
    height: usize,
    color_type: JxlColorType,
) -> Vec<u8> {
    let samples_per_pixel = color_type.samples_per_pixel();
    let mut rgba = vec![0u8; width * height * 4];
    let to_u8 = |v: u16| {
        let f = half::f16::from_bits(v).to_f32();
        (f.clamp(0.0, 1.0) * 255.0) as u8
    };

    for y in 0..height {
        let row = main_buffer.row(y);
        for x in 0..width {
            let src = x * samples_per_pixel;
            let dst = (y * width + x) * 4;
            match color_type {
                JxlColorType::Grayscale => {
                    let g = to_u8(row[src]);
                    rgba[dst] = g;
                    rgba[dst + 1] = g;
                    rgba[dst + 2] = g;
                    rgba[dst + 3] = 255;
                }
                JxlColorType::GrayscaleAlpha => {
                    let g = to_u8(row[src]);
                    rgba[dst] = g;
                    rgba[dst + 1] = g;
                    rgba[dst + 2] = g;
                    rgba[dst + 3] = to_u8(row[src + 1]);
                }
                JxlColorType::Rgb => {
                    rgba[dst] = to_u8(row[src]);
                    rgba[dst + 1] = to_u8(row[src + 1]);
                    rgba[dst + 2] = to_u8(row[src + 2]);
                    rgba[dst + 3] = 255;
                }
                JxlColorType::Rgba => {
                    rgba[dst] = to_u8(row[src]);
                    rgba[dst + 1] = to_u8(row[src + 1]);
                    rgba[dst + 2] = to_u8(row[src + 2]);
                    rgba[dst + 3] = to_u8(row[src + 3]);
                }
                JxlColorType::Bgr => {
                    rgba[dst] = to_u8(row[src + 2]);
                    rgba[dst + 1] = to_u8(row[src + 1]);
                    rgba[dst + 2] = to_u8(row[src]);
                    rgba[dst + 3] = 255;
                }
                JxlColorType::Bgra => {
                    rgba[dst] = to_u8(row[src + 2]);
                    rgba[dst + 1] = to_u8(row[src + 1]);
                    rgba[dst + 2] = to_u8(row[src]);
                    rgba[dst + 3] = to_u8(row[src + 3]);
                }
            }
        }
    }
    rgba
}

// ---------------------------------------------------------------------------
// Core decode: single frame (fast, non-progressive)
// ---------------------------------------------------------------------------

pub fn decode_jxl_to_rgba(data: &[u8]) -> Result<DecodedImage, String> {
    decode_jxl_with_settings(data, &MobileSettings::default())
}

pub fn decode_jxl_with_settings(
    data: &[u8],
    settings: &MobileSettings,
) -> Result<DecodedImage, String> {
    let mut reader = BufReader::new(std::io::Cursor::new(data));

    let mut options = JxlDecoderOptions::default();
    options.adjust_orientation = true;
    options.coalescing = true;
    options.premultiply_output = settings.premultiply_alpha;
    options.high_precision = settings.high_precision;

    let decoder = JxlDecoder::new(options);

    let mut decoder_with_info = match decoder.process(&mut reader) {
        Ok(ProcessingResult::Complete { result }) => result,
        Ok(ProcessingResult::NeedsMoreInput { .. }) => {
            return Err("Incomplete header data".to_string());
        }
        Err(e) => return Err(format!("Header decode error: {e}")),
    };

    let basic_info = decoder_with_info.basic_info();
    let (width, height) = basic_info.size;
    let extra_channels = basic_info.extra_channels.clone();
    let native_color_type = decoder_with_info.current_pixel_format().color_type;

    let setup = setup_pixel_format_with_settings(
        &mut decoder_with_info,
        &extra_channels,
        native_color_type,
        settings,
    );
    let samples_per_pixel = setup.color_type.samples_per_pixel();

    // Get frame
    let mut decoder_with_frame = match decoder_with_info.process(&mut reader) {
        Ok(ProcessingResult::Complete { result }) => result,
        Ok(ProcessingResult::NeedsMoreInput { .. }) => {
            return Err("Incomplete frame header".to_string());
        }
        Err(e) => return Err(format!("Frame header error: {e}")),
    };

    let rgba = match settings.data_type {
        1 => {
            // U8 path
            let mut main_buffer =
                Image::<u8>::new((width * samples_per_pixel, height)).map_err(|e| e.to_string())?;
            let mut extra_bufs: Vec<Image<u8>> = (0..setup.extra_buf_count)
                .map(|_| Image::<u8>::new((width, height)))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            let rect = Rect {
                size: main_buffer.size(),
                origin: (0, 0),
            };

            loop {
                let mut output_bufs = vec![JxlOutputBuffer::from_image_rect_mut(
                    main_buffer.get_rect_mut(rect).into_raw(),
                )];
                for extra in &mut extra_bufs {
                    let er = Rect {
                        size: extra.size(),
                        origin: (0, 0),
                    };
                    output_bufs.push(JxlOutputBuffer::from_image_rect_mut(
                        extra.get_rect_mut(er).into_raw(),
                    ));
                }

                match decoder_with_frame.process(&mut reader, &mut output_bufs) {
                    Ok(ProcessingResult::Complete { .. }) => break,
                    Ok(ProcessingResult::NeedsMoreInput { fallback, .. }) => {
                        decoder_with_frame = fallback;
                    }
                    Err(e) => return Err(format!("Decode error: {e}")),
                }
            }

            u8_buffer_to_rgba8(&main_buffer, width, height, setup.color_type)
        }
        2 => {
            // U16 path
            let mut main_buffer = Image::<u16>::new((width * samples_per_pixel, height))
                .map_err(|e| e.to_string())?;
            let mut extra_bufs: Vec<Image<u16>> = (0..setup.extra_buf_count)
                .map(|_| Image::<u16>::new((width, height)))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            let rect = Rect {
                size: main_buffer.size(),
                origin: (0, 0),
            };

            loop {
                let mut output_bufs = vec![JxlOutputBuffer::from_image_rect_mut(
                    main_buffer.get_rect_mut(rect).into_raw(),
                )];
                for extra in &mut extra_bufs {
                    let er = Rect {
                        size: extra.size(),
                        origin: (0, 0),
                    };
                    output_bufs.push(JxlOutputBuffer::from_image_rect_mut(
                        extra.get_rect_mut(er).into_raw(),
                    ));
                }

                match decoder_with_frame.process(&mut reader, &mut output_bufs) {
                    Ok(ProcessingResult::Complete { .. }) => break,
                    Ok(ProcessingResult::NeedsMoreInput { fallback, .. }) => {
                        decoder_with_frame = fallback;
                    }
                    Err(e) => return Err(format!("Decode error: {e}")),
                }
            }

            u16_buffer_to_rgba8(&main_buffer, width, height, setup.color_type)
        }
        3 => {
            // F16 path (packed as u16)
            let mut main_buffer = Image::<u16>::new((width * samples_per_pixel, height))
                .map_err(|e| e.to_string())?;
            let mut extra_bufs: Vec<Image<u16>> = (0..setup.extra_buf_count)
                .map(|_| Image::<u16>::new((width, height)))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            let rect = Rect {
                size: main_buffer.size(),
                origin: (0, 0),
            };

            loop {
                let mut output_bufs = vec![JxlOutputBuffer::from_image_rect_mut(
                    main_buffer.get_rect_mut(rect).into_raw(),
                )];
                for extra in &mut extra_bufs {
                    let er = Rect {
                        size: extra.size(),
                        origin: (0, 0),
                    };
                    output_bufs.push(JxlOutputBuffer::from_image_rect_mut(
                        extra.get_rect_mut(er).into_raw(),
                    ));
                }

                match decoder_with_frame.process(&mut reader, &mut output_bufs) {
                    Ok(ProcessingResult::Complete { .. }) => break,
                    Ok(ProcessingResult::NeedsMoreInput { fallback, .. }) => {
                        decoder_with_frame = fallback;
                    }
                    Err(e) => return Err(format!("Decode error: {e}")),
                }
            }

            f16_buffer_to_rgba8(&main_buffer, width, height, setup.color_type)
        }
        _ => {
            // F32 path
            let (mut main_buffer, mut extra_bufs) =
                alloc_buffers(width, height, samples_per_pixel, setup.extra_buf_count)?;
            let rect = Rect {
                size: main_buffer.size(),
                origin: (0, 0),
            };

            loop {
                let mut output_bufs = vec![JxlOutputBuffer::from_image_rect_mut(
                    main_buffer.get_rect_mut(rect).into_raw(),
                )];
                for extra in &mut extra_bufs {
                    let er = Rect {
                        size: extra.size(),
                        origin: (0, 0),
                    };
                    output_bufs.push(JxlOutputBuffer::from_image_rect_mut(
                        extra.get_rect_mut(er).into_raw(),
                    ));
                }

                match decoder_with_frame.process(&mut reader, &mut output_bufs) {
                    Ok(ProcessingResult::Complete { .. }) => break,
                    Ok(ProcessingResult::NeedsMoreInput { fallback, .. }) => {
                        decoder_with_frame = fallback;
                    }
                    Err(e) => return Err(format!("Decode error: {e}")),
                }
            }

            f32_buffer_to_rgba8(&main_buffer, width, height, setup.color_type)
        }
    };

    Ok(DecodedImage {
        pixels: rgba,
        width: width as u32,
        height: height as u32,
    })
}

// ---------------------------------------------------------------------------
// Core decode: animation (multi-frame)
// ---------------------------------------------------------------------------

pub fn decode_jxl_animation(data: &[u8]) -> Result<DecodedAnimation, String> {
    let mut reader = BufReader::new(std::io::Cursor::new(data));

    let mut options = JxlDecoderOptions::default();
    options.adjust_orientation = true;
    options.coalescing = true;
    options.premultiply_output = true;

    let decoder = JxlDecoder::new(options);

    let mut decoder_with_info = match decoder.process(&mut reader) {
        Ok(ProcessingResult::Complete { result }) => result,
        Ok(ProcessingResult::NeedsMoreInput { .. }) => {
            return Err("Incomplete header data".to_string());
        }
        Err(e) => return Err(format!("Header decode error: {e}")),
    };

    let basic_info = decoder_with_info.basic_info();
    let (width, height) = basic_info.size;
    let extra_channels = basic_info.extra_channels.clone();
    let native_color_type = decoder_with_info.current_pixel_format().color_type;
    let animation = basic_info.animation.clone();
    let loop_count = animation.as_ref().map(|a| a.num_loops).unwrap_or(0);

    let setup = setup_pixel_format(&mut decoder_with_info, &extra_channels, native_color_type);
    let color_type = setup.color_type;
    let samples_per_pixel = color_type.samples_per_pixel();

    let mut frames = Vec::new();
    let mut decoder = decoder_with_info;

    loop {
        // Get frame header - catch panic at end of animation
        let decoder_with_frame =
            match panic::catch_unwind(panic::AssertUnwindSafe(|| decoder.process(&mut reader))) {
                Ok(Ok(ProcessingResult::Complete { result })) => result,
                Ok(Ok(ProcessingResult::NeedsMoreInput { .. })) => break,
                Ok(Err(_)) => break,
                Err(_) => break,
            };

        let frame_header = decoder_with_frame.frame_header();
        let duration_ms = (frame_header.duration.unwrap_or(100.0) as u32).max(16);

        let (mut main_buffer, mut extra_bufs) =
            alloc_buffers(width, height, samples_per_pixel, setup.extra_buf_count)?;
        let rect = Rect {
            size: main_buffer.size(),
            origin: (0, 0),
        };

        let mut frame_decoder = decoder_with_frame;
        loop {
            let mut output_bufs = vec![JxlOutputBuffer::from_image_rect_mut(
                main_buffer.get_rect_mut(rect).into_raw(),
            )];
            for extra in &mut extra_bufs {
                let er = Rect {
                    size: extra.size(),
                    origin: (0, 0),
                };
                output_bufs.push(JxlOutputBuffer::from_image_rect_mut(
                    extra.get_rect_mut(er).into_raw(),
                ));
            }

            match frame_decoder
                .process(&mut reader, &mut output_bufs)
                .map_err(|e| e.to_string())?
            {
                ProcessingResult::Complete { result } => {
                    decoder = result;
                    break;
                }
                ProcessingResult::NeedsMoreInput { fallback, .. } => {
                    frame_decoder = fallback;
                }
            }
        }

        let rgba = f32_buffer_to_rgba8(&main_buffer, width, height, color_type);
        frames.push(AnimationFrame {
            pixels: rgba,
            width: width as u32,
            height: height as u32,
            duration_ms,
        });
    }

    if frames.is_empty() {
        return Err("No frames decoded".to_string());
    }

    Ok(DecodedAnimation {
        frames,
        width: width as u32,
        height: height as u32,
        loop_count,
    })
}

/// Check if JXL data contains an animation (peek at header).
pub fn is_jxl_animation(data: &[u8]) -> bool {
    let mut reader = BufReader::new(std::io::Cursor::new(data));
    let mut options = JxlDecoderOptions::default();
    options.adjust_orientation = true;
    let decoder = JxlDecoder::new(options);

    match decoder.process(&mut reader) {
        Ok(ProcessingResult::Complete { result }) => result.basic_info().animation.is_some(),
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Core decode: progressive (chunked with flush_pixels)
// ---------------------------------------------------------------------------

pub fn decode_jxl_progressive<F>(
    data: &[u8],
    settings: &MobileSettings,
    mut on_progress: F,
) -> Result<DecodedImage, String>
where
    F: FnMut(ProgressiveUpdate),
{
    match settings.data_type {
        1 => return decode_jxl_progressive_u8(data, settings, on_progress),
        2 => return decode_jxl_progressive_u16(data, settings, on_progress),
        3 => return decode_jxl_progressive_f16(data, settings, on_progress),
        _ => {}
    }

    use std::thread;
    use std::time::Duration;

    let file_size = data.len();
    // In slow mode, chunk is percentage of file; otherwise use fast fixed chunks.
    let chunk_size = if settings.simulate_slow {
        ((file_size as f32 * settings.slow_chunk_pct / 100.0) as usize).max(1024)
    } else {
        // Smaller chunks in fast-progressive mode -> earlier visible updates.
        4 * 1024
    };
    let slow_delay = if settings.simulate_slow && settings.slow_delay_ms > 0 {
        Some(Duration::from_millis(settings.slow_delay_ms))
    } else {
        None
    };

    let mut consumed = 0usize;

    let mut options = JxlDecoderOptions::default();
    options.adjust_orientation = true;
    options.coalescing = true;
    options.premultiply_output = settings.premultiply_alpha;
    options.high_precision = settings.high_precision;

    let mut decoder = JxlDecoder::new(options);

    // Phase 1: Feed chunks until we have image info (header)
    let mut decoder_with_info = loop {
        let end = (consumed + chunk_size).min(file_size);
        let mut chunk = &data[consumed..end];
        let chunk_len = chunk.len();

        match decoder.process(&mut chunk).map_err(|e| e.to_string())? {
            ProcessingResult::Complete { result } => {
                consumed += chunk_len - chunk.len();
                break result;
            }
            ProcessingResult::NeedsMoreInput { fallback, .. } => {
                consumed += chunk_len - chunk.len();
                if consumed >= file_size {
                    return Err("Incomplete header data".to_string());
                }
                if let Some(delay) = slow_delay {
                    thread::sleep(delay);
                }
                decoder = fallback;
            }
        }
    };

    let basic_info = decoder_with_info.basic_info();
    let (width, height) = basic_info.size;
    let extra_channels = basic_info.extra_channels.clone();
    let native_color_type = decoder_with_info.current_pixel_format().color_type;

    if basic_info.animation.is_some() {
        return decode_jxl_to_rgba(data);
    }

    let setup = setup_pixel_format_with_settings(
        &mut decoder_with_info,
        &extra_channels,
        native_color_type,
        settings,
    );
    let color_type = setup.color_type;
    let samples_per_pixel = color_type.samples_per_pixel();

    let (mut main_buffer, mut extra_bufs) =
        alloc_buffers(width, height, samples_per_pixel, setup.extra_buf_count)?;
    let rect = Rect {
        size: main_buffer.size(),
        origin: (0, 0),
    };

    // Helper: flush pixels into main_buffer, convert to RGBA8, call on_progress
    macro_rules! flush_and_send {
        ($decoder:expr, $passes:expr, $pct:expr, $is_final:expr) => {{
            let mut flush_bufs = vec![JxlOutputBuffer::from_image_rect_mut(
                main_buffer.get_rect_mut(rect).into_raw(),
            )];
            for extra in &mut extra_bufs {
                let er = Rect {
                    size: extra.size(),
                    origin: (0, 0),
                };
                flush_bufs.push(JxlOutputBuffer::from_image_rect_mut(
                    extra.get_rect_mut(er).into_raw(),
                ));
            }
            let _ = $decoder.flush_pixels(&mut flush_bufs);
            drop(flush_bufs);

            let rgba = f32_buffer_to_rgba8(&main_buffer, width, height, color_type);
            on_progress(ProgressiveUpdate {
                pixels: rgba,
                width: width as u32,
                height: height as u32,
                completed_passes: $passes,
                progress_pct: $pct,
                is_final: $is_final,
            });
        }};
    }

    // Phase 2: Parse frame header; flush LF preview on first NeedsMoreInput
    let mut sent_lf = false;
    let mut decoder_with_frame = loop {
        let end = (consumed + chunk_size).min(file_size);
        let mut chunk = &data[consumed..end];
        let chunk_len = chunk.len();

        let result = decoder_with_info.process(&mut chunk);
        consumed += chunk_len - chunk.len();

        match result.map_err(|e| e.to_string())? {
            ProcessingResult::Complete { result } => break result,
            ProcessingResult::NeedsMoreInput { mut fallback, .. } => {
                // Send LF preview immediately (blurry low-frequency image)
                if !sent_lf {
                    flush_and_send!(fallback, 0, consumed * 100 / file_size, false);
                    sent_lf = true;
                }
                if consumed >= file_size {
                    return Err("Incomplete frame header".to_string());
                }
                if let Some(delay) = slow_delay {
                    thread::sleep(delay);
                }
                decoder_with_info = fallback;
            }
        }
    };

    // Phase 3: Decode frame data with progressive flushing
    let mut last_passes = 0usize;
    let mut last_flush_pct = 0usize;
    // Slow-demo mode: many pixel updates.
    // Normal mode: frequent progress updates, pixel uploads only on pass changes.
    let flush_interval_pct: usize = if settings.simulate_slow { 1 } else { 5 };

    loop {
        let mut output_bufs = vec![JxlOutputBuffer::from_image_rect_mut(
            main_buffer.get_rect_mut(rect).into_raw(),
        )];
        for extra in &mut extra_bufs {
            let er = Rect {
                size: extra.size(),
                origin: (0, 0),
            };
            output_bufs.push(JxlOutputBuffer::from_image_rect_mut(
                extra.get_rect_mut(er).into_raw(),
            ));
        }

        let end = (consumed + chunk_size).min(file_size);
        let mut chunk = &data[consumed..end];
        let chunk_len = chunk.len();

        let result = decoder_with_frame.process(&mut chunk, &mut output_bufs);
        consumed += chunk_len - chunk.len();
        drop(output_bufs);

        match result.map_err(|e| e.to_string())? {
            ProcessingResult::Complete { .. } => break,
            ProcessingResult::NeedsMoreInput { mut fallback, .. } => {
                let pct = consumed * 100 / file_size.max(1);
                let passes = fallback.num_completed_passes();
                let pass_changed = passes > last_passes;
                // Flush on pass boundary or every N% progress (smaller => more visible pixelation)
                let interval_hit = pct >= last_flush_pct + flush_interval_pct;

                if pass_changed || interval_hit {
                    if pass_changed {
                        last_passes = passes;
                    }

                    // In fast mode, send frequent pixel updates in the first ~60% so the
                    // image appears early and visibly refines (desktop-like behavior).
                    let send_pixels = settings.simulate_slow || pass_changed || pct <= 60;

                    if send_pixels {
                        // Send full pixel update (expensive)
                        flush_and_send!(fallback, passes, pct, false);
                    } else {
                        // Fast mode: progress-only tick (cheap), keep current pixels
                        on_progress(ProgressiveUpdate {
                            pixels: Vec::new(),
                            width: width as u32,
                            height: height as u32,
                            completed_passes: passes,
                            progress_pct: pct,
                            is_final: false,
                        });
                    }

                    last_flush_pct = pct;
                }

                if consumed >= file_size {
                    return Err("Incomplete frame data".to_string());
                }
                if let Some(delay) = slow_delay {
                    thread::sleep(delay);
                }
                decoder_with_frame = fallback;
            }
        }
    }

    // Final full-quality result
    let rgba = f32_buffer_to_rgba8(&main_buffer, width, height, color_type);
    on_progress(ProgressiveUpdate {
        pixels: rgba.clone(),
        width: width as u32,
        height: height as u32,
        completed_passes: last_passes,
        progress_pct: 100,
        is_final: true,
    });

    Ok(DecodedImage {
        pixels: rgba,
        width: width as u32,
        height: height as u32,
    })
}

fn decode_jxl_progressive_u8<F>(
    data: &[u8],
    settings: &MobileSettings,
    mut on_progress: F,
) -> Result<DecodedImage, String>
where
    F: FnMut(ProgressiveUpdate),
{
    use std::thread;
    use std::time::Duration;

    let file_size = data.len();
    let chunk_size = if settings.simulate_slow {
        ((file_size as f32 * settings.slow_chunk_pct / 100.0) as usize).max(1024)
    } else {
        4 * 1024
    };
    let slow_delay = if settings.simulate_slow && settings.slow_delay_ms > 0 {
        Some(Duration::from_millis(settings.slow_delay_ms))
    } else {
        None
    };

    let mut consumed = 0usize;

    let mut options = JxlDecoderOptions::default();
    options.adjust_orientation = true;
    options.coalescing = true;
    options.premultiply_output = settings.premultiply_alpha;
    options.high_precision = settings.high_precision;

    let mut decoder = JxlDecoder::new(options);

    let mut decoder_with_info = loop {
        let end = (consumed + chunk_size).min(file_size);
        let mut chunk = &data[consumed..end];
        let chunk_len = chunk.len();

        match decoder.process(&mut chunk).map_err(|e| e.to_string())? {
            ProcessingResult::Complete { result } => {
                consumed += chunk_len - chunk.len();
                break result;
            }
            ProcessingResult::NeedsMoreInput { fallback, .. } => {
                consumed += chunk_len - chunk.len();
                if consumed >= file_size {
                    return Err("Incomplete header data".to_string());
                }
                if let Some(delay) = slow_delay {
                    thread::sleep(delay);
                }
                decoder = fallback;
            }
        }
    };

    let basic_info = decoder_with_info.basic_info();
    let (width, height) = basic_info.size;
    let extra_channels = basic_info.extra_channels.clone();
    let native_color_type = decoder_with_info.current_pixel_format().color_type;

    if basic_info.animation.is_some() {
        return decode_jxl_to_rgba(data);
    }

    let setup = setup_pixel_format_with_settings(
        &mut decoder_with_info,
        &extra_channels,
        native_color_type,
        settings,
    );
    let color_type = setup.color_type;
    let samples_per_pixel = color_type.samples_per_pixel();

    let mut main_buffer =
        Image::<u8>::new((width * samples_per_pixel, height)).map_err(|e| e.to_string())?;
    let mut extra_bufs: Vec<Image<u8>> = (0..setup.extra_buf_count)
        .map(|_| Image::<u8>::new((width, height)))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let rect = Rect {
        size: main_buffer.size(),
        origin: (0, 0),
    };

    macro_rules! flush_and_send {
        ($decoder:expr, $passes:expr, $pct:expr, $is_final:expr) => {{
            let mut flush_bufs = vec![JxlOutputBuffer::from_image_rect_mut(
                main_buffer.get_rect_mut(rect).into_raw(),
            )];
            for extra in &mut extra_bufs {
                let er = Rect {
                    size: extra.size(),
                    origin: (0, 0),
                };
                flush_bufs.push(JxlOutputBuffer::from_image_rect_mut(
                    extra.get_rect_mut(er).into_raw(),
                ));
            }
            let _ = $decoder.flush_pixels(&mut flush_bufs);
            drop(flush_bufs);

            let rgba = u8_buffer_to_rgba8(&main_buffer, width, height, color_type);
            on_progress(ProgressiveUpdate {
                pixels: rgba,
                width: width as u32,
                height: height as u32,
                completed_passes: $passes,
                progress_pct: $pct,
                is_final: $is_final,
            });
        }};
    }

    let mut sent_lf = false;
    let mut decoder_with_frame = loop {
        let end = (consumed + chunk_size).min(file_size);
        let mut chunk = &data[consumed..end];
        let chunk_len = chunk.len();

        let result = decoder_with_info.process(&mut chunk);
        consumed += chunk_len - chunk.len();

        match result.map_err(|e| e.to_string())? {
            ProcessingResult::Complete { result } => break result,
            ProcessingResult::NeedsMoreInput { mut fallback, .. } => {
                if !sent_lf {
                    flush_and_send!(fallback, 0, consumed * 100 / file_size, false);
                    sent_lf = true;
                }
                if consumed >= file_size {
                    return Err("Incomplete frame header".to_string());
                }
                if let Some(delay) = slow_delay {
                    thread::sleep(delay);
                }
                decoder_with_info = fallback;
            }
        }
    };

    let mut last_passes = 0usize;
    let mut last_flush_pct = 0usize;
    let flush_interval_pct: usize = if settings.simulate_slow { 1 } else { 5 };

    loop {
        let mut output_bufs = vec![JxlOutputBuffer::from_image_rect_mut(
            main_buffer.get_rect_mut(rect).into_raw(),
        )];
        for extra in &mut extra_bufs {
            let er = Rect {
                size: extra.size(),
                origin: (0, 0),
            };
            output_bufs.push(JxlOutputBuffer::from_image_rect_mut(
                extra.get_rect_mut(er).into_raw(),
            ));
        }

        let end = (consumed + chunk_size).min(file_size);
        let mut chunk = &data[consumed..end];
        let chunk_len = chunk.len();

        let result = decoder_with_frame.process(&mut chunk, &mut output_bufs);
        consumed += chunk_len - chunk.len();
        drop(output_bufs);

        match result.map_err(|e| e.to_string())? {
            ProcessingResult::Complete { .. } => break,
            ProcessingResult::NeedsMoreInput { mut fallback, .. } => {
                let pct = consumed * 100 / file_size.max(1);
                let passes = fallback.num_completed_passes();
                let pass_changed = passes > last_passes;
                let interval_hit = pct >= last_flush_pct + flush_interval_pct;

                if pass_changed || interval_hit {
                    if pass_changed {
                        last_passes = passes;
                    }

                    let send_pixels = settings.simulate_slow || pass_changed || pct <= 60;

                    if send_pixels {
                        flush_and_send!(fallback, passes, pct, false);
                    } else {
                        on_progress(ProgressiveUpdate {
                            pixels: Vec::new(),
                            width: width as u32,
                            height: height as u32,
                            completed_passes: passes,
                            progress_pct: pct,
                            is_final: false,
                        });
                    }

                    last_flush_pct = pct;
                }

                if consumed >= file_size {
                    return Err("Incomplete frame data".to_string());
                }
                if let Some(delay) = slow_delay {
                    thread::sleep(delay);
                }
                decoder_with_frame = fallback;
            }
        }
    }

    let rgba = u8_buffer_to_rgba8(&main_buffer, width, height, color_type);
    on_progress(ProgressiveUpdate {
        pixels: rgba.clone(),
        width: width as u32,
        height: height as u32,
        completed_passes: last_passes,
        progress_pct: 100,
        is_final: true,
    });

    Ok(DecodedImage {
        pixels: rgba,
        width: width as u32,
        height: height as u32,
    })
}

fn decode_jxl_progressive_u16<F>(
    data: &[u8],
    settings: &MobileSettings,
    mut on_progress: F,
) -> Result<DecodedImage, String>
where
    F: FnMut(ProgressiveUpdate),
{
    decode_jxl_progressive_u16_like(data, settings, &mut on_progress, false)
}

fn decode_jxl_progressive_f16<F>(
    data: &[u8],
    settings: &MobileSettings,
    mut on_progress: F,
) -> Result<DecodedImage, String>
where
    F: FnMut(ProgressiveUpdate),
{
    decode_jxl_progressive_u16_like(data, settings, &mut on_progress, true)
}

fn decode_jxl_progressive_u16_like(
    data: &[u8],
    settings: &MobileSettings,
    on_progress: &mut dyn FnMut(ProgressiveUpdate),
    interpret_as_f16: bool,
) -> Result<DecodedImage, String> {
    use std::thread;
    use std::time::Duration;

    let file_size = data.len();
    let chunk_size = if settings.simulate_slow {
        ((file_size as f32 * settings.slow_chunk_pct / 100.0) as usize).max(1024)
    } else {
        4 * 1024
    };
    let slow_delay = if settings.simulate_slow && settings.slow_delay_ms > 0 {
        Some(Duration::from_millis(settings.slow_delay_ms))
    } else {
        None
    };

    let mut consumed = 0usize;

    let mut options = JxlDecoderOptions::default();
    options.adjust_orientation = true;
    options.coalescing = true;
    options.premultiply_output = settings.premultiply_alpha;
    options.high_precision = settings.high_precision;

    let mut decoder = JxlDecoder::new(options);

    let mut decoder_with_info = loop {
        let end = (consumed + chunk_size).min(file_size);
        let mut chunk = &data[consumed..end];
        let chunk_len = chunk.len();

        match decoder.process(&mut chunk).map_err(|e| e.to_string())? {
            ProcessingResult::Complete { result } => {
                consumed += chunk_len - chunk.len();
                break result;
            }
            ProcessingResult::NeedsMoreInput { fallback, .. } => {
                consumed += chunk_len - chunk.len();
                if consumed >= file_size {
                    return Err("Incomplete header data".to_string());
                }
                if let Some(delay) = slow_delay {
                    thread::sleep(delay);
                }
                decoder = fallback;
            }
        }
    };

    let basic_info = decoder_with_info.basic_info();
    let (width, height) = basic_info.size;
    let extra_channels = basic_info.extra_channels.clone();
    let native_color_type = decoder_with_info.current_pixel_format().color_type;

    if basic_info.animation.is_some() {
        return decode_jxl_to_rgba(data);
    }

    let setup = setup_pixel_format_with_settings(
        &mut decoder_with_info,
        &extra_channels,
        native_color_type,
        settings,
    );
    let color_type = setup.color_type;
    let samples_per_pixel = color_type.samples_per_pixel();

    let mut main_buffer =
        Image::<u16>::new((width * samples_per_pixel, height)).map_err(|e| e.to_string())?;
    let mut extra_bufs: Vec<Image<u16>> = (0..setup.extra_buf_count)
        .map(|_| Image::<u16>::new((width, height)))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let rect = Rect {
        size: main_buffer.size(),
        origin: (0, 0),
    };

    macro_rules! flush_and_send {
        ($decoder:expr, $passes:expr, $pct:expr, $is_final:expr) => {{
            let mut flush_bufs = vec![JxlOutputBuffer::from_image_rect_mut(
                main_buffer.get_rect_mut(rect).into_raw(),
            )];
            for extra in &mut extra_bufs {
                let er = Rect {
                    size: extra.size(),
                    origin: (0, 0),
                };
                flush_bufs.push(JxlOutputBuffer::from_image_rect_mut(
                    extra.get_rect_mut(er).into_raw(),
                ));
            }
            let _ = $decoder.flush_pixels(&mut flush_bufs);
            drop(flush_bufs);

            let rgba = if interpret_as_f16 {
                f16_buffer_to_rgba8(&main_buffer, width, height, color_type)
            } else {
                u16_buffer_to_rgba8(&main_buffer, width, height, color_type)
            };
            on_progress(ProgressiveUpdate {
                pixels: rgba,
                width: width as u32,
                height: height as u32,
                completed_passes: $passes,
                progress_pct: $pct,
                is_final: $is_final,
            });
        }};
    }

    let mut sent_lf = false;
    let mut decoder_with_frame = loop {
        let end = (consumed + chunk_size).min(file_size);
        let mut chunk = &data[consumed..end];
        let chunk_len = chunk.len();

        let result = decoder_with_info.process(&mut chunk);
        consumed += chunk_len - chunk.len();

        match result.map_err(|e| e.to_string())? {
            ProcessingResult::Complete { result } => break result,
            ProcessingResult::NeedsMoreInput { mut fallback, .. } => {
                if !sent_lf {
                    flush_and_send!(fallback, 0, consumed * 100 / file_size, false);
                    sent_lf = true;
                }
                if consumed >= file_size {
                    return Err("Incomplete frame header".to_string());
                }
                if let Some(delay) = slow_delay {
                    thread::sleep(delay);
                }
                decoder_with_info = fallback;
            }
        }
    };

    let mut last_passes = 0usize;
    let mut last_flush_pct = 0usize;
    let flush_interval_pct: usize = if settings.simulate_slow { 1 } else { 5 };

    loop {
        let mut output_bufs = vec![JxlOutputBuffer::from_image_rect_mut(
            main_buffer.get_rect_mut(rect).into_raw(),
        )];
        for extra in &mut extra_bufs {
            let er = Rect {
                size: extra.size(),
                origin: (0, 0),
            };
            output_bufs.push(JxlOutputBuffer::from_image_rect_mut(
                extra.get_rect_mut(er).into_raw(),
            ));
        }

        let end = (consumed + chunk_size).min(file_size);
        let mut chunk = &data[consumed..end];
        let chunk_len = chunk.len();

        let result = decoder_with_frame.process(&mut chunk, &mut output_bufs);
        consumed += chunk_len - chunk.len();
        drop(output_bufs);

        match result.map_err(|e| e.to_string())? {
            ProcessingResult::Complete { .. } => break,
            ProcessingResult::NeedsMoreInput { mut fallback, .. } => {
                let pct = consumed * 100 / file_size.max(1);
                let passes = fallback.num_completed_passes();
                let pass_changed = passes > last_passes;
                let interval_hit = pct >= last_flush_pct + flush_interval_pct;

                if pass_changed || interval_hit {
                    if pass_changed {
                        last_passes = passes;
                    }

                    let send_pixels = settings.simulate_slow || pass_changed || pct <= 60;

                    if send_pixels {
                        flush_and_send!(fallback, passes, pct, false);
                    } else {
                        on_progress(ProgressiveUpdate {
                            pixels: Vec::new(),
                            width: width as u32,
                            height: height as u32,
                            completed_passes: passes,
                            progress_pct: pct,
                            is_final: false,
                        });
                    }

                    last_flush_pct = pct;
                }

                if consumed >= file_size {
                    return Err("Incomplete frame data".to_string());
                }
                if let Some(delay) = slow_delay {
                    thread::sleep(delay);
                }
                decoder_with_frame = fallback;
            }
        }
    }

    let rgba = if interpret_as_f16 {
        f16_buffer_to_rgba8(&main_buffer, width, height, color_type)
    } else {
        u16_buffer_to_rgba8(&main_buffer, width, height, color_type)
    };
    on_progress(ProgressiveUpdate {
        pixels: rgba.clone(),
        width: width as u32,
        height: height as u32,
        completed_passes: last_passes,
        progress_pct: 100,
        is_final: true,
    });

    Ok(DecodedImage {
        pixels: rgba,
        width: width as u32,
        height: height as u32,
    })
}

// ---------------------------------------------------------------------------
// C FFI (for iOS / Swift)
// ---------------------------------------------------------------------------

#[repr(C)]
pub struct JxlImage {
    pub pixels: *mut u8,
    pub width: u32,
    pub height: u32,
    pub pixels_len: u32,
}

#[repr(C)]
pub struct JxlAnimationResult {
    pub frames: *mut JxlAnimFrame,
    pub frame_count: u32,
    pub width: u32,
    pub height: u32,
    pub loop_count: u32,
}

#[repr(C)]
pub struct JxlAnimFrame {
    pub pixels: *mut u8,
    pub pixels_len: u32,
    pub width: u32,
    pub height: u32,
    pub duration_ms: u32,
}

fn box_image(img: DecodedImage) -> *mut JxlImage {
    let mut pixels = img.pixels.into_boxed_slice();
    let ptr = pixels.as_mut_ptr();
    let len = pixels.len() as u32;
    std::mem::forget(pixels);

    Box::into_raw(Box::new(JxlImage {
        pixels: ptr,
        width: img.width,
        height: img.height,
        pixels_len: len,
    }))
}

#[no_mangle]
pub extern "C" fn jxl_decode(data: *const u8, data_len: usize) -> *mut JxlImage {
    if data.is_null() || data_len == 0 {
        return std::ptr::null_mut();
    }
    let slice = unsafe { std::slice::from_raw_parts(data, data_len) };
    match decode_jxl_to_rgba(slice) {
        Ok(img) => box_image(img),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "C" fn jxl_decode_with_settings(
    data: *const u8,
    data_len: usize,
    color_type: u8,
    data_type: u8,
    premultiply_alpha: u8,
    linear_output: u8,
    high_precision: u8,
) -> *mut JxlImage {
    if data.is_null() || data_len == 0 {
        return std::ptr::null_mut();
    }
    let slice = unsafe { std::slice::from_raw_parts(data, data_len) };
    let settings = MobileSettings {
        color_type,
        data_type,
        premultiply_alpha: premultiply_alpha != 0,
        linear_output: linear_output != 0,
        high_precision: high_precision != 0,
        simulate_slow: false,
        slow_chunk_pct: 1.0,
        slow_delay_ms: 0,
    };

    match decode_jxl_with_settings(slice, &settings) {
        Ok(img) => box_image(img),
        Err(_) => std::ptr::null_mut(),
    }
}

type JxlProgressCallback = extern "C" fn(
    pixels: *const u8,
    pixels_len: u32,
    width: u32,
    height: u32,
    completed_passes: u32,
    progress_pct: u32,
    is_final: u8,
    user_data: *mut std::ffi::c_void,
);

#[no_mangle]
pub extern "C" fn jxl_decode_progressive(
    data: *const u8,
    data_len: usize,
    color_type: u8,
    data_type: u8,
    premultiply_alpha: u8,
    linear_output: u8,
    high_precision: u8,
    simulate_slow: u8,
    slow_chunk_pct: f32,
    slow_delay_ms: u64,
    callback: Option<JxlProgressCallback>,
    user_data: *mut std::ffi::c_void,
) -> *mut JxlImage {
    if data.is_null() || data_len == 0 {
        return std::ptr::null_mut();
    }
    let slice = unsafe { std::slice::from_raw_parts(data, data_len) };
    let settings = MobileSettings {
        color_type,
        data_type,
        premultiply_alpha: premultiply_alpha != 0,
        linear_output: linear_output != 0,
        high_precision: high_precision != 0,
        simulate_slow: simulate_slow != 0,
        slow_chunk_pct,
        slow_delay_ms,
    };

    let decoded = decode_jxl_progressive(slice, &settings, |update| {
        if let Some(cb) = callback {
            cb(
                update.pixels.as_ptr(),
                update.pixels.len() as u32,
                update.width,
                update.height,
                update.completed_passes as u32,
                update.progress_pct as u32,
                if update.is_final { 1 } else { 0 },
                user_data,
            );
        }
    });

    match decoded {
        Ok(img) => box_image(img),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "C" fn jxl_is_animation(data: *const u8, data_len: usize) -> u8 {
    if data.is_null() || data_len == 0 {
        return 0;
    }
    let slice = unsafe { std::slice::from_raw_parts(data, data_len) };
    if is_jxl_animation(slice) {
        1
    } else {
        0
    }
}

#[no_mangle]
pub extern "C" fn jxl_decode_animation(
    data: *const u8,
    data_len: usize,
) -> *mut JxlAnimationResult {
    if data.is_null() || data_len == 0 {
        return std::ptr::null_mut();
    }
    let slice = unsafe { std::slice::from_raw_parts(data, data_len) };
    match decode_jxl_animation(slice) {
        Ok(anim) => {
            let mut c_frames: Vec<JxlAnimFrame> = anim
                .frames
                .into_iter()
                .map(|f| {
                    let mut pixels = f.pixels.into_boxed_slice();
                    let ptr = pixels.as_mut_ptr();
                    let len = pixels.len() as u32;
                    std::mem::forget(pixels);
                    JxlAnimFrame {
                        pixels: ptr,
                        pixels_len: len,
                        width: f.width,
                        height: f.height,
                        duration_ms: f.duration_ms,
                    }
                })
                .collect();
            let frame_count = c_frames.len() as u32;
            let frames_ptr = c_frames.as_mut_ptr();
            std::mem::forget(c_frames);

            Box::into_raw(Box::new(JxlAnimationResult {
                frames: frames_ptr,
                frame_count,
                width: anim.width,
                height: anim.height,
                loop_count: anim.loop_count,
            }))
        }
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "C" fn jxl_animation_free(anim: *mut JxlAnimationResult) {
    if !anim.is_null() {
        unsafe {
            let anim = Box::from_raw(anim);
            let frames = Vec::from_raw_parts(
                anim.frames,
                anim.frame_count as usize,
                anim.frame_count as usize,
            );
            for f in frames {
                if !f.pixels.is_null() {
                    let _ =
                        Vec::from_raw_parts(f.pixels, f.pixels_len as usize, f.pixels_len as usize);
                }
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn jxl_image_free(img: *mut JxlImage) {
    if !img.is_null() {
        unsafe {
            let img = Box::from_raw(img);
            if !img.pixels.is_null() {
                let _ = Vec::from_raw_parts(
                    img.pixels,
                    img.pixels_len as usize,
                    img.pixels_len as usize,
                );
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn jxl_decode_with_error(
    data: *const u8,
    data_len: usize,
    error_buf: *mut u8,
    error_buf_len: usize,
) -> *mut JxlImage {
    if data.is_null() || data_len == 0 {
        write_error(error_buf, error_buf_len, "Null or empty input");
        return std::ptr::null_mut();
    }
    let slice = unsafe { std::slice::from_raw_parts(data, data_len) };
    match decode_jxl_to_rgba(slice) {
        Ok(img) => box_image(img),
        Err(e) => {
            write_error(error_buf, error_buf_len, &e);
            std::ptr::null_mut()
        }
    }
}

fn write_error(buf: *mut u8, buf_len: usize, msg: &str) {
    if buf.is_null() || buf_len == 0 {
        return;
    }
    let bytes = msg.as_bytes();
    let copy_len = bytes.len().min(buf_len - 1);
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf, copy_len);
        *buf.add(copy_len) = 0;
    }
}

// ---------------------------------------------------------------------------
// JNI (for Android / Kotlin)
// ---------------------------------------------------------------------------

#[cfg(feature = "android")]
mod android {
    use super::*;
    use jni::objects::{JByteArray, JClass, JObject, JValue};
    use jni::sys::jobject;
    use jni::JNIEnv;

    /// Standard single-frame decode
    #[no_mangle]
    pub extern "system" fn Java_com_jxlui_JxlDecoder_nativeDecode<'a>(
        mut env: JNIEnv<'a>,
        _class: JClass<'a>,
        data: JByteArray<'a>,
    ) -> jobject {
        let bytes = match env.convert_byte_array(&data) {
            Ok(b) => b,
            Err(_) => return JObject::null().into_raw(),
        };

        let decoded = match decode_jxl_to_rgba(&bytes) {
            Ok(img) => img,
            Err(_) => return JObject::null().into_raw(),
        };

        create_decoded_image_object(&mut env, &decoded)
    }

    /// Decode with full settings (color_type, data_type, premultiply, linear, high_precision)
    #[no_mangle]
    pub extern "system" fn Java_com_jxlui_JxlDecoder_nativeDecodeWithSettings<'a>(
        mut env: JNIEnv<'a>,
        _class: JClass<'a>,
        data: JByteArray<'a>,
        color_type: i32,
        data_type: i32,
        premultiply_alpha: u8,
        linear_output: u8,
        high_precision: u8,
    ) -> jobject {
        let bytes = match env.convert_byte_array(&data) {
            Ok(b) => b,
            Err(_) => return JObject::null().into_raw(),
        };

        let settings = MobileSettings {
            color_type: color_type as u8,
            data_type: data_type as u8,
            premultiply_alpha: premultiply_alpha != 0,
            linear_output: linear_output != 0,
            high_precision: high_precision != 0,
            simulate_slow: false,
            slow_chunk_pct: 1.0,
            slow_delay_ms: 50,
        };

        let decoded = match decode_jxl_with_settings(&bytes, &settings) {
            Ok(img) => img,
            Err(_) => return JObject::null().into_raw(),
        };

        create_decoded_image_object(&mut env, &decoded)
    }

    /// Check if data is animation
    #[no_mangle]
    pub extern "system" fn Java_com_jxlui_JxlDecoder_nativeIsAnimation<'a>(
        env: JNIEnv<'a>,
        _class: JClass<'a>,
        data: JByteArray<'a>,
    ) -> u8 {
        let bytes = match env.convert_byte_array(&data) {
            Ok(b) => b,
            Err(_) => return 0,
        };
        if is_jxl_animation(&bytes) {
            1
        } else {
            0
        }
    }

    /// Decode animation, returns array of DecodedImage with durations
    #[no_mangle]
    pub extern "system" fn Java_com_jxlui_JxlDecoder_nativeDecodeAnimation<'a>(
        mut env: JNIEnv<'a>,
        _class: JClass<'a>,
        data: JByteArray<'a>,
    ) -> jobject {
        let bytes = match env.convert_byte_array(&data) {
            Ok(b) => b,
            Err(_) => return JObject::null().into_raw(),
        };

        let anim = match decode_jxl_animation(&bytes) {
            Ok(a) => a,
            Err(_) => return JObject::null().into_raw(),
        };

        // Create ArrayList<AnimFrame>
        let list_class = match env.find_class("java/util/ArrayList") {
            Ok(c) => c,
            Err(_) => return JObject::null().into_raw(),
        };
        let list = match env.new_object(&list_class, "()V", &[]) {
            Ok(o) => o,
            Err(_) => return JObject::null().into_raw(),
        };

        let frame_class = match env.find_class("com/jxlui/AnimFrame") {
            Ok(c) => c,
            Err(_) => return JObject::null().into_raw(),
        };

        for frame in &anim.frames {
            let pixel_array = match env.byte_array_from_slice(&frame.pixels) {
                Ok(a) => a,
                Err(_) => continue,
            };

            let frame_obj = match env.new_object(
                &frame_class,
                "([BIII)V",
                &[
                    JValue::Object(&pixel_array.into()),
                    JValue::Int(frame.width as i32),
                    JValue::Int(frame.height as i32),
                    JValue::Int(frame.duration_ms as i32),
                ],
            ) {
                Ok(o) => o,
                Err(_) => continue,
            };

            let _ = env.call_method(
                &list,
                "add",
                "(Ljava/lang/Object;)Z",
                &[JValue::Object(&frame_obj)],
            );
        }

        list.into_raw()
    }

    /// Progressive decode with callback (full settings)
    #[no_mangle]
    pub extern "system" fn Java_com_jxlui_JxlDecoder_nativeDecodeProgressive<'a>(
        mut env: JNIEnv<'a>,
        _class: JClass<'a>,
        data: JByteArray<'a>,
        color_type: i32,
        data_type: i32,
        premultiply_alpha: u8,
        linear_output: u8,
        high_precision: u8,
        simulate_slow: u8,
        slow_chunk_pct: f32,
        slow_delay_ms: i64,
        listener: JObject<'a>,
    ) -> jobject {
        let bytes = match env.convert_byte_array(&data) {
            Ok(b) => b,
            Err(_) => return JObject::null().into_raw(),
        };

        let settings = MobileSettings {
            color_type: color_type as u8,
            data_type: data_type as u8,
            premultiply_alpha: premultiply_alpha != 0,
            linear_output: linear_output != 0,
            high_precision: high_precision != 0,
            simulate_slow: simulate_slow != 0,
            slow_chunk_pct,
            slow_delay_ms: slow_delay_ms as u64,
        };

        let listener_global = match env.new_global_ref(&listener) {
            Ok(g) => g,
            Err(_) => return JObject::null().into_raw(),
        };

        let jvm = match env.get_java_vm() {
            Ok(vm) => vm,
            Err(_) => return JObject::null().into_raw(),
        };

        // Rust-side already throttles to pass boundaries + 5% intervals.
        // Just forward every callback to Java, deleting local refs to avoid OOM.
        let decoded = decode_jxl_progressive(&bytes, &settings, |update| {
            if let Ok(mut cb_env) = jvm.attach_current_thread() {
                if let Ok(pixel_array) = cb_env.byte_array_from_slice(&update.pixels) {
                    let pixel_obj: JObject = pixel_array.into();
                    let _ = cb_env.call_method(
                        &listener_global,
                        "onProgress",
                        "([BIIII)V",
                        &[
                            JValue::Object(&pixel_obj),
                            JValue::Int(update.width as i32),
                            JValue::Int(update.height as i32),
                            JValue::Int(update.completed_passes as i32),
                            JValue::Int(update.progress_pct as i32),
                        ],
                    );
                    let _ = cb_env.delete_local_ref(pixel_obj);
                }
            }
        });

        match decoded {
            Ok(img) => create_decoded_image_object(&mut env, &img),
            Err(_) => JObject::null().into_raw(),
        }
    }

    fn create_decoded_image_object(env: &mut JNIEnv, decoded: &DecodedImage) -> jobject {
        let pixel_array = match env.byte_array_from_slice(&decoded.pixels) {
            Ok(a) => a,
            Err(_) => return JObject::null().into_raw(),
        };

        let class = match env.find_class("com/jxlui/DecodedImage") {
            Ok(c) => c,
            Err(_) => return JObject::null().into_raw(),
        };

        match env.new_object(
            class,
            "([BII)V",
            &[
                JValue::Object(&pixel_array.into()),
                JValue::Int(decoded.width as i32),
                JValue::Int(decoded.height as i32),
            ],
        ) {
            Ok(obj) => obj.into_raw(),
            Err(_) => JObject::null().into_raw(),
        }
    }
}
