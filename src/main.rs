//! JXL-UI - A beautiful cross-platform JPEG XL viewer

mod decoder;
mod encoder;

use eframe::egui::{self, Color32, RichText, Rounding, Stroke, Vec2};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use decoder::{DecoderSettings, OutputColorType, OutputDataType};

// Black matte theme - matching mobile + januschka.com
mod theme {
    use eframe::egui::Color32;

    pub const BG_BASE: Color32 = Color32::from_rgb(18, 18, 18); // #121212
    pub const BG_ELEVATED: Color32 = Color32::from_rgb(24, 24, 24); // close to #1E1E1E
    pub const BG_SURFACE: Color32 = Color32::from_rgb(30, 30, 30); // #1E1E1E
    pub const BG_HOVER: Color32 = Color32::from_rgb(42, 42, 42);
    pub const BG_ACTIVE: Color32 = Color32::from_rgb(51, 51, 51); // #333333

    pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(190, 190, 190);
    pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(161, 161, 170);
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(138, 138, 141);

    pub const ACCENT: Color32 = Color32::from_rgb(255, 193, 7); // #FFC107
    #[allow(dead_code)]
    pub const ACCENT_HOVER: Color32 = Color32::from_rgb(255, 214, 80);
    #[allow(dead_code)]
    pub const ACCENT_MUTED: Color32 = Color32::from_rgb(230, 142, 13);

    pub const BORDER: Color32 = Color32::from_rgb(51, 51, 51);
    pub const BORDER_SUBTLE: Color32 = Color32::from_rgb(42, 42, 42);

    pub const ERROR: Color32 = Color32::from_rgb(211, 95, 95);
    pub const SUCCESS: Color32 = Color32::from_rgb(34, 197, 94);
}

fn main() -> eframe::Result<()> {
    env_logger::init();

    let args: Vec<String> = std::env::args().collect();
    let initial_file = args.get(1).map(PathBuf::from);

    // Load app icon from embedded PNG
    let icon_data = {
        let icon_bytes = include_bytes!("../assets/icon.png");
        let image = image::load_from_memory(icon_bytes).expect("Failed to load icon");
        let rgba = image.to_rgba8();
        let (w, h) = rgba.dimensions();
        egui::IconData {
            rgba: rgba.into_raw(),
            width: w,
            height: h,
        }
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 820.0])
            .with_title("JXL-UI")
            .with_decorations(true)
            .with_icon(std::sync::Arc::new(icon_data)),
        ..Default::default()
    };

    eframe::run_native(
        "JXL-UI",
        options,
        Box::new(|cc| {
            setup_fonts(&cc.egui_ctx);
            setup_style(&cc.egui_ctx);
            Ok(Box::new(JxlApp::new(cc, initial_file)))
        }),
    )
}

fn setup_fonts(ctx: &egui::Context) {
    // Prefer monospace typography across the app for terminal-style look.
    let mut fonts = egui::FontDefinitions::default();
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "Hack".to_owned());
    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .insert(0, "Hack".to_owned());
    ctx.set_fonts(fonts);
}

fn setup_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();

    // Minimal rounding - modern flat design
    style.visuals.window_rounding = Rounding::same(12.0);
    style.visuals.menu_rounding = Rounding::same(8.0);

    // Widget rounding
    let widget_rounding = Rounding::same(6.0);
    style.visuals.widgets.noninteractive.rounding = widget_rounding;
    style.visuals.widgets.inactive.rounding = widget_rounding;
    style.visuals.widgets.hovered.rounding = widget_rounding;
    style.visuals.widgets.active.rounding = widget_rounding;

    // Remove harsh shadows
    style.visuals.popup_shadow = egui::epaint::Shadow::NONE;
    style.visuals.window_shadow = egui::epaint::Shadow {
        offset: Vec2::new(0.0, 8.0),
        blur: 32.0,
        spread: 0.0,
        color: Color32::from_black_alpha(60),
    };

    // Colors
    style.visuals.panel_fill = theme::BG_BASE;
    style.visuals.window_fill = theme::BG_ELEVATED;
    style.visuals.extreme_bg_color = theme::BG_BASE;
    style.visuals.faint_bg_color = theme::BG_SURFACE;

    // Widget colors
    style.visuals.widgets.noninteractive.bg_fill = Color32::TRANSPARENT;
    style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, theme::TEXT_MUTED);
    style.visuals.widgets.inactive.bg_fill = theme::BG_SURFACE;
    style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, theme::TEXT_SECONDARY);
    style.visuals.widgets.hovered.bg_fill = theme::BG_HOVER;
    style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, theme::TEXT_PRIMARY);
    style.visuals.widgets.active.bg_fill = theme::BG_ACTIVE;
    style.visuals.widgets.active.fg_stroke = Stroke::new(1.0, theme::TEXT_PRIMARY);

    // Selection
    style.visuals.selection.bg_fill = theme::ACCENT.gamma_multiply(0.3);
    style.visuals.selection.stroke = Stroke::new(1.0, theme::ACCENT);

    // Spacing - generous but not wasteful
    style.spacing.item_spacing = Vec2::new(8.0, 6.0);
    style.spacing.button_padding = Vec2::new(12.0, 6.0);
    style.spacing.window_margin = egui::Margin::same(16.0);

    // Interaction
    style.interaction.show_tooltips_only_when_still = false;

    ctx.set_style(style);
}

enum DecoderMessage {
    ProgressiveUpdate {
        rgba: Vec<u8>,
        width: u32,
        height: u32,
        completed_passes: usize,
        is_final: bool,
        elapsed: Duration,
    },
    AnimationFrame {
        rgba: Vec<u8>,
        width: u32,
        height: u32,
        duration_ms: u32,
        frame_index: usize,
        total_frames: usize,
    },
    Complete,
    Error(String),
}

enum EncodeMessage {
    Progress(String),
    Success {
        output: PathBuf,
        stats: encoder::EncodeStats,
    },
    Error(String),
}

struct EncodeState {
    input_path: Option<PathBuf>,
    output_path: Option<PathBuf>,
    settings: encoder::EncoderSettings,
    is_encoding: bool,
    started_at: Option<Instant>,
    rx: Option<Receiver<EncodeMessage>>,
    last_status: Option<String>,
    last_error: Option<String>,
    last_output: Option<PathBuf>,
}

impl Default for EncodeState {
    fn default() -> Self {
        Self {
            input_path: None,
            output_path: None,
            settings: encoder::EncoderSettings::default(),
            is_encoding: false,
            started_at: None,
            rx: None,
            last_status: None,
            last_error: None,
            last_output: None,
        }
    }
}

struct AnimationState {
    frames: Vec<egui::TextureHandle>,
    durations: Vec<u32>,
    current_frame: usize,
    last_frame_time: Instant,
    is_playing: bool,
}

struct ImageTab {
    id: usize,
    title: String,
    file_path: Option<PathBuf>,
    texture: Option<egui::TextureHandle>,
    animation: Option<AnimationState>,
    dimensions: Option<(u32, u32)>,
    decode_time: Option<Duration>,
    is_loading: bool,
    error: Option<String>,
    decoder_rx: Option<Receiver<DecoderMessage>>,
    // Compare mode: reference (non-progressive) decode
    reference_texture: Option<egui::TextureHandle>,
    reference_decode_time: Option<Duration>,
    reference_is_loading: bool,
    reference_rx: Option<Receiver<DecoderMessage>>,
    // Zoom & pan
    zoom: f32,      // 1.0 = fit-to-window, >1.0 = zoomed in
    zoom_fit: bool, // true = auto-fit to window
    pan: Vec2,      // pan offset in screen pixels
}

impl ImageTab {
    fn new(id: usize) -> Self {
        Self {
            id,
            title: "New Tab".to_string(),
            file_path: None,
            texture: None,
            animation: None,
            dimensions: None,
            decode_time: None,
            is_loading: false,
            error: None,
            decoder_rx: None,
            reference_texture: None,
            reference_decode_time: None,
            reference_is_loading: false,
            reference_rx: None,
            zoom: 1.0,
            zoom_fit: true,
            pan: Vec2::ZERO,
        }
    }

