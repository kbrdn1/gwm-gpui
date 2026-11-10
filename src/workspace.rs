//! The window's root view.

use std::path::PathBuf;

use gpui::{AnyElement, Context, SharedString, Window, div, prelude::*, rgb};
use gwm_client::{
  Worktree,
  watch::{self, Event, Sockets, Source, Spawned},
};

use crate::theme;

pub(crate) enum Listing {
  Loading,
  Ready(Vec<Worktree>),
  Failed(String),
}

/// The window's root view: the worktree list, and where it comes from.
pub struct Workspace {
  listing: Listing,
  /// Where the list comes from: shown, so a stale list never passes for live.
  source: Option<Source>,
}

impl Workspace {
  /// Follows the repo at `repo` live (see [`watch::watch`]). A daemon it starts is
  /// kept in `spawned`, for the app to stop on quit.
  pub fn new(repo: PathBuf, spawned: Spawned, cx: &mut Context<Self>) -> Self {
    let (events, received) = async_channel::unbounded();
    // `watch` blocks on the daemon's socket for as long as the app runs. On the
    // background executor it would hold a worker for good: it gets its own thread.
    std::thread::spawn(move || {
      watch::watch(&repo, Sockets::for_repo, &spawned, |event| {
        events.send_blocking(event).is_ok()
      });
    });
    cx.spawn(async move |this, cx| {
      while let Ok(event) = received.recv().await {
        let applied = this.update(cx, |view, cx| {
          view.apply(event);
          cx.notify();
        });
        if applied.is_err() {
          break;
        }
      }
    })
    .detach();
    Self {
      listing: Listing::Loading,
      source: None,
    }
  }

  /// Shows `worktrees` as they are, without following gwm: the screenshot example.
  pub fn showing(worktrees: Vec<Worktree>, source: Source) -> Self {
    Self {
      listing: Listing::Ready(worktrees),
      source: Some(source),
    }
  }

  fn apply(&mut self, event: Event) {
    match event {
      Event::Snapshot(worktrees) => self.listing = Listing::Ready(worktrees),
      Event::Source(source) => self.source = Some(source),
      Event::Failed(error) => {
        // Nothing follows a failure: a live or reconnecting line would lie. Why
        // the list was not live, though, still holds.
        if !matches!(self.source, Some(Source::Once(_))) {
          self.source = None;
        }
        self.listing = Listing::Failed(error);
      }
    }
  }
}

impl Render for Workspace {
  fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
    let body = match &self.listing {
      Listing::Loading => message("Loading worktrees…".into(), theme::MUTED),
      Listing::Failed(error) => message(error.clone().into(), theme::ERROR),
      Listing::Ready(worktrees) => div()
        .flex()
        .flex_col()
        .gap_1()
        .children(worktrees.iter().map(row))
        .into_any_element(),
    };
    div()
      .size_full()
      .p_4()
      .flex()
      .flex_col()
      .gap_3()
      .bg(rgb(theme::BACKGROUND))
      .text_color(rgb(theme::TEXT))
      .text_sm()
      .children(self.source.as_ref().map(source_line))
      .child(body)
  }
}

fn source_line(source: &Source) -> AnyElement {
  let (text, color) = match source {
    Source::Daemon(_) => ("live, from gwm daemon".to_owned(), theme::MUTED),
    Source::OwnDaemon(_) => ("live, from a gwm daemon started by this app".to_owned(), theme::MUTED),
    Source::Lost(reason) => (format!("reconnecting: {reason}"), theme::WARNING),
    Source::Once(reason) => (format!("not live: {reason}"), theme::WARNING),
  };
  message(text.into(), color)
}

fn message(text: SharedString, color: u32) -> AnyElement {
  div().text_color(rgb(color)).child(text).into_any_element()
}

fn row(worktree: &Worktree) -> impl IntoElement {
  let (state, state_color) = if worktree.status.is_dirty {
    ("dirty", theme::DIRTY)
  } else {
    ("clean", theme::CLEAN)
  };
  let branch = worktree.branch.clone().unwrap_or_else(|| "(detached)".to_owned());
  div()
    .flex()
    .gap_4()
    .px_2()
    .py_1()
    .rounded_md()
    .bg(rgb(theme::SURFACE))
    .child(div().flex_1().child(worktree.name.clone()))
    .child(div().flex_1().text_color(rgb(theme::ACCENT)).child(branch))
    .child(div().text_color(rgb(state_color)).child(state))
}

#[cfg(test)]
mod tests {
  use super::*;

  fn view(source: Source) -> Workspace {
    Workspace {
      listing: Listing::Loading,
      source: Some(source),
    }
  }

  #[test]
  fn a_failure_takes_down_a_live_or_reconnecting_line() {
    for source in [Source::Daemon("/d.sock".into()), Source::Lost("gone".into())] {
      let mut view = view(source);
      view.apply(Event::Failed("contract drift".to_owned()));
      assert_eq!(view.source, None);
      assert!(matches!(view.listing, Listing::Failed(_)));
    }
  }

  #[test]
  fn a_failure_keeps_why_the_list_is_not_live() {
    let mut view = view(Source::Once("no daemon".into()));
    view.apply(Event::Failed("gwm list failed".to_owned()));
    assert_eq!(view.source, Some(Source::Once("no daemon".into())));
  }
}
