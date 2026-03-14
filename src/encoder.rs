use anyhow::{anyhow, bail, Context, Result};
use jxl::encode::vardct::{encode_vardct_animation_u8_rgba, VarDctConfig};
use jxl::encode::{JxlEncoder, JxlEncoderImageData, JxlEncoderMode, JxlEncoderOptions};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncoderMode {
    Modular,
    VarDct,
}

impl EncoderMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Modular => "Modular",
            Self::VarDct => "VarDCT",
        }
    }
}

#[derive(Clone, Debug)]
pub struct EncoderSettings {
    pub mode: EncoderMode,
    pub lossless: bool,
    pub effort: u8,
    pub distance: f32,
    pub near_lossless: u8,
    pub fast_lossless: bool,
    pub use_ffmpeg_decode: bool,
    pub encode_animation_if_possible: bool,
    pub max_animation_frames: u16,
    pub animation_fps_cap: u8,
    pub animation_max_edge: u16,
}

impl Default for EncoderSettings {
    fn default() -> Self {
        Self {
            mode: EncoderMode::Modular,
            lossless: true,
            effort: 4,
            distance: 1.0,
            near_lossless: 0,
            fast_lossless: false,
            use_ffmpeg_decode: true,
            encode_animation_if_possible: true,
            max_animation_frames: 48,
            animation_fps_cap: 12,
            animation_max_edge: 640,
        }
    }
}

#[derive(Clone, Debug)]
pub struct EncodeStats {
    pub width: u32,
    pub height: u32,
    pub input_size_bytes: u64,
    pub output_size_bytes: usize,
    pub elapsed: Duration,
    pub used_ffmpeg: bool,
    pub source_had_multiple_frames: bool,
    pub encoded_animation: bool,
    pub frames_encoded: usize,
    pub frame_duration_ms: u32,
}

#[derive(Debug)]
struct DecodedInput {
    frames_rgba: Vec<Vec<u8>>,
    frame_duration_ms: u32,
    width: u32,
    height: u32,
    used_ffmpeg: bool,
    source_had_multiple_frames: bool,
}

#[derive(Debug, Default)]
struct ProbeInfo {
    has_multiple_frames: bool,
    fps: Option<f32>,
}

pub fn suggest_output_path(input: &Path) -> PathBuf {
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    let mut out = input.with_file_name(format!("{stem}.jxl"));
    if out == input {
        out = input.with_file_name(format!("{stem}_encoded.jxl"));
    }
    out
}

fn parse_rational_fps(value: &str) -> Option<f32> {
    let value = value.trim();
    if value.is_empty() || value == "N/A" {
        return None;
    }
    if let Some((num, den)) = value.split_once('/') {
        let n: f32 = num.trim().parse().ok()?;
        let d: f32 = den.trim().parse().ok()?;
        if d <= 0.0 {
            return None;
        }
        return Some(n / d);
    }
    value.parse::<f32>().ok().filter(|v| *v > 0.0)
}

fn ffprobe_info(input: &Path) -> Result<ProbeInfo> {
    let probe = Command::new("ffprobe")
        .arg("-v")
        .arg("error")
        .arg("-select_streams")
        .arg("v:0")
        .arg("-show_entries")
        .arg("stream=nb_frames,r_frame_rate")
        .arg("-of")
        .arg("csv=p=0")
        .arg(input)
        .output()
        .context("failed to run ffprobe")?;

    if !probe.status.success() {
        bail!(
            "ffprobe failed: {}",
            String::from_utf8_lossy(&probe.stderr).trim()
        );
    }

    let line = String::from_utf8_lossy(&probe.stdout)
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or_default()
        .trim()
        .to_string();

    let cols: Vec<&str> = line.split(',').collect();
    let has_multiple_frames = cols
        .first()
        .and_then(|v| v.trim().parse::<u32>().ok())
        .map(|n| n > 1)
        .unwrap_or(false);
    let fps = cols.get(1).and_then(|v| parse_rational_fps(v));

    Ok(ProbeInfo {
        has_multiple_frames,
        fps,
    })
}

fn looks_like_video_input(input: &Path) -> bool {
    let ext = input
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    matches!(
        ext.as_str(),
        "mp4" | "mov" | "m4v" | "mkv" | "avi" | "webm" | "mpeg" | "mpg"
    )
}

fn read_ppm_token(data: &[u8], cursor: &mut usize) -> Result<String> {
    while *cursor < data.len() {
        let b = data[*cursor];
        if b == b'#' {
            while *cursor < data.len() && data[*cursor] != b'\n' {
                *cursor += 1;
            }
        } else if b.is_ascii_whitespace() {
            *cursor += 1;
        } else {
            break;
        }
    }

    if *cursor >= data.len() {
        bail!("unexpected end of PPM header");
    }

    let start = *cursor;
    while *cursor < data.len() && !data[*cursor].is_ascii_whitespace() {
        *cursor += 1;
    }

    let token = str::from_utf8(&data[start..*cursor]).context("PPM header is not UTF-8")?;
    Ok(token.to_string())
}

