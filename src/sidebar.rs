//! The sidebar: the repo's project picker and the filter's magnifier, the
//! filter's field once open, the "New" row, then its worktrees in dense rows,
//! grouped the way Monocode groups sessions (`shell/sidebar_list.rs`,
//! `shell.rs` session rows), and under them the "Working" panel.

use std::path::Path;

use gpui::{Context, Entity, FontWeight, Hsla, ScrollHandle, SharedString, div, prelude::*, px, svg};
use gwm_client::Worktree;

use crate::{
  age::{Ages, Bucket},
  assets,
  filter::{self, Query},
  project_picker::ProjectPicker,
  sessions::{self, Carried, Mark},
  text_input::TextInput,
  theme, working,
  workspace::{Place, State, Workspace, order, place},
};

pub(crate) struct Section {
  pub title: &'static str,
  /// Indexes into the worktrees, in sidebar order.
  pub rows: Vec<usize>,
}

/// The sidebar's sections: the main worktree, one per day bucket, then the
/// branches gwm gives no age. `bucket` says which bucket an age falls in.
/// Only the worktrees `query` keeps are in them: a section with none is not
/// there.
pub(crate) fn sections(worktrees: &[Worktree], query: &Query, bucket: impl Fn(u64) -> Bucket) -> Vec<Section> {
  let mut sections: Vec<Section> = Vec::new();
  for index in order(worktrees, query) {
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
  /// The filter keeps no worktree: the list says so, in place of its rows.
  NoMatch,
}

/// The list's children: a title per section, then its worktrees, those
/// `query` keeps.
pub(crate) fn children(worktrees: &[Worktree], query: &Query, bucket: impl Fn(u64) -> Bucket) -> Vec<Child> {
  let mut children = Vec::new();
  for section in sections(worktrees, query, bucket) {
    children.push(Child::Title(section.title));
    children.extend(section.rows.into_iter().map(Child::Worktree));
  }
  if children.is_empty() && !query.is_empty() {
    children.push(Child::NoMatch);
  }
  children
}

/// The list's child index of the worktree `selected`, in the list as `query`
/// leaves it: `None` for a worktree it hides.
pub(crate) fn child_of(
  worktrees: &[Worktree],
  query: &Query,
  selected: Option<&Path>,
  bucket: impl Fn(u64) -> Bucket,
) -> Option<usize> {
  let selected = selected?;
  children(worktrees, query, bucket)
    .iter()
    .position(|child| matches!(child, Child::Worktree(index) if worktrees[*index].path == selected))
}

/// `filter`: the filter's field, while it is open. `form_open`: the "New
/// worktree" form is shown, which the "New" row says.
pub(crate) fn sidebar(
  state: &State,
  selected: Option<&Worktree>,
  ages: Ages,
  scroll: &ScrollHandle,
  filter: Option<&Entity<TextInput>>,
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
  let shown = children(worktrees, &state.query, |age| ages.bucket(age));
  for (position, child) in shown.into_iter().enumerate() {
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
      Child::NoMatch => div()
        .px(px(8.))
        .py(px(20.))
        .flex()
        .justify_center()
        .text_size(px(12.))
        .text_color(theme::ink(0.45))
        .child("No worktrees match")
        .into_any_element(),
    });
  }
  div()
    .w(theme::SIDEBAR_WIDTH)
    .h_full()
    .flex()
    .flex_col()
    .bg(theme::wash())
    // Monocode's header: the project picker, then its buttons. Of those, the
    // magnifier has a function here.
    .children(main.map(|main| {
      div()
        .flex_none()
        .pr(px(12.))
        .flex()
        .items_center()
        .child(div().flex_1().min_w_0().child(ProjectPicker {
          name: main.name.clone(),
          branch: main.branch.clone(),
        }))
        .child(filter::button(filter.is_some(), cx))
    }))
    // Monocode's navigation block: its search field, here the filter's, over
    // its rows, here "New" alone.
    .child(
      div()
        .flex_none()
        .px(px(8.))
        .pt(px(4.))
        .pb(px(12.))
        .children(filter.map(filter::field))
        // No list yet, no "New": nothing would tell the new worktree apart,
        // and the title bar has no `+` either.
        .when(state.can_create(), |navigation| {
          navigation.child(new_row(form_open, cx))
        }),
    )
    .child(list)
    // Under the list, as in Monocode: nothing when no session is active.
    .children(
      working::working(worktrees, live)
        .map(|working| working::panel(working, selected.map(|selected| selected.path.as_path()), cx)),
    )
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

