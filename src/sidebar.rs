//! The sidebar: the repo's project picker, the "New" row, then its worktrees
//! in dense rows, grouped the way Monocode groups sessions
//! (`shell/sidebar_list.rs`, `shell.rs` session rows).

use std::path::Path;

use gpui::{Context, FontWeight, Hsla, ScrollHandle, SharedString, div, prelude::*, px, svg};
use gwm_client::Worktree;

use crate::{
  age::{Ages, Bucket},
  assets,
  project_picker::ProjectPicker,
  sessions::{self, Carried, Mark},
  theme,
  workspace::{Place, State, Workspace, order, place},
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

/// A direct child of the sidebar's list. `sidebar` draws one element for
/// each, in this order, so a child's position here is the index
/// `scroll_to_item` takes.
#[derive(Debug, PartialEq)]
pub(crate) enum Child {
  Title(&'static str),
  /// The worktree at this index, with its sessions inside it once it is
  /// selected: they are no children of the list.
  Worktree(usize),
}

/// The list's children: a title per section, then its worktrees.
pub(crate) fn children(worktrees: &[Worktree], bucket: impl Fn(u64) -> Bucket) -> Vec<Child> {
  let mut children = Vec::new();
  for section in sections(worktrees, bucket) {
    children.push(Child::Title(section.title));
    children.extend(section.rows.into_iter().map(Child::Worktree));
  }
  children
}

/// The list's child index of the worktree `selected`.
pub(crate) fn child_of(
  worktrees: &[Worktree],
  selected: Option<&Path>,
  bucket: impl Fn(u64) -> Bucket,
) -> Option<usize> {
  let selected = selected?;
  children(worktrees, bucket)
    .iter()
    .position(|child| matches!(child, Child::Worktree(index) if worktrees[*index].path == selected))
}

/// `form_open`: the "New worktree" form is shown, which the "New" row says.
pub(crate) fn sidebar(
  state: &State,
  selected: Option<&Worktree>,
  ages: Ages,
  scroll: &ScrollHandle,
  form_open: bool,
  cx: &mut Context<Workspace>,
) -> impl IntoElement {
  let (worktrees, live) = (state.worktrees(), state.live());
  let main = worktrees.iter().find(|worktree| worktree.is_main);
  // One element per child, in `children`'s order: a child's position is the
  // index `scroll_to_item` takes. A loop, not a `map`: every row borrows `cx`
  // for its click listener.
  let mut list = div()
    .id("worktrees")
    .track_scroll(scroll)
    .flex_1()
    .min_h_0()
    .overflow_y_scroll()
    .px(px(8.))
    .pb(px(12.));
  for (position, child) in children(worktrees, |age| ages.bucket(age)).into_iter().enumerate() {
    list = list.child(match child {
      Child::Title(title) => div()
        .when(position > 0, |title| title.mt(px(12.)))
        .child(section_title(title))
        .into_any_element(),
      Child::Worktree(index) => {
        let worktree = &worktrees[index];
        let age = worktree.age_seconds.map(|age| ages.label(age));
        let is_selected = selected.is_some_and(|selected| selected.path == worktree.path);
        let carried = sessions::carried(worktree, live, ages);
        // Only the selected worktree lists its sessions. Monocode opens a rail
        // while its thread is in view, chosen or working, on its own knowledge
        // of a response under way; gwm's `active` is a 300 s window on the last
        // artefact written, so only "chosen" carries over.
        let nested = is_selected.then(|| sessions::nested(worktree, live, ages)).flatten();
        div()
          .child(row(worktree, is_selected, age, carried, cx))
          .children(nested)
          .into_any_element()
      }
    });
  }
  div()
    .w(theme::SIDEBAR_WIDTH)
    .h_full()
    .flex()
    .flex_col()
    .bg(theme::wash())
    .children(main.map(|main| ProjectPicker {
      name: main.name.clone(),
      branch: main.branch.clone(),
    }))
    // Monocode's navigation block. No list yet, no "New": nothing would tell
    // the new worktree apart, and the title bar has no `+` either.
    .when(state.can_create(), |sidebar| {
      sidebar.child(
        div()
          .flex_none()
          .px(px(8.))
          .pt(px(4.))
          .pb(px(12.))
          .child(new_row(form_open, cx)),
      )
    })
    .child(list)
}

/// Monocode's "New" navigation row (`sidebar_navigation`): its glyph, the
/// word, and at the right the shortcut that does the same. It opens the
/// "New worktree" form, and stands selected while the form is shown.
fn new_row(form_open: bool, cx: &mut Context<Workspace>) -> impl IntoElement {
  div()
    .id("new-worktree-row")
    .h(theme::NAV_ROW_HEIGHT)
    .px(px(8.))
    .flex()
    .items_center()
    .gap(px(8.))
    .rounded(theme::CONTROL_RADIUS)
    .cursor_pointer()
    .text_size(px(14.))
    .font_weight(FontWeight::MEDIUM)
    .text_color(theme::ink(if form_open { 1. } else { 0.5 }))
    .when(form_open, |row| row.bg(theme::selected()))
    .hover(|row| row.bg(theme::selected()).text_color(theme::ink(1.)))
    .on_click(cx.listener(|workspace, _, window, cx| workspace.open_form(window, cx)))
    .child(icon(assets::NEW, 16., theme::ink(if form_open { 0.7 } else { 0.35 })))
    .child(div().flex_1().child("New"))
    .child(
      div()
        .w(theme::NAV_TRAILING_SLOT)
        .flex_none()
        .flex()
        .justify_center()
        .whitespace_nowrap()
        .text_size(px(11.))
        .font_weight(FontWeight::NORMAL)
        .text_color(theme::ink(0.35))
        .child(crate::shortcut(crate::NEW_WORKTREE_KEY)),
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
/// branch, what gwm knows of its state, and the agent of its top session.
/// While gwm calls that session active, its state stands where the age is, as
/// "Working…" takes the time's place in Monocode's row (`session_suffix`).
fn row(
  worktree: &Worktree,
  selected: bool,
  age: Option<String>,
  carried: Option<Carried>,
  cx: &mut Context<Workspace>,
) -> impl IntoElement {
  let agent = carried.as_ref().map(sessions::agent);
  let active = matches!(carried, Some(Carried::Top { active: true, .. }));
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
            .children(if active {
              Some(sessions::mark_label(Mark::Active))
            } else {
              age.map(|age| {
                div()
                  .flex_none()
                  .text_size(px(11.))
                  .text_color(theme::ink(0.4))
                  .child(age)
              })
            }),
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
            })
            .children(agent),
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

  /// A worktree's sessions are drawn inside its own child of the list: they
  /// add none, so the rows after it keep the index the scroll is given.
  #[test]
  fn a_worktrees_sessions_are_no_children_of_the_list() {
    let mut worktrees = [
      worktree("main", true, None),
      worktree("today", false, Some(1)),
      worktree("later", false, Some(2)),
    ];
    let session = r#"{"kind":"claude","freshness":"idle","last_activity":1794491219,"id":"s"}"#;
    let agents = format!(r#"{{"top":{session},"sessions":[{session},{session},{session}]}}"#);
    worktrees[1].agents = Some(Ok(serde_json::from_str(&agents).unwrap()));
    let bucket = |_| Bucket::Today;
    assert_eq!(
      children(&worktrees, bucket),
      [
        Child::Title("Main"),
        Child::Worktree(0),
        Child::Title("Today"),
        Child::Worktree(1),
        Child::Worktree(2),
      ]
    );
    assert_eq!(child_of(&worktrees, Some(Path::new("/later")), bucket), Some(4));
  }
}
