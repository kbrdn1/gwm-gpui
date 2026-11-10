//! The icons, embedded in the binary: Hugeicons Stroke Rounded (MIT), as
//! Monocode Native ships them. See NOTICE and `assets/icons/LICENSE-HUGEICONS`.

use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};

pub const SIDEBAR: &str = "icons/sidebar.svg";
pub const BRANCH: &str = "icons/git-branch.svg";
pub const PULL_REQUEST: &str = "icons/git-pull-request.svg";
pub const ISSUE: &str = "icons/circle-dot.svg";
pub const FOLDER: &str = "icons/folder-01.svg";

const ICONS: &[(&str, &[u8])] = &[
  (SIDEBAR, include_bytes!("../assets/icons/sidebar.svg")),
  (BRANCH, include_bytes!("../assets/icons/git-branch.svg")),
  (PULL_REQUEST, include_bytes!("../assets/icons/git-pull-request.svg")),
  (ISSUE, include_bytes!("../assets/icons/circle-dot.svg")),
  (FOLDER, include_bytes!("../assets/icons/folder-01.svg")),
];

pub struct Assets;

impl AssetSource for Assets {
  fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
    Ok(
      ICONS
        .iter()
        .find(|(name, _)| *name == path)
        .map(|(_, bytes)| Cow::Borrowed(*bytes)),
    )
  }

  fn list(&self, path: &str) -> Result<Vec<SharedString>> {
    Ok(
      ICONS
        .iter()
        .filter(|(name, _)| name.starts_with(path))
        .map(|(name, _)| (*name).into())
        .collect(),
    )
  }
}
