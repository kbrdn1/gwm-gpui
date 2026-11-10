//! Renders the main window off screen to a PNG, to look at the UI without a
//! screen or a screen-recording permission:
//!
//! ```text
//! cargo run --example screenshot --features screenshot -- <repo> <out.png>
//! ```
//!
//! The worktrees are read once with `gwm list`. The window's blur comes from the
//! window server, not from GPUI, so it is not in the image: translucent surfaces
//! come out translucent, over nothing.

use std::{error::Error, path::PathBuf};

use gpui::{AppContext, Bounds, VisualTestAppContext, WindowBounds, point, px, size};
use gwm_client::watch::Source;
use gwm_gpui::Workspace;

fn main() -> Result<(), Box<dyn Error>> {
  let mut args = std::env::args_os().skip(1).map(PathBuf::from);
  let (Some(repo), Some(out)) = (args.next(), args.next()) else {
    return Err("usage: screenshot <repo> <out.png>".into());
  };
  let worktrees = gwm_client::list(&repo)?;

  let mut cx = VisualTestAppContext::new(gpui_platform::current_platform(false));
  let window = {
    let mut app = cx.app.borrow_mut();
    let mut options = gwm_gpui::window_options(&app);
    // Off every display, like `open_offscreen_window`, but with the app's own options.
    options.window_bounds = Some(WindowBounds::Windowed(Bounds {
      origin: point(px(-10000.), px(-10000.)),
      size: size(px(1200.), px(760.)),
    }));
    options.focus = false;
    let source = Source::Once("rendered off screen by examples/screenshot.rs".to_owned());
    app.open_window(options, |_, cx| cx.new(|_| Workspace::showing(worktrees, source)))?
  };
  cx.run_until_parked();
  cx.capture_screenshot(window.into())?.save(&out)?;
  println!("{}", out.display());
  Ok(())
}