    fn load_file(&mut self, path: PathBuf, settings: DecoderSettings, compare_mode: bool) {
        self.title = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Image".to_string());
        self.is_loading = true;
        self.error = None;
        self.texture = None;
        self.animation = None;
        self.dimensions = None;
        self.decode_time = None;
        self.file_path = Some(path.clone());
        self.reference_texture = None;
        self.reference_decode_time = None;
        self.reference_is_loading = false;
        self.reference_rx = None;

        let (tx, rx) = channel();
        self.decoder_rx = Some(rx);

        if compare_mode {
            let (ref_tx, ref_rx) = channel();
            self.reference_rx = Some(ref_rx);
            self.reference_is_loading = true;

            let settings_clone = settings.clone();
            let path_clone = path.clone();
            // Both decoders run in parallel -- jxl-rs is single-threaded
            // internally so they won't heavily compete for CPU
            thread::spawn(move || {
                decode_file(path_clone, tx, settings_clone);
            });
            thread::spawn(move || {
                decode_file_standard(path, ref_tx, settings);
            });
        } else {
            thread::spawn(move || {
                decode_file(path, tx, settings);
            });
        }
    }

    fn process_messages(&mut self, ctx: &egui::Context, tex_options: egui::TextureOptions) {
        if let Some(rx) = &self.decoder_rx {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    DecoderMessage::ProgressiveUpdate {
                        rgba,
                        width,
                        height,
                        completed_passes,
                        is_final,
                        elapsed,
                    } => {
                        let image = egui::ColorImage::from_rgba_unmultiplied(
                            [width as usize, height as usize],
                            &rgba,
                        );
                        self.texture = Some(ctx.load_texture(
                            format!("tab-{}-pass-{}", self.id, completed_passes),
                            image,
                            tex_options,
                        ));
                        self.dimensions = Some((width, height));
                        if is_final {
                            self.decode_time = Some(elapsed);
                            self.is_loading = false;
                        }
                    }
                    DecoderMessage::AnimationFrame {
                        rgba,
                        width,
                        height,
                        duration_ms,
                        frame_index,
                        total_frames,
                    } => {
                        let image = egui::ColorImage::from_rgba_unmultiplied(
                            [width as usize, height as usize],
                            &rgba,
                        );
                        let texture = ctx.load_texture(
                            format!("tab-{}-frame-{}", self.id, frame_index),
                            image,
                            tex_options,
                        );

                        if self.animation.is_none() {
                            self.animation = Some(AnimationState {
                                frames: Vec::with_capacity(total_frames),
                                durations: Vec::with_capacity(total_frames),
                                current_frame: 0,
                                last_frame_time: Instant::now(),
                                is_playing: true,
                            });
                        }

                        if let Some(anim) = &mut self.animation {
                            anim.frames.push(texture);
                            anim.durations.push(duration_ms);
                        }

                        self.dimensions = Some((width, height));
                    }
                    DecoderMessage::Complete => {
                        self.is_loading = false;
                    }
                    DecoderMessage::Error(e) => {
                        self.error = Some(e);
                        self.is_loading = false;
                    }
                }
            }
        }
        // Process reference (non-progressive) decoder messages
        if let Some(rx) = &self.reference_rx {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    DecoderMessage::ProgressiveUpdate {
                        rgba,
                        width,
                        height,
                        is_final,
                        elapsed,
                        ..
                    } => {
                        let image = egui::ColorImage::from_rgba_unmultiplied(
                            [width as usize, height as usize],
                            &rgba,
                        );
                        self.reference_texture = Some(ctx.load_texture(
                            format!("tab-{}-ref", self.id),
                            image,
                            tex_options,
                        ));
                        if is_final {
                            self.reference_decode_time = Some(elapsed);
                            self.reference_is_loading = false;
                        }
                    }
                    DecoderMessage::Complete => {
                        self.reference_is_loading = false;
                    }
                    DecoderMessage::Error(_) => {
                        self.reference_is_loading = false;
                    }
                    _ => {}
                }
            }
        }
    }

    fn update_animation(&mut self, ctx: &egui::Context) {
        if let Some(anim) = &mut self.animation {
            if anim.is_playing && !anim.frames.is_empty() {
                let current_duration = anim
                    .durations
                    .get(anim.current_frame)
                    .copied()
                    .unwrap_or(100);
                if anim.last_frame_time.elapsed() >= Duration::from_millis(current_duration as u64)
                {
                    anim.current_frame = (anim.current_frame + 1) % anim.frames.len();
                    anim.last_frame_time = Instant::now();
                    ctx.request_repaint();
                }
                ctx.request_repaint_after(Duration::from_millis(current_duration as u64));
            }
        }
    }
}

struct JxlApp {
    tabs: Vec<ImageTab>,
    active_tab: usize,
    next_tab_id: usize,
    show_about: bool,
    show_info: bool,
    show_settings: bool,
    show_encoder: bool,
    decoder_settings: DecoderSettings,
    compare_mode: bool,
    nearest_filter: bool, // false = linear (smooth), true = nearest (sharp pixels)
    encoder_state: EncodeState,
}

impl JxlApp {
    fn new(_cc: &eframe::CreationContext<'_>, initial_file: Option<PathBuf>) -> Self {
        let mut app = Self {
            tabs: vec![],
            active_tab: 0,
            next_tab_id: 0,
            show_about: false,
            show_info: false,
            show_settings: false,
            show_encoder: false,
            decoder_settings: DecoderSettings::default(),
            compare_mode: false,
            nearest_filter: false,
            encoder_state: EncodeState::default(),
        };

        if let Some(path) = initial_file {
            app.open_file_in_new_tab(path);
        } else {
            app.tabs.push(ImageTab::new(app.next_tab_id));
            app.next_tab_id += 1;
        }

        app
    }

    fn open_file_in_new_tab(&mut self, path: PathBuf) {
        let mut tab = ImageTab::new(self.next_tab_id);
        self.next_tab_id += 1;
        tab.load_file(path, self.decoder_settings.clone(), self.compare_mode);
        self.tabs.push(tab);
        self.active_tab = self.tabs.len() - 1;
    }

    fn open_file_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("JPEG XL", &["jxl", "JXL"])
            .add_filter("All files", &["*"])
            .pick_file()
        {
            self.open_file_in_new_tab(path);
        }
    }

    fn prefill_encoder_from_active_tab(&mut self) {
        if self.encoder_state.input_path.is_some() {
            return;
        }

        if let Some(tab) = self.tabs.get(self.active_tab) {
            if let Some(path) = &tab.file_path {
                self.encoder_state.input_path = Some(path.clone());
                self.encoder_state.output_path = Some(encoder::suggest_output_path(path));
            }
        }
    }

    fn pick_encoder_input(&mut self) {
        if let Some(path) = rfd::FileDialog::new().pick_file() {
            self.encoder_state.output_path = Some(encoder::suggest_output_path(&path));
            self.encoder_state.input_path = Some(path);
            self.encoder_state.last_status = None;
            self.encoder_state.last_error = None;
        }
    }

    fn pick_encoder_output(&mut self) {
        let mut dialog = rfd::FileDialog::new().add_filter("JPEG XL", &["jxl"]);
        if let Some(input) = &self.encoder_state.input_path {
            dialog = dialog.set_file_name(
                encoder::suggest_output_path(input)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("output.jxl"),
            );
        }

        if let Some(path) = dialog.save_file() {
            self.encoder_state.output_path = Some(path);
        }
    }

    fn start_encode(&mut self) {
        if self.encoder_state.is_encoding {
            return;
        }

        let Some(input) = self.encoder_state.input_path.clone() else {
            self.encoder_state.last_error = Some("Select an input file first.".to_string());
            return;
        };
        let Some(output) = self.encoder_state.output_path.clone() else {
            self.encoder_state.last_error = Some("Select an output file first.".to_string());
            return;
        };

        self.encoder_state.last_error = None;
        self.encoder_state.last_status = Some("Queued encoding job...".to_string());
        self.encoder_state.last_output = None;
        self.encoder_state.is_encoding = true;
        self.encoder_state.started_at = Some(Instant::now());

        let settings = self.encoder_state.settings.clone();
        let (tx, rx) = channel();
        self.encoder_state.rx = Some(rx);

        thread::spawn(move || {
            let tx_progress = tx.clone();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                encoder::encode_to_jxl_with_progress(&input, &output, &settings, |msg| {
                    let _ = tx_progress.send(EncodeMessage::Progress(msg));
                })
            }));

            match result {
                Ok(Ok(stats)) => {
                    let _ = tx.send(EncodeMessage::Success { output, stats });
                }
                Ok(Err(e)) => {
                    let _ = tx.send(EncodeMessage::Error(e.to_string()));
                }
                Err(_) => {
                    let _ = tx.send(EncodeMessage::Error(
                        "Encoder worker panicked. Try lower max frames / FPS / max edge."
                            .to_string(),
                    ));
                }
            }
        });
    }

    fn poll_encode_messages(&mut self) {
        if let Some(rx) = &self.encoder_state.rx {
            loop {
                match rx.try_recv() {
                    Ok(msg) => match msg {
                        EncodeMessage::Progress(msg) => {
                            self.encoder_state.last_status = Some(msg);
                        }
                        EncodeMessage::Success { output, stats } => {
                            self.encoder_state.is_encoding = false;
                            self.encoder_state.started_at = None;
                            let compression_ratio = if stats.input_size_bytes > 0 {
                                stats.output_size_bytes as f64 / stats.input_size_bytes as f64
                            } else {
                                0.0
                            };
                            let elapsed_s = stats.elapsed.as_secs_f64().max(0.001);
                            let total_pixels = (stats.width as f64)
                                * (stats.height as f64)
                                * (stats.frames_encoded as f64);
                            let mpx_per_sec = (total_pixels / 1_000_000.0) / elapsed_s;
                            let mut status = format!(
                                    "Encoded {}x{} in {:.1} ms - {} -> {} (ratio {:.2}x, {:.1}% out, {:.2} MP/s)",
                                    stats.width,
                                    stats.height,
                                    stats.elapsed.as_secs_f64() * 1000.0,
                                    format_bytes(stats.input_size_bytes as usize),
                                    format_bytes(stats.output_size_bytes),
                                    compression_ratio,
                                    compression_ratio * 100.0,
                                    mpx_per_sec
                                );
                            if stats.encoded_animation {
                                let enc_fps = (stats.frames_encoded as f64) / elapsed_s;
                                status.push_str(&format!(
                                    " | animated JXL: {} frames @ {}ms (encode {:.2} fps)",
                                    stats.frames_encoded, stats.frame_duration_ms, enc_fps
                                ));
                            } else if stats.source_had_multiple_frames {
                                status.push_str(
                                    " | source had multiple frames but encoded as still image",
                                );
                            }
                            if stats.used_ffmpeg {
                                status.push_str(" | input preprocessed via ffmpeg->PPM");
                            }
                            self.encoder_state.last_status = Some(status);
                            self.encoder_state.last_output = Some(output);
                            self.encoder_state.last_error = None;
                        }
                        EncodeMessage::Error(e) => {
                            self.encoder_state.is_encoding = false;
                            self.encoder_state.started_at = None;
                            self.encoder_state.last_error = Some(e);
                        }
                    },
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        if self.encoder_state.is_encoding {
                            self.encoder_state.is_encoding = false;
                            self.encoder_state.started_at = None;
                            self.encoder_state.last_error =
                                Some("Encoder worker disconnected unexpectedly.".to_string());
                        }
                        break;
                    }
                }
            }
        }
    }

    fn close_tab(&mut self, index: usize) {
        if self.tabs.len() > 1 {
            self.tabs.remove(index);
            if self.active_tab >= self.tabs.len() {
                self.active_tab = self.tabs.len() - 1;
            }
        }
    }
}

