//! A worktree's agent sessions, as the sidebar shows them: what its row says
//! of the top one, and the lines nested under it once it is selected, on a
//! rail, after Monocode's (`shell/rails.rs`, the nested row and the suffix of
//! its session row in `shell.rs`, `rail_from` in `activity.rs`).
//!
//! All of it is gwm's: `freshness` is its word, never derived here from
//! `last_activity`, and `last_activity` its timestamp, worded as an age.

use gpui::{div, prelude::*, px};
use gwm_client::{Agents, Freshness, Session, Worktree};

use crate::{
  age::Ages,
  assets,
  sidebar::{icon, one_line},
  theme,
};

/// How many of a worktree's sessions are listed, the most recent ones: a main
/// checkout gathers them by the dozen (gwm looks 30 days back), and the
/// worktrees below must stay in reach.
const LISTED: usize = 5;

/// What stands for a session's state.
#[derive(Debug, PartialEq)]
pub(crate) enum Mark {
  /// gwm calls it active, on a list that is live.
  Active,
  /// A freshness this build does not know, as gwm wrote it, on a live list.
  Word(String),
  /// The age of its last activity.
  Since(String),
}

/// `live`: the list follows a daemon, which sends a new one when gwm's word
/// for a session changes. On a list that does not, the word is as old as the
/// list and `active` may have expired: only the timestamp still holds.
pub(crate) fn mark(session: &Session, live: bool, ages: Ages) -> Mark {
  match &session.freshness {
    Freshness::Active if live => Mark::Active,
    Freshness::Other(word) if live => Mark::Word(word.clone()),
    _ => Mark::Since(ages.since(session.last_activity)),
  }
}

/// What a worktree's row says of the sessions gwm matched to it.
#[derive(Debug, PartialEq)]
pub(crate) enum Carried {
  /// The top session: its kind, for the row's second line, and whether gwm
  /// calls it active, which takes the age's place on the first.
  Top { kind: String, active: bool },
  /// gwm sent sessions this build could not read.
  Unreadable,
}

pub(crate) fn carried(worktree: &Worktree, live: bool, ages: Ages) -> Option<Carried> {
  Some(match worktree.agents.as_ref()? {
    Ok(agents) => Carried::Top {
      kind: agents.top.kind.clone(),
      active: mark(&agents.top, live, ages) == Mark::Active,
    },
    Err(_) => Carried::Unreadable,
  })
}

/// A session's line under its worktree.
#[derive(Debug, PartialEq)]
pub(crate) struct Line {
  pub title: String,
  /// Its kind, named only where the worktree's sessions are not all of one
  /// kind: otherwise the worktree's row has said it.
  pub kind: Option<String>,
  pub mark: Mark,
}

/// The lines of a worktree's sessions, in gwm's order, the most recent first,
/// and how many older ones are left out.
pub(crate) fn lines(agents: &Agents, live: bool, ages: Ages) -> (Vec<Line>, usize) {
  let mixed = agents.sessions.iter().any(|session| session.kind != agents.top.kind);
  let lines = agents
    .sessions
    .iter()
    .take(LISTED)
    .map(|session| Line {
      title: session.name.clone().unwrap_or_else(|| "unnamed session".to_owned()),
      kind: mixed.then(|| session.kind.clone()),
      mark: mark(session, live, ages),
    })
    .collect();
  (lines, agents.sessions.len().saturating_sub(LISTED))
}

/// Why the card says the sessions could not be read, for a worktree whose
/// `agents` gwm sent in a shape this build does not read.
pub(crate) fn unreadable(worktree: &Worktree) -> Option<String> {
  let why = worktree.agents.as_ref()?.as_ref().err()?;
  Some(format!(
    "Agent sessions: gwm sent them in a shape this build does not read, which its contract allows for this field \
     ({why})"
  ))
}

/// A mark as it stands at the right of a line: gwm's `active` in gwm's own
/// colour for it, the rest in the muted ink of an age.
pub(crate) fn mark_label(mark: Mark) -> gpui::Div {
  let label = div().flex_none().flex().items_center().gap(px(4.)).text_size(px(11.));
  match mark {
    Mark::Active => {
      let active = theme::color(theme::ACTIVE);
      label
        .text_color(active)
        .child(div().size(px(6.)).rounded_full().bg(active))
        .child("active")
    }
    Mark::Word(text) | Mark::Since(text) => label.text_color(theme::ink(0.4)).child(text),
  }
}

/// The agent at the right end of a worktree row's second line, where
/// Monocode's row names its model: the kind in text, then the glyph.
pub(crate) fn agent(carried: &Carried) -> impl IntoElement + use<> {
  let (label, color) = match carried {
    Carried::Top { kind, .. } => (kind.clone(), theme::ink(0.5)),
    Carried::Unreadable => ("unreadable".to_owned(), theme::color(theme::WARNING)),
  };
  div()
    .flex_none()
    .flex()
    .items_center()
    .gap(px(4.))
    .text_size(px(11.))
    .text_color(color)
    .child(label)
    .child(icon(assets::AGENT, 12., color))
}

