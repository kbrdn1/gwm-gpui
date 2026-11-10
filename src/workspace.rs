//! The window's root view: the state, the selection, and the frame the other
//! views sit in. The frame is Monocode's rounded card: a washed chrome (title
//! bar, sidebar, gutter, status bar) around an unwashed, rounded central card.

use std::{path::PathBuf, time::Duration};

use gpui::{
  App, Context, FocusHandle, Focusable, PathBuilder, Pixels, ScrollHandle, SharedString, Window, actions, canvas, div,
  point, prelude::*, px,
};
use gwm_client::{
  Worktree,
  watch::{self, Event, Sockets, Source, Spawned},
};

use crate::{age, detail, sidebar, status_bar, theme, title_bar};

actions!(workspace, [SelectNext, SelectPrevious]);

pub(crate) enum Listing {
  Loading,
  Ready(Vec<Worktree>),
  Failed(String),
}

/// What the window shows, without the window, so it is tested on its own.
pub(crate) struct State {
  pub listing: Listing,
  /// Where the list comes from: shown, so a stale list never passes for live.
  pub source: Option<Source>,
  /// The selected worktree's path, which a new snapshot keeps. Not git's id:
  /// gwm gives the main worktree its directory's name as id, which a linked
  /// worktree can share.
  pub selected: Option<PathBuf>,
  /// When the shown snapshot arrived: its ages count from there.
  pub received_at: i64,
}

impl State {
  fn new(listing: Listing, source: Option<Source>) -> Self {
    Self {
      listing,
      source,
      selected: None,
      received_at: age::now(),
    }
  }

  fn apply(&mut self, event: Event) {
    match event {
      Event::Snapshot(worktrees) => {
        self.listing = Listing::Ready(worktrees);
        self.received_at = age::now();
      }
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

  pub fn worktrees(&self) -> &[Worktree] {
    match &self.listing {
      Listing::Ready(worktrees) => worktrees,
      Listing::Loading | Listing::Failed(_) => &[],
    }
  }

  /// The selected worktree, else the sidebar's first: a selection whose
  /// worktree is gone falls back to it.
  pub fn selected(&self) -> Option<&Worktree> {
    let worktrees = self.worktrees();
    let order = order(worktrees);
    let position = self.position(&order).unwrap_or(0);
    order.get(position).map(|&index| &worktrees[index])
  }

  /// Moves the selection `delta` rows down the sidebar, stopping at either end.
  fn step(&mut self, delta: isize) {
    let worktrees = self.worktrees();
    let order = order(worktrees);
    let Some(last) = order.len().checked_sub(1) else {
      return;
    };
    let position = self
      .position(&order)
      .unwrap_or(0)
      .saturating_add_signed(delta)
      .min(last);
    self.selected = Some(worktrees[order[position]].path.clone());
  }

  fn position(&self, order: &[usize]) -> Option<usize> {
    let selected = self.selected.as_deref()?;
    order.iter().position(|&index| self.worktrees()[index].path == selected)
  }
}

/// Where a worktree goes in the sidebar, in order: the main worktree, then
/// the branches gwm dates, then those it does not. `age_seconds` is null on the
/// trunk, but also on a branch that has not diverged from it yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Place {
  Main,
  Dated(u64),
  Undated,
}

pub(crate) fn place(worktree: &Worktree) -> Place {
  match (worktree.is_main, worktree.age_seconds) {
    (true, _) => Place::Main,
    (false, Some(age)) => Place::Dated(age),
    (false, None) => Place::Undated,
  }
}

/// The sidebar's order, as indexes into `worktrees`: by [`Place`], the dated
/// branches youngest first, and gwm's own order otherwise.
pub(crate) fn order(worktrees: &[Worktree]) -> Vec<usize> {
  let mut order: Vec<usize> = (0..worktrees.len()).collect();
  order.sort_by_key(|&index| place(&worktrees[index]));
  order
}

pub struct Workspace {
  pub(crate) state: State,
  pub(crate) sidebar_open: bool,
  sidebar_scroll: ScrollHandle,
  focus: FocusHandle,
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
        let applied = this.update(cx, |workspace, cx| {
          workspace.state.apply(event);
          cx.notify();
        });
        if applied.is_err() {
          break;
        }
      }
    })
    .detach();
    Self::with_state(State::new(Listing::Loading, None), cx)
  }

  /// Shows `worktrees` as they are, without following gwm: the screenshot example.
  pub fn showing(worktrees: Vec<Worktree>, source: Source, cx: &mut Context<Self>) -> Self {
    Self::with_state(State::new(Listing::Ready(worktrees), Some(source)), cx)
  }

  fn with_state(state: State, cx: &mut Context<Self>) -> Self {
    // Ages are worded to the minute ("25m"): repaint once a minute, or they
    // would only move with the next snapshot.
    cx.spawn(async move |this, cx| {
      loop {
        cx.background_executor().timer(Duration::from_secs(60)).await;
        if this.update(cx, |_, cx| cx.notify()).is_err() {
          break;
        }
      }
    })
    .detach();
    Self {
      state,
      sidebar_open: true,
      sidebar_scroll: ScrollHandle::new(),
      focus: cx.focus_handle(),
    }
  }

  /// Selects the row `down` rows below the first and opens or closes the
  /// sidebar, as a user would: for the screenshot example.
  pub fn show(&mut self, down: usize, sidebar_open: bool) {
    self.state.step(down.cast_signed());
    self.reveal_selection();
    self.sidebar_open = sidebar_open;
  }

  fn ages(&self) -> age::Ages {
    age::Ages {
      received_at: self.state.received_at,
      now: age::now(),
    }
  }

  /// Scrolls the sidebar to the selected row. The handle keeps the request
  /// while the sidebar is closed, and applies it once it is drawn again.
  fn reveal_selection(&self) {
    let ages = self.ages();
    let selected = self.state.selected().map(|worktree| worktree.path.as_path());
    if let Some(child) = sidebar::child_of(self.state.worktrees(), selected, |age| ages.bucket(age)) {
      self.sidebar_scroll.scroll_to_item(child);
    }
  }

  pub(crate) fn select(&mut self, path: PathBuf, cx: &mut Context<Self>) {
    self.state.selected = Some(path);
    cx.notify();
  }

  pub(crate) fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
    self.sidebar_open = !self.sidebar_open;
    cx.notify();
  }

  fn step(&mut self, delta: isize, cx: &mut Context<Self>) {
    self.state.step(delta);
    self.reveal_selection();
    cx.notify();
  }
}

