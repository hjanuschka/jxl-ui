use jxl::{api::JxlColorType, image::Image};

/// Convert JXL's planar f32 format to interleaved RGBA8 format for GPUI
///
/// JXL outputs separate channels as f32 values in [0.0, 1.0] range.
/// GPUI expects interleaved RGBA8 with u8 values in [0, 255] range.
///
/// # Arguments
/// * `channels` - Slice of Image<f32> channels (R, G, B, and optionally A)
/// * `color_type` - The JXL color type (Grayscale, GrayscaleAlpha, Rgb, Rgba, etc.)
/// * `width` - Image width in pixels
/// * `height` - Image height in pixels
///
/// # Returns
/// Vec<u8> containing RGBA8 data in interleaved format (RGBARGBARGBA...)
pub fn jxl_to_rgba8(
    channels: &[Image<f32>],
    color_type: JxlColorType,
    width: usize,
    height: usize,
) -> Vec<u8> {
    let mut rgba = vec![0u8; width * height * 4];

    match color_type {
        JxlColorType::Grayscale => {
            // Single channel -> RGB (same value for all) + opaque alpha
            if channels.is_empty() {
                log::warn!("No channels provided for Grayscale image");
                return rgba;
            }

            for y in 0..height {
                let gray_row = channels[0].row(y);
                for x in 0..width {
                    let idx = (y * width + x) * 4;
                    let gray = (gray_row[x] * 255.0).clamp(0.0, 255.0) as u8;

                    rgba[idx] = gray;     // R
                    rgba[idx + 1] = gray; // G
                    rgba[idx + 2] = gray; // B
                    rgba[idx + 3] = 255;  // A (opaque)
                }
            }
        }
        JxlColorType::GrayscaleAlpha => {
            // Two channels -> RGB (same value) + alpha
            if channels.len() < 2 {
                log::warn!("Insufficient channels for GrayscaleAlpha image");
                return rgba;
            }

            for y in 0..height {
                let gray_row = channels[0].row(y);
                let alpha_row = channels[1].row(y);
                for x in 0..width {
                    let idx = (y * width + x) * 4;
                    let gray = (gray_row[x] * 255.0).clamp(0.0, 255.0) as u8;
                    let alpha = (alpha_row[x] * 255.0).clamp(0.0, 255.0) as u8;

                    rgba[idx] = gray;     // R
                    rgba[idx + 1] = gray; // G
                    rgba[idx + 2] = gray; // B
                    rgba[idx + 3] = alpha; // A
                }
            }
        }
        JxlColorType::Rgb | JxlColorType::Bgr => {
            // Three or four channels (RGB/BGR + optional alpha from extra channel)
            if channels.len() < 3 {
                log::warn!("Insufficient channels for RGB/BGR image");
                return rgba;
            }

            let is_bgr = matches!(color_type, JxlColorType::Bgr);
            let has_alpha = channels.len() >= 4;

            for y in 0..height {
                let row0 = channels[0].row(y);
                let row1 = channels[1].row(y);
                let row2 = channels[2].row(y);
                let row3 = if has_alpha { Some(channels[3].row(y)) } else { None };

                for x in 0..width {
                    let idx = (y * width + x) * 4;

                    if is_bgr {
                        // BGR -> RGB
                        rgba[idx] = (row2[x] * 255.0).clamp(0.0, 255.0) as u8;     // R from B channel
                        rgba[idx + 1] = (row1[x] * 255.0).clamp(0.0, 255.0) as u8; // G
                        rgba[idx + 2] = (row0[x] * 255.0).clamp(0.0, 255.0) as u8; // B from R channel
                    } else {
                        // RGB
                        rgba[idx] = (row0[x] * 255.0).clamp(0.0, 255.0) as u8;     // R
                        rgba[idx + 1] = (row1[x] * 255.0).clamp(0.0, 255.0) as u8; // G
                        rgba[idx + 2] = (row2[x] * 255.0).clamp(0.0, 255.0) as u8; // B
                    }

                    // Use alpha channel if present, otherwise opaque
                    rgba[idx + 3] = if let Some(alpha_row) = row3 {
                        (alpha_row[x] * 255.0).clamp(0.0, 255.0) as u8
                    } else {
                        255
                    };
                }
            }
        }
        JxlColorType::Rgba | JxlColorType::Bgra => {
            // Four channels (RGBA or BGRA)
            if channels.len() < 4 {
                log::warn!("Insufficient channels for RGBA/BGRA image");
                return rgba;
            }

            let is_bgra = matches!(color_type, JxlColorType::Bgra);

            for y in 0..height {
                let row0 = channels[0].row(y);
                let row1 = channels[1].row(y);
                let row2 = channels[2].row(y);
                let row3 = channels[3].row(y);

                for x in 0..width {
                    let idx = (y * width + x) * 4;

                    if is_bgra {
                        // BGRA -> RGBA
                        rgba[idx] = (row2[x] * 255.0).clamp(0.0, 255.0) as u8;     // R from B channel
                        rgba[idx + 1] = (row1[x] * 255.0).clamp(0.0, 255.0) as u8; // G
                        rgba[idx + 2] = (row0[x] * 255.0).clamp(0.0, 255.0) as u8; // B from R channel
                        rgba[idx + 3] = (row3[x] * 255.0).clamp(0.0, 255.0) as u8; // A
                    } else {
                        // RGBA
                        rgba[idx] = (row0[x] * 255.0).clamp(0.0, 255.0) as u8;     // R
                        rgba[idx + 1] = (row1[x] * 255.0).clamp(0.0, 255.0) as u8; // G
                        rgba[idx + 2] = (row2[x] * 255.0).clamp(0.0, 255.0) as u8; // B
                        rgba[idx + 3] = (row3[x] * 255.0).clamp(0.0, 255.0) as u8; // A
                    }
                }
            }
        }
    }

    rgba
}

#[cfg(test)]
mod tests {
    use super::*;
    use jxl::image::Image;

    #[test]
    fn test_rgb_conversion() {
        // Create a simple 2x2 RGB image with known values
        let width = 2;
        let height = 2;

        // Red channel: all 1.0 (255)
        let mut r_img = Image::<f32>::new((width, height)).unwrap();
        r_img.fill(1.0_f32);

        // Green channel: all 0.5 (127)
        let mut g_img = Image::<f32>::new((width, height)).unwrap();
        g_img.fill(0.5_f32);

        // Blue channel: all 0.0 (0)
        let mut b_img = Image::<f32>::new((width, height)).unwrap();
        b_img.fill(0.0_f32);

        let channels = vec![r_img, g_img, b_img];
        let rgba = jxl_to_rgba8(&channels, JxlColorType::Rgb, width, height);

        // Check first pixel (should be: 255, 127, 0, 255)
        assert_eq!(rgba[0], 255); // R
        assert_eq!(rgba[1], 127); // G
        assert_eq!(rgba[2], 0);   // B
        assert_eq!(rgba[3], 255); // A

        // Verify length
        assert_eq!(rgba.len(), width * height * 4);
    }

    #[test]
    fn test_grayscale_conversion() {
        let width = 2;
        let height = 2;

        let mut gray_img = Image::<f32>::new((width, height)).unwrap();
        gray_img.fill(0.5_f32);

        let channels = vec![gray_img];
        let rgba = jxl_to_rgba8(&channels, JxlColorType::Grayscale, width, height);

        // First pixel should be: 127, 127, 127, 255
        assert_eq!(rgba[0], 127); // R
        assert_eq!(rgba[1], 127); // G
        assert_eq!(rgba[2], 127); // B
        assert_eq!(rgba[3], 255); // A (opaque)
    }
}
