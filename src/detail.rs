//! The central card: the selected worktree, from the contract's fields and
//! nothing else. Its changed files are not here: `gwm status --json` carries
//! `branch`, `issue` and `pr`, no file list.

use gpui::{FontWeight, Hsla, SharedString, div, prelude::*, px};
use gwm_client::Worktree;

use crate::{age::Ages, sidebar::one_line, theme, title_bar::tile};

/// How much of the HEAD oid to show, as `git log --oneline` would.
const SHORT_OID: usize = 12;

pub(crate) fn detail(worktree: &Worktree, ages: Ages) -> impl IntoElement {
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
