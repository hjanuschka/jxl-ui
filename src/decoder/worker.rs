use super::{DecodeResult, DecodedFrame, ImageMetadata};
use super::rgb_conversion::jxl_to_rgba8;
use anyhow::Result;
use jxl::api::{
    states::WithImageInfo,
    JxlAnimation, JxlBitDepth, JxlColorType, JxlDataFormat, JxlDecoder, JxlDecoderOptions,
    JxlOutputBuffer, JxlPixelFormat, ProcessingResult,
};
use jxl::image::{Image, Rect};
use std::fs::File;
use std::io::BufReader;
use std::panic;
use std::path::Path;
use std::time::Instant;

/// Convert our DecoderSettings to jxl-rs JxlPixelFormat
fn settings_to_pixel_format(
    settings: &super::DecoderSettings,
    num_extra_channels: usize,
    native_color_type: JxlColorType,
) -> Option<JxlPixelFormat> {
    use super::{OutputColorType, OutputDataType};

    let color_type = match settings.color_type {
        OutputColorType::Auto => native_color_type,
        OutputColorType::Rgb => JxlColorType::Rgb,
        OutputColorType::Rgba => JxlColorType::Rgba,
        OutputColorType::Bgr => JxlColorType::Bgr,
        OutputColorType::Bgra => JxlColorType::Bgra,
        OutputColorType::Grayscale => JxlColorType::Grayscale,
        OutputColorType::GrayscaleAlpha => JxlColorType::GrayscaleAlpha,
    };

    let data_format = match settings.data_type {
        OutputDataType::U8 => Some(JxlDataFormat::U8 { bit_depth: 8 }),
        OutputDataType::U16 => Some(JxlDataFormat::U16 {
            endianness: jxl::api::Endianness::native(),
            bit_depth: 16,
        }),
        OutputDataType::F16 => Some(JxlDataFormat::F16 {
            endianness: jxl::api::Endianness::native(),
        }),
        OutputDataType::F32 => Some(JxlDataFormat::f32()),
    };

    Some(JxlPixelFormat {
        color_type,
        color_data_format: data_format.clone(),
        extra_channel_format: vec![data_format; num_extra_channels],
    })
}

/// Get bytes per sample for a data format
fn bytes_per_sample(data_type: &super::OutputDataType) -> usize {
    use super::OutputDataType;
    match data_type {
        OutputDataType::U8 => 1,
        OutputDataType::U16 | OutputDataType::F16 => 2,
        OutputDataType::F32 => 4,
    }
}

/// Split interleaved RGB channels into separate planar channels
fn split_rgb_channels(
    interleaved: &Image<f32>,
    width: usize,
    height: usize,
) -> (Image<f32>, Image<f32>, Image<f32>) {
    let mut r = Image::<f32>::new((width, height)).unwrap();
    let mut g = Image::<f32>::new((width, height)).unwrap();
    let mut b = Image::<f32>::new((width, height)).unwrap();

    for y in 0..height {
        let interleaved_row = interleaved.row(y);
        let r_row = r.row_mut(y);
        let g_row = g.row_mut(y);
        let b_row = b.row_mut(y);

        for x in 0..width {
            let interleaved_idx = x * 3;
            r_row[x] = interleaved_row[interleaved_idx];
            g_row[x] = interleaved_row[interleaved_idx + 1];
            b_row[x] = interleaved_row[interleaved_idx + 2];
        }
    }

    (r, g, b)
}

