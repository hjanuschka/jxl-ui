use crate::decoder::{worker, DecodeResult, DecodedFrame, ImageMetadata, ProgressiveUpdate};
use gpui::{Context, FocusHandle, Focusable, RenderImage, ImageSource, Window, div, img, prelude::*, px, rgb, white};
use image::{ImageBuffer, RgbaImage};
use smallvec::SmallVec;
use std::path::PathBuf;
use std::sync::Arc;

/// Animation playback state
enum AnimationState {
    SingleFrame {
        frame: DecodedFrame,
        cached_image: Option<Arc<RenderImage>>,
    },
    Animation {
        frames: Vec<DecodedFrame>,
        cached_images: Vec<Option<Arc<RenderImage>>>,
        current_frame: usize,
        is_playing: bool,
        playback_started: bool,
        gpu_warmup_done: bool, // Track if all textures have been warmed up
        warmup_render_count: u32, // Count renders to ensure warmup completes
    },
}

/// ImageTab represents a single tab displaying a JXL image (single or animated)
pub struct ImageTab {
    pub file_path: Option<PathBuf>,
    animation_state: Option<AnimationState>,
    metadata: Option<ImageMetadata>,
    error_message: Option<String>,
    show_metrics: bool,
    focus_handle: FocusHandle,
    is_loading: bool, // Track if we're currently decoding
    spinner_phase: usize, // For animated spinner
    progressive_image: Option<Arc<RenderImage>>, // Intermediate progressive preview
    progressive_pass: usize, // Current pass being displayed
}

impl ImageTab {
    pub fn new(file_path: Option<PathBuf>, cx: &mut Context<Self>) -> Self {
        log::info!("Creating new ImageTab with file: {:?}", file_path);

        let focus_handle = cx.focus_handle();
        let has_file = file_path.is_some();

        let mut tab = Self {
            file_path: file_path.clone(),
            animation_state: None,
            metadata: None,
            error_message: None,
            show_metrics: false,
            focus_handle,
            is_loading: has_file, // Start loading if we have a file
            spinner_phase: 0,
            progressive_image: None,
            progressive_pass: 0,
        };

        // Start background decoding if we have a file
        if let Some(path) = file_path {
            tab.start_decoding(path, cx);
        }

        tab
    }

