mod theme;
mod worktrees;

use std::path::PathBuf;

use gpui::{
  AnyElement, App, Application, Bounds, Context, KeyBinding, SharedString, TitlebarOptions, Window, WindowBounds,
  WindowOptions, actions, div, prelude::*, px, rgb, size,
};

use crate::worktrees::Worktree;

actions!(gwm_gpui, [Quit]);

enum Listing {
  Loading,
  Ready(Vec<Worktree>),
  Failed(String),
}

struct WorktreesView {
  listing: Listing,
}

impl WorktreesView {
  fn new(repo: PathBuf, cx: &mut Context<Self>) -> Self {
    cx.spawn(async move |this, cx| {
      let result = cx.background_spawn(async move { worktrees::list(&repo) }).await;
      this
        .update(cx, |view, cx| {
          view.listing = match result {
            Ok(worktrees) => Listing::Ready(worktrees),
            Err(error) => Listing::Failed(error),
          };
          cx.notify();
        })
        .ok();
    })
    .detach();
    Self {
      listing: Listing::Loading,
    }
  }
}

impl Render for WorktreesView {
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
      .bg(rgb(theme::BACKGROUND))
      .text_color(rgb(theme::TEXT))
      .text_sm()
      .child(body)
  }
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

fn main() {
  // The repo to list: the first argument, else wherever the app was launched.
  let repo = std::env::args_os()
    .nth(1)
    .map_or_else(|| PathBuf::from("."), PathBuf::from);

  Application::with_platform(gpui_platform::current_platform(false)).run(move |cx: &mut App| {
    cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
    cx.on_action(|_: &Quit, cx: &mut App| cx.quit());
    cx.on_window_closed(|cx, _| {
      if cx.windows().is_empty() {
        cx.quit();
      }
    })
    .detach();

    let bounds = Bounds::centered(None, size(px(900.), px(600.)), cx);
    cx.open_window(
      WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
          title: Some("gwm".into()),
          ..Default::default()
        }),
        ..Default::default()
      },
      |_window, cx| cx.new(|cx| WorktreesView::new(repo, cx)),
    )
    .expect("the main window opens");
    cx.activate(true);
  });
}