/// Unified decode function that automatically handles both single frames and animations
pub fn decode_jxl<P: AsRef<Path>>(path: P) -> Result<DecodeResult> {
    let start = Instant::now();

    log::info!("Opening JXL file: {:?}", path.as_ref());

    let file = File::open(path.as_ref())?;
    let mut reader = BufReader::new(file);

    // Set up decoder options
    let mut options = JxlDecoderOptions::default();
    options.adjust_orientation = true;
    options.coalescing = true; // Blend frames for animation
    options.premultiply_output = true; // Premultiply alpha for better compositing

    log::info!("Creating JXL decoder...");
    let decoder = JxlDecoder::new(options);

    // Get image info
    let decoder_with_info = match decoder.process(&mut reader)? {
        ProcessingResult::Complete { result } => result,
        ProcessingResult::NeedsMoreInput { .. } => {
            anyhow::bail!("Unexpected NeedsMoreInput during header decode");
        }
    };

    let basic_info = decoder_with_info.basic_info();
    let (width, height) = basic_info.size;
    let extra_channels_count = basic_info.extra_channels.len();
    let bit_depth = basic_info.bit_depth.clone();
    let animation = basic_info.animation.clone();

    // Get the color type from the decoder's pixel format
    let pixel_format = decoder_with_info.current_pixel_format();
    let color_type = pixel_format.color_type;

    log::info!(
        "Image info: {}x{}, color type: {:?}, animation: {:?}",
        width,
        height,
        color_type,
        animation
    );

    // Check if this is an animation
    let is_animated = animation.is_some();

    if !is_animated {
        // Single frame - use existing logic
        let (frame, metadata) = decode_single_frame_from_decoder(
            decoder_with_info,
            &mut reader,
            width,
            height,
            color_type,
            extra_channels_count,
            &bit_depth,
            &animation,
            start,
        )?;

        return Ok(DecodeResult::SingleFrame { frame, metadata });
    }

    // Animation - decode all frames
    log::info!("Detected animation, decoding all frames...");

    let mut frames = Vec::new();
    let mut decoder = decoder_with_info;
    let mut frame_index = 0;

    loop {
        let frame_start = Instant::now();

        // Get frame info - catch panic if we've reached the end of frames
        // The JXL decoder panics with "assertion failed: self.has_more_frames"
        // when trying to decode beyond the last frame
        let decoder_with_frame = match panic::catch_unwind(panic::AssertUnwindSafe(|| decoder.process(&mut reader))) {
            Ok(Ok(ProcessingResult::Complete { result })) => result,
            Ok(Ok(ProcessingResult::NeedsMoreInput { .. })) => {
                // No more frames
                log::info!("Decoded {} frames total", frames.len());
                break;
            }
            Ok(Err(e)) => {
                // Decoding error
                return Err(e.into());
            }
            Err(_) => {
                // Panic caught - likely end of animation
                log::info!("Decoded {} frames total (end of animation detected)", frames.len());
                break;
            }
        };

        let frame_header = decoder_with_frame.frame_header();
        let raw_duration = frame_header.duration.unwrap_or(100.0); // Raw duration value from JXL

        // It appears jxl-rs returns duration already in milliseconds
        // Just need to ensure it's reasonable and convert to u32
        let duration_ms = raw_duration as u32;
        let duration_ms = duration_ms.max(16); // Minimum 16ms (60fps)

        log::info!("Decoding frame {} (duration: {}ms)...", frame_index, duration_ms);

        // Determine samples per pixel
        let samples_per_pixel = match color_type {
            JxlColorType::Grayscale => 1,
            JxlColorType::GrayscaleAlpha => 1,
            JxlColorType::Rgb | JxlColorType::Bgr => 3,
            JxlColorType::Rgba | JxlColorType::Bgra => 3,
        };

        // Create output buffers
        let mut main_channel = Image::<f32>::new((width * samples_per_pixel, height))?;
        let mut extra_channel_buffers: Vec<Image<f32>> = (0..extra_channels_count)
            .map(|_| Image::<f32>::new((width, height)))
            .collect::<Result<Vec<_>, _>>()?;

        let rect = Rect {
            size: main_channel.size(),
            origin: (0, 0),
        };

        let mut output_bufs = vec![JxlOutputBuffer::from_image_rect_mut(
            main_channel.get_rect_mut(rect).into_raw(),
        )];
        for extra in &mut extra_channel_buffers {
            let extra_rect = Rect {
                size: extra.size(),
                origin: (0, 0),
            };
            output_bufs.push(JxlOutputBuffer::from_image_rect_mut(
                extra.get_rect_mut(extra_rect).into_raw(),
            ));
        }

        // Decode the frame
        decoder = match decoder_with_frame.process(&mut reader, &mut output_bufs)? {
            ProcessingResult::Complete { result } => result,
            ProcessingResult::NeedsMoreInput { .. } => {
                anyhow::bail!("Unexpected NeedsMoreInput during frame decode");
            }
        };

        let decode_time = frame_start.elapsed();

        // Prepare channels for RGB conversion
        let mut channels = Vec::new();
        match color_type {
            JxlColorType::Grayscale => {
                channels.push(main_channel);
            }
            JxlColorType::GrayscaleAlpha => {
                channels.push(main_channel);
                if !extra_channel_buffers.is_empty() {
                    channels.push(extra_channel_buffers.remove(0));
                }
            }
            JxlColorType::Rgb | JxlColorType::Bgr => {
                let (r, g, b) = split_rgb_channels(&main_channel, width, height);
                channels.push(r);
                channels.push(g);
                channels.push(b);
                // Add alpha channel if present as extra channel
                if !extra_channel_buffers.is_empty() {
                    channels.push(extra_channel_buffers.remove(0));
                }
            }
            JxlColorType::Rgba | JxlColorType::Bgra => {
                let (r, g, b) = split_rgb_channels(&main_channel, width, height);
                channels.push(r);
                channels.push(g);
                channels.push(b);
                if !extra_channel_buffers.is_empty() {
                    channels.push(extra_channel_buffers.remove(0));
                }
            }
        }

        // Convert to RGBA8
        let rgba_data = jxl_to_rgba8(&channels, color_type, width, height);

        let frame = DecodedFrame {
            rgba_data,
            width: width as u32,
            height: height as u32,
            decode_time,
            duration_ms,
        };

        frames.push(frame);
        frame_index += 1;
    }

    let total_time = start.elapsed();
    log::info!("Decoded all {} frames in {:?}", frames.len(), total_time);

    // Create metadata
    let metadata = ImageMetadata {
        width: width as u32,
        height: height as u32,
        bit_depth: format_bit_depth(&bit_depth),
        has_animation: true,
        frame_count: frames.len(),
        loop_count: animation.as_ref().map(|a| a.num_loops).unwrap_or(0),
    };

    Ok(DecodeResult::Animation { frames, metadata })
}

/// Helper function to decode a single frame from an existing decoder
fn decode_single_frame_from_decoder(
    decoder_with_info: JxlDecoder<WithImageInfo>,
    reader: &mut BufReader<File>,
    width: usize,
    height: usize,
    color_type: JxlColorType,
    extra_channels_count: usize,
    bit_depth: &JxlBitDepth,
    animation: &Option<JxlAnimation>,
    start: Instant,
) -> Result<(DecodedFrame, ImageMetadata)> {
    // Get frame info
    let decoder_with_frame = match decoder_with_info.process(reader)? {
        ProcessingResult::Complete { result } => result,
        ProcessingResult::NeedsMoreInput { .. } => {
            anyhow::bail!("Unexpected NeedsMoreInput during frame header decode");
        }
    };

    log::info!("Color type: {:?}, extra channels: {}", color_type, extra_channels_count);

    // Determine samples per pixel
    let samples_per_pixel = match color_type {
        JxlColorType::Grayscale => 1,
        JxlColorType::GrayscaleAlpha => 1,
        JxlColorType::Rgb | JxlColorType::Bgr => 3,
        JxlColorType::Rgba | JxlColorType::Bgra => 3,
    };

    // Create output buffers
    let mut main_channel = Image::<f32>::new((width * samples_per_pixel, height))?;
    let mut extra_channel_buffers: Vec<Image<f32>> = (0..extra_channels_count)
        .map(|_| Image::<f32>::new((width, height)))
        .collect::<Result<Vec<_>, _>>()?;

    let rect = Rect {
        size: main_channel.size(),
        origin: (0, 0),
    };

    let mut output_bufs = vec![JxlOutputBuffer::from_image_rect_mut(
        main_channel.get_rect_mut(rect).into_raw(),
    )];
    for extra in &mut extra_channel_buffers {
        let extra_rect = Rect {
            size: extra.size(),
            origin: (0, 0),
        };
        output_bufs.push(JxlOutputBuffer::from_image_rect_mut(
            extra.get_rect_mut(extra_rect).into_raw(),
        ));
    }

    // Decode the frame
    log::info!("Decoding frame...");
    let _decoder_with_info = match decoder_with_frame.process(reader, &mut output_bufs)? {
        ProcessingResult::Complete { result } => result,
        ProcessingResult::NeedsMoreInput { .. } => {
            anyhow::bail!("Unexpected NeedsMoreInput during frame decode");
        }
    };

    let decode_time = start.elapsed();
    log::info!("Decoded in {:?}", decode_time);

    // Prepare channels for RGB conversion
    let mut channels = Vec::new();
    match color_type {
        JxlColorType::Grayscale => {
            channels.push(main_channel);
        }
        JxlColorType::GrayscaleAlpha => {
            channels.push(main_channel);
            if !extra_channel_buffers.is_empty() {
                channels.push(extra_channel_buffers.remove(0));
            }
        }
        JxlColorType::Rgb | JxlColorType::Bgr => {
            let (r, g, b) = split_rgb_channels(&main_channel, width, height);
            channels.push(r);
            channels.push(g);
            channels.push(b);
            // Add alpha channel if present as extra channel
            if !extra_channel_buffers.is_empty() {
                channels.push(extra_channel_buffers.remove(0));
            }
        }
        JxlColorType::Rgba | JxlColorType::Bgra => {
            let (r, g, b) = split_rgb_channels(&main_channel, width, height);
            channels.push(r);
            channels.push(g);
            channels.push(b);
            if !extra_channel_buffers.is_empty() {
                channels.push(extra_channel_buffers.remove(0));
            }
        }
    }

    // Convert to RGBA8
    let rgba_data = jxl_to_rgba8(&channels, color_type, width, height);

    // Create metadata
    let metadata = ImageMetadata {
        width: width as u32,
        height: height as u32,
        bit_depth: format_bit_depth(bit_depth),
        has_animation: animation.is_some(),
        frame_count: 1,
        loop_count: animation.as_ref().map(|a| a.num_loops).unwrap_or(0),
    };

    let frame = DecodedFrame {
        rgba_data,
        width: width as u32,
        height: height as u32,
        decode_time,
        duration_ms: 0,
    };

    Ok((frame, metadata))
}