    /// Start the spinner animation timer
    fn start_spinner_animation(cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(std::time::Duration::from_millis(100)).await;
            this.update(cx, |this, cx| {
                if this.is_loading {
                    this.spinner_phase = (this.spinner_phase + 1) % 8;
                    cx.notify();
                    Self::start_spinner_animation(cx);
                }
            })
        }).detach();
    }

    /// Start decoding a file in the background with progressive rendering support
    fn start_decoding(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.is_loading = true;
        self.spinner_phase = 0;
        self.progressive_image = None;
        self.progressive_pass = 0;
        cx.notify();

        // Start spinner animation
        Self::start_spinner_animation(cx);

        // Create channel for progressive updates
        let (sender, receiver) = smol::channel::unbounded::<ProgressiveUpdate>();

        // Spawn task to handle progressive updates
        cx.spawn({
            let receiver = receiver.clone();
            async move |this, cx| {
                while let Ok(update) = receiver.recv().await {
                    // Skip non-final updates that are too close together (debounce)
                    if !update.is_final {
                        // Convert rgba to render image in background
                        let rgba_data = update.rgba_data;
                        let width = update.width;
                        let height = update.height;
                        let pass = update.completed_passes;

                        // Create the render image
                        let render_image = smol::unblock(move || {
                            // Convert RGBA to BGRA for GPUI
                            let mut bgra_data = rgba_data;
                            for pixel in bgra_data.chunks_exact_mut(4) {
                                pixel.swap(0, 2);
                            }
                            let img_buffer: image::RgbaImage = image::ImageBuffer::from_raw(
                                width,
                                height,
                                bgra_data,
                            ).expect("Failed to create progressive image buffer");
                            let img_frame = image::Frame::new(img_buffer);
                            Arc::new(RenderImage::new(SmallVec::from_elem(img_frame, 1)))
                        }).await;

                        // Update UI with progressive image
                        let _ = this.update(cx, |this, cx| {
                            this.progressive_image = Some(render_image);
                            this.progressive_pass = pass;
                            cx.notify();
                        });
                    }
                }
            }
        }).detach();

        // Spawn background task for decoding with progressive support
        cx.spawn(async move |this, cx| {
            log::info!("Background decoding started for: {:?}", path);

            // Do ALL heavy lifting in background thread (decoding + image caching)
            let result: Result<(Option<AnimationState>, ImageMetadata), anyhow::Error> = smol::unblock(move || {
                // Use progressive decode - send updates through channel
                let decode_result = worker::decode_jxl_progressive(&path, |update: ProgressiveUpdate| {
                    log::info!(
                        "Progressive update: pass {}/{}, {}x{}, elapsed {:?}, final: {}",
                        update.completed_passes,
                        update.total_passes.map(|t| t.to_string()).unwrap_or_else(|| "?".to_string()),
                        update.width,
                        update.height,
                        update.elapsed,
                        update.is_final
                    );
                    // Send update through channel (ignore errors if receiver dropped)
                    let _ = sender.send_blocking(update);
                })?;

                // Pre-cache frames in background thread too (conversion is CPU intensive)
                match decode_result {
                    DecodeResult::SingleFrame { frame, metadata } => {
                        log::info!("Pre-caching single frame...");
                        let cache_start = std::time::Instant::now();
                        let cached_image = Some(Self::frame_to_render_image(&frame));
                        log::info!("Pre-cached single frame in {:?}", cache_start.elapsed());
                        Ok((Some(AnimationState::SingleFrame { frame, cached_image }), metadata))
                    }
                    DecodeResult::Animation { frames, metadata } => {
                        log::info!("Pre-caching {} animation frames...", frames.len());
                        let cache_start = std::time::Instant::now();
                        let cached_images = Self::precache_animation_frames(&frames);
                        log::info!("Pre-cached all frames in {:?}", cache_start.elapsed());
                        Ok((Some(AnimationState::Animation {
                            frames,
                            cached_images,
                            current_frame: 0,
                            is_playing: true,
                            playback_started: false,
                            gpu_warmup_done: false,
                            warmup_render_count: 0,
                        }), metadata))
                    }
                }
            }).await;

            // Update UI with result (minimal work on main thread)
            this.update(cx, |this, cx| {
                this.is_loading = false;
                this.progressive_image = None; // Clear progressive preview

                match result {
                    Ok((animation_state, metadata)) => {
                        log::info!("Loaded image: {}x{}", metadata.width, metadata.height);
                        this.animation_state = animation_state;
                        this.metadata = Some(metadata);
                    }
                    Err(e) => {
                        let msg = format!("Failed to decode image: {}", e);
                        log::error!("{}", msg);
                        this.error_message = Some(msg);
                    }
                }

                cx.notify();
            }).ok();
        }).detach();
    }

    fn schedule_next_frame(cx: &mut Context<Self>, duration_ms: u32) {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(std::time::Duration::from_millis(duration_ms as u64)).await;
            this.update(cx, |this, cx| {
                this.advance_frame_if_needed(cx);
            })
        })
        .detach();
    }

    fn advance_frame_if_needed(&mut self, cx: &mut Context<Self>) {
        if let Some(AnimationState::Animation {
            frames,
            current_frame,
            is_playing,
            ..
        }) = &mut self.animation_state
        {
            if *is_playing && !frames.is_empty() {
                // Move to next frame
                *current_frame = (*current_frame + 1) % frames.len();

                cx.notify();

                let frame = &frames[*current_frame];
                let duration_ms = frame.duration_ms.max(16);

                Self::schedule_next_frame(cx, duration_ms);
            }
        }
    }

    pub fn toggle_playback(&mut self, cx: &mut Context<Self>) {
        if let Some(AnimationState::Animation {
            frames,
            current_frame,
            is_playing,
            ..
        }) = &mut self.animation_state
        {
            *is_playing = !*is_playing;
            log::info!("Animation playback: {}", if *is_playing { "PLAYING" } else { "PAUSED" });

            if *is_playing && !frames.is_empty() {
                // Use the duration of the current frame
                let duration_ms = frames[*current_frame].duration_ms.max(16);
                Self::schedule_next_frame(cx, duration_ms);
            }

            cx.notify();
        }
    }

    pub fn next_frame(&mut self, cx: &mut Context<Self>) {
        if let Some(AnimationState::Animation {
            frames,
            current_frame,
            ..
        }) = &mut self.animation_state
        {
            if !frames.is_empty() {
                *current_frame = (*current_frame + 1) % frames.len();
                log::info!("Next frame: {}/{}", *current_frame + 1, frames.len());
                cx.notify();
            }
        }
    }

    pub fn previous_frame(&mut self, cx: &mut Context<Self>) {
        if let Some(AnimationState::Animation {
            frames,
            current_frame,
            ..
        }) = &mut self.animation_state
        {
            if !frames.is_empty() {
                *current_frame = if *current_frame == 0 {
                    frames.len() - 1
                } else {
                    *current_frame - 1
                };
                log::info!("Previous frame: {}/{}", *current_frame + 1, frames.len());
                cx.notify();
            }
        }
    }

    fn get_current_frame(&self) -> Option<&DecodedFrame> {
        match &self.animation_state {
            Some(AnimationState::SingleFrame { frame, .. }) => Some(frame),
            Some(AnimationState::Animation { frames, current_frame, .. }) => {
                frames.get(*current_frame)
            }
            None => None,
        }
    }

    pub fn toggle_metrics(&mut self, cx: &mut Context<Self>) {
        self.show_metrics = !self.show_metrics;
        log::info!("Metrics display: {}", if self.show_metrics { "ON" } else { "OFF" });
        cx.notify();
    }
}

