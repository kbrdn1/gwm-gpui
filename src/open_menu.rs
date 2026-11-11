//! The "Open in…" popover: the editors and terminals found on the Mac, as
//! Monocode lists editors in its project menu (`shell/project_removal.rs`).
//! Drawn deferred, so the card's rounded clip does not cut it.

use std::path::Path;

use gpui::{AnyElement, Context, FontWeight, SharedString, anchored, deferred, div, prelude::*, px};

use crate::{
  apps::{Installed, Kind},
  sidebar::one_line,
  theme,
  workspace::Workspace,
};

const WIDTH: f32 = 220.;

/// `apps`: `None` while they are being looked for. `dir`: the worktree to open.
pub(crate) fn open_menu(apps: Option<&[Installed]>, dir: &Path, cx: &mut Context<Workspace>) -> impl IntoElement {
  let mut menu = div()
    .id("open-in-menu")
    .occlude()
    .w(px(WIDTH))
    .p(px(4.))
    .flex()
    .flex_col()
    .rounded(theme::POPOVER_RADIUS)
    .bg(theme::popover())
    .border_1()
    .border_color(theme::popover_border())
    .shadow(theme::popover_shadow())
    .text_size(px(13.));
  match apps {
    None => menu = menu.child(note("Looking for apps…")),
    Some([]) => menu = menu.child(note("No known editor or terminal found")),
    Some(apps) => {
      for (title, kind) in [("Editors", Kind::Editor), ("Terminals", Kind::Terminal)] {
        let rows: Vec<AnyElement> = apps
          .iter()
          .filter(|app| app.kind == kind)
          .map(|app| row(app, dir, cx))
          .collect();
        if !rows.is_empty() {
          menu = menu.child(title_row(title)).children(rows);
        }
      }
    }
  }
  deferred(anchored().snap_to_window_with_margin(px(8.)).child(menu)).with_priority(1)
}

fn row(app: &Installed, dir: &Path, cx: &mut Context<Workspace>) -> AnyElement {
  let (bundle, dir) = (app.bundle.clone(), dir.to_owned());
  div()
    .id(SharedString::from(format!("open-in-{}", app.name)))
    .h(theme::MENU_ROW_HEIGHT)
    .px(px(8.))
    .flex()
    .items_center()
    .rounded(theme::MENU_ROW_RADIUS)
    .cursor_pointer()
    .hover(|row| row.bg(theme::selected()))
    .on_click(cx.listener(move |workspace, _, _, cx| {
      workspace.open_in(bundle.clone(), dir.clone(), cx);
    }))
    .child(one_line(app.name))
    .into_any_element()
}

fn title_row(title: &'static str) -> impl IntoElement {
  div()
    .h(px(24.))
    .px(px(8.))
    .flex()
    .items_center()
    .text_size(px(11.))
    .font_weight(FontWeight::MEDIUM)
    .text_color(theme::ink(0.45))
    .child(title)
}

fn note(text: &'static str) -> impl IntoElement {
  div()
    .h(theme::MENU_ROW_HEIGHT)
    .px(px(8.))
    .flex()
    .items_center()
    .text_color(theme::ink(0.5))
    .child(text)
}
