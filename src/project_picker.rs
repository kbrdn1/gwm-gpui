//! The sidebar's project row and its menu: the repos opened before, the
//! window's own checked, then "Open folder…". After Monocode's sidebar project
//! header and its project picker (`shell.rs`, `pickers.rs` ProjectPicker).
//! A click shows the repo in the window, in place of the one it showed; a
//! ⌘-click opens it in a window of its own. One another window shows already
//! is brought to the front.

use std::path::{Path, PathBuf};

use gpui::{
  Action, App, ClickEvent, FontWeight, MouseButton, SharedString, Window, anchored, deferred, div, point, prelude::*,
  px,
};

use crate::{
  assets,
  open::Target,
  recent::{Entry, Projects},
  sidebar::{icon, one_line},
  theme,
};

/// Opens the repo at this path where the target says, or brings the window
/// that shows it to the front.
#[derive(Clone, PartialEq, Action)]
#[action(namespace = gwm_gpui, no_json)]
pub struct OpenRepo(pub PathBuf, pub Target);

/// Asks for a folder, then opens it where the target says.
#[derive(Clone, PartialEq, Action)]
#[action(namespace = gwm_gpui, no_json)]
pub struct OpenFolder(pub Target);

/// The menu's width, and the landing view's list, which has its rows.
pub(crate) const WIDTH: f32 = 286.;

/// The row: the repo's name and the main worktree's branch, as before, now
/// a button.
#[derive(IntoElement)]
pub(crate) struct ProjectPicker {
  pub name: String,
  pub branch: Option<String>,
}

impl RenderOnce for ProjectPicker {
  fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
    // The menu's open state lives with the row: the window's view repaints
    // when it changes.
    let open = window.use_keyed_state("project-picker", cx, |_, _| false);
    let shown = *open.read(cx);
    let toggle = open.clone();
    div()
      .h(px(39.))
      .px(px(12.))
      .pb(px(1.))
      .relative()
      .flex()
      .items_center()
      .child(
        div()
          .id("project-picker-button")
          .h(px(28.))
          .px(px(6.))
          .ml(px(-6.))
          .min_w_0()
          .max_w_full()
          .flex()
          .items_center()
          .gap(px(8.))
          .rounded(px(6.))
          .cursor_pointer()
          .hover(|button| button.bg(theme::selected()))
          .when(shown, |button| button.bg(theme::selected()))
          .on_click(move |_, _, cx| toggle.update(cx, |open, cx| set(open, !*open, cx)))
          .child(
            div()
              .size(px(20.))
              .flex_none()
              .rounded(px(5.))
              .bg(theme::ink(0.08))
              .flex()
              .items_center()
              .justify_center()
              .child(icon(assets::FOLDER, 12., theme::ink(0.7))),
          )
          .child(one_line(self.name).text_size(px(13.)).font_weight(FontWeight::SEMIBOLD))
          .children(
            self
              .branch
              .map(|branch| one_line(branch).flex_shrink_0().text_color(theme::ink(0.45))),
          )
          .child(icon(assets::CHEVRON_DOWN, 14., theme::ink(0.5))),
      )
      .when(shown, |row| {
        let projects = cx.try_global::<Projects>();
        let current = projects.and_then(|projects| projects.key_of(window.window_handle()));
        let recent = projects.map_or(&[][..], |projects| &projects.recent[..]);
        row
          .child(
            div()
              .absolute()
              .top(px(36.))
              .left(px(6.))
              .child(menu(recent, current, &open)),
          )
          .child(veil(window, &open))
      })
  }
}

fn set(open: &mut bool, value: bool, cx: &mut gpui::Context<bool>) {
  *open = value;
  cx.notify();
}

/// While the menu is open, a press anywhere else closes it and does nothing
/// more: not even the row reopening it.
fn veil(window: &Window, open: &gpui::Entity<bool>) -> impl IntoElement {
  let open = open.clone();
  deferred(
    anchored().position(point(px(0.), px(0.))).child(
      div()
        .id("dismiss-project-picker")
        .w(window.viewport_size().width)
        .h(window.viewport_size().height)
        .occlude()
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
          open.update(cx, |open, cx| set(open, false, cx));
        }),
    ),
  )
  .with_priority(1)
}

