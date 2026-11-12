//! The sidebar's filter: the magnifier in its header, at the right of the
//! project picker (Monocode's "sidebar-search" `chrome_button`, `shell.rs` and
//! `theme.rs`), the one-line field it opens, and what a worktree's row must
//! say for the text typed to keep it in the list.

use gpui::{Context, Entity, div, prelude::*, px};
use gwm_client::Worktree;

use crate::{
  assets,
  sidebar::{self, icon},
  text_input::TextInput,
  theme,
  workspace::Workspace,
};

/// What the field holds, as it is matched: trimmed, in lower case.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Query(String);

impl Query {
  pub fn new(text: &str) -> Self {
    Self(text.trim().to_lowercase())
  }

  pub fn is_empty(&self) -> bool {
    self.0.is_empty()
  }

  /// Whether `worktree` stays in the list: the text is found, whatever the
  /// case, in what its row says. Its name, its branch, its issue and its PR
  /// as the row writes them (`#21`, `PR 22`), or the kind of one of its
  /// sessions. An empty query keeps every worktree.
  ///
  /// A session's name is not looked at: it is the user's own prompt, and the
  /// row does not show it.
  pub fn matches(&self, worktree: &Worktree) -> bool {
    let says = |text: &str| text.to_lowercase().contains(&self.0);
    says(&worktree.name)
      || says(sidebar::branch(worktree))
      || worktree.issue.is_some_and(|issue| says(&sidebar::issue(issue)))
      || worktree.pr.is_some_and(|pr| says(&sidebar::pr(pr)))
      || kinds(worktree).any(says)
  }
}

/// The kinds of the sessions gwm matched to `worktree`, those it could read.
fn kinds(worktree: &Worktree) -> impl Iterator<Item = &str> {
  worktree
    .agents
    .iter()
    .flatten()
    .flat_map(|agents| std::iter::once(&agents.top).chain(&agents.sessions))
    .map(|session| session.kind.as_str())
}

/// The header's magnifier: it opens the field, and closes it when it is open.
pub(crate) fn button(open: bool, cx: &mut Context<Workspace>) -> impl IntoElement {
  div()
    .id("filter-worktrees")
    .size(theme::CHROME_BUTTON)
    .flex_none()
    .flex()
    .items_center()
    .justify_center()
    .rounded(theme::CONTROL_RADIUS)
    .cursor_pointer()
    .when(open, |button| button.bg(theme::selected()))
    .hover(|button| button.bg(theme::selected()))
    .on_click(cx.listener(|workspace, _, window, cx| workspace.toggle_filter(window, cx)))
    .child(icon(assets::SEARCH, 14., theme::ink(0.5)))
}

/// The field, over the "New" row. Its key context is what gives `up` and
/// `down` to the list under it, in place of the caret.
pub(crate) fn field(input: &Entity<TextInput>) -> impl IntoElement {
  div().key_context("Filter").mb(px(4.)).child(input.clone())
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::workspace::tests::worktree;

  /// A worktree as its row says it: a name, a branch, an issue and a PR, and
  /// sessions of these kinds, the first one its `top`.
  fn listed(kinds: &[&str]) -> Worktree {
    let mut worktree = worktree("fix-57-rate-limit", false, Some(60));
    worktree.branch = Some("fix/#57-Rate-Limit-headers".to_owned());
    worktree.issue = Some(57);
    worktree.pr = Some(706);
    if let Some(top) = kinds.first() {
      let session = |kind: &&str| {
        format!(r#"{{"kind":"{kind}","freshness":"idle","last_activity":1794491219,"name":"Tidy the queue"}}"#)
      };
      let sessions: Vec<_> = kinds.iter().map(session).collect();
      let json = format!(r#"{{"top":{},"sessions":[{}]}}"#, session(top), sessions.join(","));
      worktree.agents = Some(Ok(serde_json::from_str(&json).unwrap()));
    }
    worktree
  }

  fn keeps(text: &str, worktree: &Worktree) -> bool {
    Query::new(text).matches(worktree)
  }

  #[test]
  fn a_query_is_found_in_the_name_or_the_branch() {
    let worktree = listed(&[]);
    assert!(keeps("rate-limit", &worktree));
    // Only the branch says "headers".
    assert!(keeps("headers", &worktree));
    assert!(keeps("fix/#57", &worktree));
    assert!(!keeps("payment", &worktree));
  }

  #[test]
  fn a_query_is_found_in_the_issue_and_the_pr_as_the_row_writes_them() {
    let mut worktree = listed(&[]);
    worktree.name = "spike".to_owned();
    worktree.branch = Some("spike".to_owned());
    assert!(keeps("#57", &worktree));
    assert!(keeps("57", &worktree));
    assert!(keeps("pr 706", &worktree));
    assert!(keeps("706", &worktree));
    // The PR is not written with a `#`, nor the issue with "PR".
    assert!(!keeps("#706", &worktree));
    assert!(!keeps("pr 57", &worktree));
    worktree.issue = None;
    worktree.pr = None;
    assert!(!keeps("57", &worktree));
    assert!(!keeps("pr", &worktree));
  }

  #[test]
  fn a_query_is_found_in_the_kind_of_any_session_not_in_its_name() {
    let worktree = listed(&["claude", "codex"]);
    assert!(keeps("claude", &worktree));
    // Not the top session's kind: an older session's.
    assert!(keeps("codex", &worktree));
    assert!(!keeps("opencode", &worktree));
    // A session's name is the user's own prompt: not looked at.
    assert!(!keeps("tidy", &worktree));
    assert!(!keeps("claude", &listed(&[])));
    let mut unreadable = listed(&[]);
    unreadable.agents = Some(Err("missing field `top`".to_owned()));
    assert!(!keeps("claude", &unreadable));
  }

  #[test]
  fn a_worktree_on_no_branch_is_found_as_its_row_says_it() {
    let mut worktree = listed(&[]);
    worktree.branch = None;
    assert!(keeps("detached", &worktree));
    assert!(!keeps("headers", &worktree));
  }

  #[test]
  fn the_case_and_the_spaces_around_do_not_count() {
    let worktree = listed(&["claude"]);
    assert!(keeps("RATE-LIMIT", &worktree));
    assert!(keeps("  Headers ", &worktree));
    assert!(keeps("Claude", &worktree));
    assert!(keeps("PR 706", &worktree));
    assert_eq!(Query::new(" Fix "), Query::new("fix"));
  }

  #[test]
  fn an_empty_query_keeps_every_worktree() {
    for text in ["", "   "] {
      let query = Query::new(text);
      assert!(query.is_empty());
      assert!(query.matches(&listed(&[])));
      assert!(query.matches(&worktree("dev", false, None)));
    }
    assert!(!Query::new("x").is_empty());
  }
}