/// Format bit depth for display
fn format_bit_depth(bit_depth: &JxlBitDepth) -> String {
    match bit_depth {
        JxlBitDepth::Int { bits_per_sample } => format!("{}-bit int", bits_per_sample),
        JxlBitDepth::Float {
            bits_per_sample,
            exponent_bits_per_sample,
        } => format!(
            "{}-bit float (exp: {})",
            bits_per_sample, exponent_bits_per_sample
        ),
    }
}

/// Progressive decode function that calls a callback for each completed pass.
/// This enables displaying partial results as the image decodes.
///
/// The callback receives a `ProgressiveUpdate` each time new passes are completed.
/// Uses chunked input to enable true streaming progressive rendering with
/// flush_pixels() for real progressive display of partially decoded data.
pub fn decode_jxl_progressive<P, F>(
    path: P,
    settings: &super::DecoderSettings,
    mut on_progress: F,
) -> Result<DecodeResult>
where
    P: AsRef<Path>,
    F: FnMut(super::ProgressiveUpdate),
{
    use super::{OutputDataType, ProgressiveUpdate};
    use std::io::Read;
    use std::thread;

    let start = Instant::now();
    log::info!("Progressive decode: Opening JXL file: {:?}", path.as_ref());
    log::info!("Progressive decode: Settings: {:?}", settings);

    // Read file into memory for chunked processing
    let mut file = File::open(path.as_ref())?;
    let file_size = file.metadata()?.len() as usize;
    let mut file_data = Vec::with_capacity(file_size);
    file.read_to_end(&mut file_data)?;

    // Use chunks for progressive decoding - smaller chunks = more frequent updates
    let slow_delay = if settings.simulate_slow {
        Some(std::time::Duration::from_millis(settings.slow_delay_ms))
    } else {
        None
    };
    // In slow mode, chunk size is a percentage of file size (min 1KB)
    // In normal mode, use 16KB fixed chunks
    let chunk_size = if settings.simulate_slow {
        ((file_size as f32 * settings.slow_chunk_pct / 100.0) as usize).max(1024)
    } else {
        16 * 1024
    };
    let mut input = &file_data[..];
    let mut chunk_input = &input[0..0];

    // Set up decoder options from settings
    let mut options = JxlDecoderOptions::default();
    options.adjust_orientation = true;
    options.coalescing = true;
    options.premultiply_output = settings.premultiply_alpha;
    options.high_precision = settings.high_precision;

    log::info!("Progressive decode: Creating JXL decoder, file size: {} bytes", file_size);
    let mut decoder = JxlDecoder::new(options);

    // Helper macro to advance decoder with chunked input (no flushing)
    macro_rules! advance_decoder {
        ($decoder:ident $(, $extra_arg:expr)?) => {{
            loop {
                // Expand available input by chunk_size
                chunk_input = &input[..(chunk_input.len().saturating_add(chunk_size)).min(input.len())];
                let available_before = chunk_input.len();

                let process_result = $decoder.process(&mut chunk_input $(, $extra_arg)?);

                // Update input pointer (consumed bytes)
                input = &input[(available_before - chunk_input.len())..];

                match process_result? {
                    ProcessingResult::Complete { result } => break result,
                    ProcessingResult::NeedsMoreInput { fallback, size_hint } => {
                        if input.is_empty() {
                            anyhow::bail!("Unexpected end of input, need {} more bytes", size_hint);
                        }
                        if let Some(delay) = slow_delay {
                            thread::sleep(delay);
                        }
                        $decoder = fallback;
                    }
                }
            }
        }};
    }

    // Process until we have image info
    let mut decoder_with_info = advance_decoder!(decoder);

    let basic_info = decoder_with_info.basic_info();
    let (width, height) = basic_info.size;
    let extra_channels_count = basic_info.extra_channels.len();
    let bit_depth = basic_info.bit_depth.clone();
    let animation = basic_info.animation.clone();
    let native_color_type = decoder_with_info.current_pixel_format().color_type;

    // Apply requested pixel format from settings if not Auto
    // Always set pixel format to ensure buffer type matches data type.
    // When color_type is Auto, we use the native color type but still set the
    // data format so that e.g. U8 buffers get U8 data from the decoder.
    if let Some(requested_format) = settings_to_pixel_format(settings, extra_channels_count, native_color_type) {
        log::info!(
            "Progressive decode: Setting pixel format to {:?} with data format {:?}",
            requested_format.color_type,
            settings.data_type
        );
        decoder_with_info.set_pixel_format(requested_format);
    }

    let pixel_format = decoder_with_info.current_pixel_format();
    let color_type = pixel_format.color_type;

    log::info!(
        "Progressive decode: Image {}x{}, color type: {:?} (native: {:?})",
        width, height, color_type, native_color_type
    );

    // For animations, fall back to regular decode
    if animation.is_some() {
        log::info!("Progressive decode: Animation detected, using standard decode");
        return decode_jxl(path);
    }

    // Determine output samples per pixel based on color type
    let output_samples = color_type.samples_per_pixel();
    let bps = bytes_per_sample(&settings.data_type);

    log::info!(
        "Progressive decode: Output format - {} samples/pixel, {} bytes/sample",
        output_samples, bps
    );

    // Track progress for progressive updates
    let mut last_passes = 0usize;
    let mut last_flush_pct = 0usize;
    // Flush every N% of progress to show tiles filling in within a pass.
    // Smaller interval = more tile updates. In slow mode, flush more often.
    let flush_interval_pct: usize = if slow_delay.is_some() { 2 } else { 5 };

    // Determine samples per pixel for buffer allocation
    let samples_per_pixel = match color_type {
        JxlColorType::Grayscale => 1,
        JxlColorType::GrayscaleAlpha => 2,
        JxlColorType::Rgb | JxlColorType::Bgr => 3,
        JxlColorType::Rgba | JxlColorType::Bgra => 4,
    };

    // Decode based on data type - use appropriately typed buffers
    // The decode loop must be inline because JxlDecoder uses consuming state machine pattern
    //
    // For each data type path:
    // 1. Allocate buffers early (before frame header decode)
    // 2. Parse frame header with flush_pixels() on NeedsMoreInput for LF preview
    // 3. Decode frame data with flush_pixels() on each new pass for progressive display
    let rgba_data = match settings.data_type {
        OutputDataType::F32 => {
            // F32 path - use Image<f32>
            let mut main_buffer = Image::<f32>::new((width * samples_per_pixel, height))?;
            let mut extra_channel_buffers: Vec<Image<f32>> = (0..extra_channels_count)
                .map(|_| Image::<f32>::new((width, height)))
                .collect::<Result<Vec<_>, _>>()?;

            let rect = Rect { size: main_buffer.size(), origin: (0, 0) };

            // Parse frame header with flushing for early LF preview
            let mut sent_lf_preview = false;
            let mut decoder_with_frame = loop {
                chunk_input = &input[..(chunk_input.len().saturating_add(chunk_size)).min(input.len())];
                let available_before = chunk_input.len();
                let process_result = decoder_with_info.process(&mut chunk_input);
                input = &input[(available_before - chunk_input.len())..];

                match process_result? {
                    ProcessingResult::Complete { result } => break result,
                    ProcessingResult::NeedsMoreInput { mut fallback, size_hint } => {
                        // Flush partial pixel data (e.g. LF frame) during header parse
                        // Only send one LF preview to avoid spamming the UI
                        if !input.is_empty() && !sent_lf_preview {
                            let mut flush_bufs = vec![JxlOutputBuffer::from_image_rect_mut(
                                main_buffer.get_rect_mut(rect).into_raw(),
                            )];
                            for extra in &mut extra_channel_buffers {
                                let extra_rect = Rect { size: extra.size(), origin: (0, 0) };
                                flush_bufs.push(JxlOutputBuffer::from_image_rect_mut(
                                    extra.get_rect_mut(extra_rect).into_raw(),
                                ));
                            }
                            let _ = fallback.flush_pixels(&mut flush_bufs);
                            drop(flush_bufs);

                            // Send progressive update with whatever we have
                            let partial_rgba = f32_buffer_to_rgba8(&main_buffer, width, height, color_type)?;
                            // Only send if we have non-zero data (some pixels decoded)
                            if partial_rgba.iter().any(|&b| b != 0) {
                                log::info!("Progressive decode: LF preview available during header parse");
                                on_progress(ProgressiveUpdate {
                                    rgba_data: partial_rgba,
                                    width: width as u32,
                                    height: height as u32,
                                    completed_passes: 0,
                                    total_passes: None,
                                    is_final: false,
                                    elapsed: start.elapsed(),
                                });
                                sent_lf_preview = true;
                            }
                        }

                        if input.is_empty() {
                            anyhow::bail!("Unexpected end of input, need {} more bytes", size_hint);
                        }
                        if let Some(delay) = slow_delay { thread::sleep(delay); }
                        decoder_with_info = fallback;
                    }
                }
            };

            // Progressive frame decode loop with flushing
            loop {
                let mut output_bufs = vec![JxlOutputBuffer::from_image_rect_mut(
                    main_buffer.get_rect_mut(rect).into_raw(),
                )];
                for extra in &mut extra_channel_buffers {
                    let extra_rect = Rect { size: extra.size(), origin: (0, 0) };
                    output_bufs.push(JxlOutputBuffer::from_image_rect_mut(
                        extra.get_rect_mut(extra_rect).into_raw(),
                    ));
                }

                chunk_input = &input[..(chunk_input.len().saturating_add(chunk_size)).min(input.len())];
                let available_before = chunk_input.len();

                let process_result = decoder_with_frame.process(&mut chunk_input, &mut output_bufs);
                input = &input[(available_before - chunk_input.len())..];

                drop(output_bufs);

                match process_result? {
                    ProcessingResult::Complete { result: _ } => break,
                    ProcessingResult::NeedsMoreInput { mut fallback, size_hint } => {
                        let progress_pct = if file_size > 0 { (file_size - input.len()) * 100 / file_size } else { 0 };
                        let current_passes = fallback.num_completed_passes();
                        let pass_changed = current_passes > last_passes;
                        let interval_hit = progress_pct >= last_flush_pct + flush_interval_pct;

                        // Flush on pass boundary OR at regular progress intervals
                        // to show tiles filling in within a pass
                        if pass_changed || interval_hit {
                            if pass_changed {
                                log::info!(
                                    "Progressive decode: Pass {} completed at {}%, flushing pixels",
                                    current_passes, progress_pct
                                );
                                last_passes = current_passes;
                            }

                            let mut flush_bufs = vec![JxlOutputBuffer::from_image_rect_mut(
                                main_buffer.get_rect_mut(rect).into_raw(),
                            )];
                            for extra in &mut extra_channel_buffers {
                                let extra_rect = Rect { size: extra.size(), origin: (0, 0) };
                                flush_bufs.push(JxlOutputBuffer::from_image_rect_mut(
                                    extra.get_rect_mut(extra_rect).into_raw(),
                                ));
                            }
                            fallback.flush_pixels(&mut flush_bufs)?;
                            drop(flush_bufs);

                            let partial_rgba = f32_buffer_to_rgba8(&main_buffer, width, height, color_type)?;
                            on_progress(ProgressiveUpdate {
                                rgba_data: partial_rgba,
                                width: width as u32,
                                height: height as u32,
                                completed_passes: current_passes,
                                total_passes: None,
                                is_final: false,
                                elapsed: start.elapsed(),
                            });

                            last_flush_pct = progress_pct;
                        }

                        if input.is_empty() {
                            anyhow::bail!("Unexpected end of input, need {} more bytes", size_hint);
                        }

                        if let Some(delay) = slow_delay { thread::sleep(delay); }
                        decoder_with_frame = fallback;
                    }
                }
            }

            f32_buffer_to_rgba8(&main_buffer, width, height, color_type)?
        }

        OutputDataType::U8 => {
            // U8 path - use Image<u8>
            let mut main_buffer = Image::<u8>::new((width * samples_per_pixel, height))?;
            let mut extra_channel_buffers: Vec<Image<u8>> = (0..extra_channels_count)
                .map(|_| Image::<u8>::new((width, height)))
                .collect::<Result<Vec<_>, _>>()?;

            let rect = Rect { size: main_buffer.size(), origin: (0, 0) };

            // Parse frame header with flushing
            let mut sent_lf_preview = false;
            let mut decoder_with_frame = loop {
                chunk_input = &input[..(chunk_input.len().saturating_add(chunk_size)).min(input.len())];
                let available_before = chunk_input.len();
                let process_result = decoder_with_info.process(&mut chunk_input);
                input = &input[(available_before - chunk_input.len())..];

                match process_result? {
                    ProcessingResult::Complete { result } => break result,
                    ProcessingResult::NeedsMoreInput { mut fallback, size_hint } => {
                        if !input.is_empty() && !sent_lf_preview {
                            let mut flush_bufs = vec![JxlOutputBuffer::from_image_rect_mut(
                                main_buffer.get_rect_mut(rect).into_raw(),
                            )];
                            for extra in &mut extra_channel_buffers {
                                let extra_rect = Rect { size: extra.size(), origin: (0, 0) };
                                flush_bufs.push(JxlOutputBuffer::from_image_rect_mut(
                                    extra.get_rect_mut(extra_rect).into_raw(),
                                ));
                            }
                            let _ = fallback.flush_pixels(&mut flush_bufs);
                            drop(flush_bufs);

                            let partial_rgba = u8_buffer_to_rgba8(&main_buffer, width, height, color_type)?;
                            if partial_rgba.iter().any(|&b| b != 0) {
                                log::info!("Progressive decode: LF preview available during header parse");
                                on_progress(ProgressiveUpdate {
                                    rgba_data: partial_rgba,
                                    width: width as u32,
                                    height: height as u32,
                                    completed_passes: 0,
                                    total_passes: None,
                                    is_final: false,
                                    elapsed: start.elapsed(),
                                });
                                sent_lf_preview = true;
                            }
                        }

                        if input.is_empty() {
                            anyhow::bail!("Unexpected end of input, need {} more bytes", size_hint);
                        }
                        if let Some(delay) = slow_delay { thread::sleep(delay); }
                        decoder_with_info = fallback;
                    }
                }
            };

            // Progressive frame decode loop with flushing
            loop {
                let mut output_bufs = vec![JxlOutputBuffer::from_image_rect_mut(
                    main_buffer.get_rect_mut(rect).into_raw(),
                )];
                for extra in &mut extra_channel_buffers {
                    let extra_rect = Rect { size: extra.size(), origin: (0, 0) };
                    output_bufs.push(JxlOutputBuffer::from_image_rect_mut(
                        extra.get_rect_mut(extra_rect).into_raw(),
                    ));
                }

                chunk_input = &input[..(chunk_input.len().saturating_add(chunk_size)).min(input.len())];
                let available_before = chunk_input.len();

                let process_result = decoder_with_frame.process(&mut chunk_input, &mut output_bufs);
                input = &input[(available_before - chunk_input.len())..];

                drop(output_bufs);

                match process_result? {
                    ProcessingResult::Complete { result: _ } => break,
                    ProcessingResult::NeedsMoreInput { mut fallback, size_hint } => {
                        let progress_pct = if file_size > 0 { (file_size - input.len()) * 100 / file_size } else { 0 };
                        let current_passes = fallback.num_completed_passes();

                        let pass_changed = current_passes > last_passes;
                        let interval_hit = progress_pct >= last_flush_pct + flush_interval_pct;

                        if pass_changed || interval_hit {
                            if pass_changed {
                                log::info!(
                                    "Progressive decode: Pass {} completed at {}%, flushing pixels",
                                    current_passes, progress_pct
                                );
                                last_passes = current_passes;
                            }

                            let mut flush_bufs = vec![JxlOutputBuffer::from_image_rect_mut(
                                main_buffer.get_rect_mut(rect).into_raw(),
                            )];
                            for extra in &mut extra_channel_buffers {
                                let extra_rect = Rect { size: extra.size(), origin: (0, 0) };
                                flush_bufs.push(JxlOutputBuffer::from_image_rect_mut(
                                    extra.get_rect_mut(extra_rect).into_raw(),
                                ));
                            }
                            fallback.flush_pixels(&mut flush_bufs)?;
                            drop(flush_bufs);

                            let partial_rgba = u8_buffer_to_rgba8(&main_buffer, width, height, color_type)?;
                            on_progress(ProgressiveUpdate {
                                rgba_data: partial_rgba,
                                width: width as u32,
                                height: height as u32,
                                completed_passes: current_passes,
                                total_passes: None,
                                is_final: false,
                                elapsed: start.elapsed(),
                            });

                            last_flush_pct = progress_pct;
                        }

                        if input.is_empty() {
                            anyhow::bail!("Unexpected end of input, need {} more bytes", size_hint);
                        }

                        if let Some(delay) = slow_delay { thread::sleep(delay); }
                        decoder_with_frame = fallback;
                    }
                }
            }

            u8_buffer_to_rgba8(&main_buffer, width, height, color_type)?
        }

        OutputDataType::U16 => {
            // U16 path - use Image<u16>
            let mut main_buffer = Image::<u16>::new((width * samples_per_pixel, height))?;
            let mut extra_channel_buffers: Vec<Image<u16>> = (0..extra_channels_count)
                .map(|_| Image::<u16>::new((width, height)))
                .collect::<Result<Vec<_>, _>>()?;

            let rect = Rect { size: main_buffer.size(), origin: (0, 0) };

            // Parse frame header with flushing
            let mut sent_lf_preview = false;
            let mut decoder_with_frame = loop {
                chunk_input = &input[..(chunk_input.len().saturating_add(chunk_size)).min(input.len())];
                let available_before = chunk_input.len();
                let process_result = decoder_with_info.process(&mut chunk_input);
                input = &input[(available_before - chunk_input.len())..];

                match process_result? {
                    ProcessingResult::Complete { result } => break result,
                    ProcessingResult::NeedsMoreInput { mut fallback, size_hint } => {
                        if !input.is_empty() && !sent_lf_preview {
                            let mut flush_bufs = vec![JxlOutputBuffer::from_image_rect_mut(
                                main_buffer.get_rect_mut(rect).into_raw(),
                            )];
                            for extra in &mut extra_channel_buffers {
                                let extra_rect = Rect { size: extra.size(), origin: (0, 0) };
                                flush_bufs.push(JxlOutputBuffer::from_image_rect_mut(
                                    extra.get_rect_mut(extra_rect).into_raw(),
                                ));
                            }
                            let _ = fallback.flush_pixels(&mut flush_bufs);
                            drop(flush_bufs);

                            let partial_rgba = u16_buffer_to_rgba8(&main_buffer, width, height, color_type)?;
                            if partial_rgba.iter().any(|&b| b != 0) {
                                log::info!("Progressive decode: LF preview available during header parse");
                                on_progress(ProgressiveUpdate {
                                    rgba_data: partial_rgba,
                                    width: width as u32,
                                    height: height as u32,
                                    completed_passes: 0,
                                    total_passes: None,
                                    is_final: false,
                                    elapsed: start.elapsed(),
                                });
                                sent_lf_preview = true;
                            }
                        }

                        if input.is_empty() {
                            anyhow::bail!("Unexpected end of input, need {} more bytes", size_hint);
                        }
                        if let Some(delay) = slow_delay { thread::sleep(delay); }
                        decoder_with_info = fallback;
                    }
                }
            };

            // Progressive frame decode loop with flushing
            loop {
                let mut output_bufs = vec![JxlOutputBuffer::from_image_rect_mut(
                    main_buffer.get_rect_mut(rect).into_raw(),
                )];
                for extra in &mut extra_channel_buffers {
                    let extra_rect = Rect { size: extra.size(), origin: (0, 0) };
                    output_bufs.push(JxlOutputBuffer::from_image_rect_mut(
                        extra.get_rect_mut(extra_rect).into_raw(),
                    ));
                }

                chunk_input = &input[..(chunk_input.len().saturating_add(chunk_size)).min(input.len())];
                let available_before = chunk_input.len();

                let process_result = decoder_with_frame.process(&mut chunk_input, &mut output_bufs);
                input = &input[(available_before - chunk_input.len())..];

                drop(output_bufs);

                match process_result? {
                    ProcessingResult::Complete { result: _ } => break,
                    ProcessingResult::NeedsMoreInput { mut fallback, size_hint } => {
                        let progress_pct = if file_size > 0 { (file_size - input.len()) * 100 / file_size } else { 0 };
                        let current_passes = fallback.num_completed_passes();

                        let pass_changed = current_passes > last_passes;
                        let interval_hit = progress_pct >= last_flush_pct + flush_interval_pct;

                        if pass_changed || interval_hit {
                            if pass_changed {
                                log::info!(
                                    "Progressive decode: Pass {} completed at {}%, flushing pixels",
                                    current_passes, progress_pct
                                );
                                last_passes = current_passes;
                            }

                            let mut flush_bufs = vec![JxlOutputBuffer::from_image_rect_mut(
                                main_buffer.get_rect_mut(rect).into_raw(),
                            )];
                            for extra in &mut extra_channel_buffers {
                                let extra_rect = Rect { size: extra.size(), origin: (0, 0) };
                                flush_bufs.push(JxlOutputBuffer::from_image_rect_mut(
                                    extra.get_rect_mut(extra_rect).into_raw(),
                                ));
                            }
                            fallback.flush_pixels(&mut flush_bufs)?;
                            drop(flush_bufs);

                            let partial_rgba = u16_buffer_to_rgba8(&main_buffer, width, height, color_type)?;
                            on_progress(ProgressiveUpdate {
                                rgba_data: partial_rgba,
                                width: width as u32,
                                height: height as u32,
                                completed_passes: current_passes,
                                total_passes: None,
                                is_final: false,
                                elapsed: start.elapsed(),
                            });

                            last_flush_pct = progress_pct;
                        }

                        if input.is_empty() {
                            anyhow::bail!("Unexpected end of input, need {} more bytes", size_hint);
                        }

                        if let Some(delay) = slow_delay { thread::sleep(delay); }
                        decoder_with_frame = fallback;
                    }
                }
            }

            u16_buffer_to_rgba8(&main_buffer, width, height, color_type)?
        }

        OutputDataType::F16 => {
            // F16 path - use Image<u16> as storage (reinterpreted as f16)
            let mut main_buffer = Image::<u16>::new((width * samples_per_pixel, height))?;
            let mut extra_channel_buffers: Vec<Image<u16>> = (0..extra_channels_count)
                .map(|_| Image::<u16>::new((width, height)))
                .collect::<Result<Vec<_>, _>>()?;

            let rect = Rect { size: main_buffer.size(), origin: (0, 0) };

            // Parse frame header with flushing
            let mut sent_lf_preview = false;
            let mut decoder_with_frame = loop {
                chunk_input = &input[..(chunk_input.len().saturating_add(chunk_size)).min(input.len())];
                let available_before = chunk_input.len();
                let process_result = decoder_with_info.process(&mut chunk_input);
                input = &input[(available_before - chunk_input.len())..];

                match process_result? {
                    ProcessingResult::Complete { result } => break result,
                    ProcessingResult::NeedsMoreInput { mut fallback, size_hint } => {
                        if !input.is_empty() && !sent_lf_preview {
                            let mut flush_bufs = vec![JxlOutputBuffer::from_image_rect_mut(
                                main_buffer.get_rect_mut(rect).into_raw(),
                            )];
                            for extra in &mut extra_channel_buffers {
                                let extra_rect = Rect { size: extra.size(), origin: (0, 0) };
                                flush_bufs.push(JxlOutputBuffer::from_image_rect_mut(
                                    extra.get_rect_mut(extra_rect).into_raw(),
                                ));
                            }
                            let _ = fallback.flush_pixels(&mut flush_bufs);
                            drop(flush_bufs);

                            let partial_rgba = f16_buffer_to_rgba8(&main_buffer, width, height, color_type)?;
                            if partial_rgba.iter().any(|&b| b != 0) {
                                log::info!("Progressive decode: LF preview available during header parse");
                                on_progress(ProgressiveUpdate {
                                    rgba_data: partial_rgba,
                                    width: width as u32,
                                    height: height as u32,
                                    completed_passes: 0,
                                    total_passes: None,
                                    is_final: false,
                                    elapsed: start.elapsed(),
                                });
                                sent_lf_preview = true;
                            }
                        }

                        if input.is_empty() {
                            anyhow::bail!("Unexpected end of input, need {} more bytes", size_hint);
                        }
                        if let Some(delay) = slow_delay { thread::sleep(delay); }
                        decoder_with_info = fallback;
                    }
                }
            };

            // Progressive frame decode loop with flushing
            loop {
                let mut output_bufs = vec![JxlOutputBuffer::from_image_rect_mut(
                    main_buffer.get_rect_mut(rect).into_raw(),
                )];
                for extra in &mut extra_channel_buffers {
                    let extra_rect = Rect { size: extra.size(), origin: (0, 0) };
                    output_bufs.push(JxlOutputBuffer::from_image_rect_mut(
                        extra.get_rect_mut(extra_rect).into_raw(),
                    ));
                }

                chunk_input = &input[..(chunk_input.len().saturating_add(chunk_size)).min(input.len())];
                let available_before = chunk_input.len();

                let process_result = decoder_with_frame.process(&mut chunk_input, &mut output_bufs);
                input = &input[(available_before - chunk_input.len())..];

                drop(output_bufs);

                match process_result? {
                    ProcessingResult::Complete { result: _ } => break,
                    ProcessingResult::NeedsMoreInput { mut fallback, size_hint } => {
                        let progress_pct = if file_size > 0 { (file_size - input.len()) * 100 / file_size } else { 0 };
                        let current_passes = fallback.num_completed_passes();

                        let pass_changed = current_passes > last_passes;
                        let interval_hit = progress_pct >= last_flush_pct + flush_interval_pct;

                        if pass_changed || interval_hit {
                            if pass_changed {
                                log::info!(
                                    "Progressive decode: Pass {} completed at {}%, flushing pixels",
                                    current_passes, progress_pct
                                );
                                last_passes = current_passes;
                            }

                            let mut flush_bufs = vec![JxlOutputBuffer::from_image_rect_mut(
                                main_buffer.get_rect_mut(rect).into_raw(),
                            )];
                            for extra in &mut extra_channel_buffers {
                                let extra_rect = Rect { size: extra.size(), origin: (0, 0) };
                                flush_bufs.push(JxlOutputBuffer::from_image_rect_mut(
                                    extra.get_rect_mut(extra_rect).into_raw(),
                                ));
                            }
                            fallback.flush_pixels(&mut flush_bufs)?;
                            drop(flush_bufs);

                            let partial_rgba = f16_buffer_to_rgba8(&main_buffer, width, height, color_type)?;
                            on_progress(ProgressiveUpdate {
                                rgba_data: partial_rgba,
                                width: width as u32,
                                height: height as u32,
                                completed_passes: current_passes,
                                total_passes: None,
                                is_final: false,
                                elapsed: start.elapsed(),
                            });

                            last_flush_pct = progress_pct;
                        }

                        if input.is_empty() {
                            anyhow::bail!("Unexpected end of input, need {} more bytes", size_hint);
                        }

                        if let Some(delay) = slow_delay { thread::sleep(delay); }
                        decoder_with_frame = fallback;
                    }
                }
            }

            f16_buffer_to_rgba8(&main_buffer, width, height, color_type)?
        }
    };

    let decode_time = start.elapsed();
    log::info!(
        "Progressive decode: Completed in {:?}, {} passes",
        decode_time, last_passes
    );

    // Send final update with pixel data
    on_progress(ProgressiveUpdate {
        rgba_data: rgba_data.clone(),
        width: width as u32,
        height: height as u32,
        completed_passes: last_passes.max(1),
        total_passes: Some(last_passes.max(1)),
        is_final: true,
        elapsed: decode_time,
    });

    // Create metadata
    let metadata = ImageMetadata {
        width: width as u32,
        height: height as u32,
        bit_depth: format_bit_depth(&bit_depth),
        has_animation: false,
        frame_count: 1,
        loop_count: 0,
    };

    let frame = DecodedFrame {
        rgba_data,
        width: width as u32,
        height: height as u32,
        decode_time,
        duration_ms: 0,
    };

    Ok(DecodeResult::SingleFrame { frame, metadata })
}

