//! An Artwork keeps to the size contract with the Device: the Backdrop is
//! exactly the screen and the Artwork fits the Artwork box. One that does
//! not cannot be made, nor read from a message.

// Test code fails loudly on purpose.
#![allow(clippy::unwrap_used)]

use protocol::{
    ARTWORK_BOX, Artwork, ArtworkError, ArtworkId, Image, Rgb, RgbPixels, SCREEN_HEIGHT,
    SCREEN_WIDTH,
};
use serde::Serialize;

fn image(width: u32, height: u32) -> Image {
    Image::encode(&RgbPixels {
        width,
        height,
        rgb: [200, 30, 40].repeat((width * height) as usize),
    })
    .unwrap()
}

fn screen() -> Image {
    image(SCREEN_WIDTH, SCREEN_HEIGHT)
}

const ACCENT: Rgb = Rgb {
    r: 255,
    g: 200,
    b: 200,
};

/// A QOI header claiming `width` by `height` pixels, with no pixels after it:
/// what a hostile peer sends to make the Device allocate.
fn header_only(width: u32, height: u32) -> Image {
    let mut bytes = b"qoif".to_vec();
    bytes.extend_from_slice(&width.to_be_bytes());
    bytes.extend_from_slice(&height.to_be_bytes());
    bytes.extend_from_slice(&[3, 0]);
    postcard::from_bytes(&postcard::to_allocvec(&Bytes(&bytes)).unwrap()).unwrap()
}

/// Serializes as a byte string, the way an [`Image`] travels.
struct Bytes<'a>(&'a [u8]);

impl Serialize for Bytes<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(self.0)
    }
}

/// An Artwork as a peer could put it on the wire, unchecked: the same fields
/// in the same order as [`Artwork`].
#[derive(Serialize)]
struct WireArtwork {
    id: ArtworkId,
    image: Image,
    backdrop: Image,
    accent: Rgb,
}

#[test]
fn artwork_within_the_artwork_box_on_a_screen_sized_backdrop_is_made() {
    for (width, height) in [
        (ARTWORK_BOX, ARTWORK_BOX),
        (ARTWORK_BOX, 1),
        (1, ARTWORK_BOX),
        (200, 113),
    ] {
        let artwork = Artwork::new(ArtworkId(1), image(width, height), screen(), ACCENT).unwrap();

        assert_eq!(artwork.image().size(), Some((width, height)));
        assert_eq!(
            artwork.backdrop().size(),
            Some((SCREEN_WIDTH, SCREEN_HEIGHT))
        );
        assert_eq!(artwork.accent(), ACCENT);
        assert_eq!(artwork.id(), ArtworkId(1));
    }
}

#[test]
fn artwork_larger_than_the_artwork_box_is_refused() {
    for (width, height) in [(ARTWORK_BOX + 1, 100), (100, ARTWORK_BOX + 1)] {
        let error = Artwork::new(ArtworkId(1), image(width, height), screen(), ACCENT).unwrap_err();

        assert!(
            matches!(error, ArtworkError::ImageSize { width: w, height: h } if (w, h) == (width, height)),
            "{error:?}"
        );
    }
}

#[test]
fn a_hostile_header_is_refused_before_anything_is_decoded() {
    // 20000x20000 pixels would take over a gigabyte to decode.
    let error =
        Artwork::new(ArtworkId(1), header_only(20_000, 20_000), screen(), ACCENT).unwrap_err();

    assert!(
        matches!(
            error,
            ArtworkError::ImageSize {
                width: 20_000,
                height: 20_000
            }
        ),
        "{error:?}"
    );
}

#[test]
fn a_backdrop_of_any_other_size_than_the_screen_is_refused() {
    for (width, height) in [
        (SCREEN_WIDTH - 1, SCREEN_HEIGHT),
        (SCREEN_WIDTH, SCREEN_HEIGHT + 1),
        (8, 5),
    ] {
        let error =
            Artwork::new(ArtworkId(1), image(4, 4), image(width, height), ACCENT).unwrap_err();

        assert!(
            matches!(error, ArtworkError::BackdropSize { width: w, height: h } if (w, h) == (width, height)),
            "{error:?}"
        );
    }
}

#[test]
fn images_that_cannot_be_read_are_refused() {
    let not_an_image = header_only(0, 0);

    assert!(matches!(
        Artwork::new(ArtworkId(1), not_an_image.clone(), screen(), ACCENT),
        Err(ArtworkError::ImageUnreadable)
    ));
    assert!(matches!(
        Artwork::new(ArtworkId(1), image(4, 4), not_an_image, ACCENT),
        Err(ArtworkError::BackdropUnreadable)
    ));
}

#[test]
fn an_artwork_that_breaks_the_contract_cannot_be_read_from_a_message() {
    let read = |image: Image, backdrop: Image| {
        let wire = WireArtwork {
            id: ArtworkId(1),
            image,
            backdrop,
            accent: ACCENT,
        };
        postcard::from_bytes::<Artwork>(&postcard::to_allocvec(&wire).unwrap())
    };

    let good = read(image(ARTWORK_BOX, 198), screen()).unwrap();
    assert_eq!(good.image().size(), Some((ARTWORK_BOX, 198)));
    assert!(read(header_only(20_000, 20_000), screen()).is_err());
    assert!(read(image(4, 4), image(8, 5)).is_err());
}
