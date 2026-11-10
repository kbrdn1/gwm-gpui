use std::path::PathBuf;

use gpui::{App, Application, Focusable, KeyBinding, actions, prelude::*};
use gwm_client::watch::Spawned;
use gwm_gpui::{Workspace, assets::Assets};

actions!(gwm_gpui, [Quit]);

fn main() {
  // The repo to list: the first argument, else wherever the app was launched.
  let repo = std::env::args_os()
    .nth(1)
    .map_or_else(|| PathBuf::from("."), PathBuf::from);

  Application::with_platform(gpui_platform::current_platform(false))
    .with_assets(Assets)
    .run(move |cx: &mut App| {
      // A daemon this app started would outlive it: stop it on the way out.
      let spawned = Spawned::default();
      let on_quit = spawned.clone();
      cx.on_app_quit(move |_| {
        on_quit.stop();
        async {}
      })
      .detach();
      cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
      gwm_gpui::bind_keys(cx);
      cx.on_action(|_: &Quit, cx: &mut App| cx.quit());
      cx.on_window_closed(|cx, _| {
        if cx.windows().is_empty() {
          cx.quit();
        }
      })
      .detach();

      cx.open_window(gwm_gpui::window_options(cx), |window, cx| {
        let workspace = cx.new(|cx| Workspace::new(repo, spawned, cx));
        window.focus(&workspace.focus_handle(cx), cx);
        workspace
      })
      .expect("the main window opens");
      cx.activate(true);
    });
}
