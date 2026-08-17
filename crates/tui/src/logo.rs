//! Loads the real logo (`UI/assets/logos/logo_venice_v1.png`) and encodes
//! it as colored Unicode half-blocks (`ratatui-image`, `Picker::halfblocks`
//! forced explicitly — no sixel/kitty/iterm2 auto-detection, and
//! `default-features = false` in Cargo.toml to skip the `chafa` system
//! dependency, which isn't installed here) — the real image, not the
//! computed ASCII interpretation in `ui.rs`, wherever it can be shown.
//!
//! Embedded via `include_bytes!` rather than read from a runtime path: the
//! binary should show the logo regardless of the current working directory
//! it's launched from.

use ratatui::layout::Size;
use ratatui_image::Resize;
use ratatui_image::picker::Picker;
use ratatui_image::protocol::Protocol;

const LOGO_PNG: &[u8] = include_bytes!("../../../UI/assets/logos/logo_venice_v1.png");

/// The encoded protocol plus the terminal-cell size it was encoded at —
/// `ui.rs` needs the size to center the image's `Rect` itself
/// (`Resize::Fit` scales *within* whatever area it's given, it doesn't
/// center a smaller image inside a larger one).
pub struct Logo {
    pub protocol: Protocol,
    pub cells: Size,
}

/// Target size for the splash logo, in terminal cells. Deliberately *not*
/// the image's native pixel-to-cell mapping: the source PNG is 1254×1254px,
/// which maps to ~126×63 cells at a typical font size — real bug hit while
/// building this (confirmed via a debug print, not guessed): a Rect that
/// large collapsed the splash layout to zero height on anything but a huge
/// terminal, so nothing rendered at all. `Resize::Fit` scales the image
/// *down* into whatever target it's given, so picking a deliberately small
/// target here is what actually keeps this a splash logo, not a full-screen
/// takeover.
const TARGET_CELLS: Size = Size::new(40, 20);

/// `None` on any failure (decode error, unexpected font metrics) — the
/// splash screen falls back to the computed ASCII badge rather than
/// crashing the whole TUI over a logo that failed to load.
pub fn load() -> Option<Logo> {
    let picker = Picker::halfblocks();
    let image = image::load_from_memory(LOGO_PNG).ok()?;
    let protocol = picker
        .new_protocol(image, TARGET_CELLS, Resize::Fit(None))
        .ok()?;
    Some(Logo {
        protocol,
        cells: TARGET_CELLS,
    })
}