/// A worktree's branch as its row says it, which is what the filter looks in.
pub(crate) fn branch(worktree: &Worktree) -> &str {
  worktree.branch.as_deref().unwrap_or("detached HEAD")
}

/// An issue and a PR as a row writes them.
pub(crate) fn issue(number: u64) -> String {
  format!("#{number}")
}

pub(crate) fn pr(number: u64) -> String {
  format!("PR {number}")
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
  let branch = branch(worktree).to_owned();
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
            .children(worktree.issue.map(|number| meta(issue(number))))
            .children(worktree.pr.map(|number| meta(pr(number))))
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
    let (worktrees, bucket) = (dated(), day);
    let all = Query::default();
    let sections = sections(&worktrees, &all, bucket);
    let titles: Vec<_> = sections.iter().map(|section| section.title).collect();
    assert_eq!(titles, ["Main", "Today", "Yesterday", "Older", "Undated"]);
    let older: Vec<_> = sections[3]
      .rows
      .iter()
      .map(|&index| worktrees[index].id.as_str())
      .collect();
    assert_eq!(older, ["old", "older"]);
    // Main's title and row, Today's title and row, Yesterday's title: 5.
    assert_eq!(
      child_of(&worktrees, &all, Some(Path::new("/yesterday")), bucket),
      Some(5)
    );
    assert_eq!(child_of(&worktrees, &all, Some(Path::new("/gone")), bucket), None);
  }

  fn dated() -> [Worktree; 6] {
    [
      worktree("old", false, Some(40)),
      worktree("not-diverged", false, None),
      worktree("main", true, None),
      worktree("today", false, Some(1)),
      worktree("older", false, Some(50)),
      worktree("yesterday", false, Some(20)),
    ]
  }

  /// Ages stand for days here: the bucket is the test's, not the clock's.
  fn day(age: u64) -> Bucket {
    match age {
      0..=9 => Bucket::Today,
      10..=29 => Bucket::Yesterday,
      _ => Bucket::Older,
    }
  }

  /// A filter keeps the sections that have a match, in the sidebar's order,
  /// and the scroll index is read from the list it leaves.
  #[test]
  fn a_section_with_no_match_is_not_drawn() {
    let worktrees = dated();
    // "old" and "older", in one section; "today" and "yesterday", in two.
    let old = Query::new("old");
    let titles: Vec<_> = sections(&worktrees, &old, day)
      .iter()
      .map(|section| section.title)
      .collect();
    assert_eq!(titles, ["Older"]);
    assert_eq!(
      children(&worktrees, &old, day),
      [Child::Title("Older"), Child::Worktree(0), Child::Worktree(4)]
    );
    assert_eq!(
      children(&worktrees, &Query::new("day"), day),
      [
        Child::Title("Today"),
        Child::Worktree(3),
        Child::Title("Yesterday"),
        Child::Worktree(5),
      ]
    );
    // "older" is the list's third child once the filter is on, its ninth without.
    assert_eq!(child_of(&worktrees, &old, Some(Path::new("/older")), day), Some(2));
    assert_eq!(
      child_of(&worktrees, &Query::default(), Some(Path::new("/older")), day),
      Some(8)
    );
    // A worktree the filter hides has no child to scroll to.
    assert_eq!(child_of(&worktrees, &old, Some(Path::new("/today")), day), None);
  }

  #[test]
  fn a_filter_that_keeps_nothing_says_so() {
    let worktrees = dated();
    let none = Query::new("nothing-of-the-kind");
    assert!(sections(&worktrees, &none, day).is_empty());
    assert_eq!(children(&worktrees, &none, day), [Child::NoMatch]);
    assert_eq!(child_of(&worktrees, &none, Some(Path::new("/main")), day), None);
    // No filter, no worktree: nothing to say, the list is not there yet.
    assert_eq!(children(&[], &Query::default(), day), []);
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
      children(&worktrees, &Query::default(), bucket),
      [
        Child::Title("Main"),
        Child::Worktree(0),
        Child::Title("Today"),
        Child::Worktree(1),
        Child::Worktree(2),
      ]
    );
    assert_eq!(
      child_of(&worktrees, &Query::default(), Some(Path::new("/later")), bucket),
      Some(4)
    );
  }
}
