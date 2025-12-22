//! JXL-UI egui version - Cross-platform JPEG XL viewer

mod decoder;

use anyhow::Result;
use eframe::egui;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

fn main() -> eframe::Result<()> {
    env_logger::init();

    let args: Vec<String> = std::env::args().collect();
    let initial_file = args.get(1).map(PathBuf::from);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_title("JXL-UI (egui)"),
        ..Default::default()
    };

    eframe::run_native(
        "JXL-UI",
        options,
        Box::new(|cc| Ok(Box::new(JxlApp::new(cc, initial_file)))),
    )
}

/// Message from decoder thread
enum DecoderMessage {
    /// Progressive update (partial decode)
    ProgressiveUpdate {
        rgba: Vec<u8>,
        width: u32,
        height: u32,
        completed_passes: usize,
        is_final: bool,
        elapsed: Duration,
    },
    /// Animation frame decoded
    AnimationFrame {
        rgba: Vec<u8>,
        width: u32,
        height: u32,
        duration_ms: u32,
        frame_index: usize,
        total_frames: usize,
    },
    /// Decoding complete
    Complete,
    /// Error occurred
    Error(String),
}

struct AnimationState {
    frames: Vec<egui::TextureHandle>,
    durations: Vec<u32>,
    current_frame: usize,
    last_frame_time: Instant,
    is_playing: bool,
}

struct JxlApp {
    /// Currently displayed texture
    texture: Option<egui::TextureHandle>,
    /// Animation state (if animated)
    animation: Option<AnimationState>,
    /// Image dimensions
    dimensions: Option<(u32, u32)>,
    /// Decode time
    decode_time: Option<Duration>,
    /// Current file path
    file_path: Option<PathBuf>,
    /// Loading state
    is_loading: bool,
    /// Error message
    error: Option<String>,
    /// Receiver for decoder messages
    decoder_rx: Option<Receiver<DecoderMessage>>,
    /// Show metrics overlay
    show_metrics: bool,
}

impl JxlApp {
    fn new(_cc: &eframe::CreationContext<'_>, initial_file: Option<PathBuf>) -> Self {
        let mut app = Self {
            texture: None,
            animation: None,
            dimensions: None,
            decode_time: None,
            file_path: None,
            is_loading: false,
            error: None,
            decoder_rx: None,
            show_metrics: false,
        };

        if let Some(path) = initial_file {
            app.load_file(path);
        }

        app
    }

    fn load_file(&mut self, path: PathBuf) {
        self.is_loading = true;
        self.error = None;
        self.texture = None;
        self.animation = None;
        self.dimensions = None;
        self.decode_time = None;
        self.file_path = Some(path.clone());

        let (tx, rx) = channel();
        self.decoder_rx = Some(rx);

        // Spawn decoder thread
        thread::spawn(move || {
            decode_file(path, tx);
        });
    }

