use gpui::{
    div, prelude::*, px, rgb, size, white, App, Context, FocusHandle, Focusable,
    Window, WindowBounds, WindowKind, WindowOptions,
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
        // Compact window size, centered on screen
        let window_size = size(px(380.0), px(520.0));
        let window_bounds = WindowBounds::centered(window_size, cx);

        let options = WindowOptions {
            titlebar: None,
            window_bounds: Some(window_bounds),
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
                if event.keystroke.key == "escape" || event.keystroke.key == "enter" {
                    window.remove_window();
                }
            }))
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0x2a2a2a))
            .rounded(px(8.0))
            .border_1()
            .border_color(rgb(0x404040))
            // Title bar with close button
            .child(
                div()
                    .flex()
                    .flex_row()
                    .justify_between()
                    .items_center()
                    .px_4()
                    .py_2()
                    .bg(rgb(0x1a1a1a))
                    .rounded_t(px(8.0))
                    .child(div().text_sm().text_color(rgb(0x888888)).child("About JXL-UI"))
                    .child(
                        div()
                            .w(px(24.0))
                            .h(px(24.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(4.0))
                            .cursor_pointer()
                            .hover(|style| style.bg(rgb(0xff5555)))
                            .on_mouse_down(gpui::MouseButton::Left, |_event, window, _cx| {
                                window.remove_window();
                            })
                            .child(div().text_sm().text_color(rgb(0xaaaaaa)).child("×")),
                    ),
            )
            // Content area
            .child(
                div()
                    .flex()
                    .flex_col()
                    .p_4()
                    .gap_3()
                    // Header
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap_1()
                            .child(div().text_xl().font_weight(gpui::FontWeight::BOLD).text_color(white()).child("JXL-UI"))
                            .child(div().text_sm().text_color(rgb(0x888888)).child(format!("Version {}", env!("CARGO_PKG_VERSION"))))
                            .child(div().text_xs().text_color(rgb(0x666666)).child("A native JPEG XL image viewer")),
                    )
                    .child(div().h_px().bg(rgb(0x404040)))
                    // Key bindings section
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(div().text_sm().font_weight(gpui::FontWeight::SEMIBOLD).text_color(rgb(0x4a9eff)).child("Keyboard Shortcuts"))
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
                            .gap_1()
                            .child(div().text_sm().font_weight(gpui::FontWeight::SEMIBOLD).text_color(rgb(0x4a9eff)).child("Built With"))
                            .child(div().text_xs().text_color(rgb(0xaaaaaa)).child("jxl-rs - Pure Rust JPEG XL decoder"))
                            .child(div().text_xs().text_color(rgb(0xaaaaaa)).child("GPUI - GPU-accelerated UI framework")),
                    )
                    .child(div().h_px().bg(rgb(0x404040)))
                    // Footer
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap_1()
                            .child(div().text_xs().text_color(rgb(0x666666)).child("© 2024 Harald Januschka"))
                            .child(div().text_xs().text_color(rgb(0x555555)).child("BSD-3-Clause License"))
                            .child(div().mt_2().text_xs().text_color(rgb(0x444444)).child("Press Escape or Enter to close")),
                    ),
            )
    }
}
