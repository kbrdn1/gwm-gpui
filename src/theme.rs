//! Design tokens, the only place a colour value lives.
//!
//! Surfaces, opacities, radii and sizes follow Monocode Native's dark theme
//! (`crates/monocode-gpui/src/theme.rs` and `shell/layout.rs`, MIT, see NOTICE),
//! in its rounded-card frame. The accent and the status colours stay gwm's
//! (`claude-dark` preset, `src/tui/theme.rs` in gwm-cli), so both UIs read alike.

use gpui::{Background, Hsla, Pixels, linear_color_stop, linear_gradient, px, rgb};

/// gwm's orange: the one token to change for another accent.
pub const ACCENT: u32 = 0xd4825d;
pub const CLEAN: u32 = 0x86e89a;
pub const DIRTY: u32 = 0xffdf61;
/// Something works, but less than it should: the list is not live. gwm calls this yellow "warning".
pub const WARNING: u32 = 0xffdf61;
pub const ERROR: u32 = 0xff7a7a;

/// Monocode's ink: every text, icon, wash and hairline is this colour at some alpha.
const INK: u32 = 0xebebeb;
/// The window's own fill, laid over the blurred desktop.
const SHELL: u32 = 0x171717;
const SHELL_ALPHA: f32 = 0.85;
/// How dark the bottom of the central card gets. Monocode paints no gradient:
/// in its screenshots the darkening is the desktop showing through the glass.
/// This stands in for it on any desktop. Set it to 0 to drop it.
const CARD_SHADE_ALPHA: f32 = 0.22;

pub const UI_FONT: &str = ".SystemUIFont";
pub const MONO_FONT: &str = "Menlo";

pub const TITLE_BAR_HEIGHT: Pixels = px(40.);
/// Room for the traffic lights, which macOS draws itself.
pub const TRAFFIC_LIGHTS_WIDTH: Pixels = px(78.);
pub const SIDEBAR_WIDTH: Pixels = px(260.);
pub const STATUS_BAR_HEIGHT: Pixels = px(30.);
pub const CARD_RADIUS: Pixels = px(10.);
/// The chrome gutter between the card and the window's right edge.
pub const GUTTER: Pixels = px(6.);
pub const TAB_HEIGHT: Pixels = px(27.);
pub const TAB_MAX_WIDTH: Pixels = px(224.);

pub fn ink(alpha: f32) -> Hsla {
  with_alpha(INK, alpha)
}

pub fn shell() -> Hsla {
  with_alpha(SHELL, SHELL_ALPHA)
}

/// The 5% wash on the chrome around the central card: title bar, sidebar,
/// gutter and status bar. The card itself has none, so it reads as recessed.
pub fn wash() -> Hsla {
  ink(0.05)
}

/// Hairlines and the card's border.
pub fn card_border() -> Hsla {
  ink(0.08)
}

/// A hovered row or button.
pub fn hover() -> Hsla {
  ink(0.05)
}

/// The selected row, the selected tab, a hovered icon button.
pub fn selected() -> Hsla {
  ink(0.1)
}

pub fn accent() -> Hsla {
  rgb(ACCENT).into()
}

pub fn color(hex: u32) -> Hsla {
  rgb(hex).into()
}

/// The accent tile behind a worktree's mark: the accent, lit from the top, as
/// Monocode lights its brand tiles.
pub fn accent_tile() -> Background {
  let accent = accent();
  let shade = |delta: f32| Hsla {
    l: (accent.l + delta).clamp(0., 1.),
    ..accent
  };
  linear_gradient(
    180.,
    linear_color_stop(shade(0.05), 0.),
    linear_color_stop(shade(-0.06), 1.),
  )
}

/// The tile's 1 px rim, and the mark on it: white, as on Monocode's brand tiles.
pub fn tile_rim() -> Hsla {
  with_alpha(0xffffff, 0.14)
}

pub fn tile_mark() -> Hsla {
  with_alpha(0xffffff, 1.)
}

pub fn card_shade() -> Background {
  linear_gradient(
    180.,
    linear_color_stop(with_alpha(0x000000, 0.), 0.),
    linear_color_stop(with_alpha(0x000000, CARD_SHADE_ALPHA), 1.),
  )
}

fn with_alpha(hex: u32, alpha: f32) -> Hsla {
  Hsla {
    a: alpha,
    ..rgb(hex).into()
  }
}
