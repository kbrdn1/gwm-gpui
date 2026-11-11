//! The central card: the selected worktree, from the contract's fields and
//! nothing else, and what can be done to it: open it in an app, remove it.
//! Its issue and PR come from `gwm status --json`, live when gwm's `gh`
//! resolved them. Its changed files are not here: no gwm contract lists them,
//! requested in kbrdn1/gwm-cli#712.

use gpui::{AnyElement, Context, FontWeight, Hsla, SharedString, div, prelude::*, px};
use gwm_client::{IssueState, Links, PrState, Worktree};

use crate::{
  age::Ages,
  apps::Installed,
  open_menu::open_menu,
  sidebar::one_line,
  theme,
  title_bar::tile,
  workspace::{Activity, Workspace},
};

/// What the card needs beyond the worktree: what is under way on it, whether
/// it can be removed now, and the "Open in…" popover, `Some` while open,
/// holding the apps found so far.
pub(crate) struct Actions<'a> {
  pub activity: Option<Activity<'a>>,
  /// The last removal's error, whichever worktree it was on.
  pub removal_error: Option<&'a str>,
  pub can_remove: bool,
  pub menu: Option<Option<&'a [Installed]>>,
  /// What `gwm status` said, `None` while it runs.
  pub links: Option<&'a Result<Links, String>>,
}

/// How much of the HEAD oid to show, as `git log --oneline` would.
const SHORT_OID: usize = 12;

pub(crate) fn detail(
  worktree: &Worktree,
  ages: Ages,
  actions: Actions,
  cx: &mut Context<Workspace>,
) -> impl IntoElement {
  let status = &worktree.status;
  let working_tree = if status.unknown {
    ("unknown", theme::ink(0.5))
  } else if status.is_dirty {
    ("uncommitted changes", theme::color(theme::DIRTY))
  } else {
    ("clean", theme::color(theme::CLEAN))
  };
  let upstream = if status.has_upstream {
    format!("{} ahead, {} behind", status.ahead, status.behind)
  } else {
    "none".to_owned()
  };
  let branch_age = worktree.age_seconds.map_or_else(
    // Null on the trunk and on a branch that has not diverged yet: no claim.
    || "—".to_owned(),
    |seconds| ages.label(seconds),
  );
  let links = actions.links;
  let flags: Vec<&str> = [
    (worktree.is_main, "main worktree"),
    (worktree.is_locked, "locked"),
    (worktree.is_prunable, "prunable"),
  ]
  .into_iter()
  .filter_map(|(set, flag)| set.then_some(flag))
  .collect();

  div()
    .size_full()
    .flex()
    .flex_col()
    .gap(px(24.))
    .p(px(28.))
    .child(
      div()
        .flex()
        .items_center()
        .gap(px(12.))
        .child(tile(px(40.), px(34.), px(8.), 20.))
        .child(
          div()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(
              one_line(worktree.name.clone())
                .text_size(px(20.))
                .font_weight(FontWeight::SEMIBOLD),
            )
            .child(
              one_line(worktree.branch.clone().unwrap_or_else(|| "detached HEAD".to_owned()))
                .text_color(theme::ink(0.5)),
            ),
        ),
    )
    .child(action_row(worktree, actions, cx))
    .child(
      div()
        .flex()
        .flex_col()
        .child(fact("Path", mono(worktree.path.display().to_string())))
        .child(fact(
          "HEAD",
          mono(
            worktree
              .head
              .as_deref()
              .map_or("unresolved", |oid| oid.get(..SHORT_OID).unwrap_or(oid))
              .to_owned(),
          ),
        ))
        .child(fact("Working tree", text(working_tree.0, working_tree.1)))
        .child(fact("Upstream", text(&upstream, theme::ink(0.9))))
        .child(fact("Issue", issue(worktree.issue, links)))
        .child(fact("Pull request", pull_request(worktree.pr, links)))
        .child(fact("Branch age", text(&branch_age, theme::ink(0.9))))
        .when(!flags.is_empty(), |facts| {
          facts.child(fact("Flags", text(&flags.join(", "), theme::ink(0.9))))
        }),
    )
    .children(
      links
        .and_then(|links| links.as_ref().err())
        .map(|error| error_text(format!("Issue and pull request: {error}"))),
    )
}

/// The issue as `gwm status` gave it. Before it answers, or when it failed,
/// the list's number.
fn issue(listed: Option<u64>, links: Option<&Result<Links, String>>) -> AnyElement {
  let Some(Ok(links)) = links else {
    return unresolved(listed, links.is_none());
  };
  let Some(issue) = &links.issue else {
    return text("none", theme::ink(0.9)).into_any_element();
  };
  let state = issue.state.map(|state| match state {
    IssueState::Open => ("Open", theme::color(theme::CLEAN)),
    IssueState::Closed => ("Closed", theme::color(theme::SETTLED)),
  });
  link(
    "issue",
    issue.number,
    state,
    None,
    issue.title.as_deref(),
    issue.url.clone(),
  )
}

/// The PR as `gwm status` gave it, with its checks. Before it answers, or
/// when it failed, the list's number.
fn pull_request(listed: Option<u64>, links: Option<&Result<Links, String>>) -> AnyElement {
  let Some(Ok(links)) = links else {
    return unresolved(listed, links.is_none());
  };
  let Some(pr) = &links.pr else {
    return text("none", theme::ink(0.9)).into_any_element();
  };
  let state = pr.state.map(|state| match state {
    PrState::Open => ("Open", theme::color(theme::CLEAN)),
    PrState::Draft => ("Draft", theme::ink(0.5)),
    PrState::Merged => ("Merged", theme::color(theme::SETTLED)),
    PrState::Closed => ("Closed", theme::color(theme::ERROR)),
  });
  let checks = match (pr.checks_passed, pr.checks_total) {
    // No checks: nothing to say, as gwm says nothing.
    (Some(passed), Some(total)) if total > 0 => {
      // Fewer passed than ran: failing or still running, the contract does
      // not say which.
      let color = if passed == total { theme::CLEAN } else { theme::WARNING };
      Some((format!("{passed}/{total} checks passed"), theme::color(color)))
    }
    _ => None,
  };
  link(
    "pull-request",
    pr.number,
    state,
    checks,
    pr.title.as_deref(),
    pr.url.clone(),
  )
}

