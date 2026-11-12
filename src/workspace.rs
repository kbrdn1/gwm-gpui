//! The window's root view: the state, the selection, and the frame the other
//! views sit in. The frame is Monocode's rounded card: a washed chrome (title
//! bar, sidebar, gutter, status bar) around an unwashed, rounded central card.

use std::{
  collections::HashMap,
  path::PathBuf,
  sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
  },
  time::Duration,
};

use gpui::{
  App, Context, FocusHandle, Focusable, MouseButton, PathBuilder, Pixels, PromptLevel, ScrollHandle, SharedString,
  Window, actions, canvas, div, point, prelude::*, px,
};
use gwm_client::{
  Create, Links, Worktree,
  watch::{self, Event, Sockets, Source, Spawned},
};

use crate::{
  age,
  apps::{self, Installed},
  create_form::{self, Form, Mode},
  detail, sidebar, status_bar, theme, title_bar,
};

actions!(workspace, [SelectNext, SelectPrevious, CloseForm]);

/// What is under way on a worktree, or came of the last action on it, shown
/// in its card.
#[derive(Debug, PartialEq)]
pub(crate) enum Activity<'a> {
  /// `gwm remove` runs, or has run and the list does not show it yet.
  Removing,
  /// The action failed: gwm's or `open`'s own words.
  Failed(&'a str),
}

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
  /// The worktree whose removal is under way: one at a time, since gwm
  /// removes by name, resolved against a list that may still show it.
  pub removing: Option<PathBuf>,
  /// The last failed open, on the worktree at that path.
  pub failed: Option<(PathBuf, String)>,
  /// The last removal's error, apart from any path: the worktree may have
  /// left the list by the time it comes (gwm prunes before it deletes the
  /// directory). Shown whatever is selected, until the next action.
  removal_error: Option<String>,
  /// The root of the worktree the app was opened in, as git names it
  /// (canonical): gwm and its daemon run there, so it is not removed.
  pub opened_in: Option<PathBuf>,
  /// The `gwm create` under way, one at a time, with the paths listed when it
  /// started.
  creating: Option<(Create, Vec<PathBuf>)>,
  /// The user picked a worktree while gwm ran: the created one does not take
  /// the selection when gwm returns.
  picked_while_creating: bool,
  /// A creation gwm finished, whose worktree is selected once a list shows it.
  to_select: Option<(Create, Vec<PathBuf>)>,
  /// The last creation's error, in gwm's words, until the next one or the
  /// form is closed.
  create_error: Option<String>,
  /// `gwm status` per worktree: what it said last, and the list's link fields
  /// it was asked for. Fetched again on each selection and when those
  /// change, never per snapshot: the daemon sends one a second, and gwm asks
  /// GitHub each time.
  links: HashMap<PathBuf, (LinkKey, Result<Links, String>)>,
  /// The `gwm status` under way, one at a time, with the key it was asked for:
  /// arrowing down the list asks GitHub for the row it stops on, not for each.
  fetching: Option<(PathBuf, LinkKey)>,
  /// The worktree the links were last asked for: they are asked again when
  /// the user comes back to it, not on every repaint.
  links_seen: Option<PathBuf>,
}

/// What `gwm status` reads that the list also shows: the branch, and the
/// issue and PR numbers gwm links to it.
type LinkKey = (Option<String>, Option<u64>, Option<u64>);

fn link_key(worktree: &Worktree) -> LinkKey {
  (worktree.branch.clone(), worktree.issue, worktree.pr)
}

/// Whether an answer asked for `asked` still holds for a list showing `now`.
/// gwm saves the PR it detects, so the next list may show the answer's own
/// PR: that is the answer coming back, not a new link to ask about.
fn still(asked: &LinkKey, answer: &Result<Links, String>, now: &LinkKey) -> bool {
  let answered = answer.as_ref().ok().map(|links| links.pr.as_ref().map(|pr| pr.number));
  asked.0 == now.0 && asked.1 == now.1 && (asked.2 == now.2 || answered == Some(now.2))
}

impl State {
  fn new(listing: Listing, source: Option<Source>) -> Self {
    Self {
      listing,
      source,
      selected: None,
      received_at: age::now(),
      removing: None,
      failed: None,
      removal_error: None,
      opened_in: None,
      creating: None,
      picked_while_creating: false,
      to_select: None,
      create_error: None,
      links: HashMap::new(),
      fetching: None,
      links_seen: None,
    }
  }