/// Convert F32 interleaved buffer to RGBA8
fn f32_buffer_to_rgba8(
    buffer: &Image<f32>,
    width: usize,
    height: usize,
    color_type: JxlColorType,
) -> Result<Vec<u8>> {
    let samples = color_type.samples_per_pixel();
    let mut rgba = vec![0u8; width * height * 4];

    for y in 0..height {
        let row = buffer.row(y);
        for x in 0..width {
            let src_offset = x * samples;
            let dst_offset = (y * width + x) * 4;

            match color_type {
                JxlColorType::Grayscale => {
                    let gray = (row[src_offset].clamp(0.0, 1.0) * 255.0) as u8;
                    rgba[dst_offset] = gray;
                    rgba[dst_offset + 1] = gray;
                    rgba[dst_offset + 2] = gray;
                    rgba[dst_offset + 3] = 255;
                }
                JxlColorType::GrayscaleAlpha => {
                    let gray = (row[src_offset].clamp(0.0, 1.0) * 255.0) as u8;
                    let alpha = (row[src_offset + 1].clamp(0.0, 1.0) * 255.0) as u8;
                    rgba[dst_offset] = gray;
                    rgba[dst_offset + 1] = gray;
                    rgba[dst_offset + 2] = gray;
                    rgba[dst_offset + 3] = alpha;
                }
                JxlColorType::Rgb => {
                    rgba[dst_offset] = (row[src_offset].clamp(0.0, 1.0) * 255.0) as u8;
                    rgba[dst_offset + 1] = (row[src_offset + 1].clamp(0.0, 1.0) * 255.0) as u8;
                    rgba[dst_offset + 2] = (row[src_offset + 2].clamp(0.0, 1.0) * 255.0) as u8;
                    rgba[dst_offset + 3] = 255;
                }
                JxlColorType::Rgba => {
                    rgba[dst_offset] = (row[src_offset].clamp(0.0, 1.0) * 255.0) as u8;
                    rgba[dst_offset + 1] = (row[src_offset + 1].clamp(0.0, 1.0) * 255.0) as u8;
                    rgba[dst_offset + 2] = (row[src_offset + 2].clamp(0.0, 1.0) * 255.0) as u8;
                    rgba[dst_offset + 3] = (row[src_offset + 3].clamp(0.0, 1.0) * 255.0) as u8;
                }
                JxlColorType::Bgr => {
                    rgba[dst_offset] = (row[src_offset + 2].clamp(0.0, 1.0) * 255.0) as u8;
                    rgba[dst_offset + 1] = (row[src_offset + 1].clamp(0.0, 1.0) * 255.0) as u8;
                    rgba[dst_offset + 2] = (row[src_offset].clamp(0.0, 1.0) * 255.0) as u8;
                    rgba[dst_offset + 3] = 255;
                }
                JxlColorType::Bgra => {
                    rgba[dst_offset] = (row[src_offset + 2].clamp(0.0, 1.0) * 255.0) as u8;
                    rgba[dst_offset + 1] = (row[src_offset + 1].clamp(0.0, 1.0) * 255.0) as u8;
                    rgba[dst_offset + 2] = (row[src_offset].clamp(0.0, 1.0) * 255.0) as u8;
                    rgba[dst_offset + 3] = (row[src_offset + 3].clamp(0.0, 1.0) * 255.0) as u8;
                }
            }
        }
    }

    Ok(rgba)
}

