use super::{DecodeResult, DecodedFrame, ImageMetadata};
use crate::util::rgb_conversion::jxl_to_rgba8;
use anyhow::Result;
use jxl::api::{
    states::WithImageInfo,
    JxlAnimation, JxlBitDepth, JxlColorType, JxlDecoder, JxlDecoderOptions, JxlOutputBuffer, ProcessingResult,
};
use jxl::image::{Image, Rect};
use std::fs::File;
use std::io::BufReader;
use std::panic;
use std::path::Path;
use std::time::Instant;

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
/// Uses chunked input to enable true streaming progressive rendering.
pub fn decode_jxl_progressive<P, F>(
    path: P,
    mut on_progress: F,
) -> Result<DecodeResult>
where
    P: AsRef<Path>,
    F: FnMut(super::ProgressiveUpdate),
{
    use super::ProgressiveUpdate;
    use std::io::Read;

    let start = Instant::now();
    log::info!("Progressive decode: Opening JXL file: {:?}", path.as_ref());

    // Read file into memory for chunked processing
    let mut file = File::open(path.as_ref())?;
    let file_size = file.metadata()?.len() as usize;
    let mut file_data = Vec::with_capacity(file_size);
    file.read_to_end(&mut file_data)?;

    // Use chunks for progressive decoding - smaller chunks = more frequent updates
    let chunk_size = 16 * 1024; // 16KB chunks
    let mut input = &file_data[..];
    let mut chunk_input = &input[0..0];

    // Set up decoder options
    let mut options = JxlDecoderOptions::default();
    options.adjust_orientation = true;
    options.coalescing = true;
    options.enable_flush_pixels = true; // Enable progressive rendering
    options.premultiply_output = true; // Premultiply alpha for better compositing

    log::info!("Progressive decode: Creating JXL decoder, file size: {} bytes", file_size);
    let mut decoder = JxlDecoder::new(options);

    // Helper macro to advance decoder with chunked input
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
    let pixel_format = decoder_with_info.current_pixel_format();
    let color_type = pixel_format.color_type;

    log::info!(
        "Progressive decode: Image {}x{}, color type: {:?}",
        width, height, color_type
    );

    // For animations, fall back to regular decode
    if animation.is_some() {
        log::info!("Progressive decode: Animation detected, using standard decode");
        return decode_jxl(path);
    }

    // Get frame info
    let mut decoder_with_frame = advance_decoder!(decoder_with_info);

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

    // Track progress for progressive updates
    let mut last_passes = 0usize;
    let mut update_count = 0usize;
    let mut last_progress_pct = 0usize;

    // Helper to create output buffers (used multiple times for progressive updates)
    macro_rules! create_output_bufs {
        ($main:expr, $extras:expr, $rect:expr) => {{
            let mut bufs = vec![JxlOutputBuffer::from_image_rect_mut(
                $main.get_rect_mut($rect).into_raw(),
            )];
            for extra in $extras.iter_mut() {
                let extra_rect = Rect {
                    size: extra.size(),
                    origin: (0, 0),
                };
                bufs.push(JxlOutputBuffer::from_image_rect_mut(
                    extra.get_rect_mut(extra_rect).into_raw(),
                ));
            }
            bufs
        }};
    }

    // Helper to extract RGBA data from current buffers
    let extract_rgba = |main: &Image<f32>, extras: &[Image<f32>], ct: JxlColorType, w: usize, h: usize| -> Vec<u8> {
        let mut channels = Vec::new();
        match ct {
            JxlColorType::Grayscale => {
                channels.push(main.try_clone().unwrap());
            }
            JxlColorType::GrayscaleAlpha => {
                channels.push(main.try_clone().unwrap());
                if !extras.is_empty() {
                    channels.push(extras[0].try_clone().unwrap());
                }
            }
            JxlColorType::Rgb | JxlColorType::Bgr => {
                let (r, g, b) = split_rgb_channels(main, w, h);
                channels.push(r);
                channels.push(g);
                channels.push(b);
                if !extras.is_empty() {
                    channels.push(extras[0].try_clone().unwrap());
                }
            }
            JxlColorType::Rgba | JxlColorType::Bgra => {
                let (r, g, b) = split_rgb_channels(main, w, h);
                channels.push(r);
                channels.push(g);
                channels.push(b);
                if !extras.is_empty() {
                    channels.push(extras[0].try_clone().unwrap());
                }
            }
        }
        jxl_to_rgba8(&channels, ct, w, h)
    };

    // Drop the initial output_bufs - we'll recreate them in the loop
    drop(output_bufs);

    // Progressive decode loop with chunked input
    loop {
        // Create fresh output buffers for this iteration
        let mut output_bufs = create_output_bufs!(&mut main_channel, &mut extra_channel_buffers, rect);

        // Expand available input by chunk_size
        chunk_input = &input[..(chunk_input.len().saturating_add(chunk_size)).min(input.len())];
        let available_before = chunk_input.len();

        let process_result = decoder_with_frame.process(&mut chunk_input, &mut output_bufs);

        // Update input pointer
        input = &input[(available_before - chunk_input.len())..];

        // Drop output_bufs to release mutable borrows before we might read from buffers
        drop(output_bufs);

        match process_result? {
            ProcessingResult::Complete { result: _ } => {
                break;
            }
            ProcessingResult::NeedsMoreInput { fallback, size_hint } => {
                decoder_with_frame = fallback;

                // Calculate progress
                let progress_pct = (file_size - input.len()) * 100 / file_size;
                let current_passes = decoder_with_frame.num_completed_passes();

                // Send progressive update when passes change or every 10% progress
                let should_update = current_passes > last_passes ||
                    (progress_pct >= last_progress_pct + 10 && current_passes > 0);

                if should_update {
                    // Recreate output buffers for flush
                    let mut flush_bufs = create_output_bufs!(&mut main_channel, &mut extra_channel_buffers, rect);

                    // Flush pixels to render any decoded data
                    match decoder_with_frame.flush_pixels(&mut flush_bufs) {
                        Ok(()) => {
                            // Drop flush buffers to read data
                            drop(flush_bufs);

                            // Extract RGBA from current buffer state
                            let rgba_data = extract_rgba(&main_channel, &extra_channel_buffers, color_type, width, height);

                            log::info!(
                                "Progressive decode: Sending update at {}% (pass {}, {} bytes)",
                                progress_pct, current_passes, rgba_data.len()
                            );

                            // Send progressive update
                            on_progress(ProgressiveUpdate {
                                rgba_data,
                                width: width as u32,
                                height: height as u32,
                                completed_passes: current_passes,
                                total_passes: None, // Unknown during progressive decode
                                is_final: false,
                                elapsed: start.elapsed(),
                            });

                            update_count += 1;
                        }
                        Err(e) => {
                            drop(flush_bufs);
                            log::debug!("flush_pixels error (expected early): {}", e);
                        }
                    }

                    if current_passes > last_passes {
                        log::info!(
                            "Progressive decode: Pass {} completed at {}%",
                            current_passes, progress_pct
                        );
                        last_passes = current_passes;
                    }
                    last_progress_pct = progress_pct;
                }

                if input.is_empty() {
                    anyhow::bail!("Unexpected end of input, need {} more bytes", size_hint);
                }
            }
        }
    }

    let decode_time = start.elapsed();
    log::info!(
        "Progressive decode: Completed in {:?}, {} progressive updates sent",
        decode_time, update_count
    );

    // Convert to RGBA
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
    let rgba_data = jxl_to_rgba8(&channels, color_type, width, height);

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
