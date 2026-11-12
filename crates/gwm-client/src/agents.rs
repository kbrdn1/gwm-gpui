//! The agent sessions gwm matched to a worktree: the `agents` field of the
//! worktree list (`$defs/agents` in gwm-cli's
//! `docs/schema/worktree-list.schema.json`).
//!
//! The field is in the contract's experimental tier: it "may change without a
//! version bump". The rest of a row is stable, so a shape this build does not
//! read is kept apart, as an `Err`, and never fails the list.

use serde::{Deserialize, Deserializer};
use serde_json::Value;

/// The sessions gwm matched to a worktree.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Agents {
  /// "What compact surfaces display", in the schema's words.
  pub top: Session,
  /// Every matched session, most recent first.
  pub sessions: Vec<Session>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Session {
  /// The agent, as gwm names it (`claude`, `codex`, `opencode`, `vibe`). Kept
  /// as written: one this build does not know is not an error.
  pub kind: String,
  pub freshness: Freshness,
  /// The last activity gwm saw in the session's artefacts, in seconds since
  /// the epoch.
  pub last_activity: u64,
  /// The agent's own id for the session: what `gwm agents attach` takes.
  pub id: String,
  /// Its title or first prompt. Left out when its artefacts carry none.
  pub name: Option<String>,
}

/// gwm's word on a session when it listed it: `active` when the last activity
/// in its artefacts was under 300 s old. It is gwm's to give: nothing here
/// derives it from `last_activity`.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(from = "String")]
pub enum Freshness {
  Active,
  Idle,
  /// A word this build does not know, as gwm wrote it.
  Other(String),
}

impl From<String> for Freshness {
  fn from(word: String) -> Self {
    match word.as_str() {
      "active" => Self::Active,
      "idle" => Self::Idle,
      _ => Self::Other(word),
    }
  }
}

