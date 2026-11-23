//! Artwork as the Device draws it, and the lossless images it travels in.

use crate::MAX_FRAME_LEN;
use serde::de::{self, Deserializer, Visitor};
use serde::{Deserialize, Serialize, Serializer};
use std::fmt;
use std::sync::Arc;

/// The Device's screen width in pixels; the Backdrop is this wide.
pub const SCREEN_WIDTH: u32 = 800;

/// The Device's screen height in pixels; the Backdrop is this high.
pub const SCREEN_HEIGHT: u32 = 480;

/// The side of the square the Device app sets aside for the Artwork, in
/// pixels. The Artwork's longer side is at most this long; the Tray app
/// scales it to the exact size the Device draws, and the Device app never
/// rescales it.
pub const ARTWORK_BOX: u32 = 352;

/// How opaque the white is that the Device app lays over the Backdrop for a
/// control's circle, 0 to 255 (`#ffffff1a`). The control icons take the
/// Accent, so the Accent needs its contrast on the Backdrop with this white
/// on top. Pressing a control only shrinks its circle and never changes its
/// color, so this is the only surface the icons are drawn on.
pub const CONTROL_CIRCLE_WHITE_ALPHA: u8 = 0x1a;

/// The most bytes QOI takes for an RGB image of `width` by `height`: the
/// header, at worst four bytes a pixel, and the end marker.
const fn max_qoi_len(width: u32, height: u32) -> usize {
    14 + (width as usize) * (height as usize) * 4 + 8
}

/// The longest message an Artwork can make: the Backdrop and the largest
/// Artwork as badly compressed as QOI gets, plus room for the id, the
/// Accent, the byte string lengths and the message around them.
const MAX_ARTWORK_MESSAGE_LEN: usize =
    max_qoi_len(SCREEN_WIDTH, SCREEN_HEIGHT) + max_qoi_len(ARTWORK_BOX, ARTWORK_BOX) + 64;

// Every Artwork fits in one frame.
const _: () = assert!(MAX_ARTWORK_MESSAGE_LEN <= MAX_FRAME_LEN);

/// Identifies an Artwork by a hash of the image bytes the Media session
/// supplied together with the version of the Tray app's Artwork pipeline,
/// never by title or artist. The same image has the same id under any
/// title, a new image a new one even when the title stays, and a Tray app
/// whose pipeline prepares Artwork differently gives new ids, so the Device
/// does not show Artwork it kept from the old way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ArtworkId(pub u64);

/// An sRGB color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// The Artwork of Now Playing, ready for the Device's screen: the image at
/// the exact size the Device draws it, the Backdrop for the whole screen, and
/// the Accent.
///
/// It keeps to the size contract with the Device app, checked from the image
/// headers when it is made and when it is read from a message: the Artwork
/// is 1 to [`ARTWORK_BOX`] pixels a side, the Backdrop exactly
/// [`SCREEN_WIDTH`] by [`SCREEN_HEIGHT`]. So decoding either image never
/// allocates more than a screen's worth, and the Artwork fits its box beside
/// the text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "UncheckedArtwork")]
pub struct Artwork {
    id: ArtworkId,
    image: Image,
    backdrop: Image,
    accent: Rgb,
}

/// How an [`Artwork`] breaks the size contract with the Device.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum ArtworkError {
    #[error("the Artwork is not an image")]
    ImageUnreadable,
    #[error("the Artwork is {width}x{height} pixels, not 1 to {ARTWORK_BOX} a side")]
    ImageSize { width: u32, height: u32 },
    #[error("the Backdrop is not an image")]
    BackdropUnreadable,
    #[error("the Backdrop is {width}x{height} pixels, not {SCREEN_WIDTH}x{SCREEN_HEIGHT}")]
    BackdropSize { width: u32, height: u32 },
}

impl Artwork {
    /// The Artwork `id`: `image` in the Artwork box, on `backdrop`, with
    /// `accent` for the icons. Refused unless both images keep to the size
    /// contract.
    pub fn new(
        id: ArtworkId,
        image: Image,
        backdrop: Image,
        accent: Rgb,
    ) -> Result<Self, ArtworkError> {
        let (width, height) = image.size().ok_or(ArtworkError::ImageUnreadable)?;
        let fits = |side: u32| (1..=ARTWORK_BOX).contains(&side);
        if !fits(width) || !fits(height) {
            return Err(ArtworkError::ImageSize { width, height });
        }
        let (width, height) = backdrop.size().ok_or(ArtworkError::BackdropUnreadable)?;
        if (width, height) != (SCREEN_WIDTH, SCREEN_HEIGHT) {
            return Err(ArtworkError::BackdropSize { width, height });
        }
        Ok(Self {
            id,
            image,
            backdrop,
            accent,
        })
    }

    pub fn id(&self) -> ArtworkId {
        self.id
    }

