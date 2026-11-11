//! Design tokens, the only place a colour value lives.
//!
//! Surfaces, opacities, radii and sizes follow Monocode Native's dark theme
//! (`crates/monocode-gpui/src/theme.rs` and `shell/layout.rs`, MIT, see NOTICE),
//! in its rounded-card frame. The accent and the status colours stay gwm's
//! (`claude-dark` preset, `src/tui/theme.rs` in gwm-cli), so both UIs read alike.

use gpui::{Background, BoxShadow, Hsla, Pixels, linear_color_stop, linear_gradient, point, px, rgb};

/// gwm's orange: the one token to change for another accent.
pub const ACCENT: u32 = 0xd4825d;
pub const CLEAN: u32 = 0x86e89a;
pub const DIRTY: u32 = 0xffdf61;
/// Something works, but less than it should: the list is not live. gwm calls this yellow "warning".
pub const WARNING: u32 = 0xffdf61;
pub const ERROR: u32 = 0xff7a7a;
/// Settled without failing: a merged PR, a closed issue. gwm's "locked"
/// purple, which `pr_badge_color` and `issue_badge_color` give them.
pub const SETTLED: u32 = 0xc79bff;

/// Monocode's ink: every text, icon, wash and hairline is this colour at some alpha.
const INK: u32 = 0xebebeb;
/// The window's own fill, laid over the blurred desktop.
const SHELL: u32 = 0x171717;
/// Monocode's default is 0.85 (its sidebar opacity setting, `settings/
/// preferences.rs`), but the captures it publishes are far more see-through,
/// and they are the reference. Measured on them, wash included: about 0.48 on
/// minicode's title bar over a plain sky, 0.35 to 0.4 on fable52-1's status bar.
const SHELL_ALPHA: f32 = 0.45;
/// How dark the bottom of the central card gets. In Monocode's captures, the
/// card's darkening is its project background image fading out
/// (`project_background.rs`, `background_fade` in monocode-core). gwm-gpui has
/// no background image: a plain gradient stands in for that fade.
const CARD_SHADE_ALPHA: f32 = 0.22;

/// Monocode frosts its popovers with a backdrop blur its patched GPUI paints
/// (`popover.rs`). Without it, its opaque fallback: `floating_fill`, its chat
/// surface off glass.
const POPOVER: u32 = 0x141414;

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
/// Monocode's popover and its menu rows (`popover.rs`).
pub const POPOVER_RADIUS: Pixels = px(12.);
pub const MENU_ROW_HEIGHT: Pixels = px(28.);
pub const MENU_ROW_RADIUS: Pixels = px(8.);

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

pub fn popover() -> Hsla {
  color(POPOVER)
}

/// The popover's border: ink at 10%, as Monocode's.
pub fn popover_border() -> Hsla {
  ink(0.1)
}

/// Monocode's two-layer popover shadow: black at 10% (`0x0000001a`), twice.
pub fn popover_shadow() -> Vec<BoxShadow> {
  let shadow = |y: f32, blur: f32, spread: f32| BoxShadow {
    color: with_alpha(0x000000, 0.1),
    offset: point(px(0.), px(y)),
    blur_radius: px(blur),
    spread_radius: px(spread),
    inset: false,
  };
  vec![shadow(20., 25., -5.), shadow(8., 10., -6.)]
}

pub fn accent() -> Hsla {
  rgb(ACCENT).into()
}

/// Selected text in a field: the accent, faint enough to read the text through.
pub fn text_selection() -> Hsla {
  with_alpha(ACCENT, 0.3)
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
