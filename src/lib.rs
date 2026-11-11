//! gwm-gpui's views and the window they open in, shared by the app and its
//! screenshot example.

mod age;
mod apps;
pub mod assets;
mod detail;
mod open_menu;
mod sidebar;
mod status_bar;
pub mod theme;
mod title_bar;
pub mod workspace;

use gpui::{
  App, Bounds, KeyBinding, TitlebarOptions, WindowBackgroundAppearance, WindowBounds, WindowOptions, point, px, size,
};

pub use workspace::Workspace;

/// The main window, centered on the main display: glass, with the app drawing
/// its own title bar around the traffic lights, as Monocode Native does
/// (`main.rs` there).
pub fn window_options(cx: &App) -> WindowOptions {
  WindowOptions {
    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
      None,
      size(px(1100.), px(700.)),
      cx,
    ))),
    window_min_size: Some(size(px(820.), px(520.))),
    window_background: WindowBackgroundAppearance::Blurred,
    titlebar: Some(TitlebarOptions {
      title: Some("gwm".into()),
      appears_transparent: true,
      // Centred in the 40 px bar: the lights are 14 px tall.
      traffic_light_position: Some(point(px(12.), (theme::TITLE_BAR_HEIGHT - px(14.)) / 2.)),
    }),
    ..Default::default()
  }
}

pub fn bind_keys(cx: &mut App) {
  cx.bind_keys([
    KeyBinding::new("down", workspace::SelectNext, Some("Workspace")),
    KeyBinding::new("up", workspace::SelectPrevious, Some("Workspace")),
  ]);
}