  fn apply(&mut self, event: Event) {
    match event {
      Event::Snapshot(worktrees) => {
        // A removal ends with the first list without its worktree, and an
        // error goes with its worktree.
        let listed = |path: &PathBuf| worktrees.iter().any(|worktree| &worktree.path == path);
        if self.removing.as_ref().is_some_and(|path| !listed(path)) {
          self.removing = None;
        }
        if self.failed.as_ref().is_some_and(|(path, _)| !listed(path)) {
          self.failed = None;
        }
        self.listing = Listing::Ready(worktrees);
        self.received_at = age::now();
        self.select_created();
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

  /// Whether a daemon follows the list: it then sends a new one when gwm's
  /// word for a session changes. Otherwise that word is as old as the list.
  pub fn live(&self) -> bool {
    matches!(self.source, Some(Source::Daemon(_) | Source::OwnDaemon(_)))
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
    self.select(worktrees[order[position]].path.clone());
  }

  /// The user picks a worktree: a created one no longer takes the selection.
  fn select(&mut self, path: PathBuf) {
    self.selected = Some(path);
    self.to_select = None;
    self.picked_while_creating = self.creating.is_some();
  }

  /// Whether a creation can start: once a list is shown, which tells the new
  /// worktree from those already there.
  pub fn can_create(&self) -> bool {
    matches!(self.listing, Listing::Ready(_))
  }

  /// Starts `gwm create`, unless one is already under way or there is no list.
  fn begin_create(&mut self, request: Create) -> bool {
    if self.creating.is_some() || !self.can_create() {
      return false;
    }
    let known = self.worktrees().iter().map(|worktree| worktree.path.clone()).collect();
    self.creating = Some((request, known));
    self.picked_while_creating = false;
    self.create_error = None;
    true
  }

  /// gwm returned. Its worktree is selected from a list, this one or the next,
  /// not from what gwm printed. A failure keeps gwm's words.
  fn end_create(&mut self, result: Result<(), String>) {
    let creating = self.creating.take();
    match result {
      Ok(()) => {
        self.to_select = creating.filter(|_| !self.picked_while_creating);
        self.select_created();
      }
      Err(error) => self.create_error = Some(error),
    }
  }

  fn select_created(&mut self) {
    let Some((request, known)) = &self.to_select else {
      return;
    };
    if let Some(created) = request.created(self.worktrees(), known) {
      self.selected = Some(created.path.clone());
      self.to_select = None;
    }
  }

  pub fn creating(&self) -> bool {
    self.creating.is_some()
  }

  pub fn create_error(&self) -> Option<&str> {
    self.create_error.as_deref()
  }

  pub(crate) fn dismiss_create_error(&mut self) {
    self.create_error = None;
  }

  pub fn activity(&self, path: &std::path::Path) -> Option<Activity<'_>> {
    if self.removing.as_deref() == Some(path) {
      return Some(Activity::Removing);
    }
    let (on, error) = self.failed.as_ref()?;
    (on == path).then_some(Activity::Failed(error))
  }

  /// Starts a removal on `path`, unless one is already under way.
  fn begin_removal(&mut self, path: PathBuf) -> bool {
    if self.removing.is_some() {
      return false;
    }
    self.forget_error(&path);
    self.removal_error = None;
    self.removing = Some(path);
    true
  }

  /// An app opens `dir`: an earlier error on it, or from a removal, no longer
  /// holds.
  fn begin_open(&mut self, dir: &std::path::Path) {
    self.forget_error(dir);
    self.removal_error = None;
  }

  /// The removal of `name` failed, in gwm's words.
  /// Only this ends a removal early: nothing else knows gwm stopped.
  /// It ends that removal only: gwm prunes first, so the next one may already run.
  fn end_removal(&mut self, path: &std::path::Path, name: &str, error: String) {
    if self.removing.as_deref() == Some(path) {
      self.removing = None;
    }
    self.removal_error = Some(format!("Could not remove {name}: {error}"));
  }

  /// The last removal's error, whatever worktree is selected.
  pub fn removal_error(&self) -> Option<&str> {
    self.removal_error.as_deref()
  }

  pub(crate) fn dismiss_removal_error(&mut self) {
    self.removal_error = None;
  }

  /// Opening `path` failed, in `open`'s words. A removal there runs on.
  fn open_failed(&mut self, path: PathBuf, error: String) {
    self.failed = Some((path, error));
  }

  fn forget_error(&mut self, path: &std::path::Path) {
    if self.failed.as_ref().is_some_and(|(on, _)| on == path) {
      self.failed = None;
    }
  }

  /// Whether the card offers "Remove…" for `worktree`: never on main, nor on
  /// the worktree the app was opened in (paths compared whole, not as
  /// prefixes), nor while another removal runs.
  pub fn can_remove(&self, worktree: &Worktree) -> bool {
    !worktree.is_main && self.opened_in.as_deref() != Some(worktree.path.as_path()) && self.removing.is_none()
  }

  /// The worktree whose `gwm status` is to run now, if any: the selected
  /// one, when no other runs, and the user just selected it (a PR merges, a
  /// CI ends, the list says nothing of it) or nothing is known for its
  /// current link fields. What was known stays shown meanwhile.
  fn begin_links(&mut self) -> Option<PathBuf> {
    if self.fetching.is_some() {
      return None;
    }
    let worktree = self.selected()?;
    let (path, key) = (worktree.path.clone(), link_key(worktree));
    let selected_now = self.links_seen.as_ref() != Some(&path);
    self.links_seen = Some(path.clone());
    let known = self
      .links
      .get(&path)
      .is_some_and(|(known, result)| still(known, result, &key));
    if known && !selected_now {
      return None;
    }
    self.fetching = Some((path.clone(), key));
    Some(path)
  }

  /// `gwm status` returned, for the worktree it was run on, whichever is
  /// selected now.
  fn end_links(&mut self, result: Result<Links, String>) {
    if let Some((path, key)) = self.fetching.take() {
      self.links.insert(path, (key, result));
    }
  }

  /// Whether `gwm status` runs for `worktree` now.
  pub fn refreshing(&self, worktree: &Worktree) -> bool {
    self.fetching.as_ref().is_some_and(|(path, _)| *path == worktree.path)
  }

  /// What `gwm status` said about `worktree`, `None` until it answers for
  /// the link fields the list shows now.
  pub fn links(&self, worktree: &Worktree) -> Option<&Result<Links, String>> {
    let (known, result) = self.links.get(&worktree.path)?;
    still(known, result, &link_key(worktree)).then_some(result)
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

/// The native alert before a removal: its level, message and detail. It says
/// what gwm will drop and what it keeps.
pub(crate) fn removal_prompt(worktree: &Worktree) -> (PromptLevel, String, String) {
  let status = &worktree.status;
  let changes = if status.unknown {
    "gwm cannot tell whether it has uncommitted changes: any it has will be lost, and gwm undo does not bring \
     them back."
  } else if status.is_dirty {
    "It has uncommitted changes: they will be lost, untracked files included, and gwm undo does not bring them \
     back."
  } else {
    "It has no uncommitted changes."
  };
  // gwm's `is_dirty` leaves out what git ignores, and `remove` deletes the
  // whole directory.
  let ignored = "Its whole directory goes, files git ignores (.env, local config) included.";
  let locked = if worktree.is_locked {
    "\n\nIt is locked, and gwm removes it anyway."
  } else {
    ""
  };
  let branch = worktree.branch.as_ref().map_or_else(
    || "It is on a detached HEAD: no branch to keep.".to_owned(),
    |branch| format!("Its branch, {branch}, is kept."),
  );
  let level = if status.is_dirty || status.unknown {
    PromptLevel::Critical
  } else {
    PromptLevel::Warning
  };
  (
    level,
    format!("Remove {}?", worktree.name),
    format!("{changes} {ignored}{locked}\n\n{branch}\n\n{}", worktree.path.display()),
  )
}

pub struct Workspace {
  pub(crate) state: State,
  pub(crate) sidebar_open: bool,
  /// Where the app was opened: gwm runs there.
  repo: PathBuf,
  /// The editors and terminals found, `None` while they are looked for.
  apps: Option<Vec<Installed>>,
  menu_open: bool,
  /// The "New worktree" form, in place of the card while open.
  form: Form,
  form_open: bool,
  sidebar_scroll: ScrollHandle,
  focus: FocusHandle,
  /// Ends the watch, and the daemon it started, when the view goes.
  _watch: Watch,
}

/// The off switch of a view's watch, and the daemon that watch started.
/// Dropped with the view, when its window closes or shows another repo.
#[derive(Default)]
struct Watch {
  stopped: Arc<AtomicBool>,
  spawned: Spawned,
}

impl Drop for Watch {
  fn drop(&mut self) {
    // The watch first, then its daemon: a watch still running looks for a
    // daemon a second after it loses one, and starts another.
    self.stopped.store(true, Ordering::Relaxed);
    // The kill waits for the daemon to exit: off the UI thread.
    let spawned = self.spawned.clone();
    std::thread::spawn(move || spawned.stop());
  }
}

impl Workspace {
  /// Follows the repo at `repo` live (see [`watch::watch`]) for as long as the
  /// view lives. Once it is dropped, the watch ends: at once when it started
  /// its daemon, which is stopped; at the repo's next change when it found one
  /// running, which is left as it is. `spawned` holds the daemon it starts, for
  /// the app to stop on quit as well.
  pub fn new(repo: PathBuf, spawned: Spawned, cx: &mut Context<Self>) -> Self {
    let (events, received) = async_channel::unbounded();
    let watched = repo.clone();
    let watch = Watch {
      stopped: Arc::default(),
      spawned: spawned.clone(),
    };
    let stopped = watch.stopped.clone();
    // `watch` blocks on the daemon's socket for as long as the view lives. On the
    // background executor it would hold a worker for good: it gets its own thread.
    std::thread::spawn(move || {
      watch::watch(&watched, Sockets::for_repo, &spawned, |event| {
        !stopped.load(Ordering::Relaxed) && events.send_blocking(event).is_ok()
      });
    });
    cx.spawn(async move |this, cx| {
      while let Ok(event) = received.recv().await {
        let applied = this.update(cx, |workspace, cx| {
          let before = workspace.state.selected.clone();
          workspace.state.apply(event);
          // A created worktree, once listed, takes the selection.
          if workspace.state.selected != before {
            workspace.reveal_selection();
          }
          cx.notify();
        });
        if applied.is_err() {
          break;
        }
      }
    })
    .detach();
    Self::with_state(repo, State::new(Listing::Loading, None), watch, cx)
  }

  /// Shows `worktrees`, listed in `repo`, without following gwm: the screenshot example.
  pub fn showing(repo: PathBuf, worktrees: Vec<Worktree>, source: Source, cx: &mut Context<Self>) -> Self {
    Self::with_state(
      repo,
      State::new(Listing::Ready(worktrees), Some(source)),
      Watch::default(),
      cx,
    )
  }

  fn with_state(repo: PathBuf, state: State, watch: Watch, cx: &mut Context<Self>) -> Self {
    let located = repo.clone();
    cx.spawn(async move |this, cx| {
      let (found, opened_in) = cx
        .background_spawn(async move {
          let opened_in = gwm_client::Repo::locate(&located).ok().map(|repo| repo.toplevel);
          (apps::installed(&apps::roots()), opened_in)
        })
        .await;
      let _ = this.update(cx, |workspace, cx| {
        workspace.apps = Some(found);
        workspace.state.opened_in = opened_in;
        cx.notify();
      });
    })
    .detach();
    // The selected worktree's links are due whenever the state moves: its
    // selection, its list. Checked on every notify, fetched only when due.
    cx.observe_self(|workspace, cx| workspace.fetch_links(cx)).detach();
    cx.spawn(async move |this, cx| this.update(cx, |workspace, cx| workspace.fetch_links(cx)))
      .detach();
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
      repo,
      apps: None,
      menu_open: false,
      form: Form::new(cx),
      form_open: false,
      sidebar_scroll: ScrollHandle::new(),
      focus: cx.focus_handle(),
      _watch: watch,
    }
  }

  /// Runs the selected worktree's `gwm status` off the UI thread, when it is
  /// due (see [`State::begin_links`]).
  fn fetch_links(&mut self, cx: &mut Context<Self>) {
    let Some(path) = self.state.begin_links() else {
      return;
    };
    cx.notify();
    cx.spawn(async move |this, cx| {
      let links = cx.background_spawn(async move { gwm_client::status(&path) }).await;
      let _ = this.update(cx, |workspace, cx| {
        workspace.state.end_links(links);
        cx.notify();
      });
    })
    .detach();
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

  /// A click on a sidebar row: the user moves on, and the form closes. The
  /// row is revealed like one the keyboard reaches: the sessions nested under
  /// the row left behind fold, and one above the clicked row takes it up the
  /// list by its height, out of view if the list was scrolled.
  pub(crate) fn select(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
    self.state.select(path);
    self.reveal_selection();
    if self.form_open {
      self.close_form(window, cx);
    }
    cx.notify();
  }

  pub(crate) fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
    self.sidebar_open = !self.sidebar_open;
    cx.notify();
  }

  pub(crate) fn dismiss_removal_error(&mut self, cx: &mut Context<Self>) {
    self.state.dismiss_removal_error();
    cx.notify();
  }

  pub(crate) fn toggle_menu(&mut self, cx: &mut Context<Self>) {
    self.menu_open = !self.menu_open;
    cx.notify();
  }

  /// Opens `dir` in the app at `bundle`, off the UI thread.
  pub(crate) fn open_in(&mut self, bundle: PathBuf, dir: PathBuf, cx: &mut Context<Self>) {
    self.menu_open = false;
    self.state.begin_open(&dir);
    cx.notify();
    cx.spawn(async move |this, cx| {
      let opened = cx
        .background_spawn({
          let dir = dir.clone();
          async move { apps::open(&bundle, &dir) }
        })
        .await;
      if let Err(error) = opened {
        let _ = this.update(cx, |workspace, cx| {
          workspace.state.open_failed(dir, error);
          cx.notify();
        });
      }
    })
    .detach();
  }

  /// Asks, then removes the selected worktree through gwm, off the UI thread.
  /// Live, the daemon's next snapshot drops it; not live, a new `gwm list` does.
  pub(crate) fn confirm_remove(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let Some(worktree) = self.state.selected().filter(|worktree| self.state.can_remove(worktree)) else {
      return;
    };
    let (level, message, detail) = removal_prompt(worktree);
    // What the alert said, so gwm drops nothing it did not mention.
    let confirmed_dirty = worktree.status.is_dirty || worktree.status.unknown;
    let answer = window.prompt(level, &message, Some(&detail), &["Remove", "Cancel"], cx);
    let (repo, path, name) = (self.repo.clone(), worktree.path.clone(), worktree.name.clone());
    cx.spawn(async move |this, cx| {
      if answer.await != Ok(0) {
        return;
      }
      // Asked now, not before the alert, which may have stayed open a while.
      let Ok(Some(relist)) = this.update(cx, |workspace, cx| {
        let started = workspace.state.begin_removal(path.clone());
        cx.notify();
        started.then_some(matches!(workspace.state.source, Some(Source::Once(_))))
      }) else {
        return;
      };
      let (removed, listed) = cx
        .background_spawn({
          let path = path.clone();
          async move {
            let removed = gwm_client::remove(&repo, &path, confirmed_dirty);
            // Refused or not: a worktree already gone elsewhere leaves the list too.
            let listed = relist.then(|| gwm_client::list(&repo));
            (removed, listed)
          }
        })
        .await;
      let _ = this.update(cx, |workspace, cx| {
        if let Err(error) = removed {
          workspace.state.end_removal(&path, &name, error);
        }
        match listed {
          Some(Ok(worktrees)) => workspace.state.apply(Event::Snapshot(worktrees)),
          Some(Err(error)) => workspace.state.apply(Event::Failed(error)),
          None => {}
        }
        cx.notify();
      });
    })
    .detach();
  }

  pub(crate) fn toggle_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    if self.form_open {
      self.close_form(window, cx);
      return;
    }
    self.form_open = true;
    self.menu_open = false;
    self.focus_field(0, window, cx);
    cx.notify();
  }

  /// Hides the form: a creation under way runs on. Its error, or the last
  /// one, goes: the user moved on.
  pub(crate) fn close_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    self.form_open = false;
    self.form.invalid = None;
    self.state.dismiss_create_error();
    window.focus(&self.focus, cx);
    cx.notify();
  }

  pub(crate) fn set_mode(&mut self, mode: Mode, window: &mut Window, cx: &mut Context<Self>) {
    self.form.mode = mode;
    self.form.invalid = None;
    self.focus_field(0, window, cx);
    cx.notify();
  }

  /// Tab: the mode's next field, back to the first after the last.
  pub(crate) fn next_field(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let fields = self.form.focus_handles(cx);
    let next = fields
      .iter()
      .position(|field| field.is_focused(window))
      .map_or(0, |focused| focused + 1);
    self.focus_field(next % fields.len().max(1), window, cx);
  }

  fn focus_field(&self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
    if let Some(field) = self.form.focus_handles(cx).get(index) {
      window.focus(field, cx);
    }
  }

  /// Creates the form's worktree through gwm, off the UI thread. Done, the
  /// form closes and the worktree is selected once a list shows it; refused,
  /// the form shows gwm's words, reopened if it was closed meanwhile.
  pub(crate) fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    if self.state.creating() {
      return;
    }
    let request = match self.form.request(cx) {
      Ok(request) => request,
      Err(invalid) => {
        self.form.invalid = Some(invalid);
        cx.notify();
        return;
      }
    };
    self.form.invalid = None;
    if !self.state.begin_create(request.clone()) {
      self.form.invalid =
        Some("Wait for the worktree list: without it, the new worktree cannot be told from an older one.".to_owned());
      cx.notify();
      return;
    }
    let relist = matches!(self.state.source, Some(Source::Once(_)));
    let repo = self.repo.clone();
    cx.notify();
    cx.spawn_in(window, async move |this, cx| {
      let (created, listed) = cx
        .background_spawn(async move {
          let created = gwm_client::create(&repo, &request);
          let listed = relist.then(|| gwm_client::list(&repo));
          (created, listed)
        })
        .await;
      let _ = this.update_in(cx, |workspace, window, cx| {
        let succeeded = created.is_ok();
        let before = workspace.state.selected.clone();
        workspace.state.end_create(created);
        match listed {
          Some(Ok(worktrees)) => workspace.state.apply(Event::Snapshot(worktrees)),
          Some(Err(error)) => workspace.state.apply(Event::Failed(error)),
          None => {}
        }
        if succeeded {
          workspace.form.clear(cx);
          if workspace.form_open {
            workspace.form_open = false;
            window.focus(&workspace.focus, cx);
          }
        } else if !workspace.form_open {
          workspace.form_open = true;
          workspace.focus_field(0, window, cx);
        }
        if workspace.state.selected != before {
          workspace.reveal_selection();
        }
        cx.notify();
      });
    })
    .detach();
  }

