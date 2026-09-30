//! Loads the real logo (`UI/assets/logos/retina-mark-small-light-1024.png`,
//! the simplified three-line mark meant for small sizes) and renders
//! it as density-based ASCII art (a character ramp from sparse to dense,
//! picked per cell from the image's real sampled luminance and colored
//! with its real sampled color) — not `ratatui-image`/half-blocks anymore.
//!
//! **Why the pivot, not a guess**: half-blocks was tried first and
//! verified working, but two real, unfixable-in-that-mode problems
//! surfaced from actual user feedback with screenshots: (1) the block
//! characters read as "blurry/pixelated" no matter the resolution — an
//! inherent property of painting solid-colored rectangles rather than
//! shaped glyphs; (2) `ratatui-image`'s half-blocks encoder always calls
//! `img.to_rgb8()` (`halfblocks/primitive.rs`, verified in the crate's own
//! source), discarding alpha entirely — there was no way to make the
//! PNG's opaque white background disappear into the terminal's own
//! background short of recoloring it to a guessed dark color. Sampling
//! into a character ramp instead means background cells can genuinely be
//! a blank space (real transparency, the terminal's own background shows
//! through, no guessed color needed) and content renders as shaped glyphs
//! instead of flat rectangles.
//!
//! Embedded via `include_bytes!` rather than read from a runtime path: the
//! binary should show the logo regardless of the current working directory
//! it's launched from.

use image::{DynamicImage, GenericImageView, Rgba, imageops::FilterType};
use ratatui::layout::Size;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

const LOGO_PNG: &[u8] = include_bytes!("../../../UI/assets/logos/retina-mark-small-light-1024.png");

/// Sparse to dense. A leading space matters: it's what "background" maps
/// to before the transparency check even applies a hard cutoff, so
/// near-background-but-not-quite pixels (anti-aliased edges) fade out
/// gently instead of showing a hard-edged dot.
const RAMP: &[char] = &[' ', '.', ':', '-', '=', '+', '*', '#', '%', '@'];