impl eframe::App for JxlApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Process messages
        let tex_options = if self.nearest_filter {
            egui::TextureOptions::NEAREST
        } else {
            egui::TextureOptions::LINEAR
        };
        for tab in &mut self.tabs {
            tab.process_messages(ctx, tex_options);
            tab.update_animation(ctx);
        }
        self.poll_encode_messages();
        if self.encoder_state.is_encoding {
            ctx.request_repaint_after(Duration::from_millis(100));
        }

        // Tab bar at top
        egui::TopBottomPanel::top("tab_bar")
            .frame(
                egui::Frame::none()
                    .fill(theme::BG_ELEVATED)
                    .inner_margin(egui::Margin {
                        left: 12.0,
                        right: 12.0,
                        top: 8.0,
                        bottom: 0.0,
                    }),
            )
            .show_separator_line(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.style_mut().spacing.item_spacing.x = 2.0;

                    let mut tab_to_close: Option<usize> = None;

                    for (i, tab) in self.tabs.iter().enumerate() {
                        let is_active = i == self.active_tab;

                        let (bg_color, text_color, border_bottom) = if is_active {
                            (theme::BG_BASE, theme::TEXT_PRIMARY, theme::ACCENT)
                        } else {
                            (
                                Color32::TRANSPARENT,
                                theme::TEXT_MUTED,
                                Color32::TRANSPARENT,
                            )
                        };

                        let response = ui.allocate_ui(Vec2::new(160.0, 36.0), |ui| {
                            let rect = ui.available_rect_before_wrap();

                            // Background
                            ui.painter().rect_filled(
                                rect,
                                Rounding {
                                    nw: 8.0,
                                    ne: 8.0,
                                    sw: 0.0,
                                    se: 0.0,
                                },
                                bg_color,
                            );

                            // Bottom accent line for active tab
                            if is_active {
                                ui.painter().rect_filled(
                                    egui::Rect::from_min_size(
                                        egui::pos2(rect.left(), rect.bottom() - 2.0),
                                        Vec2::new(rect.width(), 2.0),
                                    ),
                                    Rounding::ZERO,
                                    border_bottom,
                                );
                            }

                            ui.allocate_new_ui(
                                egui::UiBuilder::new().max_rect(rect.shrink(8.0)),
                                |ui| {
                                    ui.horizontal_centered(|ui| {
                                        // Loading indicator or icon
                                        if tab.is_loading {
                                            ui.spinner();
                                        }

                                        // Title
                                        let title = if tab.title.len() > 16 {
                                            format!("{}...", &tab.title[..15])
                                        } else {
                                            tab.title.clone()
                                        };

                                        ui.label(RichText::new(title).color(text_color).size(13.0));

                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                // Close button
                                                let close_btn = ui.add(
                                                    egui::Button::new(
                                                        RichText::new("x")
                                                            .size(14.0)
                                                            .color(theme::TEXT_MUTED),
                                                    )
                                                    .frame(false)
                                                    .min_size(Vec2::new(18.0, 18.0)),
                                                );
                                                if close_btn.clicked() {
                                                    tab_to_close = Some(i);
                                                }
                                                if close_btn.hovered() {
                                                    ui.painter().rect_filled(
                                                        close_btn.rect,
                                                        Rounding::same(4.0),
                                                        theme::BG_HOVER,
                                                    );
                                                }
                                            },
                                        );
                                    });
                                },
                            );
                        });

                        if response.response.interact(egui::Sense::click()).clicked() {
                            self.active_tab = i;
                        }
                    }

                    // New tab button
                    ui.add_space(4.0);
                    let new_tab_btn = ui.add(
                        egui::Button::new(RichText::new("+").size(16.0).color(theme::TEXT_MUTED))
                            .frame(false)
                            .min_size(Vec2::new(28.0, 28.0)),
                    );
                    if new_tab_btn.clicked() {
                        self.tabs.push(ImageTab::new(self.next_tab_id));
                        self.next_tab_id += 1;
                        self.active_tab = self.tabs.len() - 1;
                    }

                    if let Some(i) = tab_to_close {
                        self.close_tab(i);
                    }

                    // Right side - menu buttons
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(4.0);

                        // About button
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("?").size(13.0).color(theme::TEXT_MUTED),
                                )
                                .frame(false)
                                .min_size(Vec2::new(24.0, 24.0)),
                            )
                            .on_hover_text("About")
                            .clicked()
                        {
                            self.show_about = true;
                        }

                        // Settings button
                        let settings_color = if self.show_settings {
                            theme::ACCENT
                        } else {
                            theme::TEXT_MUTED
                        };
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("⚙").size(14.0).color(settings_color),
                                )
                                .frame(false)
                                .min_size(Vec2::new(24.0, 24.0)),
                            )
                            .on_hover_text("Decoder Settings (S)")
                            .clicked()
                        {
                            self.show_settings = !self.show_settings;
                        }

                        // Encode button
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Encode")
                                        .size(13.0)
                                        .color(theme::TEXT_SECONDARY),
                                )
                                .fill(theme::BG_SURFACE)
                                .rounding(Rounding::same(6.0)),
                            )
                            .clicked()
                        {
                            self.show_encoder = true;
                            self.prefill_encoder_from_active_tab();
                        }

                        // Open button
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Open")
                                        .size(13.0)
                                        .color(theme::TEXT_SECONDARY),
                                )
                                .fill(theme::BG_SURFACE)
                                .rounding(Rounding::same(6.0)),
                            )
                            .clicked()
                        {
                            self.open_file_dialog();
                        }
                    });
                });
            });

        // Status bar
        let mut toggle_anim = false;
        egui::TopBottomPanel::bottom("status")
            .frame(
                egui::Frame::none()
                    .fill(theme::BG_ELEVATED)
                    .stroke(Stroke::new(1.0, theme::BORDER_SUBTLE))
                    .inner_margin(egui::Margin::symmetric(16.0, 8.0)),
            )
            .show_separator_line(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if let Some(tab) = self.tabs.get(self.active_tab) {
                        // File info
                        if let Some((w, h)) = tab.dimensions {
                            ui.label(
                                RichText::new(format!("{}x{}", w, h))
                                    .size(12.0)
                                    .color(theme::TEXT_MUTED),
                            );
                        }

                        if let Some(time) = tab.decode_time {
                            ui.label(RichText::new("-").size(12.0).color(theme::TEXT_MUTED));
                            ui.label(
                                RichText::new(format!("{:.0}ms", time.as_secs_f64() * 1000.0))
                                    .size(12.0)
                                    .color(theme::TEXT_MUTED),
                            );
                        }

                        // Zoom indicator
                        if !tab.zoom_fit || tab.zoom != 1.0 {
                            ui.label(RichText::new("-").size(12.0).color(theme::TEXT_MUTED));
                            let zoom_label = if tab.zoom_fit {
                                format!("Fit x{:.0}%", tab.zoom * 100.0)
                            } else {
                                format!("1:1 x{:.0}%", tab.zoom * 100.0)
                            };
                            ui.label(RichText::new(zoom_label).size(12.0).color(theme::ACCENT));
                        }

                        // Animation controls
                        if let Some(anim) = &tab.animation {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    // Play/pause
                                    let icon = if anim.is_playing { "⏸" } else { "▶" };
                                    if ui
                                        .add(
                                            egui::Button::new(RichText::new(icon).size(12.0))
                                                .fill(theme::BG_SURFACE)
                                                .min_size(Vec2::new(28.0, 22.0)),
                                        )
                                        .clicked()
                                    {
                                        toggle_anim = true;
                                    }

                                    ui.label(
                                        RichText::new(format!(
                                            "{}/{}",
                                            anim.current_frame + 1,
                                            anim.frames.len()
                                        ))
                                        .size(12.0)
                                        .color(theme::TEXT_MUTED),
                                    );
                                },
                            );
                        }
                    }
                });
            });

        if toggle_anim {
            if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                if let Some(anim) = &mut tab.animation {
                    anim.is_playing = !anim.is_playing;
                }
            }
        }

        // Main content
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(theme::BG_BASE))
            .show(ctx, |ui| {
                // Keyboard shortcuts
                if ui.input(|i| i.key_pressed(egui::Key::O) && i.modifiers.command) {
                    self.open_file_dialog();
                }
                if ui.input(|i| i.key_pressed(egui::Key::E) && i.modifiers.command) {
                    self.show_encoder = true;
                    self.prefill_encoder_from_active_tab();
                }
                if ui.input(|i| i.key_pressed(egui::Key::T) && i.modifiers.command) {
                    self.tabs.push(ImageTab::new(self.next_tab_id));
                    self.next_tab_id += 1;
                    self.active_tab = self.tabs.len() - 1;
                }
                if ui.input(|i| i.key_pressed(egui::Key::W) && i.modifiers.command) {
                    if self.tabs.len() > 1 {
                        self.close_tab(self.active_tab);
                    }
                }
                if ui.input(|i| i.key_pressed(egui::Key::Space)) {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        if let Some(anim) = &mut tab.animation {
                            anim.is_playing = !anim.is_playing;
                        }
                    }
                }
                // Show about with ? key
                if ui.input(|i| i.key_pressed(egui::Key::Questionmark)) {
                    self.show_about = true;
                }
                // Show info with i key
                if ui.input(|i| i.key_pressed(egui::Key::I)) {
                    self.show_info = !self.show_info;
                }
                // Show settings with s key
                if ui.input(|i| i.key_pressed(egui::Key::S) && !i.modifiers.command) {
                    self.show_settings = !self.show_settings;
                }
                // Reload with current settings (R key)
                if ui.input(|i| i.key_pressed(egui::Key::R) && !i.modifiers.command) {
                    let settings = self.decoder_settings.clone();
                    let compare = self.compare_mode;
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        if let Some(path) = tab.file_path.clone() {
                            tab.load_file(path, settings, compare);
                        }
                    }
                }
                // Zoom: 1 = 1:1 pixel, F = fit-to-window, +/- = zoom in/out
                if ui.input(|i| i.key_pressed(egui::Key::Num1) && !i.modifiers.command) {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        if tab.dimensions.is_some() {
                            tab.zoom_fit = false;
                            tab.zoom = 1.0;
                            tab.pan = Vec2::ZERO;
                        }
                    }
                }
                if ui.input(|i| i.key_pressed(egui::Key::F) && !i.modifiers.command) {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        tab.zoom_fit = true;
                        tab.zoom = 1.0;
                        tab.pan = Vec2::ZERO;
                    }
                }
                // Keyboard zoom (+/-)
                if ui.input(|i| i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals))
                {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        let old_zoom = tab.zoom;
                        tab.zoom = (tab.zoom * 1.25).min(50.0);
                        // Scale pan to keep center stable
                        tab.pan = tab.pan * (tab.zoom / old_zoom);
                    }
                }
                if ui.input(|i| i.key_pressed(egui::Key::Minus)) {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        let old_zoom = tab.zoom;
                        tab.zoom = (tab.zoom / 1.25).max(0.1);
                        tab.pan = tab.pan * (tab.zoom / old_zoom);
                        // Reset pan when zoomed out to ~1.0
                        if tab.zoom_fit && tab.zoom < 1.05 {
                            tab.zoom = 1.0;
                            tab.pan = Vec2::ZERO;
                        }
                    }
                }
                // Mouse wheel zoom -- zoom toward cursor position
                // Only zoom when pointer is over the image area (not over floating panels)
                let content_rect = ui.available_rect_before_wrap();
                let (scroll_delta, pointer_pos) =
                    ui.input(|i| (i.smooth_scroll_delta.y, i.pointer.hover_pos()));
                // Don't zoom when pointer is over a floating panel (settings/info/about)
                let pointer_over_panel = ctx
                    .layer_id_at(pointer_pos.unwrap_or_default())
                    .map_or(false, |layer| layer.order == egui::Order::Middle);
                if scroll_delta != 0.0 && !pointer_over_panel {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        let old_zoom = tab.zoom;
                        let factor = if scroll_delta > 0.0 { 1.1 } else { 1.0 / 1.1 };
                        tab.zoom = (tab.zoom * factor).clamp(0.1, 50.0);

                        // Zoom toward cursor: adjust pan so the point under the
                        // cursor stays fixed
                        if let Some(mouse) = pointer_pos {
                            let center = content_rect.center();
                            // Vector from image center (with pan) to cursor
                            let cursor_offset = mouse - (center + tab.pan);
                            // Scale the pan so cursor point stays put
                            let zoom_ratio = tab.zoom / old_zoom;
                            tab.pan = tab.pan - cursor_offset * (zoom_ratio - 1.0);
                        }

                        // Reset when zooming back to fit
                        if tab.zoom_fit && tab.zoom < 1.05 {
                            tab.zoom = 1.0;
                            tab.pan = Vec2::ZERO;
                        }
                    }
                }
                // Mouse drag to pan
                let (drag_delta, primary_down) =
                    ui.input(|i| (i.pointer.delta(), i.pointer.primary_down()));
                if primary_down && drag_delta.length() > 0.0 {
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        tab.pan += drag_delta;
                    }
                }

                // Toggle nearest/linear filtering (N key)
                if ui.input(|i| i.key_pressed(egui::Key::N) && !i.modifiers.command) {
                    self.nearest_filter = !self.nearest_filter;
                    // Reload to apply new filter
                    let settings = self.decoder_settings.clone();
                    let compare = self.compare_mode;
                    if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                        if let Some(path) = tab.file_path.clone() {
                            tab.load_file(path, settings, compare);
                        }
                    }
                }

                // Escape to close dialogs
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    self.show_about = false;
                    self.show_info = false;
                    self.show_settings = false;
                    self.show_encoder = false;
                }

                if let Some(tab) = self.tabs.get(self.active_tab) {
                    if let Some(error) = &tab.error {
                        // Error state
                        ui.centered_and_justified(|ui| {
                            ui.vertical_centered(|ui| {
                                ui.label(RichText::new("⚠").size(48.0).color(theme::ERROR));
                                ui.add_space(16.0);
                                ui.label(
                                    RichText::new("Failed to load image")
                                        .size(18.0)
                                        .color(theme::TEXT_PRIMARY),
                                );
                                ui.add_space(8.0);
                                ui.label(RichText::new(error).size(13.0).color(theme::TEXT_MUTED));
                            });
                        });
                    } else if tab.reference_rx.is_some() || tab.reference_texture.is_some() {
                        // Compare mode: side-by-side progressive vs standard
                        ui.columns(2, |cols| {
                            // Left: Progressive
                            cols[0].vertical_centered(|ui| {
                                ui.label(
                                    RichText::new("Progressive")
                                        .size(13.0)
                                        .color(theme::ACCENT)
                                        .strong(),
                                );
                                if let Some(time) = tab.decode_time {
                                    ui.label(
                                        RichText::new(format!(
                                            "{:.0}ms",
                                            time.as_secs_f64() * 1000.0
                                        ))
                                        .size(11.0)
                                        .color(theme::TEXT_MUTED),
                                    );
                                } else if tab.is_loading {
                                    ui.label(
                                        RichText::new("decoding...")
                                            .size(11.0)
                                            .color(theme::TEXT_MUTED),
                                    );
                                }
                                ui.add_space(4.0);
                                if let Some(texture) = &tab.texture {
                                    show_image(ui, texture);
                                } else if tab.is_loading {
                                    ui.spinner();
                                }
                            });

                            // Right: Standard (no progressive)
                            cols[1].vertical_centered(|ui| {
                                ui.label(
                                    RichText::new("Standard")
                                        .size(13.0)
                                        .color(theme::TEXT_SECONDARY)
                                        .strong(),
                                );
                                if let Some(time) = tab.reference_decode_time {
                                    ui.label(
                                        RichText::new(format!(
                                            "{:.0}ms",
                                            time.as_secs_f64() * 1000.0
                                        ))
                                        .size(11.0)
                                        .color(theme::TEXT_MUTED),
                                    );
                                } else if tab.reference_is_loading {
                                    ui.label(
                                        RichText::new("decoding...")
                                            .size(11.0)
                                            .color(theme::TEXT_MUTED),
                                    );
                                }
                                ui.add_space(4.0);
                                if let Some(texture) = &tab.reference_texture {
                                    show_image(ui, texture);
                                } else if tab.reference_is_loading {
                                    ui.spinner();
                                }
                            });
                        });
                    } else if let Some(anim) = &tab.animation {
                        if let Some(texture) = anim.frames.get(anim.current_frame) {
                            show_image(ui, texture);
                        }
                    } else if let Some(texture) = &tab.texture {
                        show_image_zoomed(ui, texture, tab.zoom_fit, tab.zoom, tab.pan);
                    } else if tab.is_loading {
                        // Loading state
                        ui.centered_and_justified(|ui| {
                            ui.vertical_centered(|ui| {
                                ui.spinner();
                                ui.add_space(16.0);
                                ui.label(
                                    RichText::new("Loading...")
                                        .size(14.0)
                                        .color(theme::TEXT_MUTED),
                                );
                            });
                        });
                    } else {
                        // Empty state - beautiful welcome screen
                        ui.centered_and_justified(|ui| {
                            ui.vertical_centered(|ui| {
                                ui.add_space(60.0);

                                // Icon
                                ui.label(RichText::new("🌄").size(64.0));

                                ui.add_space(24.0);

                                ui.label(
                                    RichText::new("Drop a JPEG XL file to view")
                                        .size(16.0)
                                        .color(theme::TEXT_SECONDARY),
                                );

                                ui.add_space(8.0);

                                ui.label(RichText::new("or").size(13.0).color(theme::TEXT_MUTED));

                                ui.add_space(16.0);

                                if ui
                                    .add(
                                        egui::Button::new(
                                            RichText::new("Open File")
                                                .size(14.0)
                                                .strong()
                                                .color(theme::BG_BASE),
                                        )
                                        .fill(theme::ACCENT)
                                        .rounding(Rounding::same(8.0))
                                        .min_size(Vec2::new(120.0, 40.0)),
                                    )
                                    .clicked()
                                {
                                    self.open_file_dialog();
                                }

                                ui.add_space(32.0);

                                // Keyboard hints
                                ui.label(
                                    RichText::new("⌘O to open  -  ⌘T new tab  -  ⌘W close tab")
                                        .size(12.0)
                                        .color(theme::TEXT_MUTED),
                                );
                            });
                        });
                    }
                }
            });

        // About dialog
        if self.show_about {
            egui::Window::new("")
                .title_bar(false)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .fixed_size(Vec2::new(320.0, 380.0))
                .frame(
                    egui::Frame::none()
                        .fill(theme::BG_ELEVATED)
                        .rounding(Rounding::same(16.0))
                        .stroke(Stroke::new(1.0, theme::BORDER))
                        .shadow(egui::epaint::Shadow {
                            offset: Vec2::new(0.0, 16.0),
                            blur: 48.0,
                            spread: 0.0,
                            color: Color32::from_black_alpha(100),
                        }),
                )
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(32.0);

                        // App icon
                        ui.label(RichText::new("🌄").size(56.0));

                        ui.add_space(16.0);

                        ui.label(
                            RichText::new("JXL-UI")
                                .size(24.0)
                                .color(theme::TEXT_PRIMARY)
                                .strong(),
                        );

                        ui.add_space(4.0);

                        ui.label(
                            RichText::new("Version 0.1.0")
                                .size(13.0)
                                .color(theme::TEXT_MUTED),
                        );

                        ui.add_space(20.0);

                        ui.label(
                            RichText::new("A native JPEG XL image viewer")
                                .size(13.0)
                                .color(theme::TEXT_SECONDARY),
                        );

                        ui.add_space(24.0);

                        // Divider
                        ui.painter().rect_filled(
                            egui::Rect::from_min_size(
                                egui::pos2(
                                    ui.available_rect_before_wrap().left() + 40.0,
                                    ui.cursor().top(),
                                ),
                                Vec2::new(ui.available_width() - 80.0, 1.0),
                            ),
                            Rounding::ZERO,
                            theme::BORDER,
                        );
                        ui.add_space(20.0);

                        ui.label(
                            RichText::new("Built by Helmut Januschka")
                                .size(12.0)
                                .color(theme::TEXT_MUTED),
                        );

                        ui.add_space(8.0);

                        if ui
                            .add(egui::Hyperlink::from_label_and_url(
                                RichText::new("github.com/hjanuschka/jxl-ui")
                                    .size(12.0)
                                    .color(theme::ACCENT),
                                "https://github.com/hjanuschka/jxl-ui",
                            ))
                            .clicked()
                        {
                            let _ = open::that("https://github.com/hjanuschka/jxl-ui");
                        }

                        ui.add_space(32.0);

                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Close")
                                        .size(13.0)
                                        .color(theme::TEXT_SECONDARY),
                                )
                                .fill(theme::BG_SURFACE)
                                .rounding(Rounding::same(6.0))
                                .min_size(Vec2::new(80.0, 32.0)),
                            )
                            .clicked()
                        {
                            self.show_about = false;
                        }
                    });
                });

            // Click outside to close
            if ctx.input(|i| i.pointer.any_click()) {
                let pointer_pos = ctx.input(|i| i.pointer.interact_pos());
                if let Some(pos) = pointer_pos {
                    let window_rect = egui::Rect::from_center_size(
                        ctx.screen_rect().center(),
                        Vec2::new(320.0, 380.0),
                    );
                    if !window_rect.contains(pos) {
                        self.show_about = false;
                    }
                }
            }
        }

        // Info panel (floating overlay)
        if self.show_info {
            egui::Window::new("")
                .id(egui::Id::new("info_panel"))
                .title_bar(false)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::RIGHT_TOP, Vec2::new(-12.0, 52.0))
                .fixed_size(Vec2::new(280.0, 0.0))
                .frame(
                    egui::Frame::none()
                        .fill(theme::BG_ELEVATED)
                        .rounding(Rounding::same(12.0))
                        .stroke(Stroke::new(1.0, theme::BORDER))
                        .shadow(egui::epaint::Shadow {
                            offset: Vec2::new(0.0, 8.0),
                            blur: 24.0,
                            spread: 0.0,
                            color: Color32::from_black_alpha(80),
                        })
                        .inner_margin(egui::Margin::same(16.0)),
                )
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("Image Info")
                                .size(14.0)
                                .color(theme::TEXT_PRIMARY)
                                .strong(),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("X").size(14.0).color(theme::TEXT_MUTED),
                                    )
                                    .frame(false),
                                )
                                .clicked()
                            {
                                self.show_info = false;
                            }
                        });
                    });

                    ui.add_space(16.0);
                    ui.painter().rect_filled(
                        egui::Rect::from_min_size(
                            ui.cursor().min,
                            Vec2::new(ui.available_width(), 1.0),
                        ),
                        Rounding::ZERO,
                        theme::BORDER,
                    );
                    ui.add_space(16.0);

                    if let Some(tab) = self.tabs.get(self.active_tab) {
                        // File info section
                        ui.label(RichText::new("FILE").size(10.0).color(theme::TEXT_MUTED));
                        ui.add_space(4.0);

                        let filename = tab
                            .file_path
                            .as_ref()
                            .and_then(|p| p.file_name())
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| "Untitled".to_string());
                        ui.label(
                            RichText::new(&filename)
                                .size(13.0)
                                .color(theme::TEXT_PRIMARY),
                        );

                        if let Some(path) = &tab.file_path {
                            ui.label(
                                RichText::new(path.to_string_lossy())
                                    .size(11.0)
                                    .color(theme::TEXT_MUTED),
                            );
                        }

                        ui.add_space(16.0);

                        // Dimensions section
                        if let Some((w, h)) = tab.dimensions {
                            ui.label(
                                RichText::new("DIMENSIONS")
                                    .size(10.0)
                                    .color(theme::TEXT_MUTED),
                            );
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new(format!("{} x {} pixels", w, h))
                                    .size(13.0)
                                    .color(theme::TEXT_PRIMARY),
                            );

                            let mpx = (w as f64 * h as f64) / 1_000_000.0;
                            ui.label(
                                RichText::new(format!("{:.2} MP", mpx))
                                    .size(11.0)
                                    .color(theme::TEXT_MUTED),
                            );

                            ui.add_space(16.0);
                        }

                        // Decoder performance section
                        ui.label(
                            RichText::new("DECODER PERFORMANCE")
                                .size(10.0)
                                .color(theme::TEXT_MUTED),
                        );
                        ui.add_space(4.0);

                        if let Some(time) = tab.decode_time {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new("Decode time:")
                                        .size(12.0)
                                        .color(theme::TEXT_SECONDARY),
                                );
                                ui.label(
                                    RichText::new(format!("{:.1} ms", time.as_secs_f64() * 1000.0))
                                        .size(12.0)
                                        .color(theme::ACCENT),
                                );
                            });

                            // Calculate decode speed
                            if let Some((w, h)) = tab.dimensions {
                                let pixels = w as f64 * h as f64;
                                let mpx_per_sec = pixels / time.as_secs_f64() / 1_000_000.0;
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new("Speed:")
                                            .size(12.0)
                                            .color(theme::TEXT_SECONDARY),
                                    );
                                    ui.label(
                                        RichText::new(format!("{:.1} MP/s", mpx_per_sec))
                                            .size(12.0)
                                            .color(theme::ACCENT),
                                    );
                                });
                            }
                        } else if tab.is_loading {
                            ui.label(
                                RichText::new("Decoding...")
                                    .size(12.0)
                                    .color(theme::TEXT_MUTED),
                            );
                        } else {
                            ui.label(RichText::new("No data").size(12.0).color(theme::TEXT_MUTED));
                        }

                        ui.add_space(16.0);

                        // Animation info
                        if let Some(anim) = &tab.animation {
                            ui.label(
                                RichText::new("ANIMATION")
                                    .size(10.0)
                                    .color(theme::TEXT_MUTED),
                            );
                            ui.add_space(4.0);

                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new("Frames:")
                                        .size(12.0)
                                        .color(theme::TEXT_SECONDARY),
                                );
                                ui.label(
                                    RichText::new(format!("{}", anim.frames.len()))
                                        .size(12.0)
                                        .color(theme::TEXT_PRIMARY),
                                );
                            });

                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new("Current:")
                                        .size(12.0)
                                        .color(theme::TEXT_SECONDARY),
                                );
                                ui.label(
                                    RichText::new(format!(
                                        "{} / {}",
                                        anim.current_frame + 1,
                                        anim.frames.len()
                                    ))
                                    .size(12.0)
                                    .color(theme::TEXT_PRIMARY),
                                );
                            });

                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new("Status:")
                                        .size(12.0)
                                        .color(theme::TEXT_SECONDARY),
                                );
                                let status = if anim.is_playing { "Playing" } else { "Paused" };
                                ui.label(RichText::new(status).size(12.0).color(
                                    if anim.is_playing {
                                        theme::SUCCESS
                                    } else {
                                        theme::TEXT_MUTED
                                    },
                                ));
                            });
                        }
                    } else {
                        ui.label(
                            RichText::new("No image loaded")
                                .size(13.0)
                                .color(theme::TEXT_MUTED),
                        );
                    }
                });
        }

        // Encoder panel (floating overlay)
        if self.show_encoder {
            let mut should_pick_input = false;
            let mut should_pick_output = false;
            let mut should_start_encode = false;
            let mut should_open_output = false;

            egui::Window::new("")
                .id(egui::Id::new("encoder_panel"))
                .title_bar(false)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .fixed_size(Vec2::new(560.0, 0.0))
                .frame(egui::Frame::none()
                    .fill(theme::BG_ELEVATED)
                    .rounding(Rounding::same(12.0))
                    .stroke(Stroke::new(1.0, theme::BORDER))
                    .shadow(egui::epaint::Shadow {
                        offset: Vec2::new(0.0, 8.0),
                        blur: 24.0,
                        spread: 0.0,
                        color: Color32::from_black_alpha(80),
                    })
                    .inner_margin(egui::Margin::same(16.0)))
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Encode to JPEG XL")
                            .size(14.0)
                            .color(theme::TEXT_PRIMARY)
                            .strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.add(
                                egui::Button::new(RichText::new("X").size(14.0).color(theme::TEXT_MUTED))
                                    .frame(false)
                            ).clicked() {
                                self.show_encoder = false;
                            }
                        });
                    });

                    ui.add_space(8.0);
                    ui.label(RichText::new("Pick any input (PNG/JPG/WEBP/MP4/MOV...), preprocess via ffmpeg->PPM, then encode with jxl-encoder")
                        .size(11.0)
                        .color(theme::TEXT_MUTED));
                    ui.add_space(12.0);

                    ui.group(|ui| {
                    ui.label(RichText::new("Input")
                        .size(11.0)
                        .color(theme::TEXT_MUTED));
                    ui.horizontal(|ui| {
                        let mut input_text = self.encoder_state
                            .input_path
                            .as_ref()
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|| "No input selected".to_string());
                        ui.add_sized(
                            [430.0, 24.0],
                            egui::TextEdit::singleline(&mut input_text)
                                .font(egui::TextStyle::Monospace)
                                .interactive(false),
                        );
                        if ui.button("Choose...").clicked() {
                            should_pick_input = true;
                        }
                    });

                    ui.add_space(8.0);
                    ui.label(RichText::new("Output")
                        .size(11.0)
                        .color(theme::TEXT_MUTED));
                    ui.horizontal(|ui| {
                        let mut output_text = self.encoder_state
                            .output_path
                            .as_ref()
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|| "No output selected".to_string());
                        ui.add_sized(
                            [430.0, 24.0],
                            egui::TextEdit::singleline(&mut output_text)
                                .font(egui::TextStyle::Monospace)
                                .interactive(false),
                        );
                        if ui.button("Choose...").clicked() {
                            should_pick_output = true;
                        }
                    });
                    });

                    ui.add_space(12.0);
                    ui.painter().rect_filled(
                        egui::Rect::from_min_size(
                            ui.cursor().min,
                            Vec2::new(ui.available_width(), 1.0),
                        ),
                        Rounding::ZERO,
                        theme::BORDER,
                    );
                    ui.add_space(12.0);

                    ui.group(|ui| {
                    ui.label(RichText::new("Encoder Settings")
                        .size(11.0)
                        .color(theme::TEXT_MUTED));

                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.encoder_state.settings.lossless, "Lossless");
                        ui.label(RichText::new("(lossless forces Modular mode)")
                            .size(10.0)
                            .color(theme::TEXT_MUTED));
                    });

                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Mode:")
                            .size(12.0)
                            .color(theme::TEXT_SECONDARY));
                        let modular_selected = self.encoder_state.settings.mode == encoder::EncoderMode::Modular;
                        if ui
                            .selectable_label(modular_selected, encoder::EncoderMode::Modular.label())
                            .clicked()
                        {
                            self.encoder_state.settings.mode = encoder::EncoderMode::Modular;
                        }
                        let vardct_selected = self.encoder_state.settings.mode == encoder::EncoderMode::VarDct;
                        if ui
                            .selectable_label(vardct_selected, encoder::EncoderMode::VarDct.label())
                            .clicked()
                        {
                            self.encoder_state.settings.mode = encoder::EncoderMode::VarDct;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Effort")
                            .size(12.0)
                            .color(theme::TEXT_SECONDARY));
                        ui.add(egui::Slider::new(&mut self.encoder_state.settings.effort, 1..=9));
                    });

                    if !self.encoder_state.settings.lossless {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Distance")
                                .size(12.0)
                                .color(theme::TEXT_SECONDARY));
                            ui.add(egui::Slider::new(&mut self.encoder_state.settings.distance, 0.01..=10.0)
                                .logarithmic(true));
                        });

                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Near-lossless")
                                .size(12.0)
                                .color(theme::TEXT_SECONDARY));
                            ui.add(egui::Slider::new(&mut self.encoder_state.settings.near_lossless, 0..=100));
                        });

                        ui.horizontal(|ui| {
                            ui.checkbox(&mut self.encoder_state.settings.fast_lossless, "Fast-lossless heuristics");
                        });
                    }

                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.encoder_state.settings.use_ffmpeg_decode, "Use ffmpeg pre-conversion to PPM");
                    });

                    ui.horizontal(|ui| {
                        ui.checkbox(
                            &mut self.encoder_state.settings.encode_animation_if_possible,
                            "Encode animation when source has multiple frames"
                        );
                    });

                    if self.encoder_state.settings.encode_animation_if_possible {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Max frames")
                                .size(12.0)
                                .color(theme::TEXT_SECONDARY));
                            ui.add(egui::Slider::new(
                                &mut self.encoder_state.settings.max_animation_frames,
                                2..=300,
                            ));
                        });
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("FPS cap")
                                .size(12.0)
                                .color(theme::TEXT_SECONDARY));
                            ui.add(egui::Slider::new(
                                &mut self.encoder_state.settings.animation_fps_cap,
                                1..=60,
                            ));
                        });
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Max edge")
                                .size(12.0)
                                .color(theme::TEXT_SECONDARY));
                            ui.add(egui::Slider::new(
                                &mut self.encoder_state.settings.animation_max_edge,
                                256..=1920,
                            ));
                        });
                        ui.label(RichText::new("Video/animated input uses VarDCT animation path with automatic frame/pixel budget limits")
                            .size(10.0)
                            .color(theme::TEXT_MUTED));
                    }
                    });

                    ui.add_space(12.0);
                    let button_text = if self.encoder_state.is_encoding { "Encoding..." } else { "Encode" };
                    if ui.add_enabled(!self.encoder_state.is_encoding,
                        egui::Button::new(RichText::new(button_text)
                            .size(13.0)
                            .strong()
                            .color(theme::BG_BASE))
                        .fill(theme::ACCENT)
                        .rounding(Rounding::same(6.0))
                        .min_size(Vec2::new(ui.available_width(), 34.0))
                    ).clicked() {
                        should_start_encode = true;
                    }

                    ui.group(|ui| {
                    if self.encoder_state.is_encoding {
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            ui.spinner();
                            let elapsed = self.encoder_state
                                .started_at
                                .map(|t| t.elapsed().as_secs_f32())
                                .unwrap_or(0.0);
                            ui.label(RichText::new(format!("Working... {:.1}s", elapsed))
                                .size(11.0)
                                .color(theme::TEXT_SECONDARY));
                        });
                    }

                    if let Some(status) = &self.encoder_state.last_status {
                        ui.add_space(8.0);
                        ui.label(RichText::new(status)
                            .size(11.0)
                            .color(theme::SUCCESS));
                    }
                    if let Some(err) = &self.encoder_state.last_error {
                        ui.add_space(8.0);
                        ui.label(RichText::new(err)
                            .size(11.0)
                            .color(theme::ERROR));
                    }

                    if let Some(output) = &self.encoder_state.last_output {
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(output.display().to_string())
                                .size(11.0)
                                .color(theme::TEXT_MUTED));
                            if ui.button("Open output").clicked() {
                                should_open_output = true;
                            }
                        });
                    }
                    });
                });

            if should_pick_input {
                self.pick_encoder_input();
            }
            if should_pick_output {
                self.pick_encoder_output();
            }
            if should_start_encode {
                self.start_encode();
            }
            if should_open_output {
                if let Some(path) = self.encoder_state.last_output.clone() {
                    self.open_file_in_new_tab(path);
                }
            }
        }

        // Settings panel (floating overlay)
        let mut should_reload = false;
        if self.show_settings {
            egui::Window::new("")
                .title_bar(false)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::LEFT_TOP, Vec2::new(12.0, 52.0))
                .min_width(260.0)
                .max_width(260.0)
                .max_height(ctx.screen_rect().height() - 80.0)
                .frame(egui::Frame::none()
                    .fill(theme::BG_ELEVATED)
                    .rounding(Rounding::same(12.0))
                    .stroke(Stroke::new(1.0, theme::BORDER))
                    .shadow(egui::epaint::Shadow {
                        offset: Vec2::new(0.0, 8.0),
                        blur: 24.0,
                        spread: 0.0,
                        color: Color32::from_black_alpha(80),
                    })
                    .inner_margin(egui::Margin::same(16.0)))
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Decoder Settings")
                            .size(14.0)
                            .color(theme::TEXT_PRIMARY)
                            .strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.add(
                                egui::Button::new(RichText::new("X").size(14.0).color(theme::TEXT_MUTED))
                                    .frame(false)
                            ).clicked() {
                                self.show_settings = false;
                            }
                        });
                    });
                    ui.add_space(8.0);
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        // Ensure content fills width inside scroll area
                        ui.set_min_width(ui.available_width());

                    ui.add_space(16.0);
                    ui.painter().rect_filled(
                        egui::Rect::from_min_size(
                            ui.cursor().min,
                            Vec2::new(ui.available_width(), 1.0),
                        ),
                        Rounding::ZERO,
                        theme::BORDER,
                    );
                    ui.add_space(16.0);

                    // Output Color Type
                    ui.label(RichText::new("OUTPUT COLOR FORMAT")
                        .size(10.0)
                        .color(theme::TEXT_MUTED));
                    ui.add_space(4.0);

                    let color_types = [
                        OutputColorType::Auto,
                        OutputColorType::Rgba,
                        OutputColorType::Rgb,
                        OutputColorType::Bgra,
                        OutputColorType::Bgr,
                        OutputColorType::GrayscaleAlpha,
                        OutputColorType::Grayscale,
                    ];

                    for ct in &color_types {
                        let selected = self.decoder_settings.color_type == *ct;
                        let text_color = if selected { theme::ACCENT } else { theme::TEXT_SECONDARY };
                        if ui.add(
                            egui::Button::new(RichText::new(ct.label()).size(12.0).color(text_color))
                                .fill(if selected { theme::BG_SURFACE } else { Color32::TRANSPARENT })
                                .frame(false)
                                .min_size(Vec2::new(ui.available_width(), 24.0))
                        ).clicked() {
                            self.decoder_settings.color_type = ct.clone();
                        }
                    }

                    ui.add_space(16.0);

                    // Output Data Type
                    ui.label(RichText::new("OUTPUT DATA FORMAT")
                        .size(10.0)
                        .color(theme::TEXT_MUTED));
                    ui.add_space(4.0);

                    let data_types = [
                        OutputDataType::F32,
                        OutputDataType::F16,
                        OutputDataType::U16,
                        OutputDataType::U8,
                    ];

                    for dt in &data_types {
                        let selected = self.decoder_settings.data_type == *dt;
                        let text_color = if selected { theme::ACCENT } else { theme::TEXT_SECONDARY };
                        if ui.add(
                            egui::Button::new(RichText::new(dt.label()).size(12.0).color(text_color))
                                .fill(if selected { theme::BG_SURFACE } else { Color32::TRANSPARENT })
                                .frame(false)
                                .min_size(Vec2::new(ui.available_width(), 24.0))
                        ).clicked() {
                            self.decoder_settings.data_type = dt.clone();
                        }
                    }

                    ui.add_space(16.0);

                    // Options
                    ui.label(RichText::new("OPTIONS")
                        .size(10.0)
                        .color(theme::TEXT_MUTED));
                    ui.add_space(8.0);

                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.decoder_settings.premultiply_alpha, "");
                        ui.label(RichText::new("Premultiply Alpha")
                            .size(12.0)
                            .color(theme::TEXT_SECONDARY));
                    });

                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.decoder_settings.linear_output, "");
                        ui.label(RichText::new("Linear Output (XYB)")
                            .size(12.0)
                            .color(theme::TEXT_SECONDARY));
                    });

                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.decoder_settings.high_precision, "");
                        ui.label(RichText::new("High Precision")
                            .size(12.0)
                            .color(theme::TEXT_SECONDARY));
                    });

                    ui.add_space(16.0);

                    // Display
                    ui.label(RichText::new("DISPLAY")
                        .size(10.0)
                        .color(theme::TEXT_MUTED));
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        if ui.checkbox(&mut self.nearest_filter, "").changed() {
                            // Reload to apply new filter mode
                            should_reload = true;
                        }
                        ui.label(RichText::new("Nearest Neighbor (sharp pixels)")
                            .size(12.0)
                            .color(theme::TEXT_SECONDARY));
                    });

                    ui.add_space(16.0);

                    // Progressive demo
                    ui.label(RichText::new("PROGRESSIVE DEMO")
                        .size(10.0)
                        .color(theme::TEXT_MUTED));
                    ui.add_space(8.0);

                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.decoder_settings.simulate_slow, "");
                        ui.label(RichText::new("Slow Loading Demo")
                            .size(12.0)
                            .color(theme::TEXT_SECONDARY));
                    });

                    if self.decoder_settings.simulate_slow {
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.add_space(24.0);
                            ui.label(RichText::new("Chunk size (% of file):")
                                .size(11.0)
                                .color(theme::TEXT_MUTED));
                        });
                        ui.horizontal(|ui| {
                            ui.add_space(24.0);
                            ui.add(
                                egui::Slider::new(&mut self.decoder_settings.slow_chunk_pct, 0.1..=10.0)
                                    .step_by(0.1)
                                    .suffix("%")
                            );
                        });
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.add_space(24.0);
                            ui.label(RichText::new("Delay per chunk (ms):")
                                .size(11.0)
                                .color(theme::TEXT_MUTED));
                        });
                        ui.horizontal(|ui| {
                            ui.add_space(24.0);
                            let mut delay = self.decoder_settings.slow_delay_ms as f32;
                            if ui.add(
                                egui::Slider::new(&mut delay, 1.0..=500.0)
                                    .step_by(1.0)
                                    .suffix(" ms")
                            ).changed() {
                                self.decoder_settings.slow_delay_ms = delay as u64;
                            }
                        });
                        ui.add_space(4.0);
                        // Show effective speed
                        let speed_pct_per_sec = self.decoder_settings.slow_chunk_pct * 1000.0 / self.decoder_settings.slow_delay_ms as f32;
                        ui.horizontal(|ui| {
                            ui.add_space(24.0);
                            ui.label(RichText::new(format!("{:.1}% / sec", speed_pct_per_sec))
                                .size(11.0)
                                .color(theme::ACCENT));
                        });
                        ui.horizontal(|ui| {
                            ui.add_space(24.0);
                            ui.label(RichText::new("Simulates slow network to visualize\nprogressive rendering")
                                .size(10.0)
                                .color(theme::TEXT_MUTED));
                        });
                    }

                    ui.add_space(12.0);

                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.compare_mode, "");
                        ui.label(RichText::new("Compare Mode")
                            .size(12.0)
                            .color(theme::TEXT_SECONDARY));
                    });
                    ui.horizontal(|ui| {
                        ui.add_space(24.0);
                        ui.label(RichText::new("Side-by-side: progressive vs standard")
                            .size(10.0)
                            .color(theme::TEXT_MUTED));
                    });

                    ui.add_space(24.0);

                    // Reload button
                    if ui.add(
                        egui::Button::new(
                            RichText::new("⟳ Reload with Settings")
                                .size(13.0)
                                .strong()
                                .color(theme::BG_BASE)
                        )
                        .fill(theme::ACCENT)
                        .rounding(Rounding::same(6.0))
                        .min_size(Vec2::new(ui.available_width(), 36.0))
                    ).clicked() {
                        should_reload = true;
                    }

                    ui.add_space(8.0);
                    ui.label(RichText::new("Changes apply on reload")
                        .size(11.0)
                        .color(theme::TEXT_MUTED));
                    }); // end ScrollArea
                });
        }

        // Reload current image with new settings
        if should_reload {
            let settings = self.decoder_settings.clone();
            let compare = self.compare_mode;
            if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                if let Some(path) = tab.file_path.clone() {
                    tab.load_file(path, settings, compare);
                }
            }
        }

        // File drop
        ctx.input(|i| {
            if !i.raw.dropped_files.is_empty() {
                if let Some(path) = i.raw.dropped_files[0].path.clone() {
                    self.open_file_in_new_tab(path);
                }
            }
        });

        // Repaint if loading
        if self
            .tabs
            .iter()
            .any(|t| t.is_loading || t.reference_is_loading)
        {
            ctx.request_repaint();
        }
    }
}

