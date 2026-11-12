//! Renders the main window off screen to a PNG, to look at the UI without a
//! screen or a screen-recording permission:
//!
//! ```text
//! cargo run --example screenshot --features screenshot -- <repo> <out.png> [options]
//!
//!   --row <n>         select the sidebar's nth row first (0 is the first)
//!   --no-sidebar      close the sidebar
//!   --keys "<keys>"       then type these keystrokes, through the app's own bindings
//!   --click <x>,<y>       then click there, in window points
//!   --cmd-click <x>,<y>   then click there with ⌘ held
//!   --wait <ms>           then let <ms> pass, for what arrives off the UI thread
//!   --live <ms>           follow the repo live, as the app does, for <ms> before rendering
//! ```
//!
//! `--keys`, `--click`, `--cmd-click` and `--wait` repeat, and apply in the
//! order given: `--click` on a field, then `--keys "4 2"` types into it.
//!
//! The project menu lists the recent repos of the file `GWM_GPUI_RECENT` names,
//! if any: the app's own file is never read. A click on one goes through the
//! app's own `open::repo`: the repo is followed live whatever `--live` says, so
//! `--wait` for its list, and it is recorded in that file. A `--cmd-click` opens
//! it in a window on screen, as the app does, saved as `<out>-2.png`.
//!
//! Without `--live`, the worktrees are read once with `gwm list`. The window's
//! blur comes from the window server, not from GPUI, so it is not in the
//! image: translucent surfaces come out translucent, over nothing.

use std::{
  error::Error,
  path::PathBuf,
  sync::Arc,
  time::{Duration, Instant},
};

use gpui::{AppContext, Bounds, Focusable, Modifiers, VisualTestAppContext, WindowBounds, point, px, size};
use gwm_client::watch::{Source, Spawned};
use gwm_gpui::{
  OpenRepo, Workspace,
  assets::Assets,
  open::{self, Daemons},
  recent::{self, Projects},
};

enum Step {
  Keys(String),
  Click(f32, f32, Modifiers),
  Wait(u64),
}

/// Lets `millis` pass, taking in what arrives meanwhile: a watch thread's
/// events come in real time.
fn wait(cx: &mut VisualTestAppContext, millis: u64) {
  let deadline = Instant::now() + Duration::from_millis(millis);
  while Instant::now() < deadline {
    cx.run_until_parked();
    std::thread::sleep(Duration::from_millis(50));
  }
}

#[derive(Default)]
struct Options {
  row: usize,
  no_sidebar: bool,
  steps: Vec<Step>,
  live: Option<u64>,
}

fn main() -> Result<(), Box<dyn Error>> {
  let usage = "usage: screenshot <repo> <out.png> [--row n] [--no-sidebar] [--keys \"…\"] [--click x,y] \
               [--cmd-click x,y] [--wait ms] [--live ms]";
  let mut args = std::env::args().skip(1);
  let (Some(repo), Some(out)) = (args.next().map(PathBuf::from), args.next().map(PathBuf::from)) else {
    return Err(usage.into());
  };
  let mut options = Options::default();
  while let Some(arg) = args.next() {
    let mut value = || args.next().ok_or(usage);
    match arg.as_str() {
      "--row" => options.row = value()?.parse()?,
      "--no-sidebar" => options.no_sidebar = true,
      "--keys" => options.steps.push(Step::Keys(value()?)),
      "--click" | "--cmd-click" => {
        let value = value()?;
        let (x, y) = value.split_once(',').ok_or(usage)?;
        let modifiers = Modifiers {
          platform: arg == "--cmd-click",
          ..Modifiers::default()
        };
        options.steps.push(Step::Click(x.parse()?, y.parse()?, modifiers));
      }
      "--wait" => options.steps.push(Step::Wait(value()?.parse()?)),
      "--live" => options.live = Some(value()?.parse()?),
      _ => return Err(usage.into()),
    }
  }

  if options.live.is_some() && options.row > 0 {
    return Err("--row has no rows to pick before a live list arrives: use --keys \"down\" instead".into());
  }
  let worktrees = match options.live {
    Some(_) => None,
    None => Some(gwm_client::list(&repo)?),
  };
  let spawned = Spawned::default();
  // Every daemon started here: the first window's, and those of the repos a click opens.
  let daemons = Daemons::default();
  daemons.borrow_mut().push(spawned.clone());
  let mut cx = VisualTestAppContext::with_asset_source(gpui_platform::current_platform(false), Arc::new(Assets));
  let window = {
    let mut app = cx.app.borrow_mut();
    gwm_gpui::bind_keys(&mut app);
    let mut window_options = gwm_gpui::window_options(&app);
    // Off every display, like `open_offscreen_window`, but with the app's own options.
    window_options.window_bounds = Some(WindowBounds::Windowed(Bounds {
      origin: point(px(-10000.), px(-10000.)),
      size: size(px(1200.), px(760.)),
    }));
    let spawned = spawned.clone();
    let key = gwm_client::Repo::locate(&repo).ok().map(|located| located.common_dir);
    let window = app.open_window(window_options, |window, cx| {
      let workspace = cx.new(|cx| {
        let mut workspace = match worktrees {
          Some(worktrees) => {
            let source = Source::Once("rendered off screen by examples/screenshot.rs".to_owned());
            Workspace::showing(repo, worktrees, source, cx)
          }
          None => Workspace::new(repo, spawned, cx),
        };
        workspace.show(options.row, !options.no_sidebar);
        workspace
      });
      // As the app does: keys reach the workspace only once it has focus.
      window.focus(&workspace.focus_handle(cx), cx);
      workspace
    })?;
    // As the app has it once the repo opened: its window, and the recent repos.
    let recent = std::env::var_os("GWM_GPUI_RECENT").map_or_else(Vec::new, |file| {
      // A click on one of them, as in the app. Only with that file: `open::repo`
      // records the repo in it, and would otherwise write the app's own.
      let daemons = daemons.clone();
      app.on_action(move |OpenRepo(repo, target): &OpenRepo, cx| open::repo(repo.clone(), *target, &daemons, cx));
      recent::load(file.as_ref())
        .into_iter()
        .filter_map(recent::shown)
        .collect()
    });
    app.set_global(Projects {
      recent,
      windows: key.map(|key| (key, window.into())).into_iter().collect(),
    });
    window
  };

  if let Some(millis) = options.live {
    wait(&mut cx, millis);
  }
  cx.run_until_parked();
  for step in &options.steps {
    match step {
      Step::Keys(keys) => cx.simulate_keystrokes(window.into(), keys),
      Step::Click(x, y, modifiers) => cx.simulate_click(window.into(), point(px(*x), px(*y)), *modifiers),
      Step::Wait(millis) => wait(&mut cx, *millis),
    }
    cx.run_until_parked();
  }
  // The window, then those a ⌘-click opened.
  let mut windows = vec![window.into()];
  windows.extend(
    cx.app
      .borrow()
      .windows()
      .into_iter()
      .filter(|other| *other != window.into()),
  );
  let images: Vec<_> = windows
    .into_iter()
    .map(|window| cx.capture_screenshot(window))
    .collect();
  daemons.borrow().iter().for_each(Spawned::stop);
  for (index, image) in images.into_iter().enumerate() {
    let path = match index {
      0 => out.clone(),
      _ => {
        let stem = out.file_stem().unwrap_or_default().to_string_lossy();
        out.with_file_name(format!("{stem}-{}.png", index + 1))
      }
    };
    image?.save(&path)?;
    println!("{}", path.display());
  }
  Ok(())
}
