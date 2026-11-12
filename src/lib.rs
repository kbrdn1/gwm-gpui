//! gwm-gpui's views and the window they open in, shared by the app and its
//! screenshot example.

mod age;
mod apps;
pub mod assets;
mod create_form;
mod detail;
mod landing;
pub mod login_shell;
pub mod open;
mod open_menu;
mod project_picker;
pub mod recent;
mod sessions;
mod sidebar;
mod status_bar;
mod text_input;
pub mod theme;
mod title_bar;
pub mod workspace;

use gpui::{
  App, Bounds, KeyBinding, TitlebarOptions, WindowBackgroundAppearance, WindowBounds, WindowOptions, point, px, size,
};

pub use landing::Landing;
pub use project_picker::{OpenFolder, OpenRepo};
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
  use text_input::{Backspace, Delete, End, Home, Left, Paste, Right, SelectAll};
  cx.bind_keys([
    KeyBinding::new("down", workspace::SelectNext, Some("Workspace")),
    KeyBinding::new("up", workspace::SelectPrevious, Some("Workspace")),
    KeyBinding::new("escape", workspace::CloseForm, Some("Workspace")),
    KeyBinding::new("enter", create_form::Submit, Some("CreateForm")),
    KeyBinding::new("tab", create_form::NextField, Some("CreateForm")),
    KeyBinding::new("backspace", Backspace, Some("TextInput")),
    KeyBinding::new("delete", Delete, Some("TextInput")),
    KeyBinding::new("left", Left, Some("TextInput")),
    KeyBinding::new("right", Right, Some("TextInput")),
    KeyBinding::new("home", Home, Some("TextInput")),
    KeyBinding::new("end", End, Some("TextInput")),
    KeyBinding::new("cmd-left", Home, Some("TextInput")),
    KeyBinding::new("cmd-right", End, Some("TextInput")),
    // A one-line field, as macOS has it: and the sidebar's up and down stay out.
    KeyBinding::new("up", Home, Some("TextInput")),
    KeyBinding::new("down", End, Some("TextInput")),
    KeyBinding::new("cmd-a", SelectAll, Some("TextInput")),
    KeyBinding::new("cmd-v", Paste, Some("TextInput")),
  ]);
}
