//! The sidebar: the repo, then its worktrees in dense rows, grouped the way
//! Monocode groups sessions (`shell/sidebar_list.rs`, `shell.rs` session rows).

use std::path::Path;

use gpui::{Context, FontWeight, Hsla, ScrollHandle, SharedString, div, prelude::*, px, svg};
use gwm_client::Worktree;

use crate::{
  age::{Ages, Bucket},
  assets, theme,
  workspace::{Place, Workspace, order, place},
};

pub(crate) struct Section {
  pub title: &'static str,
  /// Indexes into the worktrees, in sidebar order.
  pub rows: Vec<usize>,
}

/// The sidebar's sections: the main worktree, one per day bucket, then the
/// branches gwm gives no age. `bucket` says which bucket an age falls in.
pub(crate) fn sections(worktrees: &[Worktree], bucket: impl Fn(u64) -> Bucket) -> Vec<Section> {
  let mut sections: Vec<Section> = Vec::new();
  for index in order(worktrees) {
    let title = match place(&worktrees[index]) {
      Place::Main => "Main",
      Place::Dated(age) => bucket(age).title(),
      Place::Undated => "Undated",
    };
    match sections.last_mut() {
      Some(section) if section.title == title => section.rows.push(index),
      _ => sections.push(Section {
        title,
        rows: vec![index],
      }),
    }
  }
  sections
}

/// The list's child index of the worktree `selected`: a title per section,
/// then its rows, as `sidebar` lays them out.
pub(crate) fn child_of(
  worktrees: &[Worktree],
  selected: Option<&Path>,
  bucket: impl Fn(u64) -> Bucket,
) -> Option<usize> {
  let selected = selected?;
  let mut child = 0;
  for section in sections(worktrees, bucket) {
    child += 1;
    for index in section.rows {
      if worktrees[index].path == selected {
        return Some(child);
      }
      child += 1;
    }
  }
  None
}

pub(crate) fn sidebar(
  worktrees: &[Worktree],
  selected: Option<&Worktree>,
  ages: Ages,
  scroll: &ScrollHandle,
  cx: &mut Context<Workspace>,
) -> impl IntoElement {
  let main = worktrees.iter().find(|worktree| worktree.is_main);
  // Titles and rows are all direct children of the list, in `child_of`'s
  // order, so a row's index is what `scroll_to_item` takes. Loops, not nested
  // `map`s: every row borrows `cx` for its click listener.
  let mut list = div()
    .id("worktrees")
    .track_scroll(scroll)
    .flex_1()
    .min_h_0()
    .overflow_y_scroll()
    .px(px(8.))
    .pb(px(12.));
  for (position, section) in sections(worktrees, |age| ages.bucket(age)).into_iter().enumerate() {
    list = list.child(
      div()
        .when(position > 0, |title| title.mt(px(12.)))
        .child(section_title(section.title)),
    );
    for index in section.rows {
      let worktree = &worktrees[index];
      let age = worktree.age_seconds.map(|age| ages.label(age));
      let is_selected = selected.is_some_and(|selected| selected.path == worktree.path);
      list = list.child(row(worktree, is_selected, age, cx));
    }
  }
  div()
    .w(theme::SIDEBAR_WIDTH)
    .h_full()
    .flex()
    .flex_col()
    .bg(theme::wash())
    .children(main.map(header))
    .child(list)
}

/// The repo, where Monocode puts its project selector. Not a menu: gwm-gpui
/// shows one repo per window.
fn header(main: &Worktree) -> impl IntoElement {
  div()
    .h(px(39.))
    .px(px(12.))
    .pb(px(1.))
    .flex()
    .items_center()
    .gap(px(8.))
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
    .child(
      one_line(main.name.clone())
        .text_size(px(13.))
        .font_weight(FontWeight::SEMIBOLD),
    )
    .children(
      main
        .branch
        .clone()
        .map(|branch| one_line(branch).flex_shrink_0().text_color(theme::ink(0.45))),
    )
}

fn section_title(title: &'static str) -> impl IntoElement {
  div()
    .h(px(28.))
    .px(px(8.))
    .flex()
    .items_center()
    .text_size(px(12.))
    .font_weight(FontWeight::MEDIUM)
    .text_color(theme::ink(0.5))
    .child(title)
}