fn format_bytes(bytes: usize) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;

    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.2} MB", b / MB)
    } else if b >= KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{} B", bytes)
    }
}

/// Show image fit-to-window (used in compare mode columns)
fn show_image(ui: &mut egui::Ui, texture: &egui::TextureHandle) {
    show_image_zoomed(ui, texture, true, 1.0, Vec2::ZERO);
}

/// Show image with zoom and pan support
fn show_image_zoomed(
    ui: &mut egui::Ui,
    texture: &egui::TextureHandle,
    fit: bool,
    zoom: f32,
    pan: Vec2,
) {
    let available = ui.available_size();
    let img_size = texture.size_vec2();
    let fit_scale = (available.x / img_size.x)
        .min(available.y / img_size.y)
        .min(1.0);

    let scale = if fit { fit_scale * zoom } else { zoom };
    let size = img_size * scale;

    // Calculate position: centered + pan offset
    let center = ui.available_rect_before_wrap().center();
    let top_left = center - size * 0.5 + pan;

    let rect = egui::Rect::from_min_size(top_left, size);
    let clip_rect = ui.available_rect_before_wrap();
    ui.set_clip_rect(clip_rect);

    ui.put(
        rect,
        egui::Image::new((texture.id(), size)).rounding(Rounding::same(4.0)),
    );
}

