//! The status bar, where Monocode shows its usage footer: whether the list is
//! live and where it comes from, and how many worktrees there are.

use gpui::{div, prelude::*, px};
use gwm_client::watch::Source;

use crate::{sidebar::one_line, theme};

pub(crate) fn status_bar(source: Option<&Source>, count: Option<usize>) -> impl IntoElement {
  div()
    .h(theme::STATUS_BAR_HEIGHT)
    .flex_none()
    .flex()
    .items_center()
    .gap(px(6.))
    .px(px(12.))
    .pb(px(2.))
    .bg(theme::wash())
    .text_size(px(11.))
    .text_color(theme::ink(0.55))
    .children(source.map(|source| {
      let (live, text) = describe(source);
      let dot = theme::color(if live { theme::CLEAN } else { theme::WARNING });
      div()
        .min_w_0()
        .flex()
        .items_center()
        .gap(px(6.))
        .child(div().flex_none().size(px(6.)).rounded_full().bg(dot))
        .child(one_line(text))
    }))
    .child(div().flex_1())
    .children(count.map(|count| {
      div()
        .flex_none()
        .child(format!("{count} worktree{}", if count == 1 { "" } else { "s" }))
    }))
}

/// Whether the list is live, and the line that says where it comes from.
fn describe(source: &Source) -> (bool, String) {
  match source {
    Source::Daemon(_) => (true, "live · gwm daemon".to_owned()),
    Source::OwnDaemon(_) => (true, "live · gwm daemon started by this app".to_owned()),
    Source::Lost(reason) => (false, format!("reconnecting · {reason}")),
    Source::Once(reason) => (false, format!("not live · {reason}")),
  }
}