impl Focusable for Workspace {
  fn focus_handle(&self, _: &App) -> FocusHandle {
    self.focus.clone()
  }
}

impl Render for Workspace {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let ages = self.ages();
    let selected = self.state.selected();
    let card = match &self.state.listing {
      Listing::Loading => message("Loading worktrees…", theme::ink(0.5)).into_any_element(),
      Listing::Failed(error) => message(error, theme::color(theme::ERROR)).into_any_element(),
      Listing::Ready(_) => match selected {
        Some(worktree) => detail::detail(worktree, ages).into_any_element(),
        None => message("No worktrees", theme::ink(0.5)).into_any_element(),
      },
    };
    let count = matches!(self.state.listing, Listing::Ready(_)).then(|| self.state.worktrees().len());

    div()
      .track_focus(&self.focus)
      .key_context("Workspace")
      .on_action(cx.listener(|this, _: &SelectNext, _, cx| this.step(1, cx)))
      .on_action(cx.listener(|this, _: &SelectPrevious, _, cx| this.step(-1, cx)))
      .size_full()
      .flex()
      .flex_col()
      .bg(theme::shell())
      .font_family(theme::UI_FONT)
      .text_size(px(13.))
      .text_color(theme::ink(1.))
      .child(title_bar::title_bar(self.sidebar_open, selected, cx))
      .child(
        // As in Monocode, the sidebar runs down to the window's bottom edge, and
        // the status bar sits under the card's column only.
        div()
          .flex_1()
          .min_h_0()
          .flex()
          .when(self.sidebar_open, |body| {
            body.child(sidebar::sidebar(
              self.state.worktrees(),
              selected,
              ages,
              &self.sidebar_scroll,
              cx,
            ))
          })
          .child(
            div()
              .flex_1()
              .min_w_0()
              .flex()
              .flex_col()
              .child(
                div()
                  .flex_1()
                  .min_h_0()
                  .flex()
                  // Closed, the sidebar leaves a gutter on the left as on the right.
                  .when(!self.sidebar_open, |row| row.child(gutter()))
                  .child(
                    // The corners are painted over the card's border box, so they sit
                    // beside it, not in it: inside, they would stop at its border.
                    div()
                      .flex_1()
                      .min_w_0()
                      .relative()
                      .child(
                        div()
                          .size_full()
                          .rounded(theme::CARD_RADIUS)
                          .border_1()
                          .border_color(theme::card_border())
                          .bg(theme::card_shade())
                          .overflow_hidden()
                          .child(card),
                      )
                      .child(card_corners()),
                  )
                  .child(gutter()),
              )
              .child(status_bar::status_bar(self.state.source.as_ref(), count)),
          ),
      )
  }
}

/// Fills the four corners outside the card's rounding with the chrome's wash.
/// The card has none, so without this each corner shows as a small dark
/// square. Monocode clips them with a mask in its patched Metal renderer, and
/// paints these same paths where it has no mask.
fn card_corners() -> impl IntoElement {
  canvas(
    |_, _, _| {},
    |bounds, (), window, _| {
      let radius = theme::CARD_RADIUS;
      // A circle's quarter as a cubic: control points at 0.5523 of the radius
      // from the ends, measured here from the corner.
      let control = radius * (1. - 0.5523);
      let corners = [
        (bounds.origin, 1., 1.),
        (bounds.top_right(), -1., 1.),
        (bounds.bottom_left(), 1., -1.),
        (bounds.bottom_right(), -1., -1.),
      ];
      for (corner, x_sign, y_sign) in corners {
        let at = |x: Pixels, y: Pixels| point(corner.x + x * x_sign, corner.y + y * y_sign);
        let mut path = PathBuilder::fill();
        path.move_to(at(px(0.), px(0.)));
        path.line_to(at(radius, px(0.)));
        path.cubic_bezier_to(at(px(0.), radius), at(control, px(0.)), at(px(0.), control));
        path.close();
        if let Ok(path) = path.build() {
          window.paint_path(path, theme::wash());
        }
      }
    },
  )
  .absolute()
  .top_0()
  .left_0()
  .size_full()
}