fn menu(recent: &[Entry], current: Option<&Path>, open: &gpui::Entity<bool>) -> impl IntoElement {
  let list = div().p(px(6.)).flex().flex_col().map(|list| {
    if recent.is_empty() {
      list.child(
        div()
          .px(px(10.))
          .py(px(20.))
          .flex()
          .justify_center()
          .text_size(px(12.))
          .text_color(theme::ink(0.45))
          .child("No recent repos"),
      )
    } else {
      list.children(
        recent
          .iter()
          .enumerate()
          .map(|(index, entry)| row(index, entry, current == Some(entry.key.as_path())).on_click(close(open))),
      )
    }
  });
  let footer = div()
    .p(px(6.))
    .border_t_1()
    .border_color(theme::ink(0.07))
    .child(open_folder().on_click(close(open)));
  deferred(
    anchored().snap_to_window_with_margin(px(8.)).child(
      div()
        .id("project-picker-menu")
        .occlude()
        .w(px(WIDTH))
        .flex()
        .flex_col()
        .rounded(theme::POPOVER_RADIUS)
        .bg(theme::popover())
        .border_1()
        .border_color(theme::popover_border())
        .shadow(theme::popover_shadow())
        .text_size(px(13.))
        .child(list)
        .child(footer),
    ),
  )
  .with_priority(2)
}

/// A click on one of the menu's items closes it, beside what the item does.
fn close(open: &gpui::Entity<bool>) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
  let open = open.clone();
  move |_, _, cx| open.update(cx, |open, cx| set(open, false, cx))
}

/// A repo: a check on the window's own, its name, then where it lives, as
/// Monocode's project rows. A click opens it where the click says, in the
/// menu as in the landing view.
pub(crate) fn row(index: usize, entry: &Entry, current: bool) -> gpui::Stateful<gpui::Div> {
  let path = entry.path.clone();
  item(SharedString::from(format!("project-recent-{index}")))
    .text_color(theme::ink(0.75))
    .hover(|item| item.bg(theme::hover()).text_color(theme::ink(1.)))
    .on_click(move |click, window, cx| {
      let target = Target::of_click(click.modifiers(), window.window_handle());
      window.dispatch_action(OpenRepo(path.clone(), target).boxed_clone(), cx);
    })
    .child(slot(if current {
      icon(assets::CHECK, 14., theme::ink(1.)).into_any_element()
    } else {
      icon(assets::FOLDER, 14., theme::ink(0.6)).into_any_element()
    }))
    .child(
      one_line(entry.name())
        .flex_shrink_0()
        .max_w(px(WIDTH - 12. - 20. - 36.))
        .font_weight(FontWeight::MEDIUM),
    )
    .child(
      one_line(pretty_parent(&entry.path))
        .flex_1()
        .max_w(px(112.))
        .font_family(theme::MONO_FONT)
        .text_size(px(11.))
        .text_color(theme::ink(0.4)),
    )
}

/// "Open folder…": asks for a folder, opened where the click says.
pub(crate) fn open_folder() -> gpui::Stateful<gpui::Div> {
  item("project-open-folder")
    .text_color(theme::ink(0.75))
    .hover(|item| item.bg(theme::selected()).text_color(theme::ink(1.)))
    .on_click(|click, window, cx| {
      let target = Target::of_click(click.modifiers(), window.window_handle());
      window.dispatch_action(OpenFolder(target).boxed_clone(), cx);
    })
    .child(slot(icon(assets::FOLDER, 14., theme::ink(0.75))))
    .child("Open folder…")
}

fn item(id: impl Into<gpui::ElementId>) -> gpui::Stateful<gpui::Div> {
  div()
    .id(id)
    .h(px(36.))
    .px(px(10.))
    .flex()
    .items_center()
    .gap(px(10.))
    .rounded(px(8.))
    .cursor_pointer()
}

fn slot(child: impl IntoElement) -> impl IntoElement {
  div()
    .size(px(16.))
    .flex_none()
    .flex()
    .items_center()
    .justify_center()
    .child(child)
}

/// The folder a worktree sits in, `~` for the home directory: Monocode's
/// `pretty_parent`.
fn pretty_parent(path: &Path) -> String {
  let parent = path.parent().unwrap_or(path);
  match std::env::var_os("HOME").map(PathBuf::from) {
    Some(home) if !home.as_os_str().is_empty() => match parent.strip_prefix(&home) {
      Ok(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
      Ok(rest) => format!("~/{}", rest.display()),
      Err(_) => parent.display().to_string(),
    },
    _ => parent.display().to_string(),
  }
}