/// Reads a row's `agents` without letting it fail the row: `Err` holds why it
/// could not be read, in serde's words. gwm leaves the field out, never null,
/// when it matched no session: serde's `default` gives `None` then, and this
/// is not called.
pub(crate) fn lenient<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Result<Agents, String>>, D::Error> {
  // Whatever is there is taken whole first: read straight into `Agents`, a
  // shape it does not fit would fail the row's own deserializer.
  let field = Value::deserialize(deserializer)?;
  Ok(Some(serde_json::from_value(field).map_err(|error| error.to_string())))
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{daemon, parse};

  /// A real `gwm list --format=json` entry from gwm 1.11.0, whole, structure
  /// as gwm wrote it. Only values were replaced, by neutral ones: the path,
  /// and each session's `id` and `name`, which is the user's own prompt.
  const REAL: &str = r#"[
  {
    "name": "gwm-gpui",
    "id": "gwm-gpui",
    "path": "/repos/gwm-gpui/",
    "branch": "dev",
    "head": "02ffce8f4c62de86cec81f1475618ef83fd32b77",
    "is_main": true,
    "is_locked": false,
    "is_prunable": false,
    "status": {
      "is_dirty": false,
      "has_upstream": true,
      "ahead": 0,
      "behind": 0,
      "unknown": false
    },
    "age_seconds": null,
    "issue": null,
    "pr": null,
    "agents": {
      "top": {
        "kind": "claude",
        "freshness": "active",
        "last_activity": 1794491219,
        "id": "00000000-0000-4000-8000-000000000001",
        "name": "Agents in the sidebar"
      },
      "sessions": [
        {
          "kind": "claude",
          "freshness": "active",
          "last_activity": 1794491219,
          "id": "00000000-0000-4000-8000-000000000001",
          "name": "Agents in the sidebar"
        },
        {
          "kind": "claude",
          "freshness": "idle",
          "last_activity": 1794444366,
          "id": "00000000-0000-4000-8000-000000000002",
          "name": "Refuse a folder in no repo"
        },
        {
          "kind": "claude",
          "freshness": "idle",
          "last_activity": 1794444304,
          "id": "00000000-0000-4000-8000-000000000003",
          "name": "The window follows its repo"
        },
        {
          "kind": "claude",
          "freshness": "idle",
          "last_activity": 1794347316,
          "id": "00000000-0000-4000-8000-000000000004",
          "name": "Open a repo from the picker"
        }
      ]
    }
  }
]"#;

  /// A real `worktrees.changed` from `gwm daemon` 1.11.0 on the same repo,
  /// whole, with the same values replaced. One session is on the four
  /// worktrees: it is pinned to three of them (`gwm agents attach`).
  const REAL_NOTIFICATION: &str = r#"{"jsonrpc":"2.0","method":"worktrees.changed","params":{"schema_version":1,"worktrees":[{"age_seconds":null,"agents":{"sessions":[{"freshness":"idle","id":"00000000-0000-4000-8000-000000000004","kind":"claude","last_activity":1794491329,"name":"Open a repo from the picker"},{"freshness":"idle","id":"00000000-0000-4000-8000-000000000002","kind":"claude","last_activity":1794444366,"name":"Refuse a folder in no repo"},{"freshness":"idle","id":"00000000-0000-4000-8000-000000000001","kind":"claude","last_activity":1794444304,"name":"Agents in the sidebar"},{"freshness":"idle","id":"00000000-0000-4000-8000-000000000003","kind":"claude","last_activity":1794347316,"name":"The window follows its repo"}],"top":{"freshness":"idle","id":"00000000-0000-4000-8000-000000000004","kind":"claude","last_activity":1794491329,"name":"Open a repo from the picker"}},"branch":"dev","head":"02ffce8f4c62de86cec81f1475618ef83fd32b77","id":"gwm-gpui","is_locked":false,"is_main":true,"is_prunable":false,"issue":null,"name":"gwm-gpui","path":"/repos/gwm-gpui/","pr":null,"status":{"ahead":0,"behind":0,"has_upstream":true,"is_dirty":false,"unknown":false}},{"age_seconds":47161,"agents":{"sessions":[{"freshness":"idle","id":"00000000-0000-4000-8000-000000000004","kind":"claude","last_activity":1794491329,"name":"Open a repo from the picker"}],"top":{"freshness":"idle","id":"00000000-0000-4000-8000-000000000004","kind":"claude","last_activity":1794491329,"name":"Open a repo from the picker"}},"branch":"feat/#17-window-follows-repo","head":"5bf78e54304e0463afd0e72a84570ebd7c23df32","id":"feat-17-window-follows-repo","is_locked":false,"is_main":false,"is_prunable":false,"issue":17,"name":"feat-17-window-follows-repo","path":"/repos/gwm-gpui-worktrees/feat-17-window-follows-repo","pr":null,"status":{"ahead":0,"behind":0,"has_upstream":true,"is_dirty":false,"unknown":false}},{"age_seconds":87,"agents":{"sessions":[{"freshness":"idle","id":"00000000-0000-4000-8000-000000000004","kind":"claude","last_activity":1794491329,"name":"Open a repo from the picker"}],"top":{"freshness":"idle","id":"00000000-0000-4000-8000-000000000004","kind":"claude","last_activity":1794491329,"name":"Open a repo from the picker"}},"branch":"feat/#21-agents-in-sidebar","head":"02ffce8f4c62de86cec81f1475618ef83fd32b77","id":"feat-21-agents-in-sidebar","is_locked":false,"is_main":false,"is_prunable":false,"issue":21,"name":"feat-21-agents-in-sidebar","path":"/repos/gwm-gpui-worktrees/feat-21-agents-in-sidebar","pr":null,"status":{"ahead":0,"behind":0,"has_upstream":false,"is_dirty":false,"unknown":false}},{"age_seconds":43837,"agents":{"sessions":[{"freshness":"idle","id":"00000000-0000-4000-8000-000000000004","kind":"claude","last_activity":1794491329,"name":"Open a repo from the picker"}],"top":{"freshness":"idle","id":"00000000-0000-4000-8000-000000000004","kind":"claude","last_activity":1794491329,"name":"Open a repo from the picker"}},"branch":"fix/#19-refuse-non-repo-folder","head":"c48316909aca7d70285789c48266e2f70972c814","id":"fix-19-refuse-non-repo-folder","is_locked":false,"is_main":false,"is_prunable":false,"issue":19,"name":"fix-19-refuse-non-repo-folder","path":"/repos/gwm-gpui-worktrees/fix-19-refuse-non-repo-folder","pr":null,"status":{"ahead":0,"behind":0,"has_upstream":true,"is_dirty":false,"unknown":false}}]}}"#;

  fn session(number: u8, freshness: Freshness, last_activity: u64, name: &str) -> Session {
    Session {
      kind: "claude".to_owned(),
      freshness,
      last_activity,
      id: format!("00000000-0000-4000-8000-00000000000{number}"),
      name: Some(name.to_owned()),
    }
  }

  #[test]
  fn reads_the_sessions_of_a_real_gwm_list() {
    let worktrees = parse(REAL.as_bytes()).unwrap();
    let top = session(1, Freshness::Active, 1_794_491_219, "Agents in the sidebar");
    assert_eq!(
      worktrees[0].agents,
      Some(Ok(Agents {
        top: top.clone(),
        sessions: vec![
          top,
          session(2, Freshness::Idle, 1_794_444_366, "Refuse a folder in no repo"),
          session(3, Freshness::Idle, 1_794_444_304, "The window follows its repo"),
          session(4, Freshness::Idle, 1_794_347_316, "Open a repo from the picker"),
        ],
      }))
    );
  }

  /// gwm leaves `name` out when the session's artefacts carry none.
  #[test]
  fn a_session_may_have_no_name() {
    let json = REAL.replacen(
      r#""id": "00000000-0000-4000-8000-000000000001",
        "name": "Agents in the sidebar""#,
      r#""id": "00000000-0000-4000-8000-000000000001""#,
      1,
    );
    let agents = parse(json.as_bytes()).unwrap().remove(0).agents.unwrap().unwrap();
    assert_eq!(agents.top.name, None);
    assert_eq!(agents.sessions[0].name.as_deref(), Some("Agents in the sidebar"));
  }

  /// The experimental tier may change under this build: what it cannot read
  /// is said, and the stable fields of the row are still read.
  #[test]
  fn an_unreadable_agents_field_keeps_the_list_and_says_why() {
    let cases = [
      (REAL.replace(r#""top": {"#, r#""first": {"#), "top"),
      (
        REAL.replace(r#""last_activity": 1794444366"#, r#""last_activity": "2026-11-12""#),
        "expected u64",
      ),
    ];
    for (json, said) in cases {
      let worktrees = parse(json.as_bytes()).unwrap();
      assert_eq!(worktrees[0].name, "gwm-gpui");
      assert_eq!(worktrees[0].branch.as_deref(), Some("dev"));
      let why = worktrees[0].agents.clone().unwrap().unwrap_err();
      assert!(why.contains(said), "{why}");
    }
  }

  #[test]
  fn an_unknown_kind_or_freshness_is_kept_as_gwm_wrote_it() {
    let json = REAL
      .replace(r#""kind": "claude""#, r#""kind": "aider""#)
      .replace(r#""freshness": "idle""#, r#""freshness": "waiting""#);
    let agents = parse(json.as_bytes()).unwrap().remove(0).agents.unwrap().unwrap();
    assert_eq!(agents.top.kind, "aider");
    assert_eq!(agents.top.freshness, Freshness::Active);
    assert_eq!(agents.sessions[1].freshness, Freshness::Other("waiting".to_owned()));
  }

  /// The daemon sends the list's own rows: its notifications carry the
  /// sessions too.
  #[test]
  fn a_real_notification_carries_the_sessions() {
    let worktrees = daemon::parse_notification(REAL_NOTIFICATION).unwrap();
    let sessions = |index: usize| worktrees[index].agents.clone().unwrap().unwrap().sessions;
    assert_eq!(sessions(0).len(), 4);
    assert_eq!(
      sessions(1),
      [session(
        4,
        Freshness::Idle,
        1_794_491_329,
        "Open a repo from the picker"
      )]
    );
  }

  /// On the daemon's stream, output that does not parse is contract drift and
  /// ends the watch: an unreadable `agents` must not be that.
  #[test]
  fn an_unreadable_agents_field_does_not_end_the_stream() {
    let line = REAL_NOTIFICATION.replacen(r#""top":{"#, r#""first":{"#, 1);
    let worktrees = daemon::parse_notification(&line).unwrap();
    assert!(matches!(&worktrees[0].agents, Some(Err(why)) if why.contains("top")));
    assert!(matches!(worktrees[1].agents, Some(Ok(_))));
  }
}
