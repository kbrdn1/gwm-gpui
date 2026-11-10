//! Colour tokens, the only place a hex value lives. They follow gwm's
//! `claude-dark` preset (`src/tui/theme.rs` in gwm-cli) so both UIs read alike.

pub const BACKGROUND: u32 = 0x1a1a1a;
pub const SURFACE: u32 = 0x2a2a2a;
pub const TEXT: u32 = 0xe0e0e0;
pub const MUTED: u32 = 0x999999;
pub const ACCENT: u32 = 0xd4825d;
pub const CLEAN: u32 = 0x86e89a;
pub const DIRTY: u32 = 0xffdf61;
pub const ERROR: u32 = 0xff7a7a;
/// Something works, but less than it should: the list is not live. gwm calls this yellow "warning".
pub const WARNING: u32 = 0xffdf61;