fn gutter() -> impl IntoElement {
  div().w(theme::GUTTER).h_full().bg(theme::wash())
}

fn message(text: &str, color: gpui::Hsla) -> impl IntoElement {
  div()
    .size_full()
    .flex()
    .items_center()
    .justify_center()
    .p(px(24.))
    .text_color(color)
    .child(SharedString::from(text.to_owned()))
}

#[cfg(test)]
pub(crate) mod tests {
  use std::path::Path;

  use super::*;

  /// A worktree as gwm sends it, every required field set.
  pub(crate) fn worktree(id: &str, is_main: bool, age_seconds: Option<u64>) -> Worktree {
    let json = format!(
      r#"{{"name":"{id}","id":"{id}","path":"/{id}","branch":"{id}","head":null,"is_main":{is_main},
        "is_locked":false,"is_prunable":false,
        "status":{{"is_dirty":false,"has_upstream":false,"ahead":0,"behind":0,"unknown":false}},
        "age_seconds":{},"issue":null,"pr":null}}"#,
      age_seconds.map_or("null".to_owned(), |age| age.to_string())
    );
    serde_json::from_str(&json).unwrap()
  }

  /// gwm's own order on the demo repo: main first, then by name, not by age.
  fn demo() -> Vec<Worktree> {
    vec![
      worktree("acme-api", true, None),
      worktree("fix-57", false, Some(1_639_683)),
      worktree("docs-71", false, Some(1_312_083)),
      worktree("dev", false, None),
      worktree("feat-42", false, Some(2_071_683)),
    ]
  }

  fn ids(worktrees: &[Worktree], order: &[usize]) -> Vec<String> {
    order.iter().map(|&index| worktrees[index].id.clone()).collect()
  }

  fn state(source: Option<Source>) -> State {
    State::new(Listing::Ready(demo()), source)
  }

  #[test]
  fn main_comes_first_then_the_youngest_branches_then_the_undated() {
    let worktrees = demo();
    assert_eq!(
      ids(&worktrees, &order(&worktrees)),
      ["acme-api", "docs-71", "fix-57", "feat-42", "dev"]
    );
  }

  #[test]
  fn nothing_selected_means_the_first_row() {
    assert_eq!(state(None).selected().unwrap().id, "acme-api");
  }

  #[test]
  fn the_keyboard_walks_the_sidebar_order_and_stops_at_the_ends() {
    let mut state = state(None);
    state.step(-1);
    assert_eq!(state.selected().unwrap().id, "acme-api");
    state.step(1);
    state.step(1);
    assert_eq!(state.selected().unwrap().id, "fix-57");
    state.step(10);
    assert_eq!(state.selected().unwrap().id, "dev");
  }

  #[test]
  fn a_new_snapshot_keeps_the_selection_or_falls_back_when_it_is_gone() {
    let mut state = state(None);
    state.selected = Some(PathBuf::from("/fix-57"));
    let mut worktrees = demo();
    worktrees.reverse();
    state.apply(Event::Snapshot(worktrees));
    assert_eq!(state.selected().unwrap().id, "fix-57");
    state.apply(Event::Snapshot(vec![worktree("acme-api", true, None)]));
    assert_eq!(state.selected().unwrap().id, "acme-api");
    assert_eq!(state.selected().unwrap().path, Path::new("/acme-api"));
  }

  /// gwm names the main worktree's id after its directory, and a linked
  /// worktree in a directory of the same name gets the same id.
  #[test]
  fn worktrees_sharing_an_id_stay_apart() {
    let mut linked = worktree("acme-api", false, Some(60));
    linked.path = PathBuf::from("/elsewhere/acme-api");
    let mut state = State::new(Listing::Ready(vec![worktree("acme-api", true, None), linked]), None);
    state.step(1);
    assert_eq!(state.selected().unwrap().path, Path::new("/elsewhere/acme-api"));
  }

  #[test]
  fn a_failure_takes_down_a_live_or_reconnecting_line() {
    for source in [Source::Daemon("/d.sock".into()), Source::Lost("gone".into())] {
      let mut state = state(Some(source));
      state.apply(Event::Failed("contract drift".to_owned()));
      assert_eq!(state.source, None);
      assert!(matches!(state.listing, Listing::Failed(_)));
    }
  }

  #[test]
  fn a_failure_keeps_why_the_list_is_not_live() {
    let mut state = state(Some(Source::Once("no daemon".into())));
    state.apply(Event::Failed("gwm list failed".to_owned()));
    assert_eq!(state.source, Some(Source::Once("no daemon".into())));
  }
}
