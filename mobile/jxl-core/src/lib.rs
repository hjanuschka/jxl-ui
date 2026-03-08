//! JXL Mobile Core - Shared FFI library for decoding JPEG XL images.
//!
//! Provides both C FFI (for iOS/Swift) and JNI (for Android/Kotlin).

use jxl::api::{
    JxlColorType, JxlDecoder, JxlDecoderOptions, JxlOutputBuffer, JxlPixelFormat,
    ProcessingResult,
};
use jxl::headers::extra_channels::ExtraChannel;
use jxl::image::{Image, Rect};
use std::io::BufReader;

// ---------------------------------------------------------------------------
// Core decode logic
// ---------------------------------------------------------------------------

/// Decoded image result
pub struct DecodedImage {
    pub pixels: Vec<u8>, // RGBA8
    pub width: u32,
    pub height: u32,
}

/// Decode JXL bytes into RGBA8 pixels.
pub fn decode_jxl_to_rgba(data: &[u8]) -> Result<DecodedImage, String> {
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

    // Auto-detect alpha and upgrade color type
    let has_alpha = extra_channels
        .iter()
        .any(|ec| ec.ec_type == ExtraChannel::Alpha);

    let target_color_type = if has_alpha {
        match native_color_type {
            JxlColorType::Rgb => JxlColorType::Rgba,
            JxlColorType::Bgr => JxlColorType::Bgra,
            JxlColorType::Grayscale => JxlColorType::GrayscaleAlpha,
            other => other,
        }
    } else {
        native_color_type
    };

    let alpha_folded = has_alpha
        && matches!(
            target_color_type,
            JxlColorType::Rgba | JxlColorType::Bgra | JxlColorType::GrayscaleAlpha
        );

    let extra_channel_format: Vec<Option<jxl::api::JxlDataFormat>> = extra_channels
        .iter()
        .map(|ec| {
            if alpha_folded && ec.ec_type == ExtraChannel::Alpha {
                None
            } else {
                Some(jxl::api::JxlDataFormat::f32())
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
        color_data_format: Some(jxl::api::JxlDataFormat::f32()),
        extra_channel_format,
    });

    let pixel_format = decoder_with_info.current_pixel_format();
    let color_type = pixel_format.color_type;
    let samples_per_pixel = color_type.samples_per_pixel();

    // Allocate buffers
    let mut main_buffer =
        Image::<f32>::new((width * samples_per_pixel, height)).map_err(|e| e.to_string())?;
    let mut extra_bufs: Vec<Image<f32>> = (0..extra_buf_count)
        .map(|_| Image::<f32>::new((width, height)))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    let rect = Rect {
        size: main_buffer.size(),
        origin: (0, 0),
    };

    // Get frame info
    let mut decoder_with_frame = match decoder_with_info.process(&mut reader) {
        Ok(ProcessingResult::Complete { result }) => result,
        Ok(ProcessingResult::NeedsMoreInput { .. }) => {
            return Err("Incomplete frame header".to_string());
        }
        Err(e) => return Err(format!("Frame header error: {e}")),
    };

    // Decode frame
    loop {
        let mut output_bufs = vec![JxlOutputBuffer::from_image_rect_mut(
            main_buffer.get_rect_mut(rect).into_raw(),
        )];
        for extra in &mut extra_bufs {
            let extra_rect = Rect {
                size: extra.size(),
                origin: (0, 0),
            };
            output_bufs.push(JxlOutputBuffer::from_image_rect_mut(
                extra.get_rect_mut(extra_rect).into_raw(),
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

    // Convert f32 buffer to RGBA8
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

    Ok(DecodedImage {
        pixels: rgba,
        width: width as u32,
        height: height as u32,
    })
}

// ---------------------------------------------------------------------------
// C FFI (for iOS / Swift)
// ---------------------------------------------------------------------------

/// Opaque handle to a decoded image
#[repr(C)]
pub struct JxlImage {
    pub pixels: *mut u8,
    pub width: u32,
    pub height: u32,
    pub pixels_len: u32,
}

/// Decode JXL data, returns null on error.
/// Caller must free with `jxl_image_free`.
#[no_mangle]
pub extern "C" fn jxl_decode(data: *const u8, data_len: usize) -> *mut JxlImage {
    if data.is_null() || data_len == 0 {
        return std::ptr::null_mut();
    }
    let slice = unsafe { std::slice::from_raw_parts(data, data_len) };

    match decode_jxl_to_rgba(slice) {
        Ok(img) => {
            let mut pixels = img.pixels.into_boxed_slice();
            let ptr = pixels.as_mut_ptr();
            let len = pixels.len() as u32;
            std::mem::forget(pixels);

            let result = Box::new(JxlImage {
                pixels: ptr,
                width: img.width,
                height: img.height,
                pixels_len: len,
            });
            Box::into_raw(result)
        }
        Err(_) => std::ptr::null_mut(),
    }
}

/// Free a decoded image.
#[no_mangle]
pub extern "C" fn jxl_image_free(img: *mut JxlImage) {
    if !img.is_null() {
        unsafe {
            let img = Box::from_raw(img);
            if !img.pixels.is_null() {
                let _ = Vec::from_raw_parts(img.pixels, img.pixels_len as usize, img.pixels_len as usize);
            }
        }
    }
}

/// Get last error message (for debugging). Returns static string.
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
        Ok(img) => {
            let mut pixels = img.pixels.into_boxed_slice();
            let ptr = pixels.as_mut_ptr();
            let len = pixels.len() as u32;
            std::mem::forget(pixels);

            let result = Box::new(JxlImage {
                pixels: ptr,
                width: img.width,
                height: img.height,
                pixels_len: len,
            });
            Box::into_raw(result)
        }
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
        *buf.add(copy_len) = 0; // null terminate
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

        // Create DecodedImage Kotlin object
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