fn parse_ppm_frame_at(data: &[u8], cursor: &mut usize) -> Result<(u32, u32, Vec<u8>)> {
    let magic = read_ppm_token(data, cursor)?;
    if magic != "P6" {
        bail!("unsupported PPM magic: {magic}");
    }

    let width: u32 = read_ppm_token(data, cursor)?
        .parse()
        .context("invalid PPM width")?;
    let height: u32 = read_ppm_token(data, cursor)?
        .parse()
        .context("invalid PPM height")?;
    let maxval: u32 = read_ppm_token(data, cursor)?
        .parse()
        .context("invalid PPM maxval")?;

    if maxval != 255 {
        bail!("unsupported PPM maxval {maxval}, expected 255");
    }

    if *cursor >= data.len() || !data[*cursor].is_ascii_whitespace() {
        bail!("PPM header missing delimiter before pixel payload");
    }
    *cursor += 1;

    let expected_rgb_len = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(3))
        .ok_or_else(|| anyhow!("PPM image size overflow"))?;

    let end = cursor
        .checked_add(expected_rgb_len)
        .ok_or_else(|| anyhow!("PPM payload offset overflow"))?;
    if end > data.len() {
        bail!(
            "PPM payload truncated: need {} bytes, have {}",
            expected_rgb_len,
            data.len().saturating_sub(*cursor)
        );
    }

    let rgb = &data[*cursor..end];
    *cursor = end;

    let mut rgba = Vec::with_capacity((width as usize) * (height as usize) * 4);
    for px in rgb.chunks_exact(3) {
        rgba.extend_from_slice(&[px[0], px[1], px[2], 255]);
    }

    Ok((width, height, rgba))
}