/// A worktree as Monocode's detailed session row: its name and age, then its
/// branch and what gwm knows of its state.
fn row(worktree: &Worktree, selected: bool, age: Option<String>, cx: &mut Context<Workspace>) -> impl IntoElement {
  let path = worktree.path.clone();
  let status = &worktree.status;
  let branch = worktree.branch.clone().unwrap_or_else(|| "detached HEAD".to_owned());
  div()
    .id(SharedString::from(format!("worktree-{}", worktree.path.display())))
    .h(px(52.))
    .mb(px(1.))
    .px(px(9.))
    .flex()
    .items_center()
    .gap(px(8.))
    .rounded(px(6.))
    .cursor_pointer()
    .map(|row| {
      if selected {
        row.bg(theme::selected())
      } else {
        row.hover(|row| row.bg(theme::hover()))
      }
    })
    .on_click(cx.listener(move |workspace, _, window, cx| workspace.select(path.clone(), window, cx)))
    .child(icon(
      assets::BRANCH,
      16.,
      if selected { theme::accent() } else { theme::ink(0.35) },
    ))
    .child(
      div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(2.))
        .child(
          div()
            .flex()
            .items_center()
            .gap(px(8.))
            .child(
              one_line(worktree.name.clone())
                .flex_1()
                .text_size(px(13.))
                .font_weight(FontWeight::SEMIBOLD),
            )
            .children(age.map(|age| {
              div()
                .flex_none()
                .text_size(px(11.))
                .text_color(theme::ink(0.4))
                .child(age)
            })),
        )
        .child(
          div()
            .flex()
            .items_center()
            .gap(px(6.))
            .text_size(px(12.))
            .text_color(theme::ink(0.45))
            .child(one_line(branch).flex_1())
            .when(status.ahead > 0 || status.behind > 0, |line| {
              line.child(meta(format!("↑{} ↓{}", status.ahead, status.behind)))
            })
            .children(worktree.issue.map(|issue| meta(format!("#{issue}"))))
            .children(worktree.pr.map(|pr| meta(format!("PR {pr}"))))
            .when(status.is_dirty, |line| {
              line.child(
                div()
                  .flex_none()
                  .size(px(6.))
                  .rounded_full()
                  .bg(theme::color(theme::DIRTY)),
              )
            }),
        ),
    )
}

fn meta(text: String) -> impl IntoElement {
  div().flex_none().text_size(px(11.)).child(text)
}

pub(crate) fn one_line(text: impl Into<SharedString>) -> gpui::Div {
  div()
    .min_w_0()
    .overflow_hidden()
    .whitespace_nowrap()
    .text_ellipsis()
    .child(text.into())
}

pub(crate) fn icon(path: &'static str, size: f32, color: Hsla) -> impl IntoElement {
  svg().path(path).flex_none().size(px(size)).text_color(color)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::workspace::tests::worktree;

  #[test]
  fn sections_follow_the_sidebar_order_and_never_repeat() {
    let worktrees = [
      worktree("old", false, Some(40)),
      worktree("not-diverged", false, None),
      worktree("main", true, None),
      worktree("today", false, Some(1)),
      worktree("older", false, Some(50)),
      worktree("yesterday", false, Some(20)),
    ];
    // Ages stand for days here: the bucket is the test's, not the clock's.
    let bucket = |age: u64| match age {
      0..=9 => Bucket::Today,
      10..=29 => Bucket::Yesterday,
      _ => Bucket::Older,
    };
    let sections = sections(&worktrees, bucket);
    let titles: Vec<_> = sections.iter().map(|section| section.title).collect();
    assert_eq!(titles, ["Main", "Today", "Yesterday", "Older", "Undated"]);
    let older: Vec<_> = sections[3]
      .rows
      .iter()
      .map(|&index| worktrees[index].id.as_str())
      .collect();
    assert_eq!(older, ["old", "older"]);
    // Main's title and row, Today's title and row, Yesterday's title: 5.
    assert_eq!(child_of(&worktrees, Some(Path::new("/yesterday")), bucket), Some(5));
    assert_eq!(child_of(&worktrees, Some(Path::new("/gone")), bucket), None);
  }
}
