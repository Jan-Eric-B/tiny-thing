//! The Artwork pipeline: turns the Artwork a Media session supplies into what
//! the Device draws: the Artwork at its exact size, the Backdrop and the
//! Accent. Pure and CPU-bound, so callers run it off their message loop.

use crate::media;
use fast_image_resize::images::Image as Pixels;
use fast_image_resize::{
    FilterType, MappingError, PixelComponentMapper, PixelType, ResizeAlg, ResizeError,
    ResizeOptions, Resizer, create_srgb_mapper,
};
use protocol::{
    ARTWORK_BOX, ArtworkError, ArtworkId, CONTROL_CIRCLE_WHITE_ALPHA, Image, ImageError, Rgb,
    RgbPixels, SCREEN_HEIGHT, SCREEN_WIDTH,
};
use std::io::Cursor;

/// The version of this pipeline, part of every [`ArtworkId`]. Raise it when
/// the pipeline makes different pixels or Accents from the same image, so
/// the Device does not show Artwork it kept from an older Tray app.
pub(crate) const PIPELINE_VERSION: u32 = 2;

/// The largest source image read, per side. Media players hand out a few
/// hundred to a few thousand pixels. At this size the pipeline takes about
/// 150 MB at its peak (the decoded image and a 16-bit linear-light copy).
const MAX_SOURCE_SIDE: u32 = 4096;

/// The most the image decoder may allocate: a source of the largest size at
/// 8 bits per channel with transparency. A 16-bit source that large is
/// refused.
const MAX_SOURCE_ALLOC: u64 = MAX_SOURCE_SIDE as u64 * MAX_SOURCE_SIDE as u64 * 4;

/// The Artwork is enlarged at most this many times; beyond that it is drawn
/// smaller than the Artwork box instead of blurry.
const MAX_ENLARGEMENT: u32 = 2;

/// The size the Backdrop is shrunk to before it is enlarged again: about 32
/// pixels, in the screen's aspect ratio (5:3).
const BACKDROP_BLUR_SIZE: (u32, u32) = (40, 24);

/// The least contrast (WCAG 2) of white text on the Backdrop and of the
/// Accent on a control's circle on the Backdrop.
const MIN_CONTRAST: f64 = 4.5;

/// Kept beyond [`MIN_CONTRAST`] so the Device's own rounding cannot fall
/// below it.
const CONTRAST_MARGIN: f64 = 0.01;

/// How colorful the Accent is when the Artwork has a hue: its HSV saturation
/// (on sRGB values) at full brightness. Lowered only as far as
/// [`ACCENT_MIN_LUMINANCE`] needs.
const ACCENT_SATURATION: f64 = 0.5;

/// The least luminance (WCAG 2) of the Accent. A control's circle lays white
/// over the Backdrop, so even on a black Backdrop the Accent needs about
/// 0.22 for 4.5:1; the lighter the Accent, the less the Backdrop has to be
/// darkened for it.
const ACCENT_MIN_LUMINANCE: f64 = 0.55;

/// Colors of the Artwork less colorful than this (the difference of the
/// strongest and the weakest sRGB channel) count as gray: they give a white
/// Accent.
const MIN_ACCENT_CHROMA: u8 = 24;