    fn open_file_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("JPEG XL", &["jxl", "JXL"])
            .add_filter("All files", &["*"])
            .pick_file()
        {
            self.load_file(path);
        }
    }

    fn process_decoder_messages(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.decoder_rx {
            // Process all available messages
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    DecoderMessage::ProgressiveUpdate { rgba, width, height, completed_passes, is_final, elapsed } => {
                        let image = egui::ColorImage::from_rgba_unmultiplied(
                            [width as usize, height as usize],
                            &rgba,
                        );
                        self.texture = Some(ctx.load_texture(
                            format!("jxl-image-pass-{}", completed_passes),
                            image,
                            egui::TextureOptions::LINEAR,
                        ));
                        self.dimensions = Some((width, height));
                        if is_final {
                            self.decode_time = Some(elapsed);
                        }
                    }
                    DecoderMessage::AnimationFrame { rgba, width, height, duration_ms, frame_index, total_frames } => {
                        let image = egui::ColorImage::from_rgba_unmultiplied(
                            [width as usize, height as usize],
                            &rgba,
                        );
                        let texture = ctx.load_texture(
                            format!("jxl-frame-{}", frame_index),
                            image,
                            egui::TextureOptions::LINEAR,
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
    }

    fn update_animation(&mut self, ctx: &egui::Context) {
        if let Some(anim) = &mut self.animation {
            if anim.is_playing && !anim.frames.is_empty() {
                let current_duration = anim.durations.get(anim.current_frame).copied().unwrap_or(100);
                if anim.last_frame_time.elapsed() >= Duration::from_millis(current_duration as u64) {
                    anim.current_frame = (anim.current_frame + 1) % anim.frames.len();
                    anim.last_frame_time = Instant::now();
                    ctx.request_repaint();
                }
                // Request repaint for animation
                ctx.request_repaint_after(Duration::from_millis(current_duration as u64));
            }
        }
    }
}

impl eframe::App for JxlApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Process decoder messages
        self.process_decoder_messages(ctx);

        // Update animation
        self.update_animation(ctx);

        // Top menu bar
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open...").clicked() {
                        self.open_file_dialog();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Quit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("View", |ui| {
                    if ui.checkbox(&mut self.show_metrics, "Show Metrics (I)").clicked() {
                        ui.close_menu();
                    }
                });
            });
        });

        // Status bar
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if let Some(path) = &self.file_path {
                    ui.label(path.file_name().unwrap_or_default().to_string_lossy().to_string());
                }
                if let Some((w, h)) = self.dimensions {
                    ui.separator();
                    ui.label(format!("{}x{}", w, h));
                }
                if let Some(time) = self.decode_time {
                    ui.separator();
                    ui.label(format!("{:.2}ms", time.as_secs_f64() * 1000.0));
                }
                if let Some(anim) = &self.animation {
                    ui.separator();
                    ui.label(format!("Frame {}/{}", anim.current_frame + 1, anim.frames.len()));
                    if ui.button(if anim.is_playing { "⏸" } else { "▶" }).clicked() {
                        if let Some(a) = &mut self.animation {
                            a.is_playing = !a.is_playing;
                        }
                    }
                }
            });
        });

        // Central panel with image
        egui::CentralPanel::default().show(ctx, |ui| {
            // Handle keyboard shortcuts
            if ui.input(|i| i.key_pressed(egui::Key::I)) {
                self.show_metrics = !self.show_metrics;
            }
            if ui.input(|i| i.key_pressed(egui::Key::O) && i.modifiers.command) {
                self.open_file_dialog();
            }
            if ui.input(|i| i.key_pressed(egui::Key::Space)) {
                if let Some(anim) = &mut self.animation {
                    anim.is_playing = !anim.is_playing;
                }
            }

            if let Some(error) = &self.error {
                // Error message
                ui.centered_and_justified(|ui| {
                    ui.colored_label(egui::Color32::RED, format!("Error: {}", error));
                });
            } else if let Some(anim) = &self.animation {
                // Animation display
                if let Some(texture) = anim.frames.get(anim.current_frame) {
                    show_image(ui, texture);
                }
            } else if let Some(texture) = &self.texture {
                // Single image display (including progressive updates)
                show_image(ui, texture);
            } else if self.is_loading {
                // Loading spinner (only if no progressive preview yet)
                ui.centered_and_justified(|ui| {
                    ui.spinner();
                    ui.label("Loading...");
                });
            } else {
                // Empty state
                ui.centered_and_justified(|ui| {
                    ui.vertical_centered(|ui| {
                        ui.label("Drop a JXL file here or use File → Open");
                        ui.add_space(10.0);
                        if ui.button("Open File...").clicked() {
                            self.open_file_dialog();
                        }
                    });
                });
            }

            // Metrics overlay
            if self.show_metrics {
                egui::Area::new(egui::Id::new("metrics_overlay"))
                    .fixed_pos(egui::pos2(10.0, 50.0))
                    .show(ctx, |ui| {
                        egui::Frame::popup(ui.style()).show(ui, |ui| {
                            ui.label("Metrics");
                            ui.separator();
                            if let Some((w, h)) = self.dimensions {
                                ui.label(format!("Size: {}x{}", w, h));
                            }
                            if let Some(time) = self.decode_time {
                                ui.label(format!("Decode: {:.2}ms", time.as_secs_f64() * 1000.0));
                            }
                            if let Some(anim) = &self.animation {
                                ui.label(format!("Frames: {}", anim.frames.len()));
                                ui.label(format!("Playing: {}", anim.is_playing));
                            }
                        });
                    });
            }
        });

        // Handle file drop
        ctx.input(|i| {
            if !i.raw.dropped_files.is_empty() {
                if let Some(path) = i.raw.dropped_files[0].path.clone() {
                    self.load_file(path);
                }
            }
        });

        // Request repaint if loading
        if self.is_loading {
            ctx.request_repaint();
        }
    }
}

fn show_image(ui: &mut egui::Ui, texture: &egui::TextureHandle) {
    let available_size = ui.available_size();
    let image_size = texture.size_vec2();

    // Calculate scaled size to fit
    let scale = (available_size.x / image_size.x).min(available_size.y / image_size.y).min(1.0);
    let scaled_size = image_size * scale;

    ui.centered_and_justified(|ui| {
        ui.image((texture.id(), scaled_size));
    });
}

fn decode_file(path: PathBuf, tx: Sender<DecoderMessage>) {
    // Use progressive decode for streaming updates
    let tx_clone = tx.clone();
    match decoder::worker::decode_jxl_progressive(&path, move |update| {
        let _ = tx_clone.send(DecoderMessage::ProgressiveUpdate {
            rgba: update.rgba_data,
            width: update.width,
            height: update.height,
            completed_passes: update.completed_passes,
            is_final: update.is_final,
            elapsed: update.elapsed,
        });
    }) {
        Ok(result) => {
            // Handle animation (progressive decode falls back to regular for animations)
            if let decoder::DecodeResult::Animation { frames, .. } = result {
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
            let _ = tx.send(DecoderMessage::Complete);
        }
        Err(e) => {
            let _ = tx.send(DecoderMessage::Error(e.to_string()));
        }
    }
}