fn decode_file(path: PathBuf, tx: Sender<DecoderMessage>, settings: DecoderSettings) {
    log::info!("Decoding with settings: {:?}", settings);
    let tx_clone = tx.clone();

    let decode_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        decoder::worker::decode_jxl_progressive(&path, &settings, move |update| {
            let _ = tx_clone.send(DecoderMessage::ProgressiveUpdate {
                rgba: update.rgba_data,
                width: update.width,
                height: update.height,
                completed_passes: update.completed_passes,
                is_final: update.is_final,
                elapsed: update.elapsed,
            });
        })
    }));

    match decode_result {
        Ok(Ok(result)) => {
            match result {
                decoder::DecodeResult::SingleFrame { frame, .. } => {
                    let _ = tx.send(DecoderMessage::ProgressiveUpdate {
                        rgba: frame.rgba_data,
                        width: frame.width,
                        height: frame.height,
                        completed_passes: 1,
                        is_final: true,
                        elapsed: frame.decode_time,
                    });
                }
                decoder::DecodeResult::Animation { frames, .. } => {
                    let total = frames.len();
                    for (i, frame) in frames.into_iter().enumerate() {
                        let _ = tx.send(DecoderMessage::AnimationFrame {
                            rgba: frame.rgba_data,
                            width: frame.width,
                            height: frame.height,
                            duration_ms: frame.duration_ms,
                            frame_index: i,
                            total_frames: total,
                        });
                    }
                }
            }
            let _ = tx.send(DecoderMessage::Complete);
        }
        Ok(Err(e)) => {
            let _ = tx.send(DecoderMessage::Error(e.to_string()));
        }
        Err(_) => {
            let _ = tx.send(DecoderMessage::Error(
                "Decoder panic (likely jxl-rs LF preview debug overflow). Try release build."
                    .to_string(),
            ));
        }
    }
}