  fn step(&mut self, delta: isize, cx: &mut Context<Self>) {
    // The selection is behind the form: moving it unseen would cancel a
    // created worktree's selection for nothing.
    if self.form_open {
      return;
    }
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
    // The popover lives in the card: no card, no popover, and no veil over
    // the window waiting to swallow a click.
    let mut menu_shown = false;
    let card = match &self.state.listing {
      _ if self.form_open => {
        create_form::create_form(&self.form, self.state.creating(), self.state.create_error(), cx).into_any_element()
      }
      Listing::Loading => message("Loading worktrees…", theme::ink(0.5)).into_any_element(),
      Listing::Failed(error) => message(error, theme::color(theme::ERROR)).into_any_element(),
      Listing::Ready(_) => match selected {
        Some(worktree) => {
          menu_shown = self.menu_open;
          let actions = detail::Actions {
            activity: self.state.activity(&worktree.path),
            removal_error: self.state.removal_error(),
            can_remove: self.state.can_remove(worktree),
            menu: self.menu_open.then_some(self.apps.as_deref()),
            links: self.state.links(worktree),
            refreshing: self.state.refreshing(worktree),
          };
          detail::detail(worktree, ages, actions, cx).into_any_element()
        }
        None => message("No worktrees", theme::ink(0.5)).into_any_element(),
      },
    };
    let count = matches!(self.state.listing, Listing::Ready(_)).then(|| self.state.worktrees().len());

    div()
      .track_focus(&self.focus)
      .key_context("Workspace")
      .on_action(cx.listener(|this, _: &SelectNext, _, cx| this.step(1, cx)))
      .on_action(cx.listener(|this, _: &SelectPrevious, _, cx| this.step(-1, cx)))
      .on_action(cx.listener(|this, _: &CloseForm, window, cx| {
        if this.form_open {
          this.close_form(window, cx);
        }
      }))
      .size_full()
      .relative()
      .flex()
      .flex_col()
      .bg(theme::shell())
      .font_family(theme::UI_FONT)
      .text_size(px(13.))
      .text_color(theme::ink(1.))
      .child(title_bar::title_bar(
        self.sidebar_open,
        self.state.can_create(),
        selected,
        cx,
      ))
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
              self.state.live(),
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
      // While the popover is open, a press anywhere else closes it and does
      // nothing more: not even the "Open in…" button reopening it.
      .when(menu_shown, |root| {
        root.child(
          div()
            .id("dismiss-menu")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .occlude()
            .on_mouse_down(
              MouseButton::Left,
              cx.listener(|workspace, _, _, cx| workspace.toggle_menu(cx)),
            ),
        )
      })
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
  fn a_removal_shows_until_the_first_list_without_its_worktree() {
    let mut state = state(None);
    assert!(state.begin_removal(PathBuf::from("/fix-57")));
    state.apply(Event::Snapshot(demo()));
    assert_eq!(state.activity(Path::new("/fix-57")), Some(Activity::Removing));
    assert_eq!(state.activity(Path::new("/docs-71")), None);
    state.apply(Event::Snapshot(vec![worktree("acme-api", true, None)]));
    assert_eq!(state.removing, None);
  }

  /// A second removal could start while gwm still runs the first, on a list
  /// that still shows it: only one at a time, and nothing else ends it.
  #[test]
  fn one_removal_at_a_time_and_nothing_else_cancels_it() {
    let mut state = state(None);
    let (fix, docs) = (PathBuf::from("/fix-57"), PathBuf::from("/docs-71"));
    assert!(state.begin_removal(fix.clone()));
    assert!(!state.begin_removal(docs.clone()));
    assert!(!state.can_remove(&worktree("docs-71", false, Some(60))));
    state.begin_open(&docs);
    state.open_failed(docs.clone(), "open failed".to_owned());
    assert_eq!(state.activity(&fix), Some(Activity::Removing));
    assert_eq!(state.activity(&docs), Some(Activity::Failed("open failed")));
    // Opening the worktree being removed fails: the removal still runs.
    state.open_failed(fix.clone(), "open failed".to_owned());
    assert_eq!(state.removing, Some(fix.clone()));
    assert!(!state.begin_removal(docs.clone()));
    // Only the removal ending lets another start.
    state.end_removal(&fix, "fix-57", "gwm refused".to_owned());
    assert!(state.begin_removal(docs.clone()));
  }

  /// gwm prunes a worktree before deleting its directory: an error after
  /// that, or the guard's own "no longer a worktree", comes for a worktree the
  /// next list no longer shows. It stays until the user moves on.
  #[test]
  fn a_removal_error_outlives_its_worktree() {
    let mut state = state(None);
    assert!(state.begin_removal(PathBuf::from("/fix-57")));
    state.end_removal(
      Path::new("/fix-57"),
      "fix-57",
      "could not delete the directory".to_owned(),
    );
    state.apply(Event::Snapshot(vec![worktree("acme-api", true, None)]));
    let error = state.removal_error().unwrap();
    assert!(
      error.contains("fix-57") && error.contains("could not delete the directory"),
      "{error}"
    );
    state.begin_open(Path::new("/acme-api"));
    assert_eq!(state.removal_error(), None);
  }

  /// gwm prunes first: a removal can leave the list, letting the next one
  /// start, and still fail late. That failure ends its own removal only.
  #[test]
  fn a_late_failure_does_not_end_the_next_removal() {
    let mut state = state(None);
    let (fix, docs) = (PathBuf::from("/fix-57"), PathBuf::from("/docs-71"));
    assert!(state.begin_removal(fix.clone()));
    state.apply(Event::Snapshot(vec![
      worktree("acme-api", true, None),
      worktree("docs-71", false, Some(60)),
    ]));
    assert!(state.begin_removal(docs.clone()));
    state.end_removal(&fix, "fix-57", "post_remove hook failed".to_owned());
    assert_eq!(state.activity(&docs), Some(Activity::Removing));
    assert!(!state.begin_removal(PathBuf::from("/feat-42")));
  }

  #[test]
  fn an_error_clears_only_for_its_own_worktree() {
    let mut state = state(None);
    let (fix, docs) = (PathBuf::from("/fix-57"), PathBuf::from("/docs-71"));
    state.open_failed(docs.clone(), "open failed".to_owned());
    state.begin_open(&fix);
    assert_eq!(state.activity(&docs), Some(Activity::Failed("open failed")));
    state.begin_open(&docs);
    assert_eq!(state.activity(&docs), None);
  }

  #[test]
  fn no_remove_on_main_nor_on_the_worktree_the_app_was_opened_in() {
    let mut state = state(None);
    state.opened_in = Some(PathBuf::from("/fix-57"));
    assert!(!state.can_remove(&worktree("acme-api", true, None)));
    assert!(!state.can_remove(&worktree("fix-57", false, Some(60))));
    assert!(state.can_remove(&worktree("docs-71", false, Some(60))));
  }

  #[test]
  fn the_removal_prompt_says_what_is_lost_and_what_is_kept() {
    let mut fix = worktree("fix-57", false, Some(60));
    fix.branch = Some("fix/#57-rate-limit-headers".to_owned());
    let (level, message, detail) = removal_prompt(&fix);
    assert_eq!((level, message.as_str()), (PromptLevel::Warning, "Remove fix-57?"));
    assert!(detail.contains("no uncommitted changes"), "{detail}");
    // gwm's `is_dirty` leaves out ignored files, `remove` deletes them anyway.
    assert!(detail.contains("files git ignores"), "{detail}");
    assert!(!detail.contains("locked"), "{detail}");
    assert!(
      detail.contains("Its branch, fix/#57-rate-limit-headers, is kept."),
      "{detail}"
    );

    fix.is_locked = true;
    let (_, _, detail) = removal_prompt(&fix);
    assert!(detail.contains("It is locked, and gwm removes it anyway."), "{detail}");
    fix.is_locked = false;

    fix.status.is_dirty = true;
    let (level, _, detail) = removal_prompt(&fix);
    assert_eq!(level, PromptLevel::Critical);
    assert!(detail.contains("uncommitted changes: they will be lost"), "{detail}");
    assert!(detail.contains("gwm undo does not bring them back"), "{detail}");
  }

  /// `demo()` and a second worktree for an issue another one already has.
  fn with_issue(id: &str, issue: u64) -> Worktree {
    let mut worktree = worktree(id, false, Some(0));
    worktree.issue = Some(issue);
    worktree
  }

  /// gwm makes the worktree, then bootstraps it: the daemon can list it before
  /// gwm returns. It is selected once gwm returned, and only the new one.
  #[test]
  fn a_created_worktree_is_selected_once_gwm_returned_and_a_list_shows_it() {
    let mut state = State::new(Listing::Ready(vec![with_issue("feat-42-old", 42)]), None);
    assert!(state.begin_create(Create::triple("feat", "42", "new").unwrap()));
    state.end_create(Ok(()));
    assert_eq!(state.selected, None);
    state.apply(Event::Snapshot(vec![
      with_issue("feat-42-old", 42),
      with_issue("feat-42-new", 42),
    ]));
    assert_eq!(state.selected, Some(PathBuf::from("/feat-42-new")));

    // Listed while gwm still runs: selected when it returns.
    assert!(state.begin_create(Create::name("spike").unwrap()));
    let mut listed = state.worktrees().to_vec();
    listed.push(worktree("spike", false, Some(0)));
    state.apply(Event::Snapshot(listed));
    assert_eq!(state.selected, Some(PathBuf::from("/feat-42-new")));
    state.end_create(Ok(()));
    assert_eq!(state.selected, Some(PathBuf::from("/spike")));
  }

  /// `gwm create --issue 57` with a worktree for #57: gwm prints it and exits 0.
  #[test]
  fn from_an_issue_that_has_a_worktree_it_is_selected() {
    let mut state = State::new(
      Listing::Ready(vec![with_issue("fix-57", 57), with_issue("docs-71", 71)]),
      None,
    );
    assert!(state.begin_create(Create::issue("57", "").unwrap()));
    state.end_create(Ok(()));
    assert_eq!(state.selected().unwrap().id, "fix-57");
    assert_eq!(state.to_select, None);
  }

  #[test]
  fn one_create_at_a_time_and_a_failure_keeps_gwms_words() {
    let mut state = state(None);
    assert!(state.begin_create(Create::triple("nope", "3", "x").unwrap()));
    assert!(state.creating());
    assert!(!state.begin_create(Create::name("spike").unwrap()));
    state.end_create(Err("error: invalid branch type 'nope'".to_owned()));
    assert!(!state.creating());
    assert_eq!(state.create_error(), Some("error: invalid branch type 'nope'"));
    // Nothing is selected from a failed creation, even if a list shows a match.
    state.apply(Event::Snapshot(vec![with_issue("nope-3-x", 3)]));
    assert_eq!(state.selected, None);
    assert_eq!(state.create_error(), Some("error: invalid branch type 'nope'"));
    assert!(state.begin_create(Create::name("spike").unwrap()));
    assert_eq!(state.create_error(), None);
  }

  /// Without a list, nothing tells the new worktree from an older one of the
  /// same issue: no creation before the first snapshot, nor after a failure.
  #[test]
  fn no_create_before_the_list_is_ready() {
    for listing in [Listing::Loading, Listing::Failed("gwm list failed".to_owned())] {
      let mut state = State::new(listing, None);
      assert!(!state.can_create());
      assert!(!state.begin_create(Create::issue("42", "").unwrap()));
      assert!(!state.creating());
    }
    assert!(state(None).can_create());
  }

  /// The user picks a worktree while gwm still runs: gwm's return does not take
  /// the selection back.
  #[test]
  fn a_selection_made_while_gwm_runs_is_not_taken_back() {
    let mut state = state(None);
    assert!(state.begin_create(Create::name("spike").unwrap()));
    state.select(PathBuf::from("/docs-71"));
    state.end_create(Ok(()));
    let mut listed = demo();
    listed.push(worktree("spike", false, Some(0)));
    state.apply(Event::Snapshot(listed));
    assert_eq!(state.selected().unwrap().id, "docs-71");
  }

  /// The user moved on before the list showed the new worktree.
  #[test]
  fn a_selection_by_the_user_is_not_taken_back() {
    let mut state = state(None);
    assert!(state.begin_create(Create::name("spike").unwrap()));
    state.end_create(Ok(()));
    state.step(1);
    let mut listed = demo();
    listed.push(worktree("spike", false, Some(0)));
    state.apply(Event::Snapshot(listed));
    assert_eq!(state.selected().unwrap().id, "docs-71");
  }

  /// What `gwm status` says of a worktree with PR `pr`.
  fn with_pr(pr: u64) -> Result<Links, String> {
    let json = format!(r#"{{"branch":"x","issue":null,"pr":{{"number":{pr},"source":"detected"}}}}"#);
    Ok(serde_json::from_str(&json).unwrap())
  }

  /// Once per selection, not per snapshot: the daemon sends one a second.
  #[test]
  fn links_are_fetched_on_each_selection_not_per_snapshot() {
    let mut state = state(None);
    assert_eq!(state.begin_links(), Some(PathBuf::from("/acme-api")));
    assert_eq!(state.links(state.selected().unwrap()), None);
    assert_eq!(state.begin_links(), None);
    state.end_links(with_pr(706));
    assert_eq!(state.links(state.selected().unwrap()), Some(&with_pr(706)));
    state.apply(Event::Snapshot(demo()));
    assert_eq!(state.begin_links(), None);
    state.step(1);
    assert_eq!(state.begin_links(), Some(PathBuf::from("/docs-71")));
    state.end_links(with_pr(9));
    // Back to a worktree already fetched: its CI may have finished since.
    state.step(-1);
    assert_eq!(state.begin_links(), Some(PathBuf::from("/acme-api")));
  }

  /// `gwm status` saves the PR it detects, and the next list shows it: that
  /// is its own answer coming back, not a new link to ask about.
  #[test]
  fn the_pr_gwm_status_detected_does_not_ask_again() {
    let mut state = state(None);
    assert_eq!(state.begin_links(), Some(PathBuf::from("/acme-api")));
    state.end_links(with_pr(706));
    let mut listed = demo();
    listed[0].pr = Some(706);
    state.apply(Event::Snapshot(listed));
    assert_eq!(state.links(state.selected().unwrap()), Some(&with_pr(706)));
    assert_eq!(state.begin_links(), None);
  }

  /// Coming back to a worktree asks gwm again, and shows what it said last
  /// until it answers.
  #[test]
  fn a_reselection_refetches_and_keeps_the_last_result_shown() {
    let mut state = state(None);
    state.begin_links();
    state.end_links(with_pr(706));
    state.step(1);
    state.begin_links();
    state.end_links(with_pr(9));
    state.step(-1);
    assert!(!state.refreshing(state.selected().unwrap()));
    assert_eq!(state.begin_links(), Some(PathBuf::from("/acme-api")));
    assert_eq!(state.links(state.selected().unwrap()), Some(&with_pr(706)));
    assert!(state.refreshing(state.selected().unwrap()));
    state.end_links(with_pr(707));
    assert_eq!(state.links(state.selected().unwrap()), Some(&with_pr(707)));
    assert!(!state.refreshing(state.selected().unwrap()));
    assert_eq!(state.begin_links(), None);
  }

  /// gh is slow: arrowing down the list asks for the row it stops on, and a
  /// result lands on the worktree it was asked for, never on the selection.
  #[test]
  fn one_fetch_at_a_time_and_its_result_stays_on_its_worktree() {
    let mut state = state(None);
    assert_eq!(state.begin_links(), Some(PathBuf::from("/acme-api")));
    state.step(1);
    state.step(1);
    assert_eq!(state.begin_links(), None);
    state.end_links(with_pr(706));
    assert_eq!(state.links(state.selected().unwrap()), None);
    assert_eq!(state.links(&state.worktrees()[0]), Some(&with_pr(706)));
    assert_eq!(state.begin_links(), Some(PathBuf::from("/fix-57")));
  }

  /// The list's branch, issue or PR changed: what gwm said before no longer
  /// holds, even when it changed while gwm ran.
  #[test]
  fn links_are_fetched_again_when_the_list_links_change() {
    let mut state = state(None);
    state.begin_links();
    state.end_links(with_pr(1));
    let mut worktrees = demo();
    worktrees[0].pr = Some(706);
    state.apply(Event::Snapshot(worktrees.clone()));
    assert_eq!(state.links(state.selected().unwrap()), None);
    assert_eq!(state.begin_links(), Some(PathBuf::from("/acme-api")));

    worktrees[0].branch = Some("dev".to_owned());
    state.apply(Event::Snapshot(worktrees));
    state.end_links(with_pr(706));
    assert_eq!(state.links(state.selected().unwrap()), None);
    assert_eq!(state.begin_links(), Some(PathBuf::from("/acme-api")));
  }

  /// A failure is shown as is, not retried on every repaint, and retried when
  /// the user comes back to the worktree, shown until gwm answers.
  #[test]
  fn a_failure_is_kept_until_the_worktree_is_selected_again() {
    let mut state = state(None);
    state.begin_links();
    state.end_links(Err("gh: HTTP 502".to_owned()));
    assert_eq!(
      state.links(state.selected().unwrap()),
      Some(&Err("gh: HTTP 502".to_owned()))
    );
    assert_eq!(state.begin_links(), None);
    state.step(1);
    state.begin_links();
    state.end_links(with_pr(9));
    state.step(-1);
    assert_eq!(state.begin_links(), Some(PathBuf::from("/acme-api")));
    assert_eq!(
      state.links(state.selected().unwrap()),
      Some(&Err("gh: HTTP 502".to_owned()))
    );
  }

  #[test]
  fn only_a_list_a_daemon_follows_is_live() {
    let cases = [
      (Some(Source::Daemon("/d.sock".into())), true),
      (Some(Source::OwnDaemon("/own.sock".into())), true),
      (Some(Source::Lost("gone".into())), false),
      (Some(Source::Once("no daemon".into())), false),
      (None, false),
    ];
    for (source, live) in cases {
      assert_eq!(state(source.clone()).live(), live, "{source:?}");
    }
  }

  /// A session's activity moves often, and the daemon sends a snapshot each
  /// time: `Worktree` compares unequal, and nothing may key on that. No
  /// `gwm status` runs again, and the selection stays where it is.
  #[test]
  fn a_snapshot_that_differs_only_by_its_sessions_refetches_nothing() {
    let sessions = |last_activity: u64| {
      let json = format!(
        r#"{{"top":{{"kind":"claude","freshness":"active","last_activity":{last_activity},"id":"s"}},"sessions":[]}}"#
      );
      Some(Ok(serde_json::from_str(&json).unwrap()))
    };
    let mut state = state(None);
    state.select(PathBuf::from("/fix-57"));
    assert_eq!(state.begin_links(), Some(PathBuf::from("/fix-57")));
    state.end_links(with_pr(706));
    let mut listed = demo();
    for last_activity in [1_794_491_219, 1_794_491_249] {
      let before = listed.clone();
      listed[1].agents = sessions(last_activity);
      assert_ne!(listed, before);
      state.apply(Event::Snapshot(listed.clone()));
      assert_eq!(state.begin_links(), None);
      assert_eq!(state.links(state.selected().unwrap()), Some(&with_pr(706)));
      assert_eq!(state.selected, Some(PathBuf::from("/fix-57")));
      assert_eq!(state.selected().unwrap().agents, sessions(last_activity));
    }
  }

  #[test]
  fn a_view_that_goes_tells_its_watch_to_stop() {
    let watch = Watch::default();
    let stopped = watch.stopped.clone();
    drop(watch);
    assert!(stopped.load(Ordering::Relaxed));
  }

  #[test]
  fn a_failure_keeps_why_the_list_is_not_live() {
    let mut state = state(Some(Source::Once("no daemon".into())));
    state.apply(Event::Failed("gwm list failed".to_owned()));
    assert_eq!(state.source, Some(Source::Once("no daemon".into())));
  }
}