    /// The Artwork itself, 1 to [`ARTWORK_BOX`] pixels a side.
    pub fn image(&self) -> &Image {
        &self.image
    }

    /// Exactly [`SCREEN_WIDTH`] by [`SCREEN_HEIGHT`].
    pub fn backdrop(&self) -> &Image {
        &self.backdrop
    }

    pub fn accent(&self) -> Rgb {
        self.accent
    }
}

/// An [`Artwork`] as read from a message, before its sizes are checked: the
/// same fields in the same order, so it reads what an `Artwork` writes.
#[derive(Deserialize)]
struct UncheckedArtwork {
    id: ArtworkId,
    image: Image,
    backdrop: Image,
    accent: Rgb,
}

impl TryFrom<UncheckedArtwork> for Artwork {
    type Error = ArtworkError;

    fn try_from(unchecked: UncheckedArtwork) -> Result<Self, ArtworkError> {
        let UncheckedArtwork {
            id,
            image,
            backdrop,
            accent,
        } = unchecked;
        Self::new(id, image, backdrop, accent)
    }
}

/// Uncompressed 8-bit sRGB pixels, row by row, three bytes (red, green,
/// blue) per pixel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbPixels {
    pub width: u32,
    pub height: u32,
    /// `width * height * 3` bytes.
    pub rgb: Vec<u8>,
}

/// Why pixels could not be turned into an [`Image`] or back.
#[derive(Debug, thiserror::Error)]
pub enum ImageError {
    #[error("not an image: {0}")]
    Format(#[from] qoi::Error),
    #[error("the image takes {expected} bytes decoded, not {actual}")]
    BufferLength { expected: usize, actual: usize },
}

/// An image as it travels: lossless (QOI), RGB. Cheap to clone, so the
/// Device daemon can keep and pass on images without copying them.
///
/// Any bytes read from a message make an `Image`; whether they are one shows
/// when it is decoded. Decoding allocates as much as the image's header
/// says, which an [`Artwork`] checks for its images.
#[derive(Clone, PartialEq, Eq)]
pub struct Image(Arc<[u8]>);

impl Image {
    /// Encodes `pixels` without losing anything.
    pub fn encode(pixels: &RgbPixels) -> Result<Self, ImageError> {
        let encoded = qoi::Encoder::new(&pixels.rgb, pixels.width, pixels.height)?
            .with_colorspace(qoi::ColorSpace::Srgb)
            .encode_to_vec()?;
        Ok(Self(encoded.into()))
    }

    /// The pixels, exactly as they were encoded.
    pub fn decode(&self) -> Result<RgbPixels, ImageError> {
        let header = qoi::decode_header(&*self.0)?;
        let mut rgb = vec![0; header.n_pixels() * 3];
        self.decode_into(&mut rgb)?;
        Ok(RgbPixels {
            width: header.width,
            height: header.height,
            rgb,
        })
    }

    /// Decodes the pixels straight into `rgb`, which must be exactly the
    /// image's [`size`](Self::size), three bytes (red, green, blue) per
    /// pixel, row by row.
    pub fn decode_into(&self, rgb: &mut [u8]) -> Result<(), ImageError> {
        let mut decoder = qoi::Decoder::new(&*self.0)?.with_channels(qoi::Channels::Rgb);
        let expected = decoder.required_buf_len();
        if rgb.len() != expected {
            return Err(ImageError::BufferLength {
                expected,
                actual: rgb.len(),
            });
        }
        decoder.decode_to_buf(rgb)?;
        Ok(())
    }

    /// Width and height in pixels, read from the image's header; `None` if
    /// the bytes are not an image.
    pub fn size(&self) -> Option<(u32, u32)> {
        qoi::decode_header(&*self.0)
            .ok()
            .map(|header| (header.width, header.height))
    }
}

impl fmt::Debug for Image {
    /// The size, not the bytes: messages are logged in readable form.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.size() {
            Some((width, height)) => {
                write!(f, "Image({width}x{height}, {} bytes)", self.0.len())
            }
            None => write!(f, "Image(invalid, {} bytes)", self.0.len()),
        }
    }
}

impl Serialize for Image {
    /// As a byte string: a length, then the bytes as they are.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(&self.0)
    }
}

impl<'de> Deserialize<'de> for Image {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_bytes(ImageVisitor)
    }
}

struct ImageVisitor;

impl<'de> Visitor<'de> for ImageVisitor {
    type Value = Image;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("the bytes of an image")
    }

    fn visit_bytes<E: de::Error>(self, bytes: &[u8]) -> Result<Image, E> {
        Ok(Image(bytes.into()))
    }

    fn visit_byte_buf<E: de::Error>(self, bytes: Vec<u8>) -> Result<Image, E> {
        Ok(Image(bytes.into()))
    }
}
