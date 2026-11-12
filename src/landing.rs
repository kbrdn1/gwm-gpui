//! The view a window opens on when the app has no repo to show: launched from
//! the Finder, its current directory is in none. The app's tile, then the
//! repos opened before and "Open folder…", as the project picker lists them
//! (`project_picker.rs`): a click shows the repo in this window, in place of
//! this view, a ⌘-click opens it in a window of its own.

use gpui::{Context, FontWeight, Window, div, prelude::*, px};

use crate::{project_picker, recent::Projects, theme, title_bar};

pub struct Landing;

impl Render for Landing {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let recent = cx
      .try_global::<Projects>()
      .map_or(&[][..], |projects| &projects.recent[..]);
    let listed = !recent.is_empty();
    let rows = recent
      .iter()
      .enumerate()
      .map(|(index, entry)| project_picker::row(index, entry, false));
    let choices = div()
      .w(px(project_picker::WIDTH))
      .flex()
      .flex_col()
      .rounded(theme::POPOVER_RADIUS)
      .bg(theme::wash())
      .border_1()
      .border_color(theme::card_border())
      .when(listed, |choices| {
        choices.child(div().p(px(6.)).flex().flex_col().children(rows))
      })
      .child(
        div()
          .p(px(6.))
          .when(listed, |footer| footer.border_t_1().border_color(theme::ink(0.07)))
          .child(project_picker::open_folder()),
      );
    div()
      .size_full()
      .flex()
      .flex_col()
      .bg(theme::shell())
      .font_family(theme::UI_FONT)
      .text_size(px(13.))
      .text_color(theme::ink(1.))
      // The traffic lights sit in it, and the window moves from it.
      .child(
        div()
          .h(theme::TITLE_BAR_HEIGHT)
          .flex_none()
          .flex()
          .child(title_bar::drag_area().flex_1()),
      )
      .child(
        div()
          .flex_1()
          .min_h_0()
          // The title bar's height again, below: the column sits in the window's middle.
          .pb(theme::TITLE_BAR_HEIGHT)
          .flex()
          .flex_col()
          .items_center()
          .justify_center()
          .child(title_bar::tile(px(56.), px(56.), px(13.), 30.))
          .child(
            div()
              .mt(px(14.))
              .text_size(px(20.))
              .font_weight(FontWeight::SEMIBOLD)
              .child("gwm"),
          )
          .child(
            div()
              .mt(px(4.))
              .mb(px(22.))
              .text_color(theme::ink(0.5))
              .child("Open a git repository to list its worktrees"),
          )
          .child(choices),
      )
  }
}