/// Convert U8 interleaved buffer to RGBA8
fn u8_buffer_to_rgba8(
    buffer: &Image<u8>,
    width: usize,
    height: usize,
    color_type: JxlColorType,
) -> Result<Vec<u8>> {
    let samples = color_type.samples_per_pixel();
    let mut rgba = vec![0u8; width * height * 4];

    for y in 0..height {
        let row = buffer.row(y);
        for x in 0..width {
            let src_offset = x * samples;
            let dst_offset = (y * width + x) * 4;

            match color_type {
                JxlColorType::Grayscale => {
                    let gray = row[src_offset];
                    rgba[dst_offset] = gray;
                    rgba[dst_offset + 1] = gray;
                    rgba[dst_offset + 2] = gray;
                    rgba[dst_offset + 3] = 255;
                }
                JxlColorType::GrayscaleAlpha => {
                    let gray = row[src_offset];
                    rgba[dst_offset] = gray;
                    rgba[dst_offset + 1] = gray;
                    rgba[dst_offset + 2] = gray;
                    rgba[dst_offset + 3] = row[src_offset + 1];
                }
                JxlColorType::Rgb => {
                    rgba[dst_offset] = row[src_offset];
                    rgba[dst_offset + 1] = row[src_offset + 1];
                    rgba[dst_offset + 2] = row[src_offset + 2];
                    rgba[dst_offset + 3] = 255;
                }
                JxlColorType::Rgba => {
                    rgba[dst_offset] = row[src_offset];
                    rgba[dst_offset + 1] = row[src_offset + 1];
                    rgba[dst_offset + 2] = row[src_offset + 2];
                    rgba[dst_offset + 3] = row[src_offset + 3];
                }
                JxlColorType::Bgr => {
                    rgba[dst_offset] = row[src_offset + 2];
                    rgba[dst_offset + 1] = row[src_offset + 1];
                    rgba[dst_offset + 2] = row[src_offset];
                    rgba[dst_offset + 3] = 255;
                }
                JxlColorType::Bgra => {
                    rgba[dst_offset] = row[src_offset + 2];
                    rgba[dst_offset + 1] = row[src_offset + 1];
                    rgba[dst_offset + 2] = row[src_offset];
                    rgba[dst_offset + 3] = row[src_offset + 3];
                }
            }
        }
    }

    Ok(rgba)
}