fn unresolved(listed: Option<u64>, loading: bool) -> AnyElement {
  div()
    .flex()
    .items_center()
    .gap(px(8.))
    .child(text(&number(listed), theme::ink(0.9)).flex_none())
    .when(loading, |row| row.child(text("loading…", theme::ink(0.45))))
    .into_any_element()
}

/// `#n`, then each live field gwm gave: its state, its checks, its title.
/// Without any, it says so. A click opens its `url`.
fn link(
  id: &'static str,
  number: u64,
  state: Option<(&'static str, Hsla)>,
  checks: Option<(String, Hsla)>,
  title: Option<&str>,
  url: Option<String>,
) -> AnyElement {
  let live = state.is_some() || checks.is_some() || title.is_some();
  div()
    .id(id)
    .min_w_0()
    .flex()
    .items_center()
    .gap(px(8.))
    .child(text(&format!("#{number}"), theme::ink(0.9)).flex_none())
    .children(state.map(|(label, color)| text(label, color).flex_none()))
    .children(checks.map(|(label, color)| text(&label, color).flex_none()))
    .children(title.map(|title| text(title, theme::ink(0.6))))
    .when(!live, |row| row.child(text("no live state from gwm", theme::ink(0.45))))
    .when_some(url, |row, url| {
      row
        .cursor_pointer()
        .rounded(px(4.))
        .hover(|row| row.bg(theme::hover()))
        .on_click(move |_, _, cx| cx.open_url(&url))
    })
    .into_any_element()
}

/// "Open in…" and "Remove…", then what came of the last action: gwm's or
/// `open`'s own words when it failed.
fn action_row(worktree: &Worktree, actions: Actions, cx: &mut Context<Workspace>) -> impl IntoElement {
  let removing = matches!(actions.activity, Some(Activity::Removing));
  let failed = match actions.activity {
    Some(Activity::Failed(error)) => Some(error.to_owned()),
    _ => None,
  };
  div()
    .flex()
    .flex_col()
    .gap(px(10.))
    .child(
      div()
        .flex()
        .items_center()
        .gap(px(8.))
        .child(
          div()
            .relative()
            .child(
              button("open-in", "Open in…", theme::ink(0.9))
                .on_click(cx.listener(|workspace, _, _, cx| workspace.toggle_menu(cx))),
            )
            .children(actions.menu.map(|apps| {
              div()
                .absolute()
                .top(px(32.))
                .left_0()
                .child(open_menu(apps, &worktree.path, cx))
            })),
        )
        // Under way: a state, not a button.
        .when(removing, |row| row.child(pill("Removing…", theme::ink(0.45))))
        // No button that could only refuse (see `State::can_remove`).
        .when(actions.can_remove, |row| {
          row.child(
            button("remove", "Remove…", theme::color(theme::ERROR))
              .on_click(cx.listener(|workspace, _, window, cx| workspace.confirm_remove(window, cx))),
          )
        }),
    )
    .children(failed.map(error_text))
    .children(actions.removal_error.map(|error| {
      div()
        .flex()
        .items_start()
        .gap(px(8.))
        .child(error_text(error.to_owned()).flex_1().min_w_0())
        .child(
          button("dismiss-removal-error", "Dismiss", theme::ink(0.9))
            .flex_none()
            .on_click(cx.listener(|workspace, _, _, cx| workspace.dismiss_removal_error(cx))),
        )
    }))
}

pub(crate) fn error_text(error: String) -> gpui::Div {
  div()
    .text_size(px(12.))
    .text_color(theme::color(theme::ERROR))
    .child(error)
}

pub(crate) fn button(id: &'static str, label: &'static str, color: Hsla) -> gpui::Stateful<gpui::Div> {
  pill(label, color)
    .id(id)
    .cursor_pointer()
    .hover(|button| button.bg(theme::selected()))
}

pub(crate) fn pill(label: &'static str, color: Hsla) -> gpui::Div {
  div()
    .h(px(28.))
    .px(px(10.))
    .flex()
    .items_center()
    .rounded(px(6.))
    .bg(theme::hover())
    .border_1()
    .border_color(theme::card_border())
    .text_size(px(12.))
    .text_color(color)
    .child(label)
}

fn number(value: Option<u64>) -> String {
  value.map_or_else(|| "none".to_owned(), |number| format!("#{number}"))
}

fn fact(label: &'static str, value: impl IntoElement) -> impl IntoElement {
  div()
    .h(px(30.))
    .flex()
    .items_center()
    .border_b_1()
    .border_color(theme::ink(0.04))
    .child(
      div()
        .w(px(132.))
        .flex_none()
        .text_size(px(12.))
        .text_color(theme::ink(0.45))
        .child(label),
    )
    .child(div().flex_1().min_w_0().child(value))
}

fn text(value: &str, color: Hsla) -> gpui::Div {
  one_line(SharedString::from(value.to_owned())).text_color(color)
}

fn mono(value: String) -> gpui::Div {
  one_line(value)
    .font_family(theme::MONO_FONT)
    .text_size(px(12.))
    .text_color(theme::ink(0.9))
}