/// Why the Artwork could not be prepared for the Device.
#[derive(Debug, thiserror::Error)]
pub(crate) enum PrepareError {
    #[error("cannot read the Artwork: {0}")]
    Decode(#[from] image::ImageError),
    #[error("the Artwork is {width}x{height} pixels: no image")]
    Empty { width: u32, height: u32 },
    #[error("cannot scale the Artwork: {0}")]
    Resize(#[from] ResizeError),
    #[error("cannot convert the Artwork's colors: {0}")]
    Colors(#[from] MappingError),
    #[error("cannot encode the Artwork: {0}")]
    Encode(#[from] ImageError),
    #[error("the prepared Artwork does not fit the Device: {0}")]
    Contract(#[from] ArtworkError),
}

/// A reader for `encoded` that guesses its format and refuses images larger
/// than [`MAX_SOURCE_SIDE`] a side or than [`MAX_SOURCE_ALLOC`] decoded.
fn reader(encoded: &[u8]) -> Result<image::ImageReader<Cursor<&[u8]>>, image::ImageError> {
    let mut reader = image::ImageReader::new(Cursor::new(encoded))
        .with_guessed_format()
        .map_err(image::ImageError::IoError)?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SOURCE_SIDE);
    limits.max_image_height = Some(MAX_SOURCE_SIDE);
    limits.max_alloc = Some(MAX_SOURCE_ALLOC);
    reader.limits(limits);
    Ok(reader)
}

/// The size of the image in `encoded`, read from its header without
/// decoding it: whether it is an image of a known format, of at least one
/// and at most [`MAX_SOURCE_SIDE`] pixels a side.
pub(crate) fn size(encoded: &[u8]) -> Result<(u32, u32), String> {
    let (width, height) = reader(encoded)
        .and_then(image::ImageReader::into_dimensions)
        .map_err(|error| error.to_string())?;
    let readable = |side: u32| (1..=MAX_SOURCE_SIDE).contains(&side);
    if readable(width) && readable(height) {
        Ok((width, height))
    } else {
        Err(format!(
            "{width}x{height} pixels, not 1 to {MAX_SOURCE_SIDE} a side"
        ))
    }
}

/// The Artwork, Backdrop and Accent the Device draws for `artwork`.
pub(crate) fn prepare(artwork: &media::Artwork) -> Result<protocol::Artwork, PrepareError> {
    let source = reader(artwork.encoded())?.decode()?.into_rgb8();
    prepare_source(artwork.id(), source)
}

/// The Artwork, Backdrop and Accent for the decoded `source`.
fn prepare_source(
    id: ArtworkId,
    source: image::RgbImage,
) -> Result<protocol::Artwork, PrepareError> {
    let (width, height) = source.dimensions();
    if width == 0 || height == 0 {
        return Err(PrepareError::Empty { width, height });
    }
    let source = Pixels::from_vec_u8(width, height, source.into_raw(), PixelType::U8x3)
        .expect("an RgbImage holds exactly width * height * 3 bytes");
    // Converted once; the Artwork and the Backdrop both come from it.
    let srgb = create_srgb_mapper();
    let linear = to_linear(&srgb, &source)?;
    let image = scale_to_artwork_box(&srgb, &source, &linear)?;
    drop(source);
    let (mut backdrop, colors) = blurred(&srgb, &linear)?;
    drop(linear);
    let accent = accent(&colors);
    darken(&mut backdrop, accent);
    Ok(protocol::Artwork::new(
        id,
        Image::encode(&rgb_pixels(image))?,
        Image::encode(&rgb_pixels(backdrop))?,
        Rgb {
            r: accent[0],
            g: accent[1],
            b: accent[2],
        },
    )?)
}

/// `source` in linear light, 16 bits per channel.
fn to_linear(
    srgb: &PixelComponentMapper,
    source: &Pixels<'_>,
) -> Result<Pixels<'static>, PrepareError> {
    let mut linear = Pixels::new(source.width(), source.height(), PixelType::U16x3);
    srgb.forward_map(source, &mut linear)?;
    Ok(linear)
}

/// `linear` back in 8-bit sRGB.
fn to_srgb(
    srgb: &PixelComponentMapper,
    linear: &Pixels<'_>,
) -> Result<Pixels<'static>, PrepareError> {
    let mut result = Pixels::new(linear.width(), linear.height(), PixelType::U8x3);
    srgb.backward_map(linear, &mut result)?;
    Ok(result)
}

/// The size the Device draws an Artwork of `width` by `height` pixels at
/// (both at least 1): the longer side fills the Artwork box, the aspect
/// ratio stays, and nothing is enlarged more than [`MAX_ENLARGEMENT`] times.
fn artwork_size(width: u32, height: u32) -> (u32, u32) {
    let longer = width.max(height);
    let target = ARTWORK_BOX.min(longer.saturating_mul(MAX_ENLARGEMENT));
    let scaled = |side: u32| {
        let side =
            (u64::from(side) * u64::from(target) + u64::from(longer) / 2) / u64::from(longer);
        u32::try_from(side).unwrap_or(target).max(1)
    };
    (scaled(width), scaled(height))
}

/// The source at [`artwork_size`]: made smaller with Lanczos3 in linear
/// light (from `linear`), larger with Mitchell in sRGB (from `source`).
fn scale_to_artwork_box(
    srgb: &PixelComponentMapper,
    source: &Pixels<'_>,
    linear: &Pixels<'_>,
) -> Result<Pixels<'static>, PrepareError> {
    let (width, height) = artwork_size(source.width(), source.height());
    if width <= source.width() {
        let mut scaled = Pixels::new(width, height, PixelType::U16x3);
        Resizer::new().resize(linear, &mut scaled, &convolution(FilterType::Lanczos3))?;
        to_srgb(srgb, &scaled)
    } else {
        let mut scaled = Pixels::new(width, height, PixelType::U8x3);
        Resizer::new().resize(source, &mut scaled, &convolution(FilterType::Mitchell))?;
        Ok(scaled)
    }
}

fn convolution(filter: FilterType) -> ResizeOptions {
    ResizeOptions::new().resize_alg(ResizeAlg::Convolution(filter))
}

/// The Backdrop before it is darkened, and the few colors it was blurred
/// from (about 32 pixels across, for picking the Accent), both in sRGB.
///
/// The middle of the source in the screen's aspect ratio is shrunk to about
/// 32 pixels with a box filter, then enlarged to the screen with a Gaussian
/// filter, both in linear light.
fn blurred(
    srgb: &PixelComponentMapper,
    linear: &Pixels<'_>,
) -> Result<(Pixels<'static>, Pixels<'static>), PrepareError> {
    let (width, height) = BACKDROP_BLUR_SIZE;
    let mut colors = Pixels::new(width, height, PixelType::U16x3);
    Resizer::new().resize(
        linear,
        &mut colors,
        &convolution(FilterType::Box).fit_into_destination(None),
    )?;
    let mut backdrop = Pixels::new(SCREEN_WIDTH, SCREEN_HEIGHT, PixelType::U16x3);
    Resizer::new().resize(&colors, &mut backdrop, &convolution(FilterType::Gaussian))?;
    Ok((to_srgb(srgb, &backdrop)?, to_srgb(srgb, &colors)?))
}

/// The Accent for an Artwork blurred to `colors`: the hue of its most
/// colorful color at full brightness and [`ACCENT_SATURATION`], made paler
/// only as far as [`ACCENT_MIN_LUMINANCE`] needs. White for a gray Artwork.
fn accent(colors: &Pixels<'_>) -> [u8; 3] {
    let chroma = |[r, g, b]: [u8; 3]| r.max(g).max(b) - r.min(g).min(b);
    let pick = pixels_of(colors)
        .max_by_key(|&color| {
            let [r, g, b] = color;
            (chroma(color), u16::from(r) + u16::from(g) + u16::from(b))
        })
        .filter(|&color| chroma(color) >= MIN_ACCENT_CHROMA);
    let Some(pick) = pick else {
        return [255, 255, 255];
    };
    // Same HSV hue at full brightness: each channel keeps its place between
    // the weakest (pulled down by the saturation) and the strongest (full).
    let [max, min] = [
        pick.into_iter().max().unwrap_or(255),
        pick.into_iter().min().unwrap_or(0),
    ];
    let with_saturation = |saturation: f64| -> [u8; 3] {
        pick.map(|channel| {
            let below_strongest = f64::from(max - channel) / f64::from(max - min);
            let value = 255.0 * (1.0 - saturation * below_strongest);
            // In 0..=255: the saturation and the share are in 0..=1.
            value.round() as u8
        })
    };
    let light_enough = |color: [u8; 3]| luminance(color) >= ACCENT_MIN_LUMINANCE;
    if light_enough(with_saturation(ACCENT_SATURATION)) {
        return with_saturation(ACCENT_SATURATION);
    }
    // Less saturated is lighter, down to white at none.
    let (mut low, mut high) = (0.0, ACCENT_SATURATION);
    for _ in 0..24 {
        let middle = (low + high) / 2.0;
        if light_enough(with_saturation(middle)) {
            low = middle;
        } else {
            high = middle;
        }
    }
    with_saturation(low)
}

/// Darkens `backdrop` (all pixels by the same factor, in linear light) as
/// little as needed for white text on every pixel, and for `accent` on a
/// control's circle (white at [`CONTROL_CIRCLE_WHITE_ALPHA`] laid over the
/// Backdrop) on every pixel, each by [`MIN_CONTRAST`]. A Backdrop that has
/// both already stays as it is. A pressed control only shrinks its circle,
/// so the icons are never drawn on anything lighter.
fn darken(backdrop: &mut Pixels<'_>, accent: [u8; 3]) {
    let required = MIN_CONTRAST + CONTRAST_MARGIN;
    // White's luminance is 1: (1 + 0.05) / (L + 0.05) >= required.
    let brightest_for_text = 1.05 / required - 0.05;
    let brightest_on_control = (luminance(accent) + 0.05) / required - 0.05;
    let readable = |factor: f64| {
        let darkened = darkening(factor);
        // Per channel: the pixel's share of the luminance, without and with
        // a control's circle on top.
        let share = |weight: f64, on_control: bool| {
            darkened.map(|component| {
                let component = if on_control {
                    under_control(component)
                } else {
                    srgb_to_linear(f64::from(component))
                };
                weight * component
            })
        };
        let plain = [0.2126, 0.7152, 0.0722].map(|weight| share(weight, false));
        let on_control = [0.2126, 0.7152, 0.0722].map(|weight| share(weight, true));
        pixels_of(backdrop).all(|[r, g, b]| {
            let (r, g, b) = (usize::from(r), usize::from(g), usize::from(b));
            plain[0][r] + plain[1][g] + plain[2][b] <= brightest_for_text
                && on_control[0][r] + on_control[1][g] + on_control[2][b] <= brightest_on_control
        })
    };
    if readable(1.0) {
        return;
    }
    // Always readable at 0, black: the Accent is light enough for that.
    let (mut low, mut high) = (0.0, 1.0);
    for _ in 0..20 {
        let middle = (low + high) / 2.0;
        if readable(middle) {
            low = middle;
        } else {
            high = middle;
        }
    }
    let darkened = darkening(low);
    for component in backdrop.buffer_mut() {
        *component = darkened[usize::from(*component)];
    }
}

/// Each sRGB component value darkened by `factor` in linear light, rounded
/// down so no pixel ends up brighter than intended.
fn darkening(factor: f64) -> [u8; 256] {
    std::array::from_fn(|component| {
        // In 0..=255 after the clamp in `linear_to_srgb`.
        linear_to_srgb(srgb_to_linear(component as f64) * factor).floor() as u8
    })
}

/// The linear light of an sRGB component value under a control's circle:
/// white at [`CONTROL_CIRCLE_WHITE_ALPHA`] blended over it in sRGB values,
/// as the Device draws it, rounded up so the result is never darker than on
/// the screen.
fn under_control(component: u8) -> f64 {
    let alpha = f64::from(CONTROL_CIRCLE_WHITE_ALPHA) / 255.0;
    let blended = (255.0 * alpha + f64::from(component) * (1.0 - alpha)).ceil();
    srgb_to_linear(blended)
}

fn pixels_of<'a>(pixels: &'a Pixels<'_>) -> impl Iterator<Item = [u8; 3]> + 'a {
    pixels.buffer().as_chunks::<3>().0.iter().copied()
}

fn rgb_pixels(pixels: Pixels<'_>) -> RgbPixels {
    RgbPixels {
        width: pixels.width(),
        height: pixels.height(),
        rgb: pixels.into_vec(),
    }
}

/// WCAG 2 relative luminance of an sRGB color, 0 to 1.
fn luminance(color: [u8; 3]) -> f64 {
    let [r, g, b] = color.map(|component| srgb_to_linear(f64::from(component)));
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// An sRGB component value (0 to 255) in linear light (0 to 1).
fn srgb_to_linear(component: f64) -> f64 {
    let c = component / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Linear light (0 to 1) as an sRGB component value (0 to 255).
fn linear_to_srgb(linear: f64) -> f64 {
    let linear = linear.clamp(0.0, 1.0);
    let c = if linear <= 0.003_130_8 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    };
    c * 255.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_source_without_pixels_is_no_artwork() {
        for (width, height) in [(0, 0), (0, 300), (300, 0)] {
            let result = prepare_source(ArtworkId(1), image::RgbImage::new(width, height));

            assert!(
                matches!(result, Err(PrepareError::Empty { .. })),
                "{width}x{height}: {result:?}"
            );
        }
    }
}
