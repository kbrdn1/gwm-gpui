//! The "Working" panel at the bottom of the sidebar: the sessions gwm calls
//! active in this repo, across its worktrees, a card each. After Monocode's
//! (`shell/live_agents.rs`): a header with a dot, the word and a count, then
//! the cards.
//!
//! All of it is gwm's, as in `sessions.rs`: a session is listed when gwm
//! calls it `active`, on a list a daemon follows, and never because of its
//! last activity. gwm gives no current tool, no elapsed working time and no
//! model: the lines Monocode's cards have for those do not exist here.
//!
//! A card is a session as gwm lists it under a worktree. One gwm matched to
//! several worktrees (started in one, pinned to another) has a card under
//! each, as each of their rows says `active`: the list does not say which
//! one it works in.

use std::path::PathBuf;

use gpui::{Context, FontWeight, div, prelude::*, px};
use gwm_client::{Freshness, Worktree};

use crate::{
  assets,
  filter::Query,
  sessions,
  sidebar::{icon, one_line},
  theme,
  workspace::{Workspace, order},
};

/// A session gwm calls active, under a worktree it matched it to.
#[derive(Debug, PartialEq)]
pub(crate) struct Card {
  pub title: String,
  /// Its agent, as gwm names it.
  pub kind: String,
  /// The worktree's name, and its path: a click selects it.
  pub worktree: String,
  pub path: PathBuf,
}

/// What the panel shows.
#[derive(Debug, PartialEq)]
pub(crate) struct Working {
  pub cards: Vec<Card>,
  /// How many worktrees have sessions this build could not read: they add
  /// no card, and the count above the cards would pass for the repo's.
  pub unreadable: usize,
}

/// The panel for `worktrees`, in the sidebar's order, each worktree's
/// sessions in gwm's. `None`, and no panel, without a session gwm calls
/// active, or on a list that is not `live`: gwm's word is then as old as the
/// list, and `active` may have expired. The filter does not narrow it.
pub(crate) fn working(worktrees: &[Worktree], live: bool) -> Option<Working> {
  if !live {
    return None;
  }
  let mut working = Working {
    cards: Vec::new(),
    unreadable: 0,
  };
  for index in order(worktrees, &Query::default()) {
    let worktree = &worktrees[index];
    match &worktree.agents {
      Some(Ok(agents)) => working.cards.extend(
        agents
          .sessions
          .iter()
          .filter(|session| session.freshness == Freshness::Active)
          .map(|session| Card {
            title: sessions::title(session),
            kind: session.kind.clone(),
            worktree: worktree.name.clone(),
            path: worktree.path.clone(),
          }),
      ),
      Some(Err(_)) => working.unreadable += 1,
      None => {}
    }
  }
  (!working.cards.is_empty()).then_some(working)
}

pub(crate) fn panel(working: Working, cx: &mut Context<Workspace>) -> impl IntoElement {
  let active = theme::color(theme::ACTIVE);
  let count = working.cards.len();
  // ponytail: past four cards the list scrolls, half of the fifth showing.
  // Monocode folds them behind an "N more" toggle: to add if the scroll
  // proves hard to find.
  let mut cards = div()
    .id("working-cards")
    .max_h(theme::WORKING_MAX_HEIGHT)
    .overflow_y_scroll()
    .flex()
    .flex_col()
    .gap(px(1.))
    .px(px(4.))
    .pb(px(4.));
  for (index, session) in working.cards.into_iter().enumerate() {
    cards = cards.child(card(index, session, cx));
  }
  div().flex_none().px(px(8.)).pb(px(8.)).child(
    div()
      .overflow_hidden()
      .rounded(theme::WORKING_RADIUS)
      .bg(theme::working())
      .child(
        div()
          .flex()
          .items_center()
          .gap(px(8.))
          .px(px(14.))
          .py(px(6.))
          .child(
            div()
              .size(px(6.))
              .flex_none()
              .rounded_full()
              .bg(active)
              .shadow(theme::glow(active)),
          )
          .child(
            one_line("Working")
              .flex_1()
              .text_size(px(12.))
              .text_color(theme::ink(0.5)),
          )
          .child(
            div()
              .flex_none()
              .text_size(px(11.))
              .text_color(theme::ink(0.4))
              .child(count.to_string()),
          ),
      )
      .child(cards)
      .when(working.unreadable > 0, |panel| {
        let plural = if working.unreadable == 1 { "" } else { "s" };
        panel.child(
          one_line(format!(
            "sessions unreadable in {} worktree{plural}",
            working.unreadable
          ))
          .px(px(12.))
          .pb(px(8.))
          .text_size(px(11.))
          .text_color(theme::color(theme::WARNING)),
        )
      }),
  )
}

