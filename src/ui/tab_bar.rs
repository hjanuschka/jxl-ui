use crate::ui::about_window::AboutWindow;
use crate::ui::image_tab::ImageTab;
use gpui::{App, Context, Entity, ExternalPaths, FocusHandle, Focusable, Window, div, hsla, prelude::*, px, rgb, white};
use rfd::FileDialog;
use std::path::PathBuf;

/// TabBar manages multiple ImageTab instances with tab switching
pub struct TabBar {
    tabs: Vec<Entity<ImageTab>>,
    active_tab_index: usize,
    focus_handle: FocusHandle,
    show_url_dialog: bool,
    url_input: String,
    url_input_selected: bool, // Track if all text is selected
}

impl TabBar {
    pub fn new(file_paths: Vec<Option<PathBuf>>, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();

        // Create tabs for each file path (or one empty tab if no files)
        let tabs = if file_paths.is_empty() {
            vec![cx.new(|cx| ImageTab::new(None, cx))]
        } else {
            file_paths
                .into_iter()
                .map(|path| cx.new(|cx| ImageTab::new(path, cx)))
                .collect()
        };

        log::info!("Created TabBar with {} tabs", tabs.len());

        Self {
            tabs,
            active_tab_index: 0,
            focus_handle,
            show_url_dialog: false,
            url_input: String::new(),
            url_input_selected: false,
        }
    }

    pub fn next_tab(&mut self, cx: &mut Context<Self>) {
        if self.tabs.len() > 1 {
            self.active_tab_index = (self.active_tab_index + 1) % self.tabs.len();
            log::info!("Switched to tab {}", self.active_tab_index);
            cx.notify();
        }
    }

    pub fn previous_tab(&mut self, cx: &mut Context<Self>) {
        if self.tabs.len() > 1 {
            self.active_tab_index = if self.active_tab_index == 0 {
                self.tabs.len() - 1
            } else {
                self.active_tab_index - 1
            };
            log::info!("Switched to tab {}", self.active_tab_index);
            cx.notify();
        }
    }

    pub fn close_active_tab(&mut self, cx: &mut Context<Self>) {
        if self.tabs.len() > 1 {
            log::info!("Closing tab {}", self.active_tab_index);
            self.tabs.remove(self.active_tab_index);

            // Adjust active index if needed
            if self.active_tab_index >= self.tabs.len() {
                self.active_tab_index = self.tabs.len() - 1;
            }

            cx.notify();
        } else {
            log::info!("Cannot close last tab");
        }
    }