impl Focusable for ImageTab {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ImageTab {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Handle GPU warmup and animation playback start
        if let Some(AnimationState::Animation {
            frames,
            is_playing,
            playback_started,
            gpu_warmup_done,
            warmup_render_count,
            ..
        }) = &mut self.animation_state
        {
            if !*gpu_warmup_done {
                // Increment warmup render count
                *warmup_render_count += 1;
                log::debug!("GPU warmup render cycle {}", warmup_render_count);

                // After 3 render cycles, consider GPU warmed up
                // (first render uploads textures, subsequent renders ensure they're ready)
                if *warmup_render_count >= 3 {
                    *gpu_warmup_done = true;
                    log::info!("GPU warmup complete after {} render cycles", warmup_render_count);
                }

                // Always notify during warmup to ensure we get enough render cycles
                cx.notify();
            }

            // Start playback if warmed up (separate check to run in same cycle as warmup complete)
            if *gpu_warmup_done && *is_playing && !*playback_started && !frames.is_empty() {
                *playback_started = true;
                log::info!("Starting animation playback (GPU warmed up), first frame duration: {}ms", frames[0].duration_ms);
                // Start immediately with a short delay to allow render to complete
                Self::schedule_next_frame(cx, 16); // Start with minimal delay
            }
        }

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0x2a2a2a))
            .child(
                // Main content area
                if self.is_loading {
                    // Loading state with spinner
                    self.render_loading()
                } else if let Some(error) = &self.error_message {
                    // Error state
                    self.render_error(error)
                } else if self.animation_state.is_some() {
                    // Image display
                    self.render_image()
                } else {
                    // Empty state
                    self.render_empty()
                }
            )
    }
}