/// Convert U16 interleaved buffer to RGBA8
fn u16_buffer_to_rgba8(
    buffer: &Image<u16>,
    width: usize,
    height: usize,
    color_type: JxlColorType,
) -> Result<Vec<u8>> {
    let samples = color_type.samples_per_pixel();
    let mut rgba = vec![0u8; width * height * 4];

    for y in 0..height {
        let row = buffer.row(y);
        for x in 0..width {
            let src_offset = x * samples;
            let dst_offset = (y * width + x) * 4;

            // Convert 16-bit to 8-bit by taking high byte
            let to_u8 = |v: u16| (v >> 8) as u8;

            match color_type {
                JxlColorType::Grayscale => {
                    let gray = to_u8(row[src_offset]);
                    rgba[dst_offset] = gray;
                    rgba[dst_offset + 1] = gray;
                    rgba[dst_offset + 2] = gray;
                    rgba[dst_offset + 3] = 255;
                }
                JxlColorType::GrayscaleAlpha => {
                    let gray = to_u8(row[src_offset]);
                    rgba[dst_offset] = gray;
                    rgba[dst_offset + 1] = gray;
                    rgba[dst_offset + 2] = gray;
                    rgba[dst_offset + 3] = to_u8(row[src_offset + 1]);
                }
                JxlColorType::Rgb => {
                    rgba[dst_offset] = to_u8(row[src_offset]);
                    rgba[dst_offset + 1] = to_u8(row[src_offset + 1]);
                    rgba[dst_offset + 2] = to_u8(row[src_offset + 2]);
                    rgba[dst_offset + 3] = 255;
                }
                JxlColorType::Rgba => {
                    rgba[dst_offset] = to_u8(row[src_offset]);
                    rgba[dst_offset + 1] = to_u8(row[src_offset + 1]);
                    rgba[dst_offset + 2] = to_u8(row[src_offset + 2]);
                    rgba[dst_offset + 3] = to_u8(row[src_offset + 3]);
                }
                JxlColorType::Bgr => {
                    rgba[dst_offset] = to_u8(row[src_offset + 2]);
                    rgba[dst_offset + 1] = to_u8(row[src_offset + 1]);
                    rgba[dst_offset + 2] = to_u8(row[src_offset]);
                    rgba[dst_offset + 3] = 255;
                }
                JxlColorType::Bgra => {
                    rgba[dst_offset] = to_u8(row[src_offset + 2]);
                    rgba[dst_offset + 1] = to_u8(row[src_offset + 1]);
                    rgba[dst_offset + 2] = to_u8(row[src_offset]);
                    rgba[dst_offset + 3] = to_u8(row[src_offset + 3]);
                }
            }
        }
    }

    Ok(rgba)
}

