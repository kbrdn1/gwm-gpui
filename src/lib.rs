//! gwm-gpui's views and the window they open in, shared by the app and its
//! screenshot example.

pub mod theme;
pub mod workspace;

use gpui::{App, Bounds, TitlebarOptions, WindowBounds, WindowOptions, px, size};

pub use workspace::Workspace;

/// The main window, centered on the main display.
pub fn window_options(cx: &App) -> WindowOptions {
  WindowOptions {
    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
      None,
      size(px(900.), px(600.)),
      cx,
    ))),
    titlebar: Some(TitlebarOptions {
      title: Some("gwm".into()),
      ..Default::default()
    }),
    ..Default::default()
  }
}