impl ImageTab {
    fn render_metrics_overlay(&self) -> Option<gpui::Div> {
        if !self.show_metrics {
            return None;
        }

        let frame = self.get_current_frame()?;
        let metadata = self.metadata.as_ref()?;

        // Get animation-specific info
        let (current_frame_num, total_frames, is_playing) = match &self.animation_state {
            Some(AnimationState::Animation { frames, current_frame, is_playing, .. }) => {
                (Some(*current_frame + 1), frames.len(), Some(*is_playing))
            }
            _ => (None, 1, None)
        };

        // Calculate throughput (pixels per second)
        let total_pixels = frame.width as f64 * frame.height as f64;
        let decode_secs = frame.decode_time.as_secs_f64();
        let throughput = if decode_secs > 0.0 {
            (total_pixels / decode_secs) / 1_000_000.0 // Convert to megapixels/sec
        } else {
            0.0
        };

        Some(
            div()
                .absolute()
                .top_4()
                .left_4()
                .p_4()
                .bg(gpui::rgba(0x000000D9)) // 0xD9 ≈ 85% opacity
                .rounded_lg()
                .border_1()
                .border_color(rgb(0x444444))
                .flex()
                .flex_col()
                .gap_2()
                .text_xs()
                .text_color(white())
                .child(
                    div()
                        .text_sm()
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(rgb(0x88ff88))
                        .child("Performance Metrics")
                )
                .child(div().h_px().bg(rgb(0x444444)))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap_2()
                        .child(div().text_color(rgb(0xaaaaaa)).child("Dimensions:"))
                        .child(div().child(format!("{}x{} px", frame.width, frame.height)))
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap_2()
                        .child(div().text_color(rgb(0xaaaaaa)).child("Bit Depth:"))
                        .child(div().child(metadata.bit_depth.clone()))
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap_2()
                        .child(div().text_color(rgb(0xaaaaaa)).child("Decode Time:"))
                        .child(div().child(format!("{:.2} ms", decode_secs * 1000.0)))
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap_2()
                        .child(div().text_color(rgb(0xaaaaaa)).child("Throughput:"))
                        .child(div().child(format!("{:.2} MP/s", throughput)))
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap_2()
                        .child(div().text_color(rgb(0xaaaaaa)).child("Total Pixels:"))
                        .child(div().child(format!("{:.2} MP", total_pixels / 1_000_000.0)))
                )
                .when(metadata.has_animation, |this| {
                    this.child(div().h_px().bg(rgb(0x444444)))
                        .when_some(current_frame_num, |this, frame_num| {
                            let playing = is_playing.unwrap_or(false);
                            this.child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .gap_2()
                                    .child(div().text_color(rgb(0xaaaaaa)).child("Frame:"))
                                    .child(div().child(format!("{} / {}", frame_num, total_frames)))
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .gap_2()
                                    .child(div().text_color(rgb(0xaaaaaa)).child("Status:"))
                                    .child(div()
                                        .text_color(if playing { rgb(0x88ff88) } else { rgb(0xffaa88) })
                                        .child(if playing { "Playing" } else { "Paused" }))
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .gap_2()
                                    .child(div().text_color(rgb(0xaaaaaa)).child("Frame Duration:"))
                                    .child(div().child(format!("{}ms", frame.duration_ms)))
                            )
                        })
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .gap_2()
                                .child(div().text_color(rgb(0xaaaaaa)).child("Total Frames:"))
                                .child(div().child(format!("{}", metadata.frame_count)))
                        )
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .gap_2()
                                .child(div().text_color(rgb(0xaaaaaa)).child("Loop Count:"))
                                .child(div().child(if metadata.loop_count == 0 {
                                    "Infinite".to_string()
                                } else {
                                    metadata.loop_count.to_string()
                                }))
                        )
                })
                .child(div().h_px().bg(rgb(0x444444)))
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(0x666666))
                        .child(if metadata.has_animation {
                            "Press 'i' to toggle, SPACE to play/pause, ←/→ for frames"
                        } else {
                            "Press 'i' or Cmd+I to toggle"
                        })
                )
        )
    }

    /// Convert a frame's RGBA data to RenderImage for GPU display
    /// This avoids expensive PNG encoding by using raw pixel data directly
    fn frame_to_render_image(frame: &DecodedFrame) -> Arc<RenderImage> {
        // Clone the RGBA data and convert to BGRA (GPUI uses BGRA internally)
        let mut bgra_data = frame.rgba_data.clone();
        for pixel in bgra_data.chunks_exact_mut(4) {
            pixel.swap(0, 2); // Swap R and B channels: RGBA -> BGRA
        }

        // Create an ImageBuffer from the raw BGRA data
        let img_buffer: RgbaImage = ImageBuffer::from_raw(
            frame.width,
            frame.height,
            bgra_data,
        )
        .expect("Failed to create image buffer");

        // Create a Frame and wrap in RenderImage - no PNG encoding needed!
        let img_frame = image::Frame::new(img_buffer);
        Arc::new(RenderImage::new(SmallVec::from_elem(img_frame, 1)))
    }

    /// Pre-cache all animation frames to avoid stuttering on first playback
    fn precache_animation_frames(frames: &[DecodedFrame]) -> Vec<Option<Arc<RenderImage>>> {
        frames
            .iter()
            .map(|frame| Some(Self::frame_to_render_image(frame)))
            .collect()
    }

    fn render_loading(&self) -> gpui::Div {
        // If we have a progressive image, show it with an overlay
        if let Some(progressive_img) = &self.progressive_image {
            return div()
                .flex()
                .flex_col()
                .size_full()
                .justify_center()
                .items_center()
                .relative()
                .child(
                    // Progressive image preview
                    img(ImageSource::Render(progressive_img.clone()))
                        .max_w_full()
                        .max_h_full()
                        .object_fit(gpui::ObjectFit::Contain)
                )
                .child(
                    // Loading overlay in top-left corner
                    div()
                        .absolute()
                        .top_4()
                        .left_4()
                        .p_3()
                        .bg(gpui::rgba(0x000000CC)) // Semi-transparent black
                        .rounded_lg()
                        .flex()
                        .flex_row()
                        .gap_3()
                        .items_center()
                        .child(
                            // Spinner
                            {
                                const SPINNER_CHARS: [&str; 8] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"];
                                let spinner_char = SPINNER_CHARS[self.spinner_phase % SPINNER_CHARS.len()];
                                div()
                                    .text_lg()
                                    .text_color(rgb(0x88aaff))
                                    .child(spinner_char)
                            }
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(white())
                                        .child(format!("Pass {}", self.progressive_pass))
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(rgb(0x888888))
                                        .child("Progressive rendering...")
                                )
                        )
                );
        }

        // Fallback: standard spinner when no progressive image available
        const SPINNER_CHARS: [&str; 8] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"];
        let spinner_char = SPINNER_CHARS[self.spinner_phase % SPINNER_CHARS.len()];

        div()
            .flex()
            .flex_col()
            .size_full()
            .justify_center()
            .items_center()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .items_center()
                    .child(
                        // Animated spinner using braille characters
                        div()
                            .text_2xl()
                            .text_color(rgb(0x88aaff))
                            .child(spinner_char)
                    )
                    .child(
                        div()
                            .text_base()
                            .text_color(white())
                            .child("Loading...")
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0x888888))
                            .child(
                                if let Some(path) = &self.file_path {
                                    path.file_name()
                                        .and_then(|n| n.to_str())
                                        .unwrap_or("Unknown")
                                        .to_string()
                                } else {
                                    "Unknown file".to_string()
                                }
                            )
                    )
            )
    }

    fn render_empty(&self) -> gpui::Div {
        div()
            .flex()
            .flex_col()
            .size_full()
            .justify_center()
            .items_center()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .items_center()
                    .child(
                        div()
                            .text_2xl()
                            .text_color(white())
                            .child("JXL Viewer")
                    )
                    .child(
                        div()
                            .text_base()
                            .text_color(rgb(0xaaaaaa))
                            .child("No file loaded")
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0x888888))
                            .child("Press Cmd+O to open a file, or drag & drop a .jxl file")
                    )
            )
    }

    fn render_error(&self, error: &String) -> gpui::Div {
        div()
            .flex()
            .flex_col()
            .size_full()
            .justify_center()
            .items_center()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .items_center()
                    .max_w(px(600.0))
                    .child(
                        div()
                            .text_xl()
                            .text_color(rgb(0xff5555))
                            .child("Error Loading Image")
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0xcccccc))
                            .child(error.clone())
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(0x888888))
                            .child(
                                if let Some(path) = &self.file_path {
                                    format!("File: {}", path.display())
                                } else {
                                    "Unknown file".to_string()
                                }
                            )
                    )
            )
    }

    fn render_image(&mut self) -> gpui::Div {
        // Check if we need to render all frames for GPU warmup
        let warmup_images: Option<Vec<Arc<RenderImage>>> = match &self.animation_state {
            Some(AnimationState::Animation { cached_images, gpu_warmup_done, .. }) if !gpu_warmup_done => {
                // During warmup, collect ALL cached images to render them all
                Some(cached_images.iter().filter_map(|img| img.clone()).collect())
            }
            _ => None,
        };

        // Get cached image directly (all frames pre-cached for animations)
        let (gpui_image, _frame_width, _frame_height) = match &mut self.animation_state {
            Some(AnimationState::SingleFrame { frame, cached_image }) => {
                let img = if let Some(cached) = cached_image {
                    cached.clone()
                } else {
                    // Cache single frame on first render
                    let cached = Self::frame_to_render_image(frame);
                    *cached_image = Some(cached.clone());
                    cached
                };
                (img, frame.width, frame.height)
            }
            Some(AnimationState::Animation { frames, cached_images, current_frame, .. }) => {
                let img = cached_images[*current_frame]
                    .as_ref()
                    .expect("Frame should be pre-cached")
                    .clone();
                let frame = &frames[*current_frame];
                (img, frame.width, frame.height)
            }
            None => {
                panic!("render_image called without animation state");
            }
        };

        // Get frame reference for metadata (after mutable borrow is done)
        let frame = self.get_current_frame().expect("Should have frame");
        let metadata = self.metadata.as_ref();

        div()
            .flex()
            .flex_col()
            .size_full()
            .child(
                // Info bar at top
                div()
                    .flex()
                    .flex_row()
                    .w_full()
                    .p_2()
                    .bg(rgb(0x1a1a1a))
                    .border_b_1()
                    .border_color(rgb(0x404040))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_4()
                            .text_xs()
                            .text_color(rgb(0xaaaaaa))
                            .child(
                                if let Some(path) = &self.file_path {
                                    path.file_name()
                                        .and_then(|n| n.to_str())
                                        .unwrap_or("Unknown")
                                        .to_string()
                                } else {
                                    "Unknown".to_string()
                                }
                            )
                            .child(format!("{}x{}", frame.width, frame.height))
                            .when_some(metadata, |this, meta| {
                                this.child(meta.bit_depth.clone())
                                    .child(format!("Decoded in {:.2}ms", frame.decode_time.as_secs_f64() * 1000.0))
                            })
                    )
            )
            .child(
                // Image display area with centering
                div()
                    .relative()
                    .flex()
                    .size_full()
                    .justify_center()
                    .items_center()
                    .bg(rgb(0x2a2a2a))
                    .child(
                        // Display image: natural size if smaller than window, scale down if larger
                        // max_w_full/max_h_full constrains to container, object_fit maintains aspect ratio
                        img(ImageSource::Render(gpui_image))
                            .max_w_full()
                            .max_h_full()
                            .object_fit(gpui::ObjectFit::Contain)
                    )
                    // During GPU warmup: render ALL frames at full size but off-screen
                    // This forces GPU to upload full textures (not just 1x1 scaled down)
                    .when_some(warmup_images, |this, images| {
                        this.child(
                            div()
                                .absolute()
                                .top(px(-10000.0)) // Position way off-screen
                                .left(px(-10000.0))
                                .children(images.into_iter().map(|image| {
                                    // Render at actual frame size to force full texture upload
                                    img(ImageSource::Render(image))
                                }))
                        )
                    })
                    .when_some(self.render_metrics_overlay(), |this, overlay| {
                        this.child(overlay)
                    })
            )
    }
}
