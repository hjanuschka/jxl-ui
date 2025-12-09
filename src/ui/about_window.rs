use gpui::{
    div, prelude::*, px, rgb, white, App, Context, FocusHandle, Focusable,
    Window, WindowKind, WindowOptions,
};

/// About window content
pub struct AboutWindow {
    focus_handle: FocusHandle,
}

impl AboutWindow {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
        }
    }

    /// Open the About window
    pub fn open(cx: &mut App) {
        let options = WindowOptions {
            titlebar: None,
            window_bounds: None,
            focus: true,
            show: true,
            kind: WindowKind::PopUp,
            is_movable: true,
            ..Default::default()
        };

        cx.open_window(options, |window, cx| {
            window.set_window_title("About JXL-UI");
            cx.new(|cx| AboutWindow::new(cx))
        })
        .ok();
    }

    fn render_keybinding(action: &str, keys: &str) -> gpui::Div {
        div()
            .flex()
            .flex_row()
            .justify_between()
            .gap_4()
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(0xcccccc))
                    .child(action.to_string()),
            )
            .child(
                div()
                    .px_2()
                    .py_1()
                    .bg(rgb(0x1a1a1a))
                    .rounded(px(3.0))
                    .text_xs()
                    .text_color(rgb(0x888888))
                    .child(keys.to_string()),
            )
    }
}

impl Focusable for AboutWindow {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for AboutWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .track_focus(&self.focus_handle)
            .key_context("AboutWindow")
            .on_key_down(cx.listener(|_this, event: &gpui::KeyDownEvent, window, _cx| {
                // Close on Escape or Enter
                if event.keystroke.key == "escape" || event.keystroke.key == "enter" {
                    window.remove_window();
                }
            }))
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0x2a2a2a))
            .p_6()
            .gap_4()
            // Header
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_2xl()
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(white())
                            .child("JXL-UI"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0x888888))
                            .child(format!("Version {}", env!("CARGO_PKG_VERSION"))),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(0x666666))
                            .child("A native JPEG XL image viewer"),
                    ),
            )
            .child(div().h_px().bg(rgb(0x404040)))
            // Key bindings section
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(0x4a9eff))
                            .child("Keyboard Shortcuts"),
                    )
                    .child(Self::render_keybinding("Open File", "O"))
                    .child(Self::render_keybinding("Open URL", "Cmd+N"))
                    .child(Self::render_keybinding("Close Tab", "Cmd+W"))
                    .child(Self::render_keybinding("Next Tab", "Cmd+]"))
                    .child(Self::render_keybinding("Previous Tab", "Cmd+["))
                    .child(Self::render_keybinding("Tab 1-9", "Cmd+1-9"))
                    .child(Self::render_keybinding("Toggle Info", "I"))
                    .child(Self::render_keybinding("Play/Pause", "Space"))
                    .child(Self::render_keybinding("Next Frame", "→"))
                    .child(Self::render_keybinding("Prev Frame", "←"))
                    .child(Self::render_keybinding("Zoom In/Out", "+/-"))
                    .child(Self::render_keybinding("Reset View", "R"))
                    .child(Self::render_keybinding("About", "?"))
                    .child(Self::render_keybinding("Quit", "Q")),
            )
            .child(div().h_px().bg(rgb(0x404040)))
            // Libraries section
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(0x4a9eff))
                            .child("Built With"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(0xaaaaaa))
                            .child("jxl-rs - Pure Rust JPEG XL decoder"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(0xaaaaaa))
                            .child("GPUI - GPU-accelerated UI framework"),
                    ),
            )
            .child(div().h_px().bg(rgb(0x404040)))
            // Footer
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(0x666666))
                            .child("© 2024 Harald Januschka"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(0x555555))
                            .child("BSD-3-Clause License"),
                    )
                    .child(
                        div()
                            .mt_2()
                            .text_xs()
                            .text_color(rgb(0x444444))
                            .child("Press Escape or Enter to close"),
                    ),
            )
    }
}
