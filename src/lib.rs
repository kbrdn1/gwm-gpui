//! gwm-gpui's views and the window they open in, shared by the app and its
//! screenshot example.

mod age;
mod apps;
pub mod assets;
mod create_form;
mod detail;
mod filter;
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
  App, Bounds, KeyBinding, Keystroke, OperatingSystem, TitlebarOptions, WindowBackgroundAppearance, WindowBounds,
  WindowOptions, point, px, size,
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

/// Opens the "New worktree" form. macOS's own key for File > New: ⌘T, which
/// Monocode's "New" row shows, is a new tab there, and this window has one.
pub const NEW_WORKTREE_KEY: &str = "cmd-n";
/// Opens the filter's field and gives it the focus. macOS's own key for
/// finding in a window: ⌘K, Monocode's, opens its search across everything.
const FILTER_KEY: &str = "cmd-f";

/// A binding's keys as macOS writes them in a menu ("⌘N"), for a row to show
/// the shortcut that does the same.
pub(crate) fn shortcut(keys: &'static str) -> String {
  Keystroke::parse(keys).map_or_else(
    |_| keys.to_owned(),
    |keystroke| keystroke.display(OperatingSystem::Mac).to_string(),
  )
}

pub fn bind_keys(cx: &mut App) {
  cx.bind_keys(bindings());
}

fn bindings() -> Vec<KeyBinding> {
  use text_input::{Backspace, Delete, End, Home, Left, Paste, Right, SelectAll};
  vec![
    KeyBinding::new("down", workspace::SelectNext, Some("Workspace")),
    KeyBinding::new("up", workspace::SelectPrevious, Some("Workspace")),
    KeyBinding::new("escape", workspace::Dismiss, Some("Workspace")),
    KeyBinding::new(NEW_WORKTREE_KEY, workspace::NewWorktree, Some("Workspace")),
    KeyBinding::new(FILTER_KEY, workspace::FilterWorktrees, Some("Workspace")),
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
    // The filter's field is over the list it narrows: there, up and down walk
    // the rows shown. After the field's own, which they outrank at one depth.
    KeyBinding::new("up", workspace::SelectPrevious, Some("Filter > TextInput")),
    KeyBinding::new("down", workspace::SelectNext, Some("Filter > TextInput")),
  ]
}

#[cfg(test)]
mod tests {
  use gpui::{Action, KeyContext, Keymap};

  use super::*;

  /// What `keys` do with the focus in the innermost of `contexts`, outermost
  /// first: the keymap's own resolution, as a window runs it.
  fn bound(keys: &'static str, contexts: &[&str]) -> Option<Box<dyn Action>> {
    let stack: Vec<_> = contexts
      .iter()
      .map(|context| KeyContext::parse(context).unwrap())
      .collect();
    let (found, _) = Keymap::new(bindings()).bindings_for_input(&[Keystroke::parse(keys).unwrap()], &stack);
    found.first().map(|binding| binding.action().boxed_clone())
  }

  fn does(keys: &'static str, contexts: &[&str], action: &dyn Action) -> bool {
    bound(keys, contexts).is_some_and(|bound| bound.partial_eq(action))
  }

  /// The "New" row shows a shortcut: it is the one bound, from the list as
  /// from a field, and the row words that same key.
  #[test]
  fn the_new_row_shows_the_key_that_opens_the_form() {
    assert!(does(NEW_WORKTREE_KEY, &["Workspace"], &workspace::NewWorktree));
    assert!(does(
      NEW_WORKTREE_KEY,
      &["Workspace", "CreateForm", "TextInput"],
      &workspace::NewWorktree
    ));
    assert_eq!(shortcut(NEW_WORKTREE_KEY), "⌘N");
  }

  /// What was bound before still is: the list's keys, and a field's own.
  #[test]
  fn the_list_and_a_field_keep_their_keys() {
    assert!(does("down", &["Workspace"], &workspace::SelectNext));
    assert!(does("up", &["Workspace"], &workspace::SelectPrevious));
    assert!(does("escape", &["Workspace"], &workspace::Dismiss));
    let field = ["Workspace", "CreateForm", "TextInput"];
    assert!(does("down", &field, &text_input::End));
    assert!(does("up", &field, &text_input::Home));
    assert!(does("enter", &field, &create_form::Submit));
    assert!(does("escape", &field, &workspace::Dismiss));
  }

  #[test]
  fn the_filter_opens_from_the_list_and_from_a_field() {
    assert!(does(FILTER_KEY, &["Workspace"], &workspace::FilterWorktrees));
    assert!(does(
      FILTER_KEY,
      &["Workspace", "CreateForm", "TextInput"],
      &workspace::FilterWorktrees
    ));
    assert!(does(
      FILTER_KEY,
      &["Workspace", "Filter", "TextInput"],
      &workspace::FilterWorktrees
    ));
  }

  /// The filter's field is over the list it narrows: up and down walk the
  /// rows shown, where a form's field moves its caret. The rest of the field
  /// is a field's.
  #[test]
  fn in_the_filters_field_up_and_down_walk_the_list() {
    let filter = ["Workspace", "Filter", "TextInput"];
    assert!(does("down", &filter, &workspace::SelectNext));
    assert!(does("up", &filter, &workspace::SelectPrevious));
    assert!(does("left", &filter, &text_input::Left));
    assert!(does("cmd-left", &filter, &text_input::Home));
    assert!(does("backspace", &filter, &text_input::Backspace));
    assert!(does("escape", &filter, &workspace::Dismiss));
    assert!(does(NEW_WORKTREE_KEY, &filter, &workspace::NewWorktree));
  }
}
