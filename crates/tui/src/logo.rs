//! Loads the real logo (`UI/assets/logos/logo_venice_v1.png`) and encodes
//! it for the terminal via `ratatui-image`.
//!
//! **Protocol: half-blocks, forced explicitly (see `picker()` below).**
//! Sixel/Kitty/iTerm2 support is compiled into the crate unconditionally —
//! `chafa` (the system dependency this project deliberately avoids, see
//! Cargo.toml) gates something else entirely, not protocol availability —
//! so auto-detecting via `Picker::from_query_stdio()` was tried first. Real
//! regression, not a guess: VS Code's integrated terminal (this project's
//! actual dev environment) answers that capability query in a way that
//! made it pick a graphics protocol it doesn't actually render, so nothing
//! showed at all. Forced back to half-blocks, the one config actually
//! verified working here.
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

/// Near-black, close to VS Code's default dark theme background (`#1e1e1e`)
/// and most other terminal dark themes — picked as a reasonable single
/// default rather than trying to detect the real one.
const DARK_BACKGROUND: [u8; 4] = [30, 30, 30, 255];

/// Replaces the PNG's opaque white background with a dark neutral color.
///
/// **Real constraint found by reading the encoder's actual source**
/// (`ratatui-image`'s `halfblocks/primitive.rs`, not chafa — that's not
/// linked here): half-blocks always calls `img.to_rgb8()`, which discards
/// alpha entirely. There is no way to make the background transparent and
/// let the terminal's own background show through in this mode — every
/// cell is always painted with an opaque color. Recoloring the background
/// pixels themselves, rather than trying to make them transparent, is the
/// only lever that actually exists here.
fn darken_background(image: &DynamicImage) -> DynamicImage {
    let background = image.get_pixel(0, 0);
    let mut rgba = image.to_rgba8();
    for pixel in rgba.pixels_mut() {
        let close_to_background = pixel
            .0
            .iter()
            .zip(background.0.iter())
            .all(|(a, b)| a.abs_diff(*b) <= 10);
        if close_to_background {
            *pixel = Rgba(DARK_BACKGROUND);
        }
    }
    DynamicImage::ImageRgba8(rgba)
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
/// a reasonable maximum *and* at what the real terminal has room for once
/// the caption below it is accounted for. Deliberately not the image's
/// native pixel-to-cell mapping: the source PNG is 1254×1254px, which maps
/// to ~126×63 cells at a typical font size.
///
/// **Real bug hit while building this** (confirmed via a debug print, not
/// guessed, then reproduced deliberately at 80×24 — the most common
/// default terminal size): a fixed 40×20 target doesn't leave room for the
/// caption on an 80×24 terminal, so the image silently rendered nothing at
/// all rather than something too big. `Resize::Fit` only scales *down*
/// into whatever target it's given — it can't rescue an unfit target after
/// the fact, so the target has to be right the first time.
///
/// Max raised from 40×20 to 64×32 after real feedback ("blurry/pixelated")
/// on a terminal window much larger than 80×24 — half-blocks resolution is
/// fundamentally capped by cell count (`primitive.rs`: 2 vertical "pixels"
/// per cell, no chafa dithering linked in), so a bigger cell budget is the
/// only lever that actually sharpens it on a terminal with room to spare.
fn target_size(available: Size) -> Size {
    let usable_height = available.height.saturating_sub(CAPTION_ROWS + 1);
    let height = usable_height.clamp(4, 32);
    // The logo is square (1254×1254px) and a terminal cell is roughly
    // twice as tall as it is wide, so width ≈ 2 × height keeps it visually
    // square — still capped by the terminal's actual width.
    let width = (height * 2).min(available.width.saturating_sub(2)).max(4);
    Size::new(width, height)
}

/// Forced back to `Picker::halfblocks()` after a real regression, not a
/// guess: `Picker::from_query_stdio()` (tried first) picks whichever
/// protocol the terminal's capability-query response implies, but VS
/// Code's integrated terminal (xterm.js) answers those queries in a way
/// that made it choose a graphics protocol (Sixel/Kitty) it doesn't
/// actually render — nothing appeared at all, worse than the resolution
/// half-blocks gives, instead of the intended graceful fallback. Since VS
/// Code's integrated terminal is this project's actual dev environment,
/// reliability here wins over chasing better fidelity on terminals that
/// happen to answer the query correctly. `PICKER_QUERY_TERMINAL=1` opts
/// back into auto-detection for testing on a terminal that's confirmed to
/// support Kitty/Sixel outside VS Code.
fn picker() -> Picker {
    if std::env::var("PICKER_QUERY_TERMINAL").is_ok()
        && let Ok(picker) = Picker::from_query_stdio()
    {
        return picker;
    }
    Picker::halfblocks()
}

/// `None` on any failure (decode error, unexpected font metrics) — the
/// splash screen falls back to the computed ASCII badge rather than
/// crashing the whole TUI over a logo that failed to load.
pub fn load(available: Size) -> Option<Logo> {
    let picker = picker();
    let image = image::load_from_memory(LOGO_PNG).ok()?;
    let image = crop_to_content(&image);
    let image = darken_background(&image);
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
        assert!(size.height <= 32);
        assert!(size.width <= 64);
    }

    #[test]
    fn darken_background_replaces_background_pixels_only() {
        let darkened = darken_background(&image_with_margin());
        assert_eq!(darkened.get_pixel(0, 0).0, DARK_BACKGROUND);
        // The red square (real content) must survive untouched.
        assert_eq!(darkened.get_pixel(9, 9).0, [255, 0, 0, 255]);
    }

    #[test]
    fn never_produces_a_zero_dimension_even_on_a_tiny_terminal() {
        let size = target_size(Size::new(20, 10));
        assert!(size.width > 0 && size.height > 0);
    }
}