/// Non-progressive decoder for compare mode.
/// Uses the same chunked decode with slow_delay but does NOT send intermediate
/// updates -- only the final result. This simulates "no progressive decoding":
/// user sees nothing until the image is fully decoded.
fn decode_file_standard(path: PathBuf, tx: Sender<DecoderMessage>, settings: DecoderSettings) {
    log::info!("Standard decode (compare mode): {:?}", settings);
    // Use the same progressive decoder internally, but ignore all intermediate callbacks.
    // The slow_delay still applies so timing is comparable.
    let decode_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        decoder::worker::decode_jxl_progressive(&path, &settings, |_update| {
            // No-op: don't send intermediate updates
        })
    }));

    match decode_result {
        Ok(Ok(result)) => {
            match result {
                decoder::DecodeResult::SingleFrame { frame, .. } => {
                    let _ = tx.send(DecoderMessage::ProgressiveUpdate {
                        rgba: frame.rgba_data,
                        width: frame.width,
                        height: frame.height,
                        completed_passes: 1,
                        is_final: true,
                        elapsed: frame.decode_time,
                    });
                }
                _ => {}
            }
            let _ = tx.send(DecoderMessage::Complete);
        }
        Ok(Err(e)) => {
            let _ = tx.send(DecoderMessage::Error(e.to_string()));
        }
        Err(_) => {
            let _ = tx.send(DecoderMessage::Error(
                "Decoder panic (likely jxl-rs LF preview debug overflow). Try release build."
                    .to_string(),
            ));
        }
    }
}