    pub fn select_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.tabs.len() {
            self.active_tab_index = index;
            log::info!("Selected tab {}", index);
            cx.notify();
        }
    }

    /// Add new tabs from file paths
    pub fn add_tabs(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        if paths.is_empty() {
            return;
        }

        let path_count = paths.len();
        log::info!("Adding {} new tab(s)", path_count);

        for path in paths {
            let tab = cx.new(|cx| ImageTab::new(Some(path), cx));
            self.tabs.push(tab);
        }

        // Switch to the first newly added tab
        self.active_tab_index = self.tabs.len() - path_count;
        cx.notify();
    }

    /// Open file picker dialog to select JXL files
    pub fn open_file_picker(&mut self, cx: &mut Context<Self>) {
        log::info!("Opening file picker dialog");

        // Spawn async task to show file dialog
        cx.spawn(async move |this, cx| {
            // Show file picker dialog (blocking, but runs in background thread)
            let files = smol::unblock(|| {
                FileDialog::new()
                    .add_filter("JXL Images", &["jxl"])
                    .add_filter("All Files", &["*"])
                    .set_title("Open JXL Image(s)")
                    .pick_files()
            })
            .await;

            if let Some(paths) = files {
                if !paths.is_empty() {
                    log::info!("Selected {} file(s)", paths.len());

                    // Update UI with selected files
                    this.update(cx, |this, cx| {
                        this.add_tabs(paths, cx);
                    }).ok();
                }
            }
        })
        .detach();
    }

    /// Show URL input dialog
    pub fn show_url_dialog(&mut self, cx: &mut Context<Self>) {
        log::info!("Opening URL dialog");
        self.show_url_dialog = true;
        self.url_input.clear();
        cx.notify();
    }

    /// Hide URL input dialog
    pub fn hide_url_dialog(&mut self, cx: &mut Context<Self>) {
        self.show_url_dialog = false;
        self.url_input.clear();
        self.url_input_selected = false;
        cx.notify();
    }

    /// Show About dialog as a separate window
    pub fn show_about_dialog(&mut self, cx: &mut App) {
        log::info!("Opening About dialog");
        AboutWindow::open(cx);
    }

    /// Download and open JXL from URL
    pub fn download_from_url(&mut self, url: String, cx: &mut Context<Self>) {
        if url.trim().is_empty() {
            log::warn!("Empty URL provided");
            return;
        }

        log::info!("Downloading from URL: {}", url);
        self.hide_url_dialog(cx);

        // Spawn async task to download file
        cx.spawn(async move |this, cx| {
            // Download file in background thread
            let result = smol::unblock(move || {
                // Download file
                let response = reqwest::blocking::get(&url)?;
                if !response.status().is_success() {
                    anyhow::bail!("HTTP error: {}", response.status());
                }

                // Get filename from URL or use default
                let filename = url
                    .split('/')
                    .last()
                    .and_then(|s| if s.contains(".jxl") { Some(s) } else { None })
                    .unwrap_or("downloaded.jxl");

                // Create temp directory and save file
                // Use keep() to prevent auto-deletion when TempDir goes out of scope
                let temp_dir = tempfile::tempdir()?;
                let temp_path = temp_dir.keep(); // This prevents auto-cleanup
                let file_path = temp_path.join(filename);

                let bytes = response.bytes()?;
                std::fs::write(&file_path, bytes)?;

                log::info!("Downloaded to: {:?}", file_path);

                Ok::<PathBuf, anyhow::Error>(file_path)
            })
            .await;

            match result {
                Ok(path) => {
                    log::info!("Successfully downloaded file, opening...");
                    this.update(cx, |this, cx| {
                        this.add_tabs(vec![path], cx);
                    }).ok();
                }
                Err(e) => {
                    log::error!("Failed to download from URL: {}", e);
                }
            }
        })
        .detach();
    }

    fn render_url_dialog(&mut self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        if !self.show_url_dialog {
            return None;
        }

        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(hsla(0.0, 0.0, 0.0, 0.7))
                .on_mouse_down(gpui::MouseButton::Left, cx.listener(|this, _event, _window, cx| {
                    this.hide_url_dialog(cx);
                }))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_4()
                        .p_6()
                        .bg(rgb(0x2a2a2a))
                        .border_1()
                        .border_color(rgb(0x404040))
                        .rounded(px(8.0))
                        .min_w(px(500.0))
                        .on_mouse_down(gpui::MouseButton::Left, |_event, _window, cx| {
                            cx.stop_propagation();
                        })
                        .child(
                            div()
                                .text_lg()
                                .text_color(white())
                                .child("Open JXL from URL")
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_2()
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(rgb(0xaaaaaa))
                                        .child("Enter the URL of a JXL image:")
                                )
                                .child({
                                    let is_selected = self.url_input_selected;
                                    let input_text = self.url_input.clone();
                                    div()
                                        .w_full()
                                        .h(px(36.0))
                                        .px_3()
                                        .flex()
                                        .items_center()
                                        .bg(rgb(0x1a1a1a))
                                        .border_2()
                                        .border_color(rgb(0x4a9eff))
                                        .rounded(px(6.0))
                                        .text_sm()
                                        .overflow_hidden()
                                        .when(input_text.is_empty(), |this| {
                                            this.child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .child(
                                                        div()
                                                            .text_color(rgb(0x666666))
                                                            .child("https://example.com/image.jxl")
                                                    )
                                                    .child(
                                                        div()
                                                            .w(px(2.0))
                                                            .h(px(18.0))
                                                            .ml(px(1.0))
                                                            .bg(rgb(0x4a9eff)) // Cursor
                                                    )
                                            )
                                        })
                                        .when(!input_text.is_empty() && !is_selected, |this| {
                                            this.child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .child(
                                                        div()
                                                            .text_color(white())
                                                            .child(input_text.clone())
                                                    )
                                                    .child(
                                                        div()
                                                            .w(px(2.0))
                                                            .h(px(18.0))
                                                            .ml(px(1.0))
                                                            .bg(rgb(0x4a9eff)) // Cursor
                                                    )
                                            )
                                        })
                                        .when(!input_text.is_empty() && is_selected, |this| {
                                            // Show selected state with highlight
                                            this.child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .child(
                                                        div()
                                                            .px_1()
                                                            .bg(rgb(0x4a9eff)) // Selection highlight
                                                            .rounded(px(2.0))
                                                            .text_color(white())
                                                            .child(input_text)
                                                    )
                                            )
                                        })
                                })
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(0x666666))
                                .child("Tip: Paste URL with Cmd+V, press Enter to download, Esc to cancel")
                        )
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .gap_2()
                                .justify_end()
                                .child(
                                    div()
                                        .px_4()
                                        .py_2()
                                        .bg(rgb(0x404040))
                                        .rounded(px(4.0))
                                        .text_sm()
                                        .text_color(white())
                                        .cursor_pointer()
                                        .hover(|style| style.bg(rgb(0x505050)))
                                        .on_mouse_down(gpui::MouseButton::Left, cx.listener(|this, _event, _window, cx| {
                                            this.hide_url_dialog(cx);
                                        }))
                                        .child("Cancel")
                                )
                                .child(
                                    div()
                                        .px_4()
                                        .py_2()
                                        .bg(rgb(0x4a9eff))
                                        .rounded(px(4.0))
                                        .text_sm()
                                        .text_color(white())
                                        .cursor_pointer()
                                        .hover(|style| style.bg(rgb(0x5aaeFF)))
                                        .on_mouse_down(gpui::MouseButton::Left, cx.listener(|this, _event, _window, cx| {
                                            let url = this.url_input.clone();
                                            this.download_from_url(url, cx);
                                        }))
                                        .child("Download")
                                )
                        )
                )
        )
    }

    fn render_tab_bar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .w_full()
            .h(px(36.0))
            .bg(rgb(0x1a1a1a))
            .border_b_1()
            .border_color(rgb(0x404040))
            .children(
                self.tabs
                    .iter()
                    .enumerate()
                    .map(|(index, tab)| {
                        let is_active = index == self.active_tab_index;
                        let tab_name = tab
                            .read(cx)
                            .file_path
                            .as_ref()
                            .and_then(|p| p.file_name())
                            .and_then(|n| n.to_str())
                            .unwrap_or("Untitled")
                            .to_string();

                        div()
                            .flex()
                            .items_center()
                            .px_4()
                            .h_full()
                            .border_r_1()
                            .border_color(rgb(0x404040))
                            .when(is_active, |this| {
                                this.bg(rgb(0x2a2a2a))
                                    .border_b_2()
                                    .border_color(rgb(0x4a9eff))
                            })
                            .when(!is_active, |this| {
                                this.bg(rgb(0x1a1a1a))
                                    .hover(|style| style.bg(rgb(0x252525)))
                            })
                            .on_mouse_down(gpui::MouseButton::Left, cx.listener(move |this, _event, _window, cx| {
                                this.select_tab(index, cx);
                            }))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(if is_active {
                                        white()
                                    } else {
                                        hsla(0.0, 0.0, 0.67, 1.0) // Gray
                                    })
                                    .child(tab_name)
                            )
                            .child(
                                div()
                                    .ml_2()
                                    .text_xs()
                                    .text_color(rgb(0x666666))
                                    .child(format!("{}", index + 1))
                            )
                    })
                    .collect::<Vec<_>>(),
            )
    }
}

