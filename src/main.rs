use gpui::{
    actions, App, Application, Bounds, Focusable, KeyBinding, Menu, MenuItem, SharedString, TitlebarOptions,
    WindowBounds, WindowOptions, prelude::*, px, size,
};
use std::env;
use std::path::PathBuf;

mod decoder;
mod ui;
mod util;

use ui::tab_bar::TabBar;

// Define actions for the application
actions!(jxl_ui, [Quit, ToggleMetrics, OpenUrl]);

fn main() {
    env_logger::init();

    // Parse command line arguments for files to open
    let args: Vec<String> = env::args().collect();
    let file_paths: Vec<Option<PathBuf>> = if args.len() > 1 {
        args[1..]
            .iter()
            .map(|arg| Some(PathBuf::from(arg)))
            .collect()
    } else {
        vec![] // Empty vec will create one empty tab
    };

    log::info!("Opening {} file(s)", file_paths.len());

    Application::new().run(move |cx: &mut App| {
        // Register action handlers
        cx.on_action(|_: &Quit, cx| {
            log::info!("Quit action triggered");
            cx.quit();
        });

        // Set up keyboard bindings
        cx.bind_keys([
            KeyBinding::new("cmd-q", Quit, None),
            KeyBinding::new("ctrl-q", Quit, None),
            KeyBinding::new("cmd-i", ToggleMetrics, None),
            KeyBinding::new("i", ToggleMetrics, None),
            KeyBinding::new("cmd-n", OpenUrl, None),
        ]);

        // Set up application menu
        cx.set_menus(vec![Menu {
            name: "JXL Viewer".into(),
            items: vec![
                // TODO: Add "Open File..." menu item when file picker is implemented
                MenuItem::action("Quit", Quit),
            ],
        }]);

        // Create main window
        let bounds = Bounds::centered(None, size(px(1200.), px(800.)), cx);
        cx.open_window(
            WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some(SharedString::from("JXL Viewer")),
                    appears_transparent: false,
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |window, cx| {
                let tab_bar = cx.new(|tab_cx| TabBar::new(file_paths, tab_cx));
                tab_bar.focus_handle(cx).focus(window);
                tab_bar
            },
        )
        .unwrap();

        cx.activate(true);
    });
}
