//! Loads the real logo (`UI/assets/logos/logo_venice_v1.png`) and encodes
//! it for the terminal via `ratatui-image`.
//!
//! **Protocol: queried from the real terminal, not forced to half-blocks.**
//! Checked against the crate's actual source (`picker.rs`), not assumed:
//! Sixel/Kitty/iTerm2 support is compiled in unconditionally — `chafa` (the
//! system dependency this crate deliberately avoids, see Cargo.toml) gates
//! something else entirely, not protocol availability. `Picker::from_query_stdio`
//! sends real capability-query escape sequences and reads the terminal's
//! response, picking whichever protocol it actually supports (near
//! pixel-accurate on Kitty/WezTerm/iTerm2/Sixel-capable terminals) and
//! falling back to half-blocks only when nothing better answers. Must run
//! after entering the alternate screen but before reading terminal events
//! (the crate's own doc comment on that function) — matches where
//! `main.rs` calls this, right after `ratatui::init()`.
//!
//! Embedded via `include_bytes!` rather than read from a runtime path: the
//! binary should show the logo regardless of the current working directory
//! it's launched from.

use image::{DynamicImage, GenericImageView, Rgba};
use ratatui::layout::Size;
use ratatui_image::Resize;
use ratatui_image::picker::Picker;
use ratatui_image::protocol::Protocol;

const LOGO_PNG: &[u8] = include_bytes!("../../../UI/assets/logos/logo_venice_v1.png");

/// The source PNG has real margin around the circular badge (measured, not
/// guessed: content spans roughly rows 97–1136 and cols 109–1143 of a
/// 1254×1254 canvas — about 17% blank border on each side). Cropping to
/// that content before encoding means the limited cell budget (see
/// `target_size`) goes toward actual logo detail instead of blank
/// background, which matters a lot more at half-blocks resolution than it
/// would for a full-size image.
fn crop_to_content(image: &DynamicImage) -> DynamicImage {
    let background = image.get_pixel(0, 0);
    let differs = |p: Rgba<u8>| {
        p.0.iter()
            .zip(background.0.iter())
            .any(|(a, b)| a.abs_diff(*b) > 10)
    };

    let (width, height) = image.dimensions();
    let mut min_x = width;
    let mut max_x = 0;
    let mut min_y = height;
    let mut max_y = 0;
    for (x, y, pixel) in image.pixels() {
        if differs(pixel) {
            min_x = min_x.min(x);
            max_x = max_x.max(x);
            min_y = min_y.min(y);
            max_y = max_y.max(y);
        }
    }

    if min_x > max_x || min_y > max_y {
        return image.clone(); // no content found — nothing to crop to
    }
    image.crop_imm(min_x, min_y, max_x - min_x + 1, max_y - min_y + 1)
}

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
///
/// Must be called after `ratatui::init()` but before the event loop starts
/// reading input — `Picker::from_query_stdio` briefly reads stdin itself
/// to parse the terminal's capability response.
pub fn load(available: Size) -> Option<Logo> {
    let picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());
    let image = image::load_from_memory(LOGO_PNG).ok()?;
    let image = crop_to_content(&image);
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
    use image::{ImageBuffer, Rgba as PixelRgba};

    /// A 20×20 white canvas with a 4×4 red square at (8,8) — a synthetic
    /// stand-in for "logo with real margin", so this test doesn't depend on
    /// the actual asset's dimensions ever staying the same.
    fn image_with_margin() -> DynamicImage {
        let mut buf = ImageBuffer::from_pixel(20, 20, PixelRgba([255, 255, 255, 255]));
        for y in 8..12 {
            for x in 8..12 {
                buf.put_pixel(x, y, PixelRgba([255, 0, 0, 255]));
            }
        }
        DynamicImage::ImageRgba8(buf)
    }

    #[test]
    fn crop_to_content_removes_the_background_margin() {
        let cropped = crop_to_content(&image_with_margin());
        assert_eq!(cropped.dimensions(), (4, 4));
    }

    #[test]
    fn crop_to_content_is_a_no_op_on_an_image_with_no_margin() {
        let solid =
            DynamicImage::ImageRgba8(ImageBuffer::from_pixel(5, 5, PixelRgba([10, 20, 30, 255])));
        let cropped = crop_to_content(&solid);
        // Every pixel matches the (0,0) "background" sample, so nothing is
        // ever flagged as content — falls back to the original image
        // rather than cropping to nothing.
        assert_eq!(cropped.dimensions(), (5, 5));
    }

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