/// A card: the session's name, then its worktree and, at the right, its
/// agent as a worktree's row names it. A click selects the worktree.
fn card(index: usize, session: Card, cx: &mut Context<Workspace>) -> impl IntoElement {
  let path = session.path;
  div()
    .id(("working-card", index))
    .flex()
    .flex_col()
    .rounded(theme::CONTROL_RADIUS)
    .px(px(8.))
    .py(px(6.))
    .cursor_pointer()
    .hover(|card| card.bg(theme::working_hover()))
    .on_click(cx.listener(move |workspace, _, window, cx| workspace.select(path.clone(), window, cx)))
    .child(
      one_line(session.title)
        .text_size(px(13.))
        .line_height(px(18.))
        .font_weight(FontWeight::SEMIBOLD),
    )
    .child(
      div()
        .mt(px(4.))
        .flex()
        .items_center()
        .gap(px(6.))
        .text_size(px(11.))
        .line_height(px(14.))
        .text_color(theme::ink(0.45))
        .child(icon(assets::BRANCH, 12., theme::ink(0.45)))
        .child(one_line(session.worktree).flex_1())
        .child(sessions::agent_label(session.kind, theme::ink(0.5))),
    )
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::workspace::tests::worktree;

  /// A session as gwm sends it. `name`: `None` leaves the field out, as gwm does.
  fn session(kind: &str, freshness: &str, name: Option<&str>) -> String {
    let name = name.map_or(String::new(), |name| format!(r#","name":"{name}""#));
    format!(r#"{{"kind":"{kind}","freshness":"{freshness}","last_activity":1794491219,"id":"s"{name}}}"#)
  }

  /// A worktree with these sessions, the first one its `top`, as gwm has it.
  fn with_sessions(id: &str, age_seconds: Option<u64>, sessions: &[String]) -> Worktree {
    let json = format!(r#"{{"top":{},"sessions":[{}]}}"#, sessions[0], sessions.join(","));
    let mut worktree = worktree(id, age_seconds.is_none(), age_seconds);
    worktree.agents = Some(Ok(serde_json::from_str(&json).unwrap()));
    worktree
  }

  fn card(title: &str, kind: &str, worktree: &str) -> Card {
    Card {
      title: title.to_owned(),
      kind: kind.to_owned(),
      worktree: worktree.to_owned(),
      path: PathBuf::from(format!("/{worktree}")),
    }
  }

  #[test]
  fn without_an_active_session_there_is_no_panel() {
    assert_eq!(working(&[], true), None);
    let quiet = [
      worktree("main", true, None),
      with_sessions(
        "feat-21",
        Some(60),
        &[
          session("claude", "idle", Some("One")),
          session("codex", "idle", Some("Two")),
        ],
      ),
    ];
    assert_eq!(working(&quiet, true), None);
  }

  #[test]
  fn an_active_session_has_a_card_with_its_name_its_kind_and_its_worktree() {
    let worktrees = [
      worktree("main", true, None),
      with_sessions(
        "feat-21",
        Some(60),
        &[session("codex", "active", Some("Tidy the queue"))],
      ),
    ];
    assert_eq!(
      working(&worktrees, true),
      Some(Working {
        cards: vec![card("Tidy the queue", "codex", "feat-21")],
        unreadable: 0,
      })
    );
  }

  /// Every session gwm calls active, and only those: the worktrees in the
  /// sidebar's order, whatever gwm's, and a worktree's sessions in gwm's.
  #[test]
  fn the_cards_are_the_active_sessions_across_the_worktrees() {
    let worktrees = [
      with_sessions(
        "older",
        Some(500),
        &[
          session("claude", "idle", Some("Done since")),
          session("opencode", "active", None),
        ],
      ),
      with_sessions(
        "main",
        None,
        &[
          session("claude", "active", Some("One")),
          session("claude", "idle", Some("Two")),
          session("codex", "active", Some("Three")),
        ],
      ),
      worktree("quiet", false, Some(20)),
    ];
    let cards = working(&worktrees, true).unwrap().cards;
    assert_eq!(
      cards,
      [
        card("One", "claude", "main"),
        card("Three", "codex", "main"),
        card("unnamed session", "opencode", "older"),
      ]
    );
  }

  /// gwm's word is as old as the list: without a daemon behind it, `active`
  /// may have expired, and the panel would list sessions that ended.
  #[test]
  fn a_list_that_is_not_live_has_no_panel() {
    let worktrees = [with_sessions("main", None, &[session("claude", "active", Some("One"))])];
    assert!(working(&worktrees, true).is_some());
    assert_eq!(working(&worktrees, false), None);
  }

  /// Only `active` is listed: a freshness this build does not know is not
  /// taken for it.
  #[test]
  fn a_freshness_this_build_does_not_know_is_not_active() {
    let worktrees = [with_sessions(
      "main",
      None,
      &[session("claude", "waiting", Some("One"))],
    )];
    assert_eq!(working(&worktrees, true), None);
  }

  /// Sessions this build cannot read add no card. With a panel to say it in,
  /// they are counted apart; alone, they draw none: the worktree's row says
  /// they are unreadable.
  #[test]
  fn unreadable_sessions_add_no_card_and_are_counted_apart() {
    let mut drifted = worktree("drifted", false, Some(60));
    drifted.agents = Some(Err("missing field `top`".to_owned()));
    assert_eq!(working(std::slice::from_ref(&drifted), true), None);
    let worktrees = [
      with_sessions("main", None, &[session("claude", "active", Some("One"))]),
      drifted,
    ];
    assert_eq!(
      working(&worktrees, true),
      Some(Working {
        cards: vec![card("One", "claude", "main")],
        unreadable: 1,
      })
    );
  }

  /// gwm lists a session under every worktree it matched it to: the one it
  /// started in, and one it is pinned to. Each has its card, as each row
  /// says `active`.
  #[test]
  fn a_session_gwm_matched_to_two_worktrees_has_a_card_under_each() {
    let pinned = session("claude", "active", Some("One"));
    let worktrees = [
      with_sessions("main", None, std::slice::from_ref(&pinned)),
      with_sessions("feat-23", Some(60), std::slice::from_ref(&pinned)),
    ];
    assert_eq!(
      working(&worktrees, true).unwrap().cards,
      [card("One", "claude", "main"), card("One", "claude", "feat-23")]
    );
  }
}