impl Focusable for TabBar {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TabBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let active_tab_index = self.active_tab_index;

        div()
            .track_focus(&self.focus_handle)
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _window, cx| {
                // Extract paths from the drop event
                let dropped_paths: Vec<PathBuf> = paths
                    .paths()
                    .iter()
                    .filter(|path| {
                        // Only accept .jxl files
                        path.extension()
                            .and_then(|ext| ext.to_str())
                            .map(|ext| ext.eq_ignore_ascii_case("jxl"))
                            .unwrap_or(false)
                    })
                    .cloned()
                    .collect();

                if !dropped_paths.is_empty() {
                    log::info!("Dropped {} JXL file(s)", dropped_paths.len());
                    this.add_tabs(dropped_paths, cx);
                } else {
                    log::warn!("Dropped files contain no JXL images");
                }
            }))
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _window, cx| {
                let active_tab_idx = this.active_tab_index;

                // Handle URL dialog input
                if this.show_url_dialog {
                    if event.keystroke.key == "escape" {
                        this.hide_url_dialog(cx);
                        return;
                    } else if event.keystroke.key == "enter" {
                        let url = this.url_input.clone();
                        this.download_from_url(url, cx);
                        return;
                    } else if event.keystroke.key == "backspace" {
                        if this.url_input_selected {
                            // Clear all when selected
                            this.url_input.clear();
                            this.url_input_selected = false;
                        } else {
                            this.url_input.pop();
                        }
                        cx.notify();
                        return;
                    } else if event.keystroke.key == "a" && event.keystroke.modifiers.platform {
                        // Cmd+A: Select all
                        if !this.url_input.is_empty() {
                            this.url_input_selected = true;
                            cx.notify();
                        }
                        return;
                    } else if event.keystroke.key == "v" && event.keystroke.modifiers.platform {
                        // Handle paste from clipboard
                        if let Some(clipboard_item) = cx.read_from_clipboard() {
                            if let Some(text) = clipboard_item.text() {
                                // Clean up pasted text (remove newlines, trim)
                                let cleaned = text.trim().replace('\n', "").replace('\r', "");
                                if this.url_input_selected {
                                    // Replace all when selected
                                    this.url_input = cleaned;
                                    this.url_input_selected = false;
                                } else {
                                    this.url_input.push_str(&cleaned);
                                }
                                log::info!("Pasted text into URL input");
                                cx.notify();
                            }
                        }
                        return;
                    } else if event.keystroke.key == "c" && event.keystroke.modifiers.platform {
                        // Cmd+C: Copy (when selected)
                        if this.url_input_selected && !this.url_input.is_empty() {
                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(this.url_input.clone()));
                            log::info!("Copied URL to clipboard");
                        }
                        return;
                    } else if event.keystroke.key == "x" && event.keystroke.modifiers.platform {
                        // Cmd+X: Cut (when selected)
                        if this.url_input_selected && !this.url_input.is_empty() {
                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(this.url_input.clone()));
                            this.url_input.clear();
                            this.url_input_selected = false;
                            log::info!("Cut URL to clipboard");
                            cx.notify();
                        }
                        return;
                    } else if event.keystroke.key.len() == 1 && !event.keystroke.modifiers.platform && !event.keystroke.modifiers.control {
                        // Add typed character to URL input
                        if this.url_input_selected {
                            // Replace all when selected
                            this.url_input.clear();
                            this.url_input_selected = false;
                        }
                        this.url_input.push_str(&event.keystroke.key);
                        cx.notify();
                        return;
                    }
                    return; // Consume all events when dialog is shown
                }

                // File operations
                if event.keystroke.key == "o" && event.keystroke.modifiers.platform {
                    this.open_file_picker(cx);
                } else if event.keystroke.key == "n" && event.keystroke.modifiers.platform {
                    this.show_url_dialog(cx);
                }
                // Tab switching shortcuts (multiple options for different keyboard layouts)
                else if (event.keystroke.key == "]" && event.keystroke.modifiers.platform)
                    || (event.keystroke.key == "right" && event.keystroke.modifiers.platform && event.keystroke.modifiers.alt) {
                    this.next_tab(cx);
                } else if (event.keystroke.key == "[" && event.keystroke.modifiers.platform)
                    || (event.keystroke.key == "left" && event.keystroke.modifiers.platform && event.keystroke.modifiers.alt) {
                    this.previous_tab(cx);
                } else if event.keystroke.key == "w" && event.keystroke.modifiers.platform {
                    this.close_active_tab(cx);
                }
                // Cmd+1 through Cmd+9 for direct tab selection
                else if event.keystroke.modifiers.platform {
                    if let Ok(num) = event.keystroke.key.parse::<usize>() {
                        if num >= 1 && num <= 9 {
                            this.select_tab(num - 1, cx);
                        }
                    }
                }
                // Image-specific shortcuts (forward to active tab)
                else if event.keystroke.key == "i" && !event.keystroke.modifiers.platform {
                    this.tabs[active_tab_idx].update(cx, |tab, cx| tab.toggle_metrics(cx));
                } else if event.keystroke.key == " " && !event.keystroke.modifiers.platform {
                    this.tabs[active_tab_idx].update(cx, |tab, cx| tab.toggle_playback(cx));
                } else if (event.keystroke.key == "right" || event.keystroke.key == ".") && !event.keystroke.modifiers.platform {
                    this.tabs[active_tab_idx].update(cx, |tab, cx| tab.next_frame(cx));
                } else if (event.keystroke.key == "left" || event.keystroke.key == ",") && !event.keystroke.modifiers.platform {
                    this.tabs[active_tab_idx].update(cx, |tab, cx| tab.previous_frame(cx));
                }
                // About dialog (? key or Shift+/)
                else if event.keystroke.key == "?" || (event.keystroke.key == "/" && event.keystroke.modifiers.shift) {
                    this.show_about_dialog(cx);
                }
            }))
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0x2a2a2a))
            .child(self.render_tab_bar(cx))
            .child(self.tabs[active_tab_index].clone())
            .children(self.render_url_dialog(cx))
    }
}