/// The lines nested under a worktree's row: its sessions, or that they could
/// not be read. They are not interactive: the app can do nothing to a session.
pub(crate) fn nested(worktree: &Worktree, live: bool, ages: Ages) -> Option<impl IntoElement + use<>> {
  // The rail drops from under the row's glyph, as Monocode's from its mark.
  let rail = div().ml(px(10.)).pt(px(4.)).pb(px(6.)).flex().flex_col();
  Some(match worktree.agents.as_ref()? {
    Ok(agents) => {
      let (lines, older) = lines(agents, live, ages);
      let count = lines.len();
      rail
        .children(
          lines
            .into_iter()
            .enumerate()
            .map(|(index, line)| step(index + 1 == count && older == 0, session_line(line))),
        )
        .when(older > 0, |rail| {
          let plural = if older == 1 { "" } else { "s" };
          rail.child(step(
            true,
            note(format!("{older} older session{plural}"), theme::ink(0.35)),
          ))
        })
    }
    Err(_) => rail.child(step(
      true,
      note(
        "could not be read: see the card".to_owned(),
        theme::color(theme::WARNING),
      ),
    )),
  })
}

/// A step off the rail (`rails::step` and `rail_from` in Monocode): the line
/// runs down its left to the next step, and a quarter turn branches to it.
fn step(last: bool, line: impl IntoElement) -> impl IntoElement {
  let rail = theme::rail();
  div()
    .relative()
    .pl(px(20.))
    .py(px(2.))
    .child(
      div()
        .absolute()
        .top_0()
        .left(px(6.))
        .w(px(9.))
        .h_full()
        .child(
          div()
            .absolute()
            .top_0()
            .left_0()
            .w(px(1.))
            .map(|down| if last { down.h(px(9.)) } else { down.h_full() })
            .bg(rail),
        )
        .child(
          div()
            .absolute()
            .top(px(8.))
            .left_0()
            .size(px(9.))
            .border_l_1()
            .border_b_1()
            .rounded_bl(px(8.))
            .border_color(rail),
        ),
    )
    .child(line)
}

fn session_line(line: Line) -> impl IntoElement {
  div()
    .h(px(28.))
    .px(px(7.))
    .flex()
    .items_center()
    .gap(px(6.))
    .child(icon(assets::AGENT, 14., theme::ink(0.45)))
    .child(one_line(line.title).flex_1().text_color(theme::ink(0.55)))
    .children(line.kind.map(|kind| {
      div()
        .flex_none()
        .text_size(px(11.))
        .text_color(theme::ink(0.4))
        .child(kind)
    }))
    .child(mark_label(line.mark))
}