/// Convert F16 (stored as u16) interleaved buffer to RGBA8
fn f16_buffer_to_rgba8(
    buffer: &Image<u16>,
    width: usize,
    height: usize,
    color_type: JxlColorType,
) -> Result<Vec<u8>> {
    use half::f16;

    let samples = color_type.samples_per_pixel();
    let mut rgba = vec![0u8; width * height * 4];

    for y in 0..height {
        let row = buffer.row(y);
        for x in 0..width {
            let src_offset = x * samples;
            let dst_offset = (y * width + x) * 4;

            // Reinterpret u16 bits as f16 and convert to f32, then to u8
            let to_u8 = |v: u16| {
                let f = f16::from_bits(v).to_f32();
                (f.clamp(0.0, 1.0) * 255.0) as u8
            };

            match color_type {
                JxlColorType::Grayscale => {
                    let gray = to_u8(row[src_offset]);
                    rgba[dst_offset] = gray;
                    rgba[dst_offset + 1] = gray;
                    rgba[dst_offset + 2] = gray;
                    rgba[dst_offset + 3] = 255;
                }
                JxlColorType::GrayscaleAlpha => {
                    let gray = to_u8(row[src_offset]);
                    rgba[dst_offset] = gray;
                    rgba[dst_offset + 1] = gray;
                    rgba[dst_offset + 2] = gray;
                    rgba[dst_offset + 3] = to_u8(row[src_offset + 1]);
                }
                JxlColorType::Rgb => {
                    rgba[dst_offset] = to_u8(row[src_offset]);
                    rgba[dst_offset + 1] = to_u8(row[src_offset + 1]);
                    rgba[dst_offset + 2] = to_u8(row[src_offset + 2]);
                    rgba[dst_offset + 3] = 255;
                }
                JxlColorType::Rgba => {
                    rgba[dst_offset] = to_u8(row[src_offset]);
                    rgba[dst_offset + 1] = to_u8(row[src_offset + 1]);
                    rgba[dst_offset + 2] = to_u8(row[src_offset + 2]);
                    rgba[dst_offset + 3] = to_u8(row[src_offset + 3]);
                }
                JxlColorType::Bgr => {
                    rgba[dst_offset] = to_u8(row[src_offset + 2]);
                    rgba[dst_offset + 1] = to_u8(row[src_offset + 1]);
                    rgba[dst_offset + 2] = to_u8(row[src_offset]);
                    rgba[dst_offset + 3] = 255;
                }
                JxlColorType::Bgra => {
                    rgba[dst_offset] = to_u8(row[src_offset + 2]);
                    rgba[dst_offset + 1] = to_u8(row[src_offset + 1]);
                    rgba[dst_offset + 2] = to_u8(row[src_offset]);
                    rgba[dst_offset + 3] = to_u8(row[src_offset + 3]);
                }
            }
        }
    }

    Ok(rgba)
}
