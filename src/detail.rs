//! The central card: the selected worktree, from the contract's fields and
//! nothing else, and what can be done to it: open it in an app, remove it.
//! Its changed files are not here: `gwm status --json` carries `branch`,
//! `issue` and `pr`, no file list.

use gpui::{Context, FontWeight, Hsla, SharedString, div, prelude::*, px};
use gwm_client::Worktree;

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
  pub can_remove: bool,
  pub menu: Option<Option<&'a [Installed]>>,
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
        .child(fact("Issue", text(&number(worktree.issue), theme::ink(0.9))))
        .child(fact("Pull request", text(&number(worktree.pr), theme::ink(0.9))))
        .child(fact("Branch age", text(&branch_age, theme::ink(0.9))))
        .when(!flags.is_empty(), |facts| {
          facts.child(fact("Flags", text(&flags.join(", "), theme::ink(0.9))))
        }),
    )
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
    .children(failed.map(|error| {
      div()
        .text_size(px(12.))
        .text_color(theme::color(theme::ERROR))
        .child(error)
    }))
}

fn button(id: &'static str, label: &'static str, color: Hsla) -> gpui::Stateful<gpui::Div> {
  pill(label, color)
    .id(id)
    .cursor_pointer()
    .hover(|button| button.bg(theme::selected()))
}

fn pill(label: &'static str, color: Hsla) -> gpui::Div {
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

fn fact(label: &'static str, value: gpui::Div) -> impl IntoElement {
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
    .child(value.flex_1())
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
