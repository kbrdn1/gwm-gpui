//! `gwm status --json` (`docs/schema/status.schema.json` in gwm-cli): a
//! worktree's issue and PR, with their live GitHub state when gwm's `gh`
//! resolved it. Slow (gwm asks GitHub) and able to fail: run off the UI thread.

use std::{
  path::Path,
  process::{Command, Stdio},
};

use serde::Deserialize;

/// The links of one worktree. `issue` and `pr` are required by the contract,
/// `null` when there is no link: their absence is an error, not "no link".
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Links {
  #[serde(deserialize_with = "Option::deserialize")]
  pub issue: Option<IssueLink>,
  #[serde(deserialize_with = "Option::deserialize")]
  pub pr: Option<PrLink>,
}

/// The live fields are there only when gwm resolved them through `gh`.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct IssueLink {
  pub number: u64,
  pub state: Option<IssueState>,
  pub title: Option<String>,
  pub url: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct PrLink {
  pub number: u64,
  pub state: Option<PrState>,
  pub title: Option<String>,
  pub checks_passed: Option<u64>,
  pub checks_total: Option<u64>,
  pub url: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum IssueState {
  Open,
  Closed,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PrState {
  Open,
  Draft,
  Closed,
  Merged,
}

/// Runs `gwm status --json` in `worktree`. By path, not `--worktree <name>`:
/// gwm's patterns skip the main worktree and fall back to a substring match.
/// The error is a message for the user, gwm's stderr as is when gwm fails.
pub fn status(worktree: &Path) -> Result<Links, String> {
  let output = Command::new("gwm")
    .args(["status", "--json"])
    .current_dir(worktree)
    .stdin(Stdio::null())
    .output()
    .map_err(|error| format!("could not run gwm: {error}"))?;
  if !output.status.success() {
    return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
  }
  parse(&output.stdout)
}

fn parse(json: &[u8]) -> Result<Links, String> {
  serde_json::from_slice(json).map_err(|error| format!("unexpected gwm status output: {error}"))
}

#[cfg(test)]
mod tests {
  use super::*;

  /// gwm 1.11.0 on gwm-cli's main worktree.
  const MAIN: &str = r#"{"branch":"dev","issue":null,"pr":{"checks_passed":28,"checks_total":28,"number":706,"source":"detected","state":"merged","title":"🍱 docs(promo): the banner in the kbrdn.dev design on main (#704)","url":"https://github.com/kbrdn1/gwm-cli/pull/706"},"repo":"kbrdn1/gwm-cli"}"#;

  /// gwm 1.11.0 on a linked worktree of gwm-cli.
  const LINKED: &str = r#"{"branch":"docs/#704-promo-new-design","issue":{"labels":["docs"],"number":704,"source":"branch-name","state":"closed","title":"[Task]: redraw the promo banner in the new kbrdn.dev design","url":"https://github.com/kbrdn1/gwm-cli/issues/704"},"pr":{"checks_passed":15,"checks_total":15,"number":705,"source":"detected","state":"merged","title":"🎨 docs(promo): redraw the banner in the kbrdn.dev design","url":"https://github.com/kbrdn1/gwm-cli/pull/705"},"repo":"kbrdn1/gwm-cli"}"#;

  /// gwm 1.11.0 on the main branch of a repo without a remote.
  const NO_LINK: &str = r#"{"branch":"main","issue":null,"pr":null}"#;

  /// gwm 1.11.0 on `feat/#42-thing` of a repo without a remote: the link,
  /// no live field.
  const NOT_LIVE: &str = r#"{"branch":"feat/#42-thing","issue":{"number":42,"source":"branch-name"},"pr":null}"#;

  #[test]
  fn parses_a_merged_pr_with_its_checks_and_no_issue() {
    let links = parse(MAIN.as_bytes()).unwrap();
    assert_eq!(links.issue, None);
    assert_eq!(
      links.pr,
      Some(PrLink {
        number: 706,
        state: Some(PrState::Merged),
        title: Some("🍱 docs(promo): the banner in the kbrdn.dev design on main (#704)".to_owned()),
        checks_passed: Some(28),
        checks_total: Some(28),
        url: Some("https://github.com/kbrdn1/gwm-cli/pull/706".to_owned()),
      })
    );
  }

  #[test]
  fn parses_an_issue_and_a_pr() {
    let links = parse(LINKED.as_bytes()).unwrap();
    assert_eq!(
      links.issue,
      Some(IssueLink {
        number: 704,
        state: Some(IssueState::Closed),
        title: Some("[Task]: redraw the promo banner in the new kbrdn.dev design".to_owned()),
        url: Some("https://github.com/kbrdn1/gwm-cli/issues/704".to_owned()),
      })
    );
    let pr = links.pr.unwrap();
    assert_eq!(
      (pr.number, pr.state, pr.checks_passed, pr.checks_total),
      (705, Some(PrState::Merged), Some(15), Some(15))
    );
  }

  #[test]
  fn no_link_is_none_and_a_link_gh_did_not_resolve_has_no_live_field() {
    assert_eq!(parse(NO_LINK.as_bytes()), Ok(Links { issue: None, pr: None }));
    assert_eq!(
      parse(NOT_LIVE.as_bytes()),
      Ok(Links {
        issue: Some(IssueLink {
          number: 42,
          state: None,
          title: None,
          url: None,
        }),
        pr: None,
      })
    );
  }

  /// `pr` is required, `null` when there is none: without it, the output is
  /// unreadable, not a worktree without a PR.
  #[test]
  fn a_missing_link_is_an_error_not_none() {
    for key in [r#","pr":null"#, r#""issue":null,"#] {
      let json = NO_LINK.replace(key, "");
      assert!(parse(json.as_bytes()).unwrap_err().contains("missing field"), "{json}");
    }
  }

  #[test]
  fn ignores_fields_added_after_this_build() {
    let json = LINKED.replace(r#""number":705,"#, r#""number":705,"from_the_future":{"a":1},"#);
    assert_eq!(parse(json.as_bytes()), parse(LINKED.as_bytes()));
  }

  #[test]
  fn malformed_output_is_an_error() {
    assert!(
      parse(b"not json")
        .unwrap_err()
        .starts_with("unexpected gwm status output")
    );
  }
}