/// The source PNG has real margin around the mark (the SVG's view box keeps
/// room for the star's spikes). Cropping to
/// that content before sampling means the limited cell budget (see
/// `target_size`) goes toward actual logo detail instead of blank
/// background.
fn crop_to_content(image: &DynamicImage, background: Rgba<u8>) -> DynamicImage {
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

fn luminance(p: Rgba<u8>) -> f32 {
    0.2126 * p.0[0] as f32 + 0.7152 * p.0[1] as f32 + 0.0722 * p.0[2] as f32
}

/// One character ramp lookup, or `None` for "this cell is background" —
/// real transparency (a blank cell, the terminal's own background shows
/// through), not a recolored guess.
fn cell_glyph(pixel: Rgba<u8>, background: Rgba<u8>) -> Option<(char, Color)> {
    let is_background = pixel
        .0
        .iter()
        .zip(background.0.iter())
        .all(|(a, b)| a.abs_diff(*b) <= 12);
    if is_background {
        return None;
    }
    // Content is dark ink on a light background here, so low luminance
    // should map to the *dense* end of the ramp, not the sparse end.
    let normalized = (luminance(pixel) / 255.0).clamp(0.0, 1.0);
    let index = ((1.0 - normalized) * (RAMP.len() - 1) as f32).round() as usize;
    Some((
        RAMP[index.min(RAMP.len() - 1)],
        Color::Rgb(pixel.0[0], pixel.0[1], pixel.0[2]),
    ))
}

pub struct Logo {
    pub lines: Vec<Line<'static>>,
    pub cells: Size,
}

/// Rows `ui::draw_splash`'s caption block actually takes — kept in sync
/// manually with `splash_caption_lines()` there.
const CAPTION_ROWS: u16 = 5;

/// Picks a target size, in terminal cells, that actually fits — capped at
/// a reasonable maximum *and* at what the real terminal has room for once
/// the caption below it is accounted for.
///
/// **Real bug hit while building this** (confirmed via a debug print, not
/// guessed, then reproduced deliberately at 80×24 — the most common
/// default terminal size): a fixed 40×20 target doesn't leave room for the
/// caption on an 80×24 terminal, so nothing rendered at all rather than
/// something too big — the size has to be right the first time.
fn target_size(available: Size) -> Size {
    let usable_height = available.height.saturating_sub(CAPTION_ROWS + 1);
    let height = usable_height.clamp(4, 32);
    // The logo is square and a terminal cell is roughly twice as tall as
    // it is wide, so width ≈ 2 × height keeps it visually square — still
    // capped by the terminal's actual width.
    let width = (height * 2).min(available.width.saturating_sub(2)).max(4);
    Size::new(width, height)
}

/// `None` on any decode failure — the splash screen falls back to the
/// computed ASCII badge rather than crashing the whole TUI over a logo
/// that failed to load.
pub fn load(available: Size) -> Option<Logo> {
    let original = image::load_from_memory(LOGO_PNG).ok()?;
    let background = original.get_pixel(0, 0);
    let cropped = crop_to_content(&original, background);
    let target = target_size(available);
    let resized = cropped.resize_exact(
        target.width as u32,
        target.height as u32,
        FilterType::Triangle,
    );

    let lines = (0..resized.height())
        .map(|y| {
            let spans: Vec<Span<'static>> = (0..resized.width())
                .map(|x| match cell_glyph(resized.get_pixel(x, y), background) {
                    Some((ch, color)) => Span::styled(ch.to_string(), Style::default().fg(color)),
                    None => Span::raw(" "),
                })
                .collect();
            Line::from(spans)
        })
        .collect();

    Some(Logo {
        lines,
        cells: target,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba as PixelRgba};

    const WHITE: Rgba<u8> = Rgba([255, 255, 255, 255]);

    /// A 20×20 white canvas with a 4×4 red square at (8,8) — a synthetic
    /// stand-in for "logo with real margin", so tests don't depend on the
    /// actual asset's dimensions ever staying the same.
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
        let cropped = crop_to_content(&image_with_margin(), WHITE);
        assert_eq!(cropped.dimensions(), (4, 4));
    }

    #[test]
    fn crop_to_content_is_a_no_op_on_an_image_with_no_margin() {
        let solid =
            DynamicImage::ImageRgba8(ImageBuffer::from_pixel(5, 5, PixelRgba([10, 20, 30, 255])));
        let cropped = crop_to_content(&solid, Rgba([10, 20, 30, 255]));
        assert_eq!(cropped.dimensions(), (5, 5));
    }

    #[test]
    fn cell_glyph_is_none_for_background_colored_pixels() {
        assert_eq!(cell_glyph(WHITE, WHITE), None);
    }

    #[test]
    fn cell_glyph_picks_a_dense_character_for_dark_content_on_a_light_background() {
        let dark_red = Rgba([180, 0, 0, 255]);
        let (ch, color) = cell_glyph(dark_red, WHITE).expect("dark content should get a glyph");
        assert!(
            RAMP[RAMP.len() / 2..].contains(&ch),
            "expected a denser glyph, got {ch:?}"
        );
        assert_eq!(color, Color::Rgb(180, 0, 0));
    }

    #[test]
    fn fits_within_a_common_80x24_terminal_leaving_room_for_the_caption() {
        let size = target_size(Size::new(80, 24));
        assert!(
            size.height + CAPTION_ROWS < 24,
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
    fn never_produces_a_zero_dimension_even_on_a_tiny_terminal() {
        let size = target_size(Size::new(20, 10));
        assert!(size.width > 0 && size.height > 0);
    }

    #[test]
    fn load_produces_as_many_lines_as_the_target_height() {
        let logo = load(Size::new(80, 24)).expect("real asset should decode");
        assert_eq!(logo.lines.len(), logo.cells.height as usize);
    }
}