/// A line that is not a session: how many are not listed, or why none is.
fn note(text: String, color: gpui::Hsla) -> impl IntoElement {
  div()
    .h(px(24.))
    .px(px(7.))
    .flex()
    .items_center()
    .text_size(px(11.))
    .text_color(color)
    .child(one_line(text))
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::workspace::tests::worktree;

  /// 2026-11-12 14:26:59 UTC, a minute after the top session's last activity
  /// in the sample below.
  const NOW: i64 = 1_794_491_279;
  const AGES: Ages = Ages {
    received_at: NOW,
    now: NOW,
  };

  /// A session as gwm sends it. `name`: `None` leaves the field out, as gwm does.
  fn session(kind: &str, freshness: &str, last_activity: i64, name: Option<&str>) -> String {
    let name = name.map_or(String::new(), |name| format!(r#","name":"{name}""#));
    format!(
      r#"{{"kind":"{kind}","freshness":"{freshness}","last_activity":{last_activity},"id":"s-{last_activity}"{name}}}"#
    )
  }

  /// A worktree with these sessions, the first one its `top`, as gwm has it.
  fn with_sessions(sessions: &[String]) -> Worktree {
    let json = format!(r#"{{"top":{},"sessions":[{}]}}"#, sessions[0], sessions.join(","));
    let mut worktree = worktree("feat-21", false, Some(3600));
    worktree.agents = Some(Ok(serde_json::from_str(&json).unwrap()));
    worktree
  }

  fn agents(worktree: &Worktree) -> &Agents {
    worktree.agents.as_ref().unwrap().as_ref().unwrap()
  }

  #[test]
  fn active_is_gwms_word_on_a_live_list_and_an_age_otherwise() {
    let active = with_sessions(&[session("claude", "active", NOW - 60, None)]);
    let idle = with_sessions(&[session("claude", "idle", NOW - 25 * 60, None)]);
    let (active, idle) = (&agents(&active).top, &agents(&idle).top);
    assert_eq!(mark(active, true, AGES), Mark::Active);
    assert_eq!(mark(idle, true, AGES), Mark::Since("25m".to_owned()));
    // Not live: the word is as old as the list, the timestamp is not.
    assert_eq!(mark(active, false, AGES), Mark::Since("1m".to_owned()));
    assert_eq!(mark(idle, false, AGES), Mark::Since("25m".to_owned()));
  }

  /// Never recomputed: a session gwm called active stays so however old its
  /// last activity has become, and an idle one never turns active.
  #[test]
  fn the_app_never_derives_freshness_from_the_last_activity() {
    let stale = with_sessions(&[session("claude", "active", NOW - 3600, None)]);
    assert_eq!(mark(&agents(&stale).top, true, AGES), Mark::Active);
    let recent = with_sessions(&[session("claude", "idle", NOW - 5, None)]);
    assert_eq!(mark(&agents(&recent).top, true, AGES), Mark::Since("now".to_owned()));
  }

  #[test]
  fn a_freshness_this_build_does_not_know_is_shown_as_gwm_wrote_it() {
    let waiting = with_sessions(&[session("claude", "waiting", NOW - 60, None)]);
    let top = &agents(&waiting).top;
    assert_eq!(mark(top, true, AGES), Mark::Word("waiting".to_owned()));
    assert_eq!(mark(top, false, AGES), Mark::Since("1m".to_owned()));
  }

  #[test]
  fn a_row_says_nothing_without_a_session() {
    assert_eq!(carried(&worktree("feat-21", false, Some(60)), true, AGES), None);
  }

  #[test]
  fn a_row_names_its_top_session_and_says_when_gwm_calls_it_active() {
    let active = with_sessions(&[
      session("claude", "active", NOW - 60, Some("One")),
      session("codex", "idle", NOW - 3600, Some("Two")),
    ]);
    let top = |active| {
      Some(Carried::Top {
        kind: "claude".to_owned(),
        active,
      })
    };
    assert_eq!(carried(&active, true, AGES), top(true));
    // Not live: the branch's age keeps its place.
    assert_eq!(carried(&active, false, AGES), top(false));
    let idle = with_sessions(&[session("claude", "idle", NOW - 3600, Some("One"))]);
    assert_eq!(carried(&idle, true, AGES), top(false));
  }

  /// Only `active` takes the age's place on the worktree's row. A word this
  /// build does not know stays on the session's own line.
  #[test]
  fn an_unknown_freshness_is_on_the_sessions_line_not_in_the_ages_place() {
    let waiting = with_sessions(&[session("claude", "waiting", NOW - 60, None)]);
    assert_eq!(
      carried(&waiting, true, AGES),
      Some(Carried::Top {
        kind: "claude".to_owned(),
        active: false,
      })
    );
    assert_eq!(
      lines(agents(&waiting), true, AGES).0[0].mark,
      Mark::Word("waiting".to_owned())
    );
  }

  #[test]
  fn sessions_gwm_sent_unreadable_are_said_so_not_passed_for_none() {
    let mut drifted = worktree("feat-21", false, Some(60));
    drifted.agents = Some(Err("missing field `top`".to_owned()));
    assert_eq!(carried(&drifted, true, AGES), Some(Carried::Unreadable));
    let why = unreadable(&drifted).unwrap();
    assert!(why.contains("missing field `top`"), "{why}");
    assert_eq!(unreadable(&worktree("feat-21", false, Some(60))), None);
    let read = with_sessions(&[session("claude", "idle", NOW - 60, None)]);
    assert_eq!(unreadable(&read), None);
  }

  #[test]
  fn the_lines_are_gwms_sessions_in_gwms_order() {
    let worktree = with_sessions(&[
      session("claude", "active", NOW - 60, Some("Agents in the sidebar")),
      session(
        "claude",
        "idle",
        NOW - 7 * 3600 - 6 * 60,
        Some("Refuse a folder in no repo"),
      ),
      session("claude", "idle", NOW - 2 * 86_400, None),
    ]);
    let line = |title: &str, mark| Line {
      title: title.to_owned(),
      kind: None,
      mark,
    };
    assert_eq!(
      lines(agents(&worktree), true, AGES),
      (
        vec![
          line("Agents in the sidebar", Mark::Active),
          line("Refuse a folder in no repo", Mark::Since("7h 6m".to_owned())),
          line("unnamed session", Mark::Since("2d".to_owned())),
        ],
        0
      )
    );
  }

  /// The worktree's row names the top session's kind. A line names its own
  /// only where that would not say it.
  #[test]
  fn a_line_names_its_kind_where_the_sessions_are_not_all_of_one() {
    let mixed = with_sessions(&[
      session("claude", "idle", NOW - 60, Some("One")),
      session("codex", "idle", NOW - 120, Some("Two")),
    ]);
    let kinds: Vec<_> = lines(agents(&mixed), true, AGES)
      .0
      .into_iter()
      .map(|line| line.kind)
      .collect();
    assert_eq!(kinds, [Some("claude".to_owned()), Some("codex".to_owned())]);
  }

  #[test]
  fn the_older_sessions_are_counted_not_listed() {
    let sessions: Vec<_> = (0..7)
      .map(|index| {
        session(
          "claude",
          "idle",
          NOW - 60 * (index + 1),
          Some(&format!("Session {index}")),
        )
      })
      .collect();
    let worktree = with_sessions(&sessions);
    let (lines, older) = lines(agents(&worktree), true, AGES);
    let titles: Vec<_> = lines.iter().map(|line| line.title.as_str()).collect();
    assert_eq!(
      titles,
      ["Session 0", "Session 1", "Session 2", "Session 3", "Session 4"]
    );
    assert_eq!(older, 2);
  }
}
