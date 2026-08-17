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

/// Rows `ui::draw_splash`'s caption block actually takes — kept in sync
/// manually with `splash_caption_lines()` there, since the `Protocol` has
/// to be sized *before* that layout runs (it doesn't resize after
/// creation, unlike `StatefulProtocol`).
const CAPTION_ROWS: u16 = 5;

/// Picks a target size, in terminal cells, that actually fits — capped at
/// a reasonable maximum (40×20) *and* at what the real terminal has room
/// for once the caption below it is accounted for. Deliberately not the
/// image's native pixel-to-cell mapping: the source PNG is 1254×1254px,
/// which maps to ~126×63 cells at a typical font size.
///
/// **Real bug hit while building this** (confirmed via a debug print, not
/// guessed, then reproduced deliberately at 80×24 — the most common
/// default terminal size): a fixed 40×20 target doesn't leave room for the
/// caption on an 80×24 terminal, so the image silently rendered nothing at
/// all rather than something too big. `Resize::Fit` only scales *down*
/// into whatever target it's given — it can't rescue an unfit target after
/// the fact, so the target has to be right the first time.
fn target_size(available: Size) -> Size {
    let usable_height = available.height.saturating_sub(CAPTION_ROWS + 1);
    let height = usable_height.clamp(4, 20);
    // The logo is square (1254×1254px) and a terminal cell is roughly
    // twice as tall as it is wide, so width ≈ 2 × height keeps it visually
    // square — still capped by the terminal's actual width.
    let width = (height * 2).min(available.width.saturating_sub(2)).max(4);
    Size::new(width, height)
}

/// `None` on any failure (decode error, unexpected font metrics) — the
/// splash screen falls back to the computed ASCII badge rather than
/// crashing the whole TUI over a logo that failed to load.
pub fn load(available: Size) -> Option<Logo> {
    let picker = Picker::halfblocks();
    let image = image::load_from_memory(LOGO_PNG).ok()?;
    let target = target_size(available);
    let protocol = picker.new_protocol(image, target, Resize::Fit(None)).ok()?;
    Some(Logo {
        protocol,
        cells: target,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fits_within_a_common_80x24_terminal_leaving_room_for_the_caption() {
        let size = target_size(Size::new(80, 24));
        assert!(
            size.height + CAPTION_ROWS + 1 <= 24,
            "{size:?} doesn't leave room for the caption"
        );
    }

    #[test]
    fn caps_at_a_reasonable_maximum_on_a_huge_terminal() {
        let size = target_size(Size::new(300, 150));
        assert!(size.height <= 20);
        assert!(size.width <= 40);
    }

    #[test]
    fn never_produces_a_zero_dimension_even_on_a_tiny_terminal() {
        let size = target_size(Size::new(20, 10));
        assert!(size.width > 0 && size.height > 0);
    }
}