fn parse_ppm_stream_to_rgba_frames(
    data: &[u8],
    max_frames: usize,
) -> Result<(u32, u32, Vec<Vec<u8>>)> {
    let mut cursor = 0usize;
    let mut frames = Vec::new();
    let mut width = 0u32;
    let mut height = 0u32;

    while cursor < data.len() && frames.len() < max_frames {
        while cursor < data.len() && data[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor >= data.len() {
            break;
        }

        let (w, h, rgba) = parse_ppm_frame_at(data, &mut cursor)?;
        if frames.is_empty() {
            width = w;
            height = h;
        } else if width != w || height != h {
            bail!(
                "frame size mismatch in ffmpeg PPM stream: expected {}x{}, got {}x{}",
                width,
                height,
                w,
                h
            );
        }
        frames.push(rgba);
    }

    if frames.is_empty() {
        bail!("ffmpeg returned no frames in PPM stream");
    }

    Ok((width, height, frames))
}

fn decode_with_ffmpeg(input: &Path, settings: &EncoderSettings) -> Result<DecodedInput> {
    let probe = ffprobe_info(input).unwrap_or_default();
    let source_had_multiple_frames = probe.has_multiple_frames || looks_like_video_input(input);

    let want_animation = settings.encode_animation_if_possible && source_had_multiple_frames;
    let max_frames = settings.max_animation_frames.max(1) as usize;

    let mut cmd = Command::new("ffmpeg");
    cmd.arg("-v")
        .arg("error")
        .arg("-i")
        .arg(input)
        .arg("-f")
        .arg("image2pipe")
        .arg("-vcodec")
        .arg("ppm");

    if want_animation {
        let fps = probe
            .fps
            .unwrap_or(settings.animation_fps_cap.max(1) as f32);
        let fps_capped = fps.min(settings.animation_fps_cap.max(1) as f32).max(1.0);
        let max_edge = settings.animation_max_edge.max(64);
        let filter = format!(
            "fps={fps_capped:.3},scale=w={max_edge}:h={max_edge}:force_original_aspect_ratio=decrease"
        );
        cmd.arg("-vf").arg(filter);
        cmd.arg("-frames:v").arg(max_frames.to_string());
    } else {
        cmd.arg("-frames:v").arg("1");
    }

    cmd.arg("-");

    let decode = cmd.output().context("failed to run ffmpeg")?;
    if !decode.status.success() {
        bail!(
            "ffmpeg conversion to PPM failed: {}",
            String::from_utf8_lossy(&decode.stderr).trim()
        );
    }

    let (width, height, frames_rgba) = parse_ppm_stream_to_rgba_frames(&decode.stdout, max_frames)?;

    let frame_duration_ms = if want_animation {
        let fps = probe
            .fps
            .unwrap_or(settings.animation_fps_cap.max(1) as f32);
        (1000.0 / fps.max(1.0)).round().clamp(1.0, 60_000.0) as u32
    } else {
        100
    };

    Ok(DecodedInput {
        frames_rgba,
        frame_duration_ms,
        width,
        height,
        used_ffmpeg: true,
        source_had_multiple_frames,
    })
}

fn decode_with_image_crate(input: &Path) -> Result<DecodedInput> {
    let dyn_img = image::ImageReader::open(input)
        .with_context(|| format!("failed to open input image: {}", input.display()))?
        .decode()
        .with_context(|| format!("failed to decode input image: {}", input.display()))?;
    let rgba = dyn_img.to_rgba8();
    let (width, height) = rgba.dimensions();

    Ok(DecodedInput {
        frames_rgba: vec![rgba.into_raw()],
        frame_duration_ms: 100,
        width,
        height,
        used_ffmpeg: false,
        source_had_multiple_frames: false,
    })
}

pub fn encode_to_jxl_with_progress<F>(
    input: &Path,
    output: &Path,
    settings: &EncoderSettings,
    mut on_progress: F,
) -> Result<EncodeStats>
where
    F: FnMut(String),
{
    let start = Instant::now();

    on_progress("Reading input metadata...".to_string());
    let input_size_bytes = std::fs::metadata(input)
        .with_context(|| format!("failed to read input metadata: {}", input.display()))?
        .len();

    on_progress("Preprocessing input...".to_string());
    let decoded = if settings.use_ffmpeg_decode {
        match decode_with_ffmpeg(input, settings) {
            Ok(data) => data,
            Err(ffmpeg_error) => {
                log::warn!(
                    "ffmpeg preprocessing failed for {}: {}. Falling back to image crate.",
                    input.display(),
                    ffmpeg_error
                );
                on_progress("ffmpeg failed, falling back to image decoder...".to_string());
                decode_with_image_crate(input)?
            }
        }
    } else {
        decode_with_image_crate(input)?
    };

    let (encoded, encoded_animation, frames_encoded, frame_duration_ms) =
        if decoded.frames_rgba.len() > 1 {
            on_progress(format!(
                "Encoding animation ({} preprocessed frames)...",
                decoded.frames_rgba.len()
            ));

            let cfg = VarDctConfig {
                distance: if settings.lossless {
                    0.0
                } else {
                    settings.distance.clamp(0.01, 25.0)
                },
                effort: settings.effort.clamp(1, 9),
                progressive: false,
            };

            // Guardrail: keep animation encode workload bounded.
            let pixel_budget: u64 = 50_000_000;
            let pixels_per_frame = (decoded.width as u64) * (decoded.height as u64);
            let max_frames_by_budget = if pixels_per_frame > 0 {
                (pixel_budget / pixels_per_frame).max(1) as usize
            } else {
                1
            };

            let mut sampled_frames: Vec<&[u8]> = Vec::new();
            let mut sampled_duration = decoded.frame_duration_ms.max(1);

            if decoded.frames_rgba.len() > max_frames_by_budget {
                let step = decoded.frames_rgba.len().div_ceil(max_frames_by_budget);
                sampled_duration = sampled_duration.saturating_mul(step as u32).max(1);
                sampled_frames.extend(
                    decoded
                        .frames_rgba
                        .iter()
                        .step_by(step)
                        .map(|f| f.as_slice()),
                );
                on_progress(format!(
                    "Frame budget applied: using {} sampled frames",
                    sampled_frames.len()
                ));
            } else {
                sampled_frames.extend(decoded.frames_rgba.iter().map(|f| f.as_slice()));
            }

            let frame_refs: Vec<(&[u8], u32)> = sampled_frames
                .iter()
                .map(|f| (*f, sampled_duration))
                .collect();

            let bytes = encode_vardct_animation_u8_rgba(
                &frame_refs,
                decoded.width as usize,
                decoded.height as usize,
                &cfg,
            )
            .context("jxl animation encoder failed")?;

            (bytes, true, frame_refs.len(), sampled_duration)
        } else {
            on_progress("Encoding still image...".to_string());

            let mut opts = JxlEncoderOptions::default();
            opts.container = true;
            opts.effort = settings.effort.clamp(1, 9);
            opts.lossless = settings.lossless;
            opts.fast_lossless = settings.fast_lossless;

            if settings.lossless {
                opts.mode = JxlEncoderMode::Modular;
                opts.near_lossless = 0;
            } else {
                opts.mode = match settings.mode {
                    EncoderMode::Modular => JxlEncoderMode::Modular,
                    EncoderMode::VarDct => JxlEncoderMode::VarDct,
                };
                opts.distance_milli = (settings.distance.clamp(0.01, 25.0) * 1000.0).round() as u16;
                opts.near_lossless = settings.near_lossless;
            }

            let encoder = JxlEncoder::new(opts);
            let bytes = encoder
                .encode_image(
                    (decoded.width, decoded.height),
                    JxlEncoderImageData::Rgba8Interleaved(&decoded.frames_rgba[0]),
                )
                .context("jxl encoder failed")?;

            (bytes, false, 1, decoded.frame_duration_ms)
        };

    on_progress("Writing output file...".to_string());
    std::fs::write(output, &encoded)
        .with_context(|| format!("failed to write output file: {}", output.display()))?;

    Ok(EncodeStats {
        width: decoded.width,
        height: decoded.height,
        input_size_bytes,
        output_size_bytes: encoded.len(),
        elapsed: start.elapsed(),
        used_ffmpeg: decoded.used_ffmpeg,
        source_had_multiple_frames: decoded.source_had_multiple_frames,
        encoded_animation,
        frames_encoded,
        frame_duration_ms,
    })
}
