//! The 40 px title bar, drawn by the app: the traffic lights sit in it, then
//! the sidebar toggle, then a tab for the selected worktree (Monocode's
//! `title_tab_plate` and `tab_tile.rs`). There is one tab: gwm-gpui has none to
//! switch between, the tile names what the window shows.

use gpui::{Context, MouseButton, MouseDownEvent, Pixels, Window, div, prelude::*, px};
use gwm_client::Worktree;

use crate::{
  assets,
  sidebar::{icon, one_line},
  theme,
  workspace::Workspace,
};

/// Monocode's title bar, sidebar closed: lights, then three 24 px buttons.
/// gwm-gpui has two, the sidebar toggle and "New worktree": the space of the
/// third stays drag area.
const CLOSED_NAV_WIDTH: Pixels = px(150.);

pub(crate) fn title_bar(
  sidebar_open: bool,
  can_create: bool,
  selected: Option<&Worktree>,
  cx: &mut Context<Workspace>,
) -> impl IntoElement {
  div()
    .h(theme::TITLE_BAR_HEIGHT)
    .flex_none()
    .flex()
    .items_center()
    .bg(theme::wash())
    .child(
      div()
        .w(if sidebar_open {
          theme::SIDEBAR_WIDTH
        } else {
          CLOSED_NAV_WIDTH
        })
        .h_full()
        .flex_none()
        .flex()
        .items_center()
        .child(drag_area().w(theme::TRAFFIC_LIGHTS_WIDTH))
        .child(
          div()
            .id("toggle-sidebar")
            .size(px(24.))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.))
            .hover(|button| button.bg(theme::selected()))
            .on_click(cx.listener(|workspace, _, window, cx| workspace.toggle_sidebar(window, cx)))
            .child(icon(assets::SIDEBAR, 14., theme::ink(0.5))),
        )
        // No list yet, no "New worktree": nothing would tell the new one apart.
        .when(can_create, |nav| {
          nav.child(
            div()
              .id("new-worktree")
              .size(px(24.))
              .flex_none()
              .flex()
              .items_center()
              .justify_center()
              .rounded(px(6.))
              .hover(|button| button.bg(theme::selected()))
              .text_size(px(17.))
              .text_color(theme::ink(0.5))
              .on_click(cx.listener(|workspace, _, window, cx| workspace.toggle_form(window, cx)))
              .child("+"),
          )
        })
        .child(drag_area().flex_1()),
    )
    .child(
      div()
        .h_full()
        .flex_1()
        .min_w_0()
        .flex()
        .items_center()
        .pl(px(6.))
        .children(selected.map(tab))
        .child(drag_area().flex_1()),
    )
}

/// Where the window moves from, as a native title bar would: a drag moves it,
/// a double click zooms it.
pub(crate) fn drag_area() -> gpui::Div {
  div()
    .h_full()
    .on_mouse_down(MouseButton::Left, |event: &MouseDownEvent, window: &mut Window, _| {
      if event.click_count == 2 {
        window.titlebar_double_click();
      } else {
        window.start_window_move();
      }
    })
}

fn tab(worktree: &Worktree) -> impl IntoElement {
  div()
    .h(theme::TAB_HEIGHT)
    // A tab alone takes Monocode's full tab width, whatever its name's length.
    .w(theme::TAB_MAX_WIDTH)
    .min_w_0()
    .flex()
    .items_center()
    .gap(px(8.))
    .pl(px(3.5))
    .pr(px(8.))
    .rounded(px(6.))
    .bg(theme::selected())
    .child(tile(px(24.), px(20.), px(5.), 14.))
    .child(one_line(worktree.name.clone()).text_size(px(13.)))
}

/// The accent tile with a white branch mark: the worktree's face, in the tab
/// and in the detail panel.
pub(crate) fn tile(width: Pixels, height: Pixels, radius: Pixels, mark: f32) -> impl IntoElement {
  div()
    .w(width)
    .h(height)
    .flex_none()
    .flex()
    .items_center()
    .justify_center()
    .rounded(radius)
    .bg(theme::accent_tile())
    .border_1()
    .border_color(theme::tile_rim())
    .child(icon(assets::BRANCH, mark, theme::tile_mark()))
}
